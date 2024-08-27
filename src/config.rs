use anyhow::Result;
use kicad_netlist::{Component, PinNum, RefDes};
use nom::{
    self,
    branch::alt,
    bytes::complete::tag,
    character::complete::{alphanumeric1, multispace0, none_of},
    combinator::{map, recognize, value},
    multi::{many0, many1, separated_list0},
    sequence::{delimited, preceded, separated_pair, terminated, tuple},
    IResult,
};

#[derive(Debug, Clone)]
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

#[derive(Debug, Clone)]
struct PartPatternRule {
    pattern: PartPattern,
    rule: PartRule,
}

#[derive(Debug, Clone)]
pub struct Config {
    part_rules: Vec<PartPatternRule>,
}

fn part_pattern(i: &str) -> IResult<&str, PartPattern> {
    alt((
        map(
            delimited(tag("["), recognize(many1(none_of("[]= \r\t\n"))), tag("]")),
            |s: &str| PartPattern::RefDes(s.to_string()),
        ),
        map(recognize(many1(none_of("= \r\t\n"))), |s: &str| {
            PartPattern::Part(s.to_string())
        }),
    ))(i)
}

fn pin_list(i: &str) -> IResult<&str, Vec<String>> {
    separated_list0(
        tuple((multispace0, tag(","), multispace0)),
        map(preceded(tag("#"), alphanumeric1), |s: &str| s.to_owned()),
    )(i)
}

fn part_rule(i: &str) -> IResult<&str, PartRule> {
    alt((
        value(PartRule::Skip, tag("skip")),
        map(
            delimited(tag("module["), pin_list, tag("]")),
            |pins: Vec<String>| PartRule::External(pins),
        ),
        map(
            tuple((
                recognize(many1(none_of("()\r\n"))),
                delimited(tag("("), pin_list, tag(")")),
            )),
            |(name, pins): (&str, Vec<String>)| PartRule::Module(name.to_string(), pins),
        ),
    ))(i)
}

fn part_pattern_rule(i: &str) -> IResult<&str, PartPatternRule> {
    let (i, (pattern, rule)) = terminated(
        separated_pair(
            part_pattern,
            tuple((multispace0, tag("=>"), multispace0)),
            part_rule,
        ),
        multispace0,
    )(i)?;
    Ok((i, PartPatternRule { pattern, rule }))
}

fn part_pattern_rules(i: &str) -> IResult<&str, Vec<PartPatternRule>> {
    many0(part_pattern_rule)(i)
}

impl Config {
    pub fn parse(i: &str) -> Result<Config> {
        let (_, part_rules) = part_pattern_rules(i).map_err(|err| err.to_owned())?;

        Ok(Config { part_rules })
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
        let input =
            std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/test-data/alu.vcfg"))
                .unwrap();
        let config = Config::parse(&input).unwrap();
        for rule in &config.part_rules {
            println!("{:?} -> {:?}", rule.pattern, rule.rule);
        }
    }
}
