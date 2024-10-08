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
    /// Verilog output file
    #[arg(long, short)]
    output: Option<PathBuf>,
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

    if let Some(path) = cli.output {
        let mut out = std::fs::File::create(&path)?;
        kicad_verilog::write_verilog(&mut out, netlist, &module_name, config)?;
    } else {
        kicad_verilog::write_verilog(&mut std::io::stdout(), netlist, &module_name, config)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verify_cli() {
        use clap::CommandFactory;
        Cli::command().debug_assert()
    }
}
