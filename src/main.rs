use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;
use kicad_netlist::NetList;
use kicad_verilog::Config;
use std::fs;

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    /// Path netlist file
    netlist: PathBuf,
    /// Config file
    #[arg(long, short)]
    config: Vec<PathBuf>,
    /// Module name
    #[arg(long, short)]
    module: Option<String>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let path = cli.netlist;
    let input = fs::read_to_string(&path)?;
    let netlist: NetList = (&input).try_into()?;

    let module_name = cli.module.unwrap_or_else(|| {
        path.file_name()
            .unwrap()
            .to_string_lossy()
            .split('.')
            .next()
            .unwrap()
            .to_string()
    });

    let mut config = Config::new();
    for path in cli.config {
        let input = fs::read_to_string(path)?;
        config.parse(&input)?;
    }

    kicad_verilog::do_it(netlist, &module_name, config)?;

    Ok(())
}
