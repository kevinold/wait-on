//! `.githooks/commit-msg` driven through git, so Windows runs it under git's own sh.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask lives one level below the repo root")
        .to_path_buf()
}

/// `git hook run` exists from git 2.36; without it there is nothing to drive. Probes with a
/// real hook name `.githooks/` lacks, since newer git rejects unknown hook names.
fn git_hook_run_unavailable() -> Option<String> {
    let out = Command::new("git")
        .args([
            "-c",
            "core.hooksPath=.githooks",
            "hook",
            "run",
            "--ignore-missing",
        ])
        .arg("pre-applypatch")
        .current_dir(repo_root())
        .output();
    match out {
        Ok(out) if out.status.success() => None,
        Ok(out) => Some(String::from_utf8_lossy(&out.stderr).into_owned()),
        Err(err) => Some(err.to_string()),
    }
}

fn run_hook(name: &str, message: &str) -> Output {
    let file = std::env::temp_dir().join(format!(
        "wait-on-commit-msg-{}-{name}.txt",
        std::process::id()
    ));
    std::fs::write(&file, message).expect("write message file");
    let out = Command::new("git")
        .args([
            "-c",
            "core.hooksPath=.githooks",
            "hook",
            "run",
            "commit-msg",
            "--",
        ])
        .arg(&file)
        .current_dir(repo_root())
        .output()
        .expect("spawn git");
    let _ = std::fs::remove_file(&file);
    out
}

fn long(prefix: &str, len: usize) -> String {
    format!("{prefix}{}", "x".repeat(len - prefix.len()))
}

/// (case name, message, rules that must be reported; empty = accepted).
fn cases() -> Vec<(&'static str, String, Vec<&'static str>)> {
    vec![
        // Real failures from #82 and the spike range.
        (
            "lane-id-subject",
            "docs(plans): L12 xtask + Justfile lane plan\n".into(),
            vec!["subject-case"],
        ),
        (
            "lane-id-subject-l6",
            "docs(plans): L6 command implementation plan (#58)\n".into(),
            vec!["subject-case"],
        ),
        (
            "header-113",
            "feat(rust): verified TLS roots, client identity, explicit proxy and unix/pipe \
             transport in the http checker (#57)\n"
                .into(),
            vec!["header-max-length"],
        ),
        (
            "pasted-log-line",
            format!(
                "fix(xtask): never rebuild the running xtask binary on Windows (#75)\n\n{}\n",
                long("error: failed to remove file `target\\debug\\xtask.exe` ", 140)
            ),
            vec!["body-max-line-length"],
        ),
        (
            "review-body-208",
            format!(
                "fix(review): apply review findings\n\n{}\n",
                long("Run cargo vet before cargo builds xtask ", 208)
            ),
            vec!["body-max-line-length"],
        ),
        (
            "hash-led-body-line",
            format!("fix(x): y\n\n{}\n", long("#82 ", 114)),
            vec!["body-max-line-length"],
        ),
        // -m keeps # lines, and CI takes the first line as the header.
        (
            "hash-led-header",
            "#123 thing\nfix: x\n".into(),
            vec!["type-empty"],
        ),
        // One row per rule.
        ("non-ascii-capital", "fix: Éclair\n".into(), vec!["subject-case"]),
        ("unknown-type", "improve: use parseArgs\n".into(), vec!["type-enum"]),
        ("bare-merged", "Merged stuff\n".into(), vec!["type-empty"]),
        ("upper-type", "Fix: thing\n".into(), vec!["type-case"]),
        ("empty-subject", "fix:\n".into(), vec!["subject-empty"]),
        ("blank-subject", "fix: \n".into(), vec!["subject-empty"]),
        ("full-stop", "fix: thing.\n".into(), vec!["subject-full-stop"]),
        ("trailing-space", "fix: thing \n".into(), vec!["header-trim"]),
        (
            "no-blank-after-header",
            "fix: thing\nbody right away\n".into(),
            vec!["body-leading-blank"],
        ),
        (
            "two-violations",
            "Fix: Thing.\n".into(),
            vec!["type-case", "subject-case", "subject-full-stop"],
        ),
        // Accepted.
        (
            "lane-merge",
            "chore(merge): merge spike-next-rs into chore-82-commit-msg-hook\n".into(),
            vec![],
        ),
        (
            "breaking-with-footer",
            "feat(scope)!: breaking change\n\nWhy it changed.\n\nBREAKING CHANGE: the old flag is gone\n"
                .into(),
            vec![],
        ),
        ("digit-subject", "fix: 2 retries\n".into(), vec![]),
        (
            "editor-comments-and-scissors",
            format!(
                "fix: thing\n\nbody\n# Please enter the commit message.\n\
                 # ------------------------ >8 ------------------------\n{}\n",
                long("diff --git ", 300)
            ),
            vec![],
        ),
        ("crlf", "fix: thing\r\n\r\nbody\r\n".into(), vec![]),
        ("merge-branch", "Merge branch 'x' into y\n".into(), vec![]),
        ("merge-pr", "Merge pull request #1 from a/b\n".into(), vec![]),
        ("merge-tag", "Merge tag 'v1'\n".into(), vec![]),
        ("revert", "Revert \"feat: x\"\n".into(), vec![]),
        ("reapply", "Reapply \"feat: x\"\n".into(), vec![]),
        ("fixup", "fixup! feat: x\n".into(), vec![]),
        ("automatic-merge", "Automatic merge from x\n".into(), vec![]),
    ]
}

#[test]
fn commit_msg_hook_reports_each_rule_and_accepts_conforming_messages() {
    if let Some(why) = git_hook_run_unavailable() {
        // CI runners have a current git, so a missing `git hook run` there is a broken runner.
        assert!(
            std::env::var_os("CI").is_none(),
            "git hook run unavailable on CI (needs git 2.36+): {why}"
        );
        eprintln!("skipping: git hook run unavailable (needs git 2.36+): {why}");
        return;
    }
    let mut failures = Vec::new();
    for (name, message, rules) in cases() {
        let out = run_hook(name, &message);
        let stderr = String::from_utf8_lossy(&out.stderr);
        if rules.is_empty() {
            if !out.status.success() || !stderr.trim().is_empty() {
                failures.push(format!(
                    "{name}: expected accept, got {:?}: {stderr}",
                    out.status
                ));
            }
        } else {
            if out.status.code() != Some(1) {
                failures.push(format!("{name}: expected exit 1, got {:?}", out.status));
            }
            for rule in rules {
                if !stderr.contains(rule) {
                    failures.push(format!("{name}: expected {rule} in: {stderr}"));
                }
            }
        }
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}
