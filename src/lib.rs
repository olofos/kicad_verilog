use anyhow::{anyhow, Result};
use config::PartRule;
use kicad_netlist::{self, NetList, NetName, PinType};
use regex::Regex;
use std::borrow::Cow;

mod config;
pub use config::Config;

struct ModPort {
    name: String,
    net: String,
    typ: String,
}

fn make_verilog_name(name: &str) -> Cow<'_, str> {
    static RE: once_cell::sync::Lazy<Regex> =
        once_cell::sync::Lazy::new(|| Regex::new(r"^[a-zA-Z_][a-zA-Z0-9$_]*$").unwrap());

    if RE.is_match(name) {
        name.into()
    } else {
        format!(r"\{name} ").into()
    }
}

fn remove_decoupling_caps(netlist: &mut NetList, vcc_nets: &[NetName], gnd_nets: &[NetName]) {
    let decoupling_caps = netlist
        .components
        .iter()
        .filter_map(|comp| {
            if comp.part_id.part != "C" {
                return None;
            }
            let nets = comp.pins.iter().map(|pin| pin.net).collect::<Vec<_>>();
            if nets.len() == 2
                && vcc_nets.iter().any(|vcc| nets.contains(vcc))
                && gnd_nets.iter().any(|vcc| nets.contains(vcc))
            {
                Some(comp.ref_des)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    netlist.remove_components(&decoupling_caps);
}

fn remove_pinless_components(netlist: &mut NetList) {
    let pinless_components = netlist
        .components
        .iter()
        .filter_map(|comp| {
            if comp.pins.is_empty() {
                Some(comp.ref_des)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    netlist.remove_components(&pinless_components);
}

fn remove_skipped_components(netlist: &mut NetList, config: &Config) {
    let skipped_components = netlist
        .components
        .iter()
        .filter_map(|comp| {
            if config.match_component(comp) == Some(&config::PartRule::Skip) {
                Some(comp.ref_des)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    netlist.remove_components(&skipped_components);
}

fn add_pullups_and_pulldowns(
    config: &mut Config,
    netlist: &NetList,
    vcc_nets: &[NetName],
    gnd_nets: &[NetName],
) {
    for comp in &netlist.components {
        if comp.part_id.part == "R" && comp.pins.len() == 2 {
            if vcc_nets.contains(&comp.pins[0].net) && !gnd_nets.contains(&comp.pins[1].net) {
                config.add_pullup(comp.ref_des, comp.pins[1].num);
            }
            if vcc_nets.contains(&comp.pins[1].net) && !gnd_nets.contains(&comp.pins[0].net) {
                config.add_pullup(comp.ref_des, comp.pins[0].num);
            }
            if gnd_nets.contains(&comp.pins[0].net) {
                config.add_pulldown(comp.ref_des, comp.pins[1].num);
            }
            if gnd_nets.contains(&comp.pins[1].net) {
                config.add_pulldown(comp.ref_des, comp.pins[0].num);
            }
        }
    }
}

pub fn write_verilog(mut netlist: NetList, module_name: &str, mut config: Config) -> Result<()> {
    let module_name = make_verilog_name(&module_name);

    let vcc_nets: &[NetName] = &[NetName::from("VCC")];
    let gnd_nets: &[NetName] = &[NetName::from("GND")];

    remove_decoupling_caps(&mut netlist, vcc_nets, gnd_nets);
    remove_pinless_components(&mut netlist);
    remove_skipped_components(&mut netlist, &config);
    add_pullups_and_pulldowns(&mut config, &netlist, vcc_nets, gnd_nets);

    let mut mod_ports = vec![];

    for (ext_comp, ext_pins) in netlist.components.iter_mut().filter_map(|comp| {
        let rule = config.match_component(comp);
        match rule {
            Some(PartRule::External(pins)) => Some((comp, pins)),
            _ => None,
        }
    }) {
        for pin_num in ext_pins {
            let Some(pin) = ext_comp
                .pins
                .iter_mut()
                .find(|pin| pin.num == pin_num.into())
            else {
                anyhow::bail!(
                    "No pin number {} found for component {}",
                    pin_num,
                    ext_comp.ref_des
                )
            };

            let Some(net) = netlist.nets.iter_mut().find(|net| net.name == pin.net) else {
                continue;
            };
            let all_input = net
                .nodes
                .iter()
                .all(|node| node.ref_des == ext_comp.ref_des || node.typ == PinType::Input);

            let any_output = net
                .nodes
                .iter()
                .any(|node| node.ref_des != ext_comp.ref_des && node.typ == PinType::Output);

            let Some(node) = net
                .nodes
                .iter_mut()
                .find(|node| node.ref_des == ext_comp.ref_des)
            else {
                continue;
            };
            if any_output {
                node.typ = PinType::Input;
            }
            if all_input {
                node.typ = PinType::Output;
            }
            pin.typ = node.typ;
            let name = format!("{}_{}", ext_comp.ref_des, pin.name);
            let name = make_verilog_name(&name).to_string();
            let net = make_verilog_name(pin.net.as_str()).to_string();
            let typ = match pin.typ {
                PinType::Input => "output",
                PinType::Output => "input",
                _ => "inout",
            }
            .to_string();
            mod_ports.push(ModPort { name, net, typ })
        }
    }

    let port_string = mod_ports
        .iter()
        .map(|ModPort { name, typ, net: _ }| format!("{typ} {name}",))
        .collect::<Vec<_>>()
        .join(",\n    ");
    println!("module {module_name}\n(\n    {port_string}\n);",);

    println!();
    for net in netlist.nets.iter() {
        println!("    wire {};", make_verilog_name(net.name.as_str()));
    }
    println!();
    println!("    assign VCC = 1;");
    println!("    assign GND = 0;");
    println!();
    for ModPort { name, net, typ: _ } in &mod_ports {
        println!("    tran({name},{net});");
    }

    println!();

    for comp in &netlist.components {
        if let Some(rule) = config.match_component(comp) {
            match rule {
                PartRule::Skip => unreachable!(),
                PartRule::External(_) => continue,
                PartRule::Module(name, pins) => {
                    let nets = pins
                        .iter()
                        .map(|pin_num| {
                            if let Some(pin) =
                                comp.pins.iter().find(|pin| pin.num.as_str() == pin_num)
                            {
                                Ok(make_verilog_name(pin.net.as_str()))
                            } else {
                                Err(anyhow!(
                                    "No pin number {} found for component {}",
                                    pin_num,
                                    comp.ref_des
                                ))
                            }
                        })
                        .collect::<Result<Vec<_>>>()?;

                    println!(
                        "    {} {}({});",
                        name,
                        make_verilog_name(comp.ref_des.as_str()),
                        nets.join(",")
                    );
                }
            }
        } else {
            return Err(anyhow!(
                "No rule matching component {}: {}",
                comp.ref_des,
                comp.part_id.part
            ));
        }
    }
    println!("endmodule");

    Ok(())
}
