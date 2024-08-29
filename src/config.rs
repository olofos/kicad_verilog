use std::collections::HashSet;

use anyhow::Result;
use kicad_netlist::{Component, PinNum, RefDes};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PartPattern {
    RefDes(String),
    Part(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PartRule {
    Skip,
    External(Vec<String>),
    Module(String, Vec<String>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PartPatternRule {
    pattern: PartPattern,
    rule: PartRule,
}

#[derive(Default, Debug, Clone)]
pub struct Config {
    part_rules: Vec<PartPatternRule>,
}

fn strip_prefix_and_suffix<'a>(input: &'a str, prefix: &str, suffix: &str) -> Option<&'a str> {
    if let Some(input) = input.strip_prefix(prefix) {
        input.strip_suffix(suffix)
    } else {
        None
    }
}

fn parse_pattern(input: &str) -> anyhow::Result<PartPattern> {
    let pattern = input.trim();

    if let Some(pattern) = strip_prefix_and_suffix(pattern, "[", "]") {
        Ok(PartPattern::RefDes(pattern.to_string()))
    } else {
        Ok(PartPattern::Part(pattern.to_string()))
    }
}

fn parse_pin(input: &str) -> anyhow::Result<String> {
    let input = input.trim();

    if let Some(pin) = input.strip_prefix("#") {
        Ok(pin.to_string())
    } else {
        Err(anyhow::anyhow!("Invalid pin: {input}"))
    }
}

fn parse_pins(input: &str) -> anyhow::Result<Vec<String>> {
    let input = input.trim();
    if input.is_empty() {
        Ok(vec![])
    } else {
        input.split(',').map(parse_pin).collect::<Result<_, _>>()
    }
}

fn parse_rule(input: &str) -> anyhow::Result<PartRule> {
    let input = input.trim();
    if input == "skip" {
        Ok(PartRule::Skip)
    } else if let Some(pins) = strip_prefix_and_suffix(input, "module[", "]") {
        Ok(PartRule::External(parse_pins(pins)?))
    } else if let Some((module, pins)) = input.split_once('(') {
        if let Some(pins) = pins.strip_suffix(")") {
            Ok(PartRule::Module(module.to_string(), parse_pins(pins)?))
        } else {
            Err(anyhow::anyhow!("Invalid rule: '{input}'"))
        }
    } else {
        Err(anyhow::anyhow!("Invalid rule: '{input}'"))
    }
}

fn parse(input: &str) -> anyhow::Result<Vec<PartPatternRule>> {
    let mut result = vec![];
    for line in input.lines() {
        let Some((pattern, rule)) = line.split_once("=>") else {
            anyhow::bail!("Line does not contain '=>'");
        };
        let pattern = parse_pattern(pattern)?;
        let rule = parse_rule(rule)?;

        result.push(PartPatternRule { pattern, rule });
    }
    Ok(result)
}

impl Config {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn parse(&mut self, i: &str) -> Result<()> {
        let part_rules = parse(i).map_err(|s| anyhow::anyhow!("{s}"))?;

        let old_set = HashSet::<_>::from_iter(self.part_rules.iter().map(|rule| &rule.pattern));
        let mut new_set = HashSet::new();
        for rule in &part_rules {
            if new_set.contains(&rule.pattern) {
                return Err(anyhow::anyhow!(
                    "Config file contains multiple rules for {:?}",
                    rule.pattern
                ));
            }
            if old_set.contains(&rule.pattern) {
                return Err(anyhow::anyhow!("Duplicate rule for {:?}", rule.pattern));
            }
            new_set.insert(&rule.pattern);
        }

        self.part_rules.extend(part_rules);
        Ok(())
    }

    #[allow(dead_code)]
    pub fn try_from(i: &str) -> Result<Self> {
        let mut config = Self::new();
        config.parse(i)?;
        Ok(config)
    }

    pub fn match_component(&self, comp: &Component) -> Option<&PartRule> {
        for rule in &self.part_rules {
            if match &rule.pattern {
                PartPattern::RefDes(ref_des) => ref_des == comp.ref_des.as_str(),
                PartPattern::Part(part) => part == comp.part_id.part,
            } {
                return Some(&rule.rule);
            }
        }
        None
    }

    pub fn add_pullup(&mut self, ref_des: RefDes, pin: PinNum) {
        self.part_rules.push(PartPatternRule {
            pattern: PartPattern::RefDes(ref_des.to_string()),
            rule: PartRule::Module("pullup".to_string(), vec![pin.to_string()]),
        })
    }

    pub fn add_pulldown(&mut self, ref_des: RefDes, pin: PinNum) {
        self.part_rules.push(PartPatternRule {
            pattern: PartPattern::RefDes(ref_des.to_string()),
            rule: PartRule::Module("pulldown".to_string(), vec![pin.to_string()]),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn can_parse_config() {
        let input1 =
            std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/alu.vcfg"))
                .unwrap();
        let input2 = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/data/common.vcfg"
        ))
        .unwrap();
        let mut config = Config::try_from(&input1).unwrap();
        config.parse(&input2).unwrap();
        for rule in &config.part_rules {
            println!("{:?} -> {:?}", rule.pattern, rule.rule);
        }
    }

    #[test]
    fn get_error_for_pins_without_hash() {
        let i = "a => a(#2)\nb => b(2)";
        let Err(_) = Config::try_from(i) else {
            panic!("expected error")
        };
    }

    #[test]
    fn get_error_when_multiple_rules_with_same_pattern() {
        let i = "a => a()\na => b()";
        let Err(_) = Config::try_from(i) else {
            panic!("expected error")
        };
    }

    #[test]
    fn get_error_when_multiple_rules_with_same_pattern2() {
        let i1 = "a => a()";
        let i2 = "a => b()";
        let mut config = Config::try_from(i1).unwrap();

        let Err(_) = config.parse(i2) else {
            panic!("expected error")
        };
    }
}
