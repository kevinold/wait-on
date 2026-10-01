//! `cargo xtask build-napi [--target <triple>] [napi build args, e.g. -x]`
//! Builds crates/wait-on-napi with @napi-rs/cli into target/napi/<dir>/ (never the repo
//! root) and copies the addon to prebuilds/<platform>-<arch>[-musl]/wait-on.node, where
//! lib/engine.js loads it. Without --target it builds for the rustc host.

use std::path::{Path, PathBuf};

use crate::host;

/// (triple, platform, arch, musl) for the eight PO4 targets.
pub const TARGETS: [(&str, &str, &str, bool); 8] = [
    ("aarch64-apple-darwin", "darwin", "arm64", false),
    ("x86_64-apple-darwin", "darwin", "x64", false),
    ("x86_64-unknown-linux-gnu", "linux", "x64", false),
    ("aarch64-unknown-linux-gnu", "linux", "arm64", false),
    ("x86_64-unknown-linux-musl", "linux", "x64", true),
    ("aarch64-unknown-linux-musl", "linux", "arm64", true),
    ("x86_64-pc-windows-msvc", "win32", "x64", false),
    ("aarch64-pc-windows-msvc", "win32", "arm64", false),
];

/// Split `--target <t>` out of the args; everything else goes to `napi build`.
pub fn parse_args(args: &[String]) -> Result<(Option<String>, Vec<String>), String> {
    let Some(i) = args.iter().position(|a| a == "--target") else {
        return Ok((None, args.to_vec()));
    };
    let target = args.get(i + 1).ok_or("--target needs a triple")?;
    let mut extra = args[..i].to_vec();
    extra.extend_from_slice(&args[i + 2..]);
    Ok((Some(target.clone()), extra))
}

/// The `host:` line of `rustc -vV`.
pub fn host_triple(rustc_vv: &str) -> Option<&str> {
    rustc_vv
        .lines()
        .find_map(|l| l.strip_prefix("host:"))
        .map(str::trim)
}

/// The one `.node` file napi wrote into `dir`.
pub fn single_node(dir: &Path) -> Result<PathBuf, String> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut built: Vec<String> = entries
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .filter(|f| f.ends_with(".node"))
        .collect();
    built.sort();
    match built.as_slice() {
        [one] => Ok(dir.join(one)),
        _ => Err(format!(
            "expected one .node file in {}, found: {}",
            dir.display(),
            if built.is_empty() {
                "none".to_string()
            } else {
                built.join(", ")
            }
        )),
    }
}

#[derive(Debug, PartialEq)]
pub struct Plan {
    pub output_dir: PathBuf,
    pub prebuild_path: PathBuf,
    pub napi_args: Vec<String>,
}

pub fn plan(target: &str, extra: &[String], root: &Path) -> Result<Plan, String> {
    let Some(&(_, platform, arch, musl)) = TARGETS.iter().find(|t| t.0 == target) else {
        let names: Vec<&str> = TARGETS.iter().map(|t| t.0).collect();
        return Err(format!(
            "unsupported target {target}; expected one of {}",
            names.join(", ")
        ));
    };
    let dir = host::prebuild_dir(platform, arch, musl);
    let output_dir = root.join("target").join("napi").join(&dir);
    let manifest = root.join("crates").join("wait-on-napi").join("Cargo.toml");
    let mut napi_args: Vec<String> = [
        "build",
        "--release",
        "--manifest-path",
        &manifest.display().to_string(),
        "--output-dir",
        &output_dir.display().to_string(),
        "--target",
        target,
    ]
    .map(String::from)
    .to_vec();
    napi_args.extend_from_slice(extra);
    Ok(Plan {
        prebuild_path: root.join("prebuilds").join(&dir).join("wait-on.node"),
        output_dir,
        napi_args,
    })
}

/// The subcommand: returns the process exit code.
pub fn run(args: &[String]) -> i32 {
    match build(args) {
        Ok(code) => code,
        Err(err) => {
            eprintln!("{err}");
            1
        }
    }
}

fn build(args: &[String]) -> Result<i32, String> {
    let root = host::repo_root();
    let (target, extra) = parse_args(args)?;
    let target = match target {
        Some(t) => t,
        None => {
            let out = std::process::Command::new("rustc")
                .arg("-vV")
                .output()
                .map_err(|e| format!("failed to run rustc -vV: {e}"))?;
            let vv = String::from_utf8_lossy(&out.stdout);
            host_triple(&vv)
                .ok_or("no host: line in rustc -vV output")?
                .to_string()
        }
    };
    let plan = plan(&target, &extra, &root)?;

    let cli_dir = root.join("node_modules").join("@napi-rs").join("cli");
    let pkg = std::fs::read_to_string(cli_dir.join("package.json"))
        .map_err(|e| format!("@napi-rs/cli not installed (run npm ci): {e}"))?;
    let pkg: serde_json::Value = serde_json::from_str(&pkg).map_err(|e| e.to_string())?;
    let bin = pkg["bin"]["napi"]
        .as_str()
        .ok_or("@napi-rs/cli package.json has no bin.napi")?;

    let mut node_args = vec![cli_dir.join(bin).display().to_string()];
    node_args.extend(plan.napi_args);
    let code = host::run(&host::node_exe(), &node_args, &root, None);
    if code != 0 {
        return Ok(code);
    }

    let built = single_node(&plan.output_dir)?;
    let dest_dir = plan
        .prebuild_path
        .parent()
        .expect("prebuild path has a dir");
    std::fs::create_dir_all(dest_dir).map_err(|e| format!("{}: {e}", dest_dir.display()))?;
    std::fs::copy(&built, &plan.prebuild_path)
        .map_err(|e| format!("copy to {}: {e}", plan.prebuild_path.display()))?;
    let rel = plan
        .prebuild_path
        .strip_prefix(&root)
        .unwrap_or(&plan.prebuild_path);
    println!("built {}", rel.display());
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root() -> PathBuf {
        PathBuf::from("repo")
    }

    fn strings(args: &[&str]) -> Vec<String> {
        args.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn parse_args_strips_the_target_and_keeps_the_rest() {
        assert_eq!(
            parse_args(&strings(&["--target", "x86_64-unknown-linux-musl", "-x"])),
            Ok((
                Some("x86_64-unknown-linux-musl".to_string()),
                strings(&["-x"])
            ))
        );
        assert_eq!(
            parse_args(&strings(&[
                "-x",
                "--target",
                "aarch64-apple-darwin",
                "--verbose"
            ])),
            Ok((
                Some("aarch64-apple-darwin".to_string()),
                strings(&["-x", "--verbose"])
            ))
        );
        assert_eq!(parse_args(&[]), Ok((None, vec![])));
        assert_eq!(parse_args(&strings(&["-x"])), Ok((None, strings(&["-x"]))));
        assert!(
            parse_args(&strings(&["--target"]))
                .unwrap_err()
                .contains("--target")
        );
    }

    fn temp_dir(cell: &str, files: &[&str]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("wait-on-{cell}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for f in files {
            std::fs::write(dir.join(f), "").unwrap();
        }
        dir
    }

    #[test]
    fn single_node_returns_the_one_addon() {
        let dir = temp_dir("napi-one", &["wait-on.darwin-arm64.node", "index.d.ts"]);
        assert_eq!(single_node(&dir), Ok(dir.join("wait-on.darwin-arm64.node")));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn single_node_rejects_an_empty_output_dir() {
        let dir = temp_dir("napi-none", &["index.d.ts"]);
        let err = single_node(&dir).unwrap_err();
        assert!(err.contains(&dir.display().to_string()), "{err}");
        assert!(err.contains("none"), "{err}");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn single_node_rejects_two_addons() {
        let dir = temp_dir("napi-two", &["a.node", "b.node"]);
        let err = single_node(&dir).unwrap_err();
        assert!(err.contains("a.node") && err.contains("b.node"), "{err}");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// T-L12-2: the Rust dir naming cannot drift from `lib/engine.js` `prebuildDir`.
    #[test]
    fn prebuild_dirs_match_lib_engine() {
        let targets: Vec<String> = TARGETS
            .iter()
            .map(|(_, p, a, m)| format!("{{platform:'{p}',arch:'{a}',musl:{m}}}"))
            .collect();
        let js = format!(
            "const {{ prebuildDir }} = require('./lib/engine');\
             console.log([{}].map(prebuildDir).join('\\n'))",
            targets.join(",")
        );
        let out = std::process::Command::new(host::node_exe())
            .args(["-e", &js])
            .current_dir(host::repo_root())
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let js_dirs: Vec<String> = String::from_utf8_lossy(&out.stdout)
            .lines()
            .map(String::from)
            .collect();
        let rust_dirs: Vec<String> = TARGETS
            .iter()
            .map(|(_, p, a, m)| host::prebuild_dir(p, a, *m))
            .collect();
        assert_eq!(rust_dirs, js_dirs);
    }

    #[test]
    fn host_triple_reads_the_rustc_host_line() {
        let vv = "rustc 1.98.1 (abc 2026-09-01)\nbinary: rustc\nhost: aarch64-apple-darwin\nrelease: 1.98.1\n";
        assert_eq!(host_triple(vv), Some("aarch64-apple-darwin"));
    }

    #[test]
    fn each_target_plans_its_prebuild_dir() {
        let expected = [
            ("aarch64-apple-darwin", "darwin-arm64"),
            ("x86_64-apple-darwin", "darwin-x64"),
            ("x86_64-unknown-linux-gnu", "linux-x64"),
            ("aarch64-unknown-linux-gnu", "linux-arm64"),
            ("x86_64-unknown-linux-musl", "linux-x64-musl"),
            ("aarch64-unknown-linux-musl", "linux-arm64-musl"),
            ("x86_64-pc-windows-msvc", "win32-x64"),
            ("aarch64-pc-windows-msvc", "win32-arm64"),
        ];
        assert_eq!(TARGETS.len(), expected.len());
        for (triple, dir) in expected {
            let p = plan(triple, &[], &root()).unwrap();
            assert_eq!(
                p.prebuild_path,
                root().join("prebuilds").join(dir).join("wait-on.node"),
                "{triple}"
            );
            assert_eq!(p.output_dir, root().join("target").join("napi").join(dir));
        }
    }

    #[test]
    fn unknown_target_names_the_input_and_the_supported_list() {
        let err = plan("riscv64gc-unknown-linux-gnu", &[], &root()).unwrap_err();
        assert!(err.contains("riscv64gc-unknown-linux-gnu"), "{err}");
        assert!(err.contains("x86_64-unknown-linux-gnu"), "{err}");
    }

    #[test]
    fn plan_builds_the_exact_napi_args() {
        let p = plan("x86_64-unknown-linux-gnu", &[], &root()).unwrap();
        let out = root().join("target").join("napi").join("linux-x64");
        let manifest = root()
            .join("crates")
            .join("wait-on-napi")
            .join("Cargo.toml");
        assert_eq!(
            p.napi_args,
            [
                "build",
                "--release",
                "--manifest-path",
                &manifest.display().to_string(),
                "--output-dir",
                &out.display().to_string(),
                "--target",
                "x86_64-unknown-linux-gnu",
            ]
        );
    }

    #[test]
    fn plan_forwards_extra_args_last() {
        let p = plan("x86_64-unknown-linux-musl", &["-x".to_string()], &root()).unwrap();
        assert_eq!(p.napi_args.last().map(String::as_str), Some("-x"));
    }
}
