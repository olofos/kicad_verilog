use assert_cmd::Command;

mod util;

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

#[test_with::executable(iverilog)]
fn can_compile_and_run_with_iverilog() {
    let dir = util::TempDir::create("can_compile_and_run_with_iverilog");

    let v_path = dir.file("simple.v");
    let exe_path = dir.file("a.out");

    let mut cmd = Command::cargo_bin(env!("CARGO_PKG_NAME")).unwrap();

    let assert = cmd
        .args([
            concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/simple.net"),
            "-c",
            concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/simple.vcfg"),
        ])
        .assert()
        .success();
    let output = assert.get_output();

    std::fs::write(&v_path, &output.stdout).unwrap();

    let mut cmd = Command::new("iverilog");
    cmd.arg("-g2012");
    cmd.arg(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/data/simple_tb.v"
    ));
    cmd.arg(&v_path);
    cmd.arg("-o");
    cmd.arg(&exe_path);
    cmd.assert().success();

    let mut cmd = Command::new(exe_path);
    cmd.assert().success();

    dir.delete();
}
