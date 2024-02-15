mod config;

use anyhow::{anyhow, Result};
use config::PartRule;
use kicad_netlist::{self, NetList, PinType};
use regex::Regex;
use std::{
    borrow::Cow,
    fs,
    path::{Path, PathBuf},
};

use crate::config::Config;

fn make_verilog_name<'a>(name: &'a str) -> Cow<'a, str> {
    static RE: once_cell::sync::Lazy<Regex> =
        once_cell::sync::Lazy::new(|| Regex::new(r"^[a-zA-Z_][a-zA-Z0-9$_]*$").unwrap());

    if RE.is_match(name) {
        name.into()
    } else {
        format!(r"\{name} ").into()
    }
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let path = if args.len() > 1 { &args[1] } else { "alu.net" };
    let path = Path::new(path);
    let input = fs::read_to_string(path)?;
    let mut netlist: NetList = (&input).try_into()?;

    let module_name = path.file_name().unwrap().to_string_lossy();
    let module_name = make_verilog_name(module_name.split(".").next().unwrap());

    let path = if args.len() > 2 {
        PathBuf::from(&args[2])
    } else {
        path.with_extension("vcfg")
    };
    let input = fs::read_to_string(path)?;
    let mut config = Config::parse(&input)?;

    let decoupling_caps = netlist
        .components
        .iter()
        .filter_map(|comp| {
            if comp.part_id.part != "C" {
                return None;
            }
            let nets = comp.pins.iter().map(|pin| pin.net).collect::<Vec<_>>();
            if nets.len() == 2 && nets.contains(&"VCC") && nets.contains(&"GND") {
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
            if comp.pins[0].net == "VCC" && comp.pins[1].net != "GND" {
                config.add_pullup(comp.ref_des, comp.pins[1].num);
            }
            if comp.pins[1].net == "VCC" && comp.pins[0].net != "GND" {
                config.add_pullup(comp.ref_des, comp.pins[0].num);
            }
            if comp.pins[0].net == "GND" {
                config.add_pulldown(comp.ref_des, comp.pins[1].num);
            }
            if comp.pins[1].net == "GND" {
                config.add_pulldown(comp.ref_des, comp.pins[0].num);
            }
        }
    }

    let (comp, pins) = netlist
        .components
        .iter_mut()
        .find_map(|comp| {
            let rule = config.match_component(comp);
            match rule {
                Some(PartRule::External(pins)) => Some((comp, pins)),
                _ => None,
            }
        })
        .unwrap();

    let nets = pins
        .iter()
        .map(|pin_num| {
            if let Some(pin) = comp.pins.iter().find(|pin| pin.num.0 == pin_num) {
                Ok(pin.net)
            } else {
                Err(anyhow!(
                    "No pin number {} found for component {}",
                    pin_num,
                    comp.ref_des.0
                ))
            }
        })
        .collect::<Result<Vec<_>>>()?;

    for net_name in &nets {
        let Some(net) = netlist.nets.iter_mut().find(|net| &net.name == net_name) else {
            continue;
        };
        let all_input = net
            .nodes
            .iter()
            .all(|node| node.ref_des == comp.ref_des || node.typ == PinType::Input);
        let any_output = net
            .nodes
            .iter()
            .any(|node| node.ref_des != comp.ref_des && node.typ == PinType::Output);
        let Some(node) = net
            .nodes
            .iter_mut()
            .find(|node| node.ref_des == comp.ref_des)
        else {
            continue;
        };
        if any_output {
            node.typ = PinType::Input;
        }
        if all_input {
            node.typ = PinType::Output;
        }
        comp.pins
            .iter_mut()
            .find(|pin| pin.num == node.num)
            .expect("There should be a matching pin")
            .typ = node.typ;
    }

    let net_string = nets
        .iter()
        .map(|net| {
            let pin = comp.pins.iter().find(|pin| &pin.net == net).unwrap();
            let name = make_verilog_name(pin.net);
            let typ = match pin.typ {
                PinType::Input => "output",
                PinType::Output => "input",
                _ => "inout",
            };
            format!("{typ} {name}")
        })
        .collect::<Vec<_>>()
        .join(",\n    ");

    println!("module {module_name}\n(\n    {net_string}\n);",);

    let external_nets = nets.into_iter().map(|name| name).collect::<Vec<_>>();

    for net in netlist
        .nets
        .iter()
        .filter(|net| external_nets.contains(&net.name))
    {
        eprint!("{}: ", net.name);
        for node in &net.nodes {
            eprint!("({}:{},{:?}) ", node.ref_des.0, node.num.0, node.typ);
        }
        eprintln!();
    }

    println!();
    for net in netlist
        .nets
        .iter()
        .filter(|net| !external_nets.contains(&net.name))
    {
        println!("    wire {};", make_verilog_name(net.name));
    }
    println!();
    println!("    assign VCC = 1;");
    println!("    assign GND = 0;");

    for comp in &netlist.components {
        if let Some(rule) = config.match_component(comp) {
            match rule {
                PartRule::Skip => unreachable!(),
                PartRule::External(_) => continue,
                PartRule::Module(name, pins) => {
                    let nets = pins
                        .iter()
                        .map(|pin_num| {
                            if let Some(pin) = comp.pins.iter().find(|pin| pin.num.0 == pin_num) {
                                Ok(make_verilog_name(pin.net))
                            } else {
                                Err(anyhow!(
                                    "No pin number {} found for component {}",
                                    pin_num,
                                    comp.ref_des.0
                                ))
                            }
                        })
                        .collect::<Result<Vec<_>>>()?;

                    println!(
                        "    {} {}({});",
                        name,
                        make_verilog_name(comp.ref_des.0),
                        nets.join(",")
                    );
                }
            }
        } else {
            eprintln!(
                "No rule matching component {}: {}",
                comp.ref_des.0, comp.part_id.part
            );
        }
    }
    println!("endmodule");

    Ok(())
}
