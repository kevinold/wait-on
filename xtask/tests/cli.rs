use std::process::{Command, Output};

fn xtask(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_xtask"))
        .args(args)
        .output()
        .expect("spawn xtask")
}

const SUBCOMMANDS: [&str; 13] = [
    "ci",
    "fmt",
    "lint",
    "test",
    "cov",
    "build-napi",
    "package",
    "contract",
    "dependents",
    "linux",
    "act",
    "bench-startup",
    "hooks",
];

fn assert_lists_every_subcommand(text: &str) {
    for name in SUBCOMMANDS {
        assert!(
            text.lines()
                .any(|l| l.split_whitespace().next() == Some(name)),
            "usage should list {name} on its own line: {text}"
        );
    }
}

#[test]
fn no_subcommand_prints_usage_and_exits_2() {
    let out = xtask(&[]);
    assert_eq!(out.status.code(), Some(2));
    assert_lists_every_subcommand(&String::from_utf8_lossy(&out.stderr));
}

#[test]
fn help_lists_every_subcommand_and_exits_0() {
    for flag in ["--help", "-h", "help"] {
        let out = xtask(&[flag]);
        assert_eq!(out.status.code(), Some(0), "{flag}");
        assert_lists_every_subcommand(&String::from_utf8_lossy(&out.stdout));
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
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("frobnicate"));
    assert_lists_every_subcommand(&stderr);
}

#[test]
fn package_refuses_to_run_outside_npm() {
    let out = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .args(["package", "--host-only"])
        .env_remove("npm_execpath")
        .output()
        .expect("spawn xtask");
    assert_ne!(out.status.code(), Some(0));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("run this through npm"), "{stderr}");
}

#[test]
fn contract_refuses_to_run_outside_npm() {
    let out = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .args(["contract", "--engine", "js"])
        .env_remove("npm_execpath")
        .output()
        .expect("spawn xtask");
    assert_ne!(out.status.code(), Some(0));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("run this through npm"), "{stderr}");
}

#[test]
fn dependents_names_an_unknown_entry_before_cloning_anything() {
    let out = xtask(&["dependents", "--only", "nope"]);
    assert_ne!(out.status.code(), Some(0));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("--only nope") && stderr.contains("start-server-and-test"),
        "{stderr}"
    );
}

#[test]
fn dependents_refuses_to_run_outside_npm() {
    let out = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .args(["dependents", "--only", "start-server-and-test"])
        .env_remove("npm_execpath")
        .output()
        .expect("spawn xtask");
    assert_ne!(out.status.code(), Some(0));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("run this through npm"), "{stderr}");
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

#[test]
fn hooks_points_core_hookspath_at_githooks_in_the_current_repo() {
    let repo = std::env::temp_dir().join(format!("wait-on-hooks-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&repo);
    std::fs::create_dir_all(&repo).expect("create temp repo dir");
    let git = |args: &[&str]| Command::new("git").args(args).current_dir(&repo).output();
    if Command::new("git").arg("--version").output().is_err() {
        eprintln!("skipping: git is not available");
        return;
    }
    let init = git(&["init", "-q"]).expect("spawn git init");
    assert!(init.status.success(), "git init failed: {init:?}");
    let out = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .arg("hooks")
        .current_dir(&repo)
        .output()
        .expect("spawn xtask");
    let set = git(&["config", "--get", "core.hooksPath"]).expect("read config");
    let _ = std::fs::remove_dir_all(&repo);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert_eq!(String::from_utf8_lossy(&set.stdout).trim(), ".githooks");
}

/// `docker info` against a dead endpoint, so the engine probe fails whether or not a docker
/// CLI and engine exist. Emptying PATH is not enough: Windows still finds docker in its
/// system directories, and the windows runner's engine answers.
fn without_docker(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_xtask"))
        .args(args)
        .env("DOCKER_HOST", "tcp://127.0.0.1:1")
        .output()
        .expect("spawn xtask")
}

#[test]
fn linux_skips_with_exit_0_when_no_docker_engine_is_reachable() {
    let out = without_docker(&["linux", "true"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(0), "{stderr}");
    assert!(stderr.contains("docker info"), "{stderr}");
    assert!(stderr.contains("skipping"), "{stderr}");
}

#[test]
fn act_skips_with_exit_0_when_no_docker_engine_is_reachable() {
    let out = without_docker(&["act", "--job", "build"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(0), "{stderr}");
    assert!(stderr.contains("docker info"), "{stderr}");
    assert!(stderr.contains("skipping"), "{stderr}");
}
