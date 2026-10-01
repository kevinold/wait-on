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
fn build_napi_rejects_an_unknown_target_before_spawning_napi() {
    let out = xtask(&["build-napi", "--target", "riscv64gc-unknown-linux-gnu"]);
    assert_ne!(out.status.code(), Some(0));
    let stderr = String::from_utf8_lossy(&out.stderr);
    for triple in [
        "riscv64gc-unknown-linux-gnu",
        "aarch64-apple-darwin",
        "aarch64-pc-windows-msvc",
    ] {
        assert!(
            stderr.contains(triple),
            "stderr should name {triple}: {stderr}"
        );
    }
}

#[test]
fn unknown_subcommand_is_named_and_exits_2() {
    let out = xtask(&["frobnicate"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("frobnicate"));
}

#[test]
fn bench_startup_names_the_addon_when_the_rust_engine_cannot_load() {
    let missing = std::env::temp_dir().join(format!("wait-on-missing-{}.node", std::process::id()));
    let out = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .args(["bench-startup", "--runs", "1"])
        .env("WAIT_ON_NATIVE_LIBRARY_PATH", &missing)
        .output()
        .expect("spawn xtask");
    assert_ne!(out.status.code(), Some(0));
    let text =
        String::from_utf8_lossy(&out.stdout).to_string() + &String::from_utf8_lossy(&out.stderr);
    assert!(text.contains("rust-strict"), "{text}");
    assert!(text.contains(&*missing.to_string_lossy()), "{text}");
}
