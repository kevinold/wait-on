//! `cargo xtask <subcommand>`: repo tooling too large for a Justfile recipe.

mod bench;
mod build_napi;
mod host;
mod package;

use std::process::ExitCode;

const USAGE: &str = "usage: cargo xtask <build-napi|package|bench-startup> [args...]";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let code = match args.first().map(String::as_str) {
        Some("build-napi") => build_napi::run(&args[1..]),
        Some("bench-startup") => bench::run(&args[1..]),
        Some("package") => package::run(&args[1..]),
        Some(other) => {
            eprintln!("unknown subcommand {other}\n{USAGE}");
            2
        }
        None => {
            eprintln!("{USAGE}");
            2
        }
    };
    ExitCode::from(u8::try_from(code).unwrap_or(1))
}
