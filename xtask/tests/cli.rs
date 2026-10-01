use std::process::{Command, Output};

fn xtask(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_xtask"))
        .args(args)
        .output()
        .expect("spawn xtask")
}

#[test]
fn no_subcommand_prints_usage_and_exits_2() {
    let out = xtask(&[]);
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    for name in ["build-napi", "package", "bench-startup"] {
        assert!(stderr.contains(name), "usage should name {name}: {stderr}");
    }
}

#[test]
fn unknown_subcommand_is_named_and_exits_2() {
    let out = xtask(&["frobnicate"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("frobnicate"));
}
