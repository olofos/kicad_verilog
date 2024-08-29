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

fn collect_mod_ports(netlist: &mut NetList, config: &Config) -> Result<Vec<ModPort>> {
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
    Ok(mod_ports)
}

fn collect_comps<'a>(netlist: &'a NetList, config: &'a Config) -> Result<Vec<Comp<'a>>> {
    let mut comps = vec![];
    for comp in &netlist.components {
        if let Some(rule) = config.match_component(comp) {
            match rule {
                PartRule::Skip => unreachable!(),
                PartRule::External(_) => continue,
                PartRule::Module(module, pins) => {
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

                    comps.push(Comp {
                        module,
                        name: make_verilog_name(comp.ref_des.as_str()),
                        nets,
                    })
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
    Ok(comps)
}

struct Comp<'a> {
    name: Cow<'a, str>,
    module: &'a str,
    nets: Vec<Cow<'a, str>>,
}

pub fn write_verilog(
    out: &mut impl std::io::Write,
    mut netlist: NetList,
    module_name: &str,
    mut config: Config,
) -> Result<()> {
    let module_name = make_verilog_name(module_name);

    let vcc_nets: &[NetName] = &[NetName::from("VCC")];
    let gnd_nets: &[NetName] = &[NetName::from("GND")];

    remove_decoupling_caps(&mut netlist, vcc_nets, gnd_nets);
    remove_pinless_components(&mut netlist);
    remove_skipped_components(&mut netlist, &config);
    add_pullups_and_pulldowns(&mut config, &netlist, vcc_nets, gnd_nets);
    let mod_ports = collect_mod_ports(&mut netlist, &config)?;
    let comps = collect_comps(&netlist, &config)?;

    if mod_ports.is_empty() {
        writeln!(out, "module {module_name}();")?;
    } else {
        write!(out, "module {module_name}\n(\n    ")?;
        let mut sep = "";
        for ModPort { name, typ, net: _ } in &mod_ports {
            write!(out, "{sep}{typ} {name}",)?;
            sep = ",\n    ";
        }
        writeln!(out, "\n);")?;
    }

    writeln!(out,)?;
    for net in netlist.nets.iter() {
        writeln!(out, "    wire {};", make_verilog_name(net.name.as_str()))?;
    }
    writeln!(out,)?;
    writeln!(out, "    assign VCC = 1;")?;
    writeln!(out, "    assign GND = 0;")?;
    writeln!(out,)?;
    for ModPort { name, net, typ: _ } in &mod_ports {
        writeln!(out, "    tran({name},{net});")?;
    }

    writeln!(out,)?;

    for comp in comps {
        write!(out, "    {}", comp.module)?;
        if !comp.module.ends_with(" ") {
            write!(out, " ")?;
        }
        write!(out, "{}(", comp.name,)?;
        let mut sep = "";
        for net in &comp.nets {
            write!(out, "{sep}{net}")?;
            sep = ",";
        }
        writeln!(out, ");")?;
    }
    writeln!(out, "endmodule")?;

    Ok(())
}
