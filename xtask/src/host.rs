//! Process and host helpers shared by every subcommand.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The repository root: the parent of this crate's manifest dir.
pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask lives one level below the repo root")
        .to_path_buf()
}

/// The Node binary: npm's own node when run through npm, else `node` on PATH.
pub fn node_exe() -> PathBuf {
    std::env::var_os("npm_node_execpath").map_or_else(|| PathBuf::from("node"), PathBuf::from)
}

/// Run a child without a shell and return its exit code (a signal death is 1).
/// `env`, when given, is the child's whole environment.
pub fn run(cmd: &Path, args: &[String], cwd: &Path, env: Option<&HashMap<String, String>>) -> i32 {
    let mut child = Command::new(cmd);
    child.args(args).current_dir(cwd);
    if let Some(env) = env {
        child.env_clear().envs(env);
    }
    match child.status() {
        Ok(status) => status.code().unwrap_or(1),
        Err(err) => {
            eprintln!("failed to run {}: {err}", cmd.display());
            1
        }
    }
}

/// Rust's OS/arch names mapped to Node's `process.platform` / `process.arch`.
pub fn node_name(os: &str, arch: &str) -> Result<(&'static str, &'static str), String> {
    let platform = match os {
        "macos" => "darwin",
        "windows" => "win32",
        "linux" => "linux",
        _ => return Err(format!("unsupported host OS {os}")),
    };
    let arch = match arch {
        "x86_64" => "x64",
        "aarch64" => "arm64",
        _ => return Err(format!("unsupported host arch {arch}")),
    };
    Ok((platform, arch))
}

/// `lib/engine.js` `prebuildDir`: `<platform>-<arch>[-musl]`.
pub fn prebuild_dir(platform: &str, arch: &str, musl: bool) -> String {
    format!("{platform}-{arch}{}", if musl { "-musl" } else { "" })
}

/// This host's prebuild dir, as `lib/engine.js` names it.
#[allow(dead_code)] // ponytail: first caller is package --host-only (U4)
pub fn host_dir() -> Result<String, String> {
    let (platform, arch) = node_name(std::env::consts::OS, std::env::consts::ARCH)?;
    Ok(prebuild_dir(platform, arch, cfg!(target_env = "musl")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_dir_matches_node() {
        let js = "process.platform + '-' + process.arch + \
            (process.platform === 'linux' && !process.report.getReport().header.glibcVersionRuntime ? '-musl' : '')";
        let out = Command::new(node_exe()).args(["-p", js]).output().unwrap();
        assert!(out.status.success());
        assert_eq!(
            host_dir(),
            Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
        );
    }

    fn node_exit(code: i32) -> i32 {
        let args = vec!["-e".to_string(), format!("process.exit({code})")];
        run(&node_exe(), &args, &repo_root(), None)
    }

    #[test]
    fn run_returns_the_child_exit_code() {
        assert_eq!(node_exit(7), 7);
        assert_eq!(node_exit(0), 0);
    }

    #[test]
    fn run_passes_arguments_without_a_shell() {
        let out = Command::new(node_exe())
            .args(["-e", "process.stdout.write(process.argv[1])", "a && b > c"])
            .output()
            .unwrap();
        assert_eq!(String::from_utf8_lossy(&out.stdout), "a && b > c");
        let args = vec![
            "-e".to_string(),
            "process.exit(process.argv[1] === 'a && b > c' ? 0 : 1)".to_string(),
            "a && b > c".to_string(),
        ];
        assert_eq!(run(&node_exe(), &args, &repo_root(), None), 0);
    }

    #[test]
    fn run_sets_the_given_env() {
        let mut env: HashMap<String, String> = std::env::vars().collect();
        env.insert("XTASK_PROBE".to_string(), "yes".to_string());
        let args = vec![
            "-e".to_string(),
            "process.exit(process.env.XTASK_PROBE === 'yes' ? 0 : 1)".to_string(),
        ];
        assert_eq!(run(&node_exe(), &args, &repo_root(), Some(&env)), 0);
    }

    #[test]
    fn node_names_match_process_platform_and_arch() {
        assert_eq!(node_name("macos", "aarch64"), Ok(("darwin", "arm64")));
        assert_eq!(node_name("windows", "x86_64"), Ok(("win32", "x64")));
        assert_eq!(node_name("linux", "aarch64"), Ok(("linux", "arm64")));
        assert_eq!(node_name("linux", "x86_64"), Ok(("linux", "x64")));
        assert!(
            node_name("freebsd", "x86_64")
                .unwrap_err()
                .contains("freebsd")
        );
        assert!(
            node_name("linux", "riscv64")
                .unwrap_err()
                .contains("riscv64")
        );
    }

    #[test]
    fn prebuild_dir_appends_musl() {
        assert_eq!(prebuild_dir("linux", "x64", true), "linux-x64-musl");
        assert_eq!(prebuild_dir("darwin", "arm64", false), "darwin-arm64");
    }
}
