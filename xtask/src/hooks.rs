//! `cargo xtask hooks`: point git at the tracked `.githooks/` (commit-msg) for this repository.

use std::path::Path;

use crate::host;

pub fn run(_args: &[String]) -> i32 {
    // The invoking directory, so it targets whichever clone or worktree the developer is in;
    // the relative path resolves per worktree at hook time.
    let args = ["config", "core.hooksPath", ".githooks"].map(String::from);
    let code = host::run(Path::new("git"), &args, Path::new("."), None);
    if code == 0 {
        println!("core.hooksPath set to .githooks (commit messages are now checked locally)");
    }
    code
}
