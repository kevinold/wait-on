//! `ci` and the cargo one-liners (`fmt`, `lint`, `test`, `cov`).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::{bench, build_napi, contract, host};

/// One step of `cargo xtask ci`.
#[derive(Debug, PartialEq)]
pub enum Step {
    Cargo(Vec<String>),
    BuildNapi,
    Mocha,
    Contract,
    BenchStartup,
}

fn strings(args: &[&str]) -> Vec<String> {
    args.iter().map(|s| s.to_string()).collect()
}

pub fn fmt_args() -> Vec<String> {
    strings(&["fmt", "--all", "--check"])
}

pub fn lint_args() -> Vec<String> {
    strings(&[
        "clippy",
        "--workspace",
        "--all-targets",
        "--",
        "-D",
        "warnings",
    ])
}

pub fn test_args() -> Vec<String> {
    strings(&["test", "--workspace"])
}

pub fn cov_args(extra: &[String]) -> Vec<String> {
    let mut args = strings(&["llvm-cov", "--workspace"]);
    args.extend_from_slice(extra);
    args
}

/// The `ci` gate, in order; the driver stops at the first failure.
/// Vet first (an unvetted crate fails before any build), the consumer contract after mocha
/// (on a pack of the working tree), bench last (never masks a test failure).
pub fn steps() -> Vec<Step> {
    vec![
        Step::Cargo(strings(&["vet", "--locked"])),
        Step::Cargo(fmt_args()),
        Step::Cargo(lint_args()),
        Step::Cargo(test_args()),
        Step::Cargo(strings(&["deny", "check"])),
        Step::BuildNapi,
        Step::Mocha,
        Step::Contract,
        Step::BenchStartup,
    ]
}

/// The mocha step's program, args and whole child env. Mocha expands the glob itself (no shell).
pub fn mocha_command(root: &Path) -> (PathBuf, Vec<String>, HashMap<String, String>) {
    let mocha = root
        .join("node_modules")
        .join("mocha")
        .join("bin")
        .join("mocha.js");
    let args = vec![
        mocha.to_string_lossy().to_string(),
        "--exit".to_string(),
        "test/**/*.mocha.js".to_string(),
    ];
    let mut env = host::env_map();
    host::env_set(&mut env, "WAIT_ON_ENGINE", "rust-strict");
    (host::node_exe(), args, env)
}

/// The line echoed before a step runs.
pub fn echo(step: &Step, root: &Path) -> String {
    let line = match step {
        Step::Cargo(args) => format!("cargo {}", args.join(" ")),
        Step::BuildNapi => "xtask build-napi".to_string(),
        Step::BenchStartup => "xtask bench-startup".to_string(),
        Step::Contract => "xtask contract".to_string(),
        Step::Mocha => {
            let (node, args, _) = mocha_command(root);
            let node = node.file_name().unwrap_or_default().to_string_lossy();
            format!("{node} {}", args.join(" "))
        }
    };
    format!("> {line}")
}

/// `$CARGO` (set by `cargo run`) or `cargo` on PATH.
fn cargo_exe() -> PathBuf {
    std::env::var_os("CARGO").map_or_else(|| PathBuf::from("cargo"), PathBuf::from)
}

/// The env for an inner cargo call: the parent env with `CARGO_TARGET_DIR` pointed at a
/// directory of its own, so it can never relink the running xtask binary (Windows locks it).
pub fn cargo_env(root: &Path, mut parent: HashMap<String, String>) -> HashMap<String, String> {
    let outer = host::env_get(&parent, "CARGO_TARGET_DIR")
        .map_or_else(|| root.join("target"), |dir| root.join(dir));
    let inner = outer.join("xtask-inner");
    host::env_set(
        &mut parent,
        "CARGO_TARGET_DIR",
        &inner.display().to_string(),
    );
    parent
}

fn cargo(args: &[String]) -> i32 {
    let root = host::repo_root();
    let env = cargo_env(&root, host::env_map());
    host::run(&cargo_exe(), args, &root, Some(&env))
}

pub fn fmt(_: &[String]) -> i32 {
    cargo(&fmt_args())
}

pub fn lint(_: &[String]) -> i32 {
    cargo(&lint_args())
}

pub fn test(_: &[String]) -> i32 {
    cargo(&test_args())
}

pub fn cov(args: &[String]) -> i32 {
    cargo(&cov_args(args))
}

/// The `ci` subcommand: stops at the first failing step and exits with its code.
// ponytail: no subprocess test for this loop (a fake cargo on PATH is not Windows-safe);
// steps() and host::run's exit-code tests cover it; the orchestrator observes it end to end.
pub fn run(_: &[String]) -> i32 {
    let root = host::repo_root();
    for step in steps() {
        println!("{}", echo(&step, &root));
        let code = match &step {
            Step::Cargo(args) => cargo(args),
            Step::BuildNapi => build_napi::run(&[]),
            Step::BenchStartup => bench::run(&[]),
            Step::Contract => contract::run(&[]),
            Step::Mocha => {
                let (node, args, env) = mocha_command(&root);
                host::run(&node, &args, &root, Some(&env))
            }
        };
        if code != 0 {
            return code;
        }
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_command_args_are_exact() {
        assert_eq!(fmt_args(), strings(&["fmt", "--all", "--check"]));
        assert_eq!(
            lint_args(),
            strings(&[
                "clippy",
                "--workspace",
                "--all-targets",
                "--",
                "-D",
                "warnings"
            ])
        );
        assert_eq!(test_args(), strings(&["test", "--workspace"]));
    }

    #[test]
    fn cov_forwards_extra_args_to_llvm_cov() {
        assert_eq!(cov_args(&[]), strings(&["llvm-cov", "--workspace"]));
        assert_eq!(
            cov_args(&strings(&["--lcov", "--output-path", "a b.info"])),
            strings(&[
                "llvm-cov",
                "--workspace",
                "--lcov",
                "--output-path",
                "a b.info"
            ])
        );
    }

    /// Every cargo argv xtask spawns: the ci steps plus the single-command subcommands.
    fn inner_cargo_commands() -> Vec<Vec<String>> {
        let mut all: Vec<Vec<String>> = steps()
            .into_iter()
            .filter_map(|s| match s {
                Step::Cargo(args) => Some(args),
                _ => None,
            })
            .collect();
        all.extend([fmt_args(), lint_args(), test_args(), cov_args(&[])]);
        all
    }

    #[test]
    fn inner_cargo_never_shares_the_running_xtask_target_dir() {
        let root = Path::new("/repo");
        let parent = HashMap::from([("PATH".to_string(), "/bin".to_string())]);
        let env = cargo_env(root, parent);
        let dir = PathBuf::from(env.get("CARGO_TARGET_DIR").expect("CARGO_TARGET_DIR set"));
        assert_eq!(dir, root.join("target").join("xtask-inner"));
        assert_eq!(
            env.get("PATH").map(String::as_str),
            Some("/bin"),
            "parent env kept"
        );
        assert_eq!(
            inner_cargo_commands().len(),
            9,
            "every inner cargo call routes through cargo()"
        );
    }

    #[test]
    fn inner_target_dir_nests_under_a_caller_set_target_dir() {
        let root = Path::new("/repo");
        let parent = HashMap::from([("CARGO_TARGET_DIR".to_string(), "/tmp/t".to_string())]);
        let env = cargo_env(root, parent);
        assert_eq!(
            PathBuf::from(&env["CARGO_TARGET_DIR"]),
            Path::new("/tmp/t").join("xtask-inner")
        );
    }

    #[test]
    fn inner_target_dir_resolves_a_relative_caller_dir_against_the_repo_root() {
        let root = Path::new("/repo");
        let parent = HashMap::from([("CARGO_TARGET_DIR".to_string(), "out".to_string())]);
        let env = cargo_env(root, parent);
        assert_eq!(
            PathBuf::from(&env["CARGO_TARGET_DIR"]),
            root.join("out").join("xtask-inner")
        );
    }

    #[test]
    fn ci_runs_vet_fmt_lint_test_deny_build_mocha_contract_then_bench() {
        assert_eq!(
            steps(),
            vec![
                Step::Cargo(strings(&["vet", "--locked"])),
                Step::Cargo(strings(&["fmt", "--all", "--check"])),
                Step::Cargo(strings(&[
                    "clippy",
                    "--workspace",
                    "--all-targets",
                    "--",
                    "-D",
                    "warnings"
                ])),
                Step::Cargo(strings(&["test", "--workspace"])),
                Step::Cargo(strings(&["deny", "check"])),
                Step::BuildNapi,
                Step::Mocha,
                Step::Contract,
                Step::BenchStartup,
            ]
        );
    }

    #[test]
    fn mocha_runs_under_node_with_rust_strict_and_the_parent_env() {
        let root = Path::new("/r");
        let (program, args, env) = mocha_command(root);
        assert_eq!(program, host::node_exe());
        let mocha = root
            .join("node_modules")
            .join("mocha")
            .join("bin")
            .join("mocha.js");
        assert_eq!(
            args,
            vec![
                mocha.to_string_lossy().to_string(),
                "--exit".to_string(),
                "test/**/*.mocha.js".to_string()
            ]
        );
        assert_eq!(
            env.get("WAIT_ON_ENGINE").map(String::as_str),
            Some("rust-strict")
        );
        assert_eq!(
            host::env_get(&env, "PATH").map(String::from),
            std::env::var("PATH").ok()
        );
    }

    #[test]
    fn each_step_echoes_its_command() {
        let root = Path::new("/r");
        let lines: Vec<String> = steps().iter().map(|s| echo(s, root)).collect();
        let mocha = root
            .join("node_modules")
            .join("mocha")
            .join("bin")
            .join("mocha.js");
        let node = host::node_exe();
        let node = node.file_name().unwrap().to_string_lossy();
        assert_eq!(
            lines,
            vec![
                "> cargo vet --locked".to_string(),
                "> cargo fmt --all --check".to_string(),
                "> cargo clippy --workspace --all-targets -- -D warnings".to_string(),
                "> cargo test --workspace".to_string(),
                "> cargo deny check".to_string(),
                "> xtask build-napi".to_string(),
                format!("> {node} {} --exit test/**/*.mocha.js", mocha.display()),
                "> xtask contract".to_string(),
                "> xtask bench-startup".to_string(),
            ]
        );
    }
}
