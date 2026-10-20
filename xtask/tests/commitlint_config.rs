//! `commitlint.config.js` ignores, evaluated by node exactly as commitlint calls them.

use std::path::Path;
use std::process::Command;

/// Whether any `ignores` predicate in the config skips `message`.
fn ignored(message: &str) -> bool {
    let config = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask lives one level below the repo root")
        .join("commitlint.config.js");
    let out = Command::new("node")
        .args([
            "-e",
            "const { ignores } = require(process.argv[1]);\
             process.stdout.write(String(ignores.some((f) => f(process.argv[2]))));",
        ])
        .arg(&config)
        .arg(message)
        .output()
        .expect("spawn node");
    assert!(out.status.success(), "{out:?}");
    String::from_utf8_lossy(&out.stdout) == "true"
}

const PUSHED_SPIKE_HEADER: &str = "docs(plans): L12 xtask + Justfile lane plan";

#[test]
fn pushed_spike_message_is_ignored_with_trailing_whitespace() {
    assert!(ignored(PUSHED_SPIKE_HEADER));
    assert!(ignored(&format!("{PUSHED_SPIKE_HEADER}\n")));
}

#[test]
fn leading_whitespace_copy_of_a_pushed_spike_message_is_still_linted() {
    assert!(!ignored(&format!(" {PUSHED_SPIKE_HEADER}")));
    assert!(!ignored(&format!("\t{PUSHED_SPIKE_HEADER}\n")));
}
