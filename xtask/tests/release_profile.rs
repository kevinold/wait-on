//! The root Cargo.toml carries the size-tuned release profile and keeps panics unwinding, so a
//! Rust panic rejects the waitOn promise instead of aborting the consumer's process (R23).

use std::path::Path;

/// `key = value` pairs of the `[profile.release]` table, values unquoted; `None` when absent.
fn release_profile(cargo_toml: &str) -> Option<Vec<(String, String)>> {
    let mut lines = cargo_toml.lines().map(str::trim);
    lines.find(|l| *l == "[profile.release]")?;
    Some(
        lines
            .take_while(|l| !l.starts_with('['))
            .filter_map(|l| {
                let (k, v) = l.split_once('=')?;
                Some((k.trim().to_string(), v.trim().trim_matches('"').to_string()))
            })
            .collect(),
    )
}

/// Why the profile breaks R23, if it does.
fn problems(cargo_toml: &str) -> Vec<String> {
    let Some(profile) = release_profile(cargo_toml) else {
        return vec!["no [profile.release]".to_string()];
    };
    let get = |key: &str| {
        profile
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    };
    let mut out = Vec::new();
    if get("opt-level") != Some("z") {
        out.push("opt-level is not \"z\"".to_string());
    }
    if let Some(panic) = get("panic").filter(|p| *p != "unwind") {
        out.push(format!(
            "panic = \"{panic}\" would abort instead of rejecting"
        ));
    }
    out
}

fn root_cargo_toml() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask lives one level below the repo root")
        .join("Cargo.toml");
    std::fs::read_to_string(path).expect("read root Cargo.toml")
}

#[test]
fn committed_release_profile_is_size_tuned_and_unwinds() {
    assert_eq!(problems(&root_cargo_toml()), Vec::<String>::new());
}

#[test]
fn panic_abort_in_the_release_profile_is_rejected() {
    let toml = "[profile.release]\nopt-level = \"z\"\npanic = \"abort\"\n";
    assert_eq!(
        problems(toml),
        ["panic = \"abort\" would abort instead of rejecting"]
    );
}

#[test]
fn panic_abort_in_another_profile_is_not_the_release_profile() {
    let toml = "[profile.release]\nopt-level = \"z\"\n\n[profile.dev]\npanic = \"abort\"\n";
    assert!(problems(toml).is_empty());
}
