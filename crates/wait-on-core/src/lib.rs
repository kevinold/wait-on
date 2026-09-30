//! Pure Rust wait-on engine. No napi here; `wait-on-napi` binds it for Node.

use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

/// Crate version, surfaced to Node as the addon's `version()`.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Size of the file at `path` via a symlink-following stat, or -1 on any error, like
/// `getFileSize` in lib/wait-on.js. Reverse mode and Windows delete-pending rely on -1.
pub fn file_size(path: &str) -> i64 {
    std::fs::metadata(path).map_or(-1, |m| m.len() as i64)
}

/// One `command:` attempt: `ok` is exit 0, `stdout` feeds the verbose success line and
/// `error` the failure line, like `commandPasses` in lib/wait-on.js.
#[derive(Debug, Default)]
pub struct CommandResult {
    pub ok: bool,
    pub stdout: String,
    pub error: String,
}

/// Run `command` through the shell Node's `child_process.exec` uses, killing the attempt
/// at `timeout_ms` (0 = no limit). Never panics; every failure is `ok: false`.
pub fn run_command(command: &str, timeout_ms: u32) -> CommandResult {
    run(shell(command), command, timeout_ms)
}

#[cfg(unix)]
fn shell(command: &str) -> Command {
    let mut cmd = Command::new("/bin/sh");
    cmd.arg("-c").arg(command);
    cmd
}

// Node's exec: ComSpec (default cmd.exe) with `/d /s /c "<command>"` passed verbatim.
#[cfg(windows)]
fn shell(command: &str) -> Command {
    use std::os::windows::process::CommandExt;
    let comspec = std::env::var_os("ComSpec").unwrap_or_else(|| "cmd.exe".into());
    let mut cmd = Command::new(comspec);
    cmd.args(["/d", "/s", "/c"])
        .raw_arg(format!("\"{command}\""));
    cmd
}

// ponytail: keep 1 MiB per stream and drain the rest (Node's maxBuffer would kill instead).
const MAX_CAPTURE: usize = 1024 * 1024;
// ponytail: 10 ms try_wait polling; the timeout fires up to one tick late.
const POLL: Duration = Duration::from_millis(10);

type Captured = Arc<Mutex<Vec<u8>>>;

fn capture(pipe: Option<impl Read + Send + 'static>) -> (Captured, JoinHandle<()>) {
    let buf = Captured::default();
    let sink = Arc::clone(&buf);
    let reader = thread::spawn(move || {
        let Some(mut pipe) = pipe else { return };
        let mut chunk = [0u8; 8192];
        while let Ok(n @ 1..) = pipe.read(&mut chunk) {
            let mut kept = sink.lock().unwrap();
            let room = MAX_CAPTURE.saturating_sub(kept.len());
            kept.extend_from_slice(&chunk[..n.min(room)]);
        }
    });
    (buf, reader)
}

fn text(buf: &Captured) -> String {
    String::from_utf8_lossy(&buf.lock().unwrap()).into_owned()
}

fn run(mut shell: Command, command: &str, timeout_ms: u32) -> CommandResult {
    let failed = |detail: &str| CommandResult {
        ok: false,
        stdout: String::new(),
        error: format!("Command failed: {command}\n{detail}"),
    };
    let spawned = shell
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();
    let mut child = match spawned {
        Ok(child) => child,
        Err(e) => return failed(&e.to_string()),
    };
    let (out, out_reader) = capture(child.stdout.take());
    let (err, err_reader) = capture(child.stderr.take());
    let deadline =
        (timeout_ms > 0).then(|| Instant::now() + Duration::from_millis(timeout_ms.into()));
    let expired = || deadline.is_some_and(|d| Instant::now() >= d);

    // Readers are never joined past the deadline: a grandchild may hold a pipe open.
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if !expired() => thread::sleep(POLL),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return failed(&format!("killed after {timeout_ms}ms"));
            }
        }
    };
    // Like Node's `close`, wait for the pipes, but only up to the same deadline.
    while !(out_reader.is_finished() && err_reader.is_finished()) && !expired() {
        thread::sleep(POLL);
    }
    if status.success() {
        CommandResult {
            ok: true,
            stdout: text(&out),
            error: String::new(),
        }
    } else {
        failed(&text(&err))
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    #[test]
    fn version_is_workspace_version() {
        assert_eq!(super::version(), "0.1.0");
    }

    fn temp(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("wait-on-core-{}-{name}", std::process::id()))
    }

    #[test]
    fn file_size_is_the_byte_length_of_an_existing_file() {
        let p = temp("exists");
        fs::write(&p, "12345").unwrap();
        let size = super::file_size(p.to_str().unwrap());
        fs::remove_file(&p).unwrap();
        assert_eq!(size, 5);
    }

    #[test]
    fn file_size_is_minus_one_for_a_missing_path() {
        assert_eq!(super::file_size(temp("missing").to_str().unwrap()), -1);
    }

    #[test]
    fn file_size_is_minus_one_for_a_path_under_a_regular_file() {
        let p = temp("notdir");
        fs::write(&p, "x").unwrap();
        let size = super::file_size(p.join("child").to_str().unwrap());
        fs::remove_file(&p).unwrap();
        assert_eq!(size, -1);
    }

    #[test]
    fn file_size_reports_a_directory() {
        assert!(super::file_size(std::env::temp_dir().to_str().unwrap()) >= 0);
    }

    #[cfg(unix)]
    #[test]
    fn file_size_is_minus_one_for_a_dangling_symlink() {
        let p = temp("dangling");
        std::os::unix::fs::symlink(temp("nowhere"), &p).unwrap();
        let size = super::file_size(p.to_str().unwrap());
        fs::remove_file(&p).unwrap();
        assert_eq!(size, -1);
    }

    #[cfg(unix)]
    #[test]
    fn file_size_is_minus_one_when_stat_is_denied() {
        use std::os::unix::fs::PermissionsExt;
        let dir = temp("locked");
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join("f");
        fs::write(&file, "x").unwrap();
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o000)).unwrap();
        let denied = fs::metadata(&file).is_err(); // root can still stat: skip then
        let size = super::file_size(file.to_str().unwrap());
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o755)).unwrap();
        fs::remove_dir_all(&dir).unwrap();
        if denied {
            assert_eq!(size, -1);
        }
    }

    // run_command: per-OS shell builtins so cargo test needs no node.
    use super::run_command;
    use std::time::{Duration, Instant};

    #[cfg(unix)]
    const FAIL3: &str = "exit 3";
    #[cfg(windows)]
    const FAIL3: &str = "exit /b 3";
    #[cfg(unix)]
    const STDERR_FAIL: &str = "echo boom 1>&2; exit 2";
    #[cfg(windows)]
    const STDERR_FAIL: &str = "echo boom 1>&2 && exit /b 2";
    #[cfg(unix)]
    const CHAINED: &str = r#"echo "quoted" && exit 0"#;
    #[cfg(windows)]
    const CHAINED: &str = r#"echo "quoted" && exit /b 0"#;
    #[cfg(unix)]
    const ENV_CHECK: &str = r#"[ "$CARGO_PKG_NAME" = wait-on-core ]"#;
    #[cfg(windows)]
    const ENV_CHECK: &str = r#"if "%CARGO_PKG_NAME%"=="wait-on-core" (exit /b 0) else (exit /b 1)"#;
    #[cfg(unix)]
    const SLEEP5: &str = "sleep 5";
    #[cfg(windows)]
    const SLEEP5: &str = "ping -n 6 127.0.0.1 >nul";
    #[cfg(unix)]
    const SLEEP1_ECHO: &str = "sleep 1; echo done";
    #[cfg(windows)]
    const SLEEP1_ECHO: &str = "ping -n 2 127.0.0.1 >nul && echo done";

    #[test]
    fn run_command_is_ready_with_stdout_when_the_command_exits_0() {
        let r = run_command("echo hello", 0);
        assert!(r.ok, "{r:?}");
        assert_eq!(r.stdout.trim(), "hello");
    }

    #[test]
    fn run_command_is_not_ready_naming_the_command_on_a_non_zero_exit() {
        let r = run_command(FAIL3, 0);
        assert!(!r.ok);
        assert!(r.error.starts_with("Command failed: "), "{r:?}");
        assert!(r.error.contains(FAIL3), "{r:?}");
    }

    #[test]
    fn run_command_puts_stderr_in_the_error_on_failure() {
        let r = run_command(STDERR_FAIL, 0);
        assert!(!r.ok);
        assert!(r.error.contains("boom"), "{r:?}");
    }

    #[test]
    fn run_command_runs_quoted_and_chained_commands_through_the_shell() {
        let r = run_command(CHAINED, 0);
        assert!(r.ok, "{r:?}");
        assert!(r.stdout.contains("quoted"), "{r:?}");
    }

    #[test]
    fn run_command_inherits_the_parent_environment() {
        let r = run_command(ENV_CHECK, 0);
        assert!(r.ok, "{r:?}");
    }

    #[test]
    fn run_command_is_not_ready_when_the_shell_cannot_find_the_command() {
        let r = run_command("wait-on-no-such-command-xyz", 0);
        assert!(!r.ok);
        assert!(r.error.starts_with("Command failed: "), "{r:?}");
    }

    #[test]
    fn run_is_not_ready_when_the_shell_itself_cannot_spawn() {
        let cmd = std::process::Command::new(temp("no-such-shell"));
        let r = super::run(cmd, "anything", 0);
        assert!(!r.ok);
        assert!(r.error.starts_with("Command failed: anything\n"), "{r:?}");
    }

    #[cfg(unix)]
    #[test]
    fn run_command_is_not_ready_on_signal_death() {
        let r = run_command("kill -9 $$", 0);
        assert!(!r.ok, "{r:?}");
    }

    #[test]
    fn run_command_kills_an_attempt_at_the_timeout() {
        let start = Instant::now();
        let r = run_command(SLEEP5, 200);
        assert!(
            start.elapsed() < Duration::from_secs(1),
            "{:?}",
            start.elapsed()
        );
        assert!(!r.ok);
        assert!(r.error.contains("killed after 200ms"), "{r:?}");
    }

    #[test]
    fn run_command_waits_for_a_slow_command_without_a_timeout() {
        let r = run_command(SLEEP1_ECHO, 0);
        assert!(r.ok, "{r:?}");
        assert_eq!(r.stdout.trim(), "done");
    }

    #[cfg(unix)]
    #[test]
    fn run_command_returns_at_the_timeout_when_a_grandchild_holds_stdout() {
        let start = Instant::now();
        let r = run_command("sleep 3 & echo hi", 300);
        assert!(
            start.elapsed() < Duration::from_secs(1),
            "{:?}",
            start.elapsed()
        );
        assert!(r.ok, "{r:?}");
        assert_eq!(r.stdout.trim(), "hi");
    }

    #[cfg(unix)]
    #[test]
    fn run_command_caps_captured_stdout_at_1_mib() {
        let r = run_command("head -c 2000000 /dev/zero", 0);
        assert!(r.ok, "{r:?}");
        assert_eq!(r.stdout.len(), 1024 * 1024);
    }
}
