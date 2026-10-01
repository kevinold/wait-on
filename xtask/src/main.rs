//! `cargo xtask <subcommand>`: the single front door for Rust tasks.

mod bench;
mod build_napi;
mod ci;
mod host;
mod package;

use std::process::ExitCode;

type Run = fn(&[String]) -> i32;

/// Every subcommand: name, one-line description, entry point. Dispatch and usage both read it.
const COMMANDS: &[(&str, &str, Run)] = &[
    (
        "ci",
        "the full Rust gate: vet, fmt, lint, test, deny, build-napi, mocha (rust-strict), bench-startup",
        ci::run,
    ),
    ("fmt", "cargo fmt --all --check", ci::fmt),
    (
        "lint",
        "cargo clippy --workspace --all-targets -- -D warnings",
        ci::lint,
    ),
    ("test", "cargo test --workspace", ci::test),
    (
        "cov",
        "cargo llvm-cov --workspace [args...] (needs cargo-llvm-cov)",
        ci::cov,
    ),
    (
        "build-napi",
        "build the napi addon into prebuilds/ [--target <triple>] [napi args...]",
        build_napi::run,
    ),
    (
        "package",
        "pack and install-test the npm package [--host-only] (run through npm)",
        package::run,
    ),
    (
        "bench-startup",
        "startup overhead of the rust engine vs js [--runs N] [--record]",
        bench::run,
    ),
];

fn usage() -> String {
    let mut text = String::from("usage: cargo xtask <subcommand> [args...]\n\nsubcommands:\n");
    for (name, desc, _) in COMMANDS {
        text.push_str(&format!("  {name:<14} {desc}\n"));
    }
    text
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let code = match args.first().map(String::as_str) {
        Some("--help" | "-h" | "help") => {
            print!("{}", usage());
            0
        }
        Some(name) => match COMMANDS.iter().find(|(n, _, _)| *n == name) {
            Some((_, _, run)) => run(&args[1..]),
            None => {
                eprint!("unknown subcommand {name}\n{}", usage());
                2
            }
        },
        None => {
            eprint!("{}", usage());
            2
        }
    };
    ExitCode::from(u8::try_from(code).unwrap_or(1))
}
