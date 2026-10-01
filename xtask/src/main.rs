//! `cargo xtask <subcommand>`: repo tooling too large for a Justfile recipe.

#[allow(dead_code)] // callers land with build-napi (U2)
mod host;

use std::process::ExitCode;

const USAGE: &str = "usage: cargo xtask <build-napi|package|bench-startup> [args...]";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let code = match args.first().map(String::as_str) {
        Some(cmd @ ("build-napi" | "package" | "bench-startup")) => {
            eprintln!("{cmd}: not implemented yet");
            2
        }
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
