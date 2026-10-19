use std::process::Command;

#[test]
fn smoke_flag_exits_zero_and_prints_ok() {
    let output = Command::new(env!("CARGO_BIN_EXE_openmixer"))
        .arg("--smoke")
        .output()
        .expect("failed to run openmixer binary");

    assert!(output.status.success(), "status: {:?}", output.status);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("smoke ok"), "stdout was: {stdout}");
}
