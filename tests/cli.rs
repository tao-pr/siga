use std::process::Command;

#[test]
fn rejects_an_unsupported_spec_file() {
    let out = Command::new(env!("CARGO_BIN_EXE_siga"))
        .args(["--api", "Cargo.toml"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains(".yaml, .yml or .json"));
}

#[test]
fn requires_the_api_flag() {
    let out = Command::new(env!("CARGO_BIN_EXE_siga")).output().unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("--api"));
}
