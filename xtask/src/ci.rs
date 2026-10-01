//! `ci` and the cargo one-liners (`fmt`, `lint`, `test`, `cov`).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::{bench, build_napi, host};

/// One step of `cargo xtask ci`.
#[derive(Debug, PartialEq)]
pub enum Step {
    Cargo(Vec<String>),
    BuildNapi,
    Mocha,
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
/// Vet first (an unvetted crate fails before any build), bench last (never masks a test failure).
pub fn steps() -> Vec<Step> {
    vec![
        Step::Cargo(strings(&["vet", "--locked"])),
        Step::Cargo(fmt_args()),
        Step::Cargo(lint_args()),
        Step::Cargo(test_args()),
        Step::Cargo(strings(&["deny", "check"])),
        Step::BuildNapi,
        Step::Mocha,
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
    env.insert("WAIT_ON_ENGINE".to_string(), "rust-strict".to_string());
    (host::node_exe(), args, env)
}

/// The line echoed before a step runs.
pub fn echo(step: &Step, root: &Path) -> String {
    let line = match step {
        Step::Cargo(args) => format!("cargo {}", args.join(" ")),
        Step::BuildNapi => "xtask build-napi".to_string(),
        Step::BenchStartup => "xtask bench-startup".to_string(),
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

fn cargo(args: &[String]) -> i32 {
    host::run(&cargo_exe(), args, &host::repo_root(), None)
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

    #[test]
    fn ci_runs_vet_fmt_lint_test_deny_build_mocha_then_bench() {
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
        assert_eq!(env.get("PATH"), std::env::var("PATH").ok().as_ref());
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
                "> xtask bench-startup".to_string(),
            ]
        );
    }
}
