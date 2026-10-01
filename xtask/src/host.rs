//! Process and host helpers shared by every subcommand.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::Duration;

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

/// This process's environment as a map (lossy, so a non-UTF-8 value cannot panic).
pub fn env_map() -> HashMap<String, String> {
    std::env::vars_os()
        .map(|(k, v)| (k.to_string_lossy().into(), v.to_string_lossy().into()))
        .collect()
}

/// Print a subcommand's error and turn it into exit code 1.
pub fn exit_code(result: Result<i32, String>) -> i32 {
    result.unwrap_or_else(|err| {
        eprintln!("{err}");
        1
    })
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
pub fn host_dir() -> Result<String, String> {
    let (platform, arch) = node_name(std::env::consts::OS, std::env::consts::ARCH)?;
    Ok(prebuild_dir(platform, arch, cfg!(target_env = "musl")))
}

/// Run `cmd` to completion with captured stderr (stdout discarded), killing it once
/// `limit` passes. `Ok(None)` means it was killed at the deadline.
pub fn output_within(cmd: &mut Command, limit: Duration) -> std::io::Result<Option<Output>> {
    use std::io::Read;
    let mut child = cmd
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()?;
    let mut pipe = child.stderr.take().expect("stderr is piped");
    // read on a thread so a chatty child can never block on a full pipe
    let reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        pipe.read_to_end(&mut buf).map(|_| buf)
    });
    let deadline = std::time::Instant::now() + limit;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break Some(status);
        }
        if std::time::Instant::now() >= deadline {
            child.kill()?;
            child.wait()?;
            break None;
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let stderr = reader.join().expect("stderr reader")?;
    Ok(status.map(|status| Output {
        status,
        stdout: Vec::new(),
        stderr,
    }))
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
        let mut env = env_map();
        env.insert("XTASK_PROBE".to_string(), "yes".to_string());
        let args = vec![
            "-e".to_string(),
            "process.exit(process.env.XTASK_PROBE === 'yes' ? 0 : 1)".to_string(),
        ];
        assert_eq!(run(&node_exe(), &args, &repo_root(), Some(&env)), 0);
    }

    #[test]
    fn output_within_returns_status_and_stderr_before_the_deadline() {
        let mut cmd = Command::new(node_exe());
        cmd.args(["-e", "process.stderr.write('boom'); process.exit(3)"]);
        let out = output_within(&mut cmd, Duration::from_secs(20))
            .unwrap()
            .unwrap();
        assert_eq!(out.status.code(), Some(3));
        assert_eq!(String::from_utf8_lossy(&out.stderr), "boom");
    }

    #[test]
    fn output_within_kills_a_child_past_the_deadline() {
        let mut cmd = Command::new(node_exe());
        cmd.args(["-e", "setTimeout(() => {}, 60000)"]);
        let start = std::time::Instant::now();
        let out = output_within(&mut cmd, Duration::from_millis(500)).unwrap();
        assert!(out.is_none(), "a hung child must be reported as timed out");
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "{:?}",
            start.elapsed()
        );
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
