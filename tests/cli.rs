use assert_cmd::Command;

#[test]
fn cli_works() {
    let mut cmd = Command::cargo_bin(env!("CARGO_PKG_NAME")).unwrap();
    cmd.arg("--help").assert().success();
}

#[test]
fn can_output_simple_verilog_file() -> anyhow::Result<()> {
    let expected_output =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/simple.v"))?;

    let mut cmd = Command::cargo_bin(env!("CARGO_PKG_NAME")).unwrap();
    cmd.args([
        concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/simple.net"),
        "-c",
        concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/simple.vcfg"),
    ])
    .assert()
    .success()
    .stdout(expected_output);

    Ok(())
}
