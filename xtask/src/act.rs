//! `cargo xtask act [--event pull_request|push] [--job J] [--workflow W] [-- act args...]`:
//! run a workflow's ubuntu jobs locally with nektos/act (pinned in mise.toml), a pinned
//! runner image and a fixed container architecture. Skips when no Docker engine is reachable.

use std::path::Path;

use crate::{host, linux, package};

#[derive(Debug, PartialEq)]
pub struct Options {
    pub event: String,
    pub job: Option<String>,
    pub workflow: String,
    pub extra: Vec<String>,
}

/// node + rustup on act's base image, without the 20 GB `full` flavour.
const RUNNER_IMAGE: &str = "catthehacker/ubuntu:rust-24.04";

pub fn parse_args(args: &[String]) -> Result<Options, String> {
    let mut opts = Options {
        event: "pull_request".into(),
        job: None,
        workflow: "node.js.yml".into(),
        extra: Vec::new(),
    };
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        let mut value = || it.next().cloned().ok_or(format!("{arg} needs a value"));
        match arg.as_str() {
            "--event" => opts.event = value()?,
            "--job" => opts.job = Some(value()?),
            "--workflow" => opts.workflow = value()?,
            "--" => {
                opts.extra = it.cloned().collect();
                break;
            }
            other => return Err(format!("unknown argument {other}")),
        }
    }
    if !["pull_request", "push"].contains(&opts.event.as_str()) {
        return Err(format!(
            "--event {}: expected pull_request or push",
            opts.event
        ));
    }
    Ok(opts)
}

/// KTD4: both ubuntu labels the workflows use map to one image; act skips every row whose
/// `runs-on` is unmapped (windows, macos, arm). The artifact server is always on because the
/// PR `napi` row uploads its prebuild.
pub fn act_args(opts: &Options, artifacts: &Path) -> Vec<String> {
    let mut args = vec![
        opts.event.clone(),
        "-W".into(),
        format!(".github/workflows/{}", opts.workflow),
        "-P".into(),
        format!("ubuntu-latest={RUNNER_IMAGE}"),
        "-P".into(),
        format!("ubuntu-24.04={RUNNER_IMAGE}"),
        "--container-architecture".into(),
        "linux/amd64".into(),
        "--artifact-server-path".into(),
        artifacts.to_string_lossy().to_string(),
        // act 0.2.89 maps every row of a `runs-on: ${{ matrix.os }}` job to one platform
        // (a windows row ran on the ubuntu image), so keep only the ubuntu rows.
        "--matrix".into(),
        "os:ubuntu-latest".into(),
        "--matrix".into(),
        "os:ubuntu-24.04".into(),
    ];
    if let Some(job) = &opts.job {
        args.extend(["-j".into(), job.clone()]);
    }
    args.extend_from_slice(&opts.extra);
    args
}

/// act copies the checkout into its container; a worktree's `.git` file points at a host
/// path that copy lacks, so git-reading steps (the commit-msg hook test) fail there.
pub fn worktree_note(git_is_file: bool, extra: &[String]) -> Option<String> {
    (git_is_file && !extra.iter().any(|a| a == "-C" || a == "--directory")).then(|| {
        "note: this checkout is a git worktree; act's copy has no git dir, so git-reading \
         steps fail. Run from a clone, or pass -- -C <clone>."
            .into()
    })
}

pub fn run(args: &[String]) -> i32 {
    host::exit_code(act(args))
}

fn act(args: &[String]) -> Result<i32, String> {
    let opts = parse_args(args)?;
    if let Some(reason) = linux::skip_reason(linux::docker_reachable()) {
        eprintln!("{reason}");
        return Ok(0);
    }
    if let Some(note) = worktree_note(host::repo_root().join(".git").is_file(), &opts.extra) {
        eprintln!("{note}");
    }
    let artifacts = package::fresh_temp_dir("act-artifacts")?;
    let status = std::process::Command::new("act")
        .args(act_args(&opts, &artifacts))
        .current_dir(host::repo_root())
        .status()
        .map_err(|e| format!("act: {e} (install it with `mise install`; mise.toml pins it)"))?;
    Ok(status.code().unwrap_or(1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ci::strings;

    fn opts(event: &str, job: Option<&str>, workflow: &str, extra: &[&str]) -> Options {
        Options {
            event: event.into(),
            job: job.map(String::from),
            workflow: workflow.into(),
            extra: strings(extra),
        }
    }

    #[test]
    fn defaults_to_the_ci_workflow_on_pull_request() {
        assert_eq!(
            parse_args(&[]).unwrap(),
            opts("pull_request", None, "node.js.yml", &[])
        );
    }

    #[test]
    fn parses_event_job_workflow_and_passthrough() {
        let args = strings(&[
            "--event",
            "push",
            "--job",
            "rust",
            "--workflow",
            "commitlint.yml",
            "--",
            "-C",
            "/old",
        ]);
        assert_eq!(
            parse_args(&args).unwrap(),
            opts("push", Some("rust"), "commitlint.yml", &["-C", "/old"])
        );
    }

    #[test]
    fn rejects_other_events_unknown_flags_and_missing_values() {
        let err = parse_args(&strings(&["--event", "workflow_dispatch"])).unwrap_err();
        assert!(err.contains("pull_request or push"), "{err}");
        let err = parse_args(&strings(&["--bogus"])).unwrap_err();
        assert!(err.contains("--bogus"), "{err}");
        let err = parse_args(&strings(&["--job"])).unwrap_err();
        assert!(err.contains("--job needs a value"), "{err}");
    }

    #[test]
    fn act_args_pin_the_runner_image_arch_and_artifact_server() {
        let image = "catthehacker/ubuntu:rust-24.04";
        assert_eq!(
            act_args(
                &opts("pull_request", Some("rust"), "node.js.yml", &["-C", "/old"]),
                Path::new("/tmp/artifacts")
            ),
            strings(&[
                "pull_request",
                "-W",
                ".github/workflows/node.js.yml",
                "-P",
                &format!("ubuntu-latest={image}"),
                "-P",
                &format!("ubuntu-24.04={image}"),
                "--container-architecture",
                "linux/amd64",
                "--artifact-server-path",
                "/tmp/artifacts",
                "--matrix",
                "os:ubuntu-latest",
                "--matrix",
                "os:ubuntu-24.04",
                "-j",
                "rust",
                "-C",
                "/old",
            ])
        );
    }

    #[test]
    fn worktree_note_only_for_a_worktree_without_another_directory() {
        let note = worktree_note(true, &[]).expect("a note");
        assert!(note.contains("git worktree"), "{note}");
        assert!(note.contains("-- -C <clone>"), "{note}");
        assert_eq!(worktree_note(false, &[]), None);
        assert_eq!(worktree_note(true, &strings(&["-C", "/clone"])), None);
    }

    #[test]
    fn act_args_without_a_job_run_every_mapped_job() {
        let args = act_args(&opts("push", None, "node.js.yml", &[]), Path::new("/a"));
        assert_eq!(args[0], "push");
        assert!(!args.iter().any(|a| a == "-j"), "{args:?}");
    }
}
