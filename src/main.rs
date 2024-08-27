mod config;

use anyhow::{anyhow, Result};
use config::PartRule;
use kicad_netlist::{self, NetList, NetName, PinType};
use regex::Regex;
use std::{
    borrow::Cow,
    fs,
    path::{Path, PathBuf},
};

use crate::config::Config;

fn make_verilog_name(name: &str) -> Cow<'_, str> {
    static RE: once_cell::sync::Lazy<Regex> =
        once_cell::sync::Lazy::new(|| Regex::new(r"^[a-zA-Z_][a-zA-Z0-9$_]*$").unwrap());

    if RE.is_match(name) {
        name.into()
    } else {
        format!(r"\{name} ").into()
    }
}

struct ModPort {
    name: String,
    net: String,
    typ: String,
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let path = if args.len() > 1 { &args[1] } else { "alu.net" };
    let path = Path::new(path);
    let input = fs::read_to_string(path)?;
    let mut netlist: NetList = (&input).try_into()?;

    let module_name = path.file_name().unwrap().to_string_lossy();
    let module_name = make_verilog_name(module_name.split('.').next().unwrap());

    let path = if args.len() > 2 {
        PathBuf::from(&args[2])
    } else {
        path.with_extension("vcfg")
    };
    let input = fs::read_to_string(path)?;
    let mut config = Config::try_from(&input)?;

    let vcc_nets: &[NetName] = &[NetName::from("VCC")];
    let gnd_nets: &[NetName] = &[NetName::from("GND")];

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

    let mut mod_ports = vec![];

    for (ext_comp, ext_pins) in netlist.components.iter_mut().filter_map(|comp| {
        let rule = config.match_component(comp);
        match rule {
            Some(PartRule::External(pins)) => Some((comp, pins)),
            _ => None,
        }
    }) {
        let ext_nets = ext_pins
            .iter()
            .map(|pin_num| {
                if let Some(pin) = ext_comp.pins.iter().find(|pin| pin.num.as_str() == pin_num) {
                    Ok(pin.net)
                } else {
                    Err(anyhow!(
                        "No pin number {} found for component {}",
                        pin_num,
                        ext_comp.ref_des
                    ))
                }
            })
            .collect::<Result<Vec<_>>>()?;

        for net_name in &ext_nets {
            let Some(net) = netlist.nets.iter_mut().find(|net| &net.name == net_name) else {
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
            ext_comp
                .pins
                .iter_mut()
                .find(|pin| pin.num == node.num)
                .expect("There should be a matching pin")
                .typ = node.typ;
        }

        for net in &ext_nets {
            let pin = ext_comp.pins.iter().find(|pin| &pin.net == net).unwrap();
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
