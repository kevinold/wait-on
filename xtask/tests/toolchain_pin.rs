//! The toolchain rust-toolchain.toml installs is the workspace MSRV.

use std::path::Path;

/// The quoted value of the first `key = "..."` line in a repo-root file.
fn value(file: &str, key: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask lives one level below the repo root")
        .join(file);
    let text = std::fs::read_to_string(&path).expect("read repo-root file");
    text.lines()
        .find_map(|line| {
            let (k, v) = line.split_once('=')?;
            (k.trim() == key).then(|| v.trim().trim_matches('"').to_string())
        })
        .unwrap_or_else(|| panic!("no {key} in {file}"))
}

#[test]
fn toolchain_channel_equals_the_workspace_rust_version() {
    let channel = value("rust-toolchain.toml", "channel");
    assert_eq!(channel.split('.').count(), 3, "{channel}");
    assert_eq!(channel, value("Cargo.toml", "rust-version"));
}
