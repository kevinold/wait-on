//! `cargo xtask linux <command> [args...]`: run a command in a pinned Linux container
//! (Node 24 + rustup) over a read-only copy of the checkout, for checks that only run on
//! Linux (the `jest-dev-server` dependent). Skips when no Docker engine is reachable.

use std::path::Path;
use std::process::{Command, Stdio};

use crate::ci::strings;
use crate::{host, package};

pub const IMAGE: &str = "wait-on-linux";
/// GHA's `ubuntu-latest` arch; Puppeteer also ships no linux-arm64 Chrome.
const PLATFORM: &str = "linux/amd64";

/// KTD2/KTD3a: trixie for the gnu addon's GLIBC 2.39+ floor; npm pinned so runs repeat (its
/// `allowScripts` gate is lifted in `run_args`). `CARGO_HOME` stays in the image (its `bin/`
/// holds the rustup proxies).
pub fn dockerfile() -> String {
    [
        "FROM node:24-trixie",
        "ENV CARGO_HOME=/usr/local/cargo RUSTUP_HOME=/usr/local/rustup PATH=/usr/local/cargo/bin:$PATH",
        "RUN npm install -g npm@11.16.0 && curl -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain none --no-modify-path",
        "",
    ]
    .join("\n")
}

pub fn build_args(context: &Path) -> Vec<String> {
    let mut args = strings(&["build", "--platform", PLATFORM, "-t", IMAGE]);
    args.push(context.to_string_lossy().to_string());
    args
}

/// Copy the read-only checkout into the container's own tree, minus host build state, then
/// install, build the Linux addon and exec the command with its words untouched (KTD3).
const SETUP: &str = "tar -C /src --exclude=./.git --exclude=./.claude --exclude=./node_modules \
--exclude=./target --exclude=./prebuilds -cf - . | tar -C /work -xf - \
&& rustup toolchain install && npm ci --engine-strict && npm run build:napi && exec \"$@\"";

pub fn run_args(root: &Path, command: &[String]) -> Vec<String> {
    let mut args = strings(&["run", "--rm", "--platform", PLATFORM]);
    // KTD3b: the bridge network has no non-loopback IPv6, so AI_ADDRCONFIG drops ::1 and
    // `localhost` misses servers bound to ::1.
    args.extend(strings(&["--network", "host"]));
    // npm 11's `allowScripts` gate skips every install script not approved by name, so a
    // `scripts: true` dependent (puppeteer, nx, @swc/core) would install without them.
    // `scripts: false` entries still pass `--ignore-scripts`.
    args.extend(strings(&[
        "-e",
        "npm_config_dangerously_allow_all_scripts=true",
        "-v",
    ]));
    args.push(format!("{}:/src:ro", root.to_string_lossy()));
    for volume in [
        "wait-on-linux-target:/work/target",
        "wait-on-linux-cargo-registry:/usr/local/cargo/registry",
        "wait-on-linux-rustup:/usr/local/rustup",
        "wait-on-linux-npm:/root/.npm",
    ] {
        args.extend(strings(&["-v", volume]));
    }
    args.extend(strings(&["-w", "/work", IMAGE, "sh", "-c", SETUP, "sh"]));
    args.extend_from_slice(command);
    args
}

/// KTD5: these are local tools, so no engine is a skip, not a failure.
pub fn skip_reason(reachable: bool) -> Option<String> {
    (!reachable).then(|| "docker engine not reachable (`docker info` failed): skipping".into())
}

/// `docker info` succeeds only when the CLI reaches a running engine.
pub fn docker_reachable() -> bool {
    Command::new("docker")
        .arg("info")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

pub fn run(args: &[String]) -> i32 {
    host::exit_code(linux(args))
}

fn linux(args: &[String]) -> Result<i32, String> {
    if args.is_empty() {
        return Err("usage: cargo xtask linux <command> [args...]".into());
    }
    if let Some(reason) = skip_reason(docker_reachable()) {
        eprintln!("{reason}");
        return Ok(0);
    }
    let root = host::repo_root();
    let context = package::fresh_temp_dir("linux-image")?;
    std::fs::write(context.join("Dockerfile"), dockerfile()).map_err(|e| e.to_string())?;
    let docker = Path::new("docker");
    if host::run(docker, &build_args(&context), &root, None) != 0 {
        return Err("docker build failed".into());
    }
    Ok(host::run(docker, &run_args(&root, args), &root, None))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dockerfile_pins_node_npm_and_rustup_without_a_toolchain() {
        let file = dockerfile();
        assert!(file.starts_with("FROM node:24-trixie\n"), "{file}");
        assert!(file.contains("npm install -g npm@11.16.0"), "{file}");
        assert!(file.contains("--default-toolchain none"), "{file}");
        assert!(file.contains("CARGO_HOME=/usr/local/cargo"), "{file}");
        assert!(file.contains("RUSTUP_HOME=/usr/local/rustup"), "{file}");
        assert!(file.contains("PATH=/usr/local/cargo/bin:$PATH"), "{file}");
    }

    #[test]
    fn build_args_tag_the_image_for_amd64() {
        assert_eq!(
            build_args(Path::new("/ctx")),
            strings(&[
                "build",
                "--platform",
                "linux/amd64",
                "-t",
                "wait-on-linux",
                "/ctx"
            ])
        );
    }

    #[test]
    fn run_args_mount_the_checkout_read_only_and_pass_the_command_verbatim() {
        let cmd = strings(&["npm", "run", "dependents", "--", "--only", "a b"]);
        let args = run_args(Path::new("/repo"), &cmd);
        let at = |flag: &str, value: &str| args.windows(2).any(|w| w[0] == flag && w[1] == value);
        assert_eq!(&args[..2], &strings(&["run", "--rm"])[..]);
        assert!(at("--platform", "linux/amd64"), "{args:?}");
        assert!(at("--network", "host"), "{args:?}");
        assert!(
            at("-e", "npm_config_dangerously_allow_all_scripts=true"),
            "{args:?}"
        );
        assert!(at("-v", "/repo:/src:ro"), "{args:?}");
        assert!(at("-w", "/work"), "{args:?}");
        for volume in [
            "wait-on-linux-target:/work/target",
            "wait-on-linux-cargo-registry:/usr/local/cargo/registry",
            "wait-on-linux-rustup:/usr/local/rustup",
            "wait-on-linux-npm:/root/.npm",
        ] {
            assert!(at("-v", volume), "{volume} in {args:?}");
        }
        // No volume or bind over the host tree besides the read-only source.
        assert!(
            !args
                .iter()
                .any(|a| a.starts_with("/repo:") && a != "/repo:/src:ro")
        );
        let image = args.iter().position(|a| a == IMAGE).expect("image");
        assert_eq!(args[image + 1], "sh");
        assert_eq!(args[image + 2], "-c");
        assert_eq!(args[image + 4], "sh");
        assert_eq!(&args[image + 5..], &cmd[..]);
    }

    #[test]
    fn setup_copies_without_host_build_state_then_installs_builds_and_execs() {
        let args = run_args(Path::new("/repo"), &strings(&["true"]));
        let image = args.iter().position(|a| a == IMAGE).expect("image");
        let setup = &args[image + 3];
        for exclude in [".git", ".claude", "node_modules", "target", "prebuilds"] {
            assert!(
                setup.contains(&format!("--exclude=./{exclude} ")),
                "{exclude}: {setup}"
            );
        }
        let order = [
            "tar -C /src",
            "tar -C /work -xf -",
            "rustup toolchain install",
            "npm ci --engine-strict",
            "npm run build:napi",
            "exec \"$@\"",
        ];
        let at: Vec<usize> = order
            .iter()
            .map(|s| setup.find(s).unwrap_or_else(|| panic!("{s} in {setup}")))
            .collect();
        assert!(at.windows(2).all(|w| w[0] < w[1]), "{setup}");
    }

    #[test]
    fn skip_reason_names_docker_only_when_the_engine_is_unreachable() {
        let reason = skip_reason(false).expect("a reason");
        assert!(reason.contains("docker info"), "{reason}");
        assert!(reason.contains("skipping"), "{reason}");
        assert_eq!(skip_reason(true), None);
    }

    #[test]
    fn run_without_a_command_is_a_usage_error() {
        assert_eq!(run(&[]), 1);
    }
}
