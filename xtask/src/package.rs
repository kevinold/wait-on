//! `cargo xtask package [--host-only]`
//! The CI `package` job: with every target's prebuild in prebuilds/, refuse a partial bundle,
//! npm pack once, check the tarball's files and manifest, print sizes, write SHA256SUMS next to
//! wait-on-*.tgz, then install the tarball (npm, npm --omit=optional, pnpm; scripts disabled)
//! and prove each install loads the host addon from inside the installed package, then run AE1
//! in read-only, no-network glibc and musl containers (skipped without docker, except in CI).
//! --host-only (developer runs) requires only the host prebuild.
//! No shell: npm runs as `node $npm_execpath`, so it behaves the same under Windows cmd.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

use crate::build_napi::TARGETS;
use crate::host;

pub const ADDON: &str = "wait-on.node";
const NOT_SHIPPED_DIRS: [&str; 8] = [
    "target/",
    "crates/",
    "scripts/",
    "docs/",
    "test/",
    "benchmarks/",
    "xtask/",
    ".cargo/",
];
const INSTALL_SCRIPTS: [&str; 4] = ["preinstall", "install", "postinstall", "prepare"];

pub fn expected_prebuild_dirs() -> Vec<String> {
    TARGETS
        .iter()
        .map(|&(_, platform, arch, musl)| host::prebuild_dir(platform, arch, musl))
        .collect()
}

pub fn required_dirs(host_only: bool) -> Result<Vec<String>, String> {
    Ok(if host_only {
        vec![host::host_dir()?]
    } else {
        expected_prebuild_dirs()
    })
}

pub fn missing_prebuilds(prebuilds_root: &Path, dirs: &[String]) -> Vec<String> {
    dirs.iter()
        .filter(|dir| !prebuilds_root.join(dir).join(ADDON).exists())
        .cloned()
        .collect()
}

pub fn format_missing(missing: &[String]) -> String {
    let lines: Vec<String> = missing
        .iter()
        .map(|dir| format!("  prebuilds/{dir}/{ADDON}"))
        .collect();
    format!(
        "missing prebuilds (download every napi artifact, or pass --host-only locally):\n{}",
        lines.join("\n")
    )
}

/// One entry of `npm pack --json`.
#[derive(Debug, Default)]
pub struct Pack {
    pub filename: String,
    pub size: u64,
    pub unpacked_size: u64,
    pub files: Vec<(String, u64)>,
}

impl Pack {
    pub fn from_json(v: &Value) -> Result<Pack, String> {
        let bad = || format!("unexpected npm pack --json output: {v}");
        let files = v["files"].as_array().ok_or_else(bad)?;
        Ok(Pack {
            filename: v["filename"].as_str().ok_or_else(bad)?.to_string(),
            size: v["size"].as_u64().ok_or_else(bad)?,
            unpacked_size: v["unpackedSize"].as_u64().ok_or_else(bad)?,
            files: files
                .iter()
                .map(|f| Some((f["path"].as_str()?.to_string(), f["size"].as_u64()?)))
                .collect::<Option<_>>()
                .ok_or_else(bad)?,
        })
    }

    fn has(&self, path: &str) -> bool {
        self.files.iter().any(|(p, _)| p == path)
    }
}

fn not_shipped(path: &str) -> bool {
    path.starts_with("Cargo.") || NOT_SHIPPED_DIRS.iter().any(|d| path.starts_with(d))
}

pub fn check_pack(pack: &Pack, dirs: &[String]) -> Vec<String> {
    let unpacked = dirs
        .iter()
        .map(|dir| format!("prebuilds/{dir}/{ADDON}"))
        .filter(|p| !pack.has(p))
        .map(|p| format!("not packed: {p}"));
    let shipped = pack
        .files
        .iter()
        .filter(|(p, _)| not_shipped(p))
        .map(|(p, _)| format!("must not ship: {p}"));
    unpacked.chain(shipped).collect()
}

pub fn check_manifest(manifest: &Value) -> Vec<String> {
    let scripts = &manifest["scripts"];
    let mut problems: Vec<String> = INSTALL_SCRIPTS
        .iter()
        .filter(|name| scripts.get(**name).is_some())
        .map(|name| format!("lifecycle script \"{name}\" is declared"))
        .collect();
    if manifest.get("optionalDependencies").is_some() {
        problems.push("optionalDependencies is declared".to_string());
    }
    problems
}

#[derive(Debug, PartialEq)]
pub struct Sizes {
    pub packed: u64,
    pub unpacked: u64,
    pub js_only_unpacked: u64,
    pub targets: Vec<(String, u64)>,
}

pub fn size_report(pack: &Pack) -> Sizes {
    let targets: Vec<(String, u64)> = pack
        .files
        .iter()
        .filter(|(p, _)| p.starts_with("prebuilds/") && p.ends_with(&format!("/{ADDON}")))
        .map(|(p, size)| (p.split('/').nth(1).unwrap_or_default().to_string(), *size))
        .collect();
    let addon_bytes: u64 = targets.iter().map(|(_, s)| s).sum();
    Sizes {
        packed: pack.size,
        unpacked: pack.unpacked_size,
        js_only_unpacked: pack.unpacked_size - addon_bytes,
        targets,
    }
}

pub fn sha256sums_line(file: &Path) -> Result<String, String> {
    let bytes = std::fs::read(file).map_err(|e| format!("{}: {e}", file.display()))?;
    let digest = ring::digest::digest(&ring::digest::SHA256, &bytes);
    let hex: String = digest.as_ref().iter().map(|b| format!("{b:02x}")).collect();
    let name = file.file_name().unwrap_or_default().to_string_lossy();
    Ok(format!("{hex}  {name}\n"))
}

pub fn parse_args(args: &[String]) -> bool {
    args.iter().any(|a| a == "--host-only")
}

pub const PNPM_VERSION: &str = "10.34.6";

#[derive(Debug)]
pub struct InstallCell {
    pub name: &'static str,
    pub cmd: PathBuf,
    pub args: Vec<String>,
    pub env: HashMap<String, String>,
}

/// One cell per install shape, all with lifecycle scripts disabled. pnpm comes from the
/// registry via npm exec (no devDependency, no corepack). The addon override is scrubbed so it
/// cannot stand in for the installed prebuild.
pub fn install_cells(
    tgz: &str,
    npm_exec_path: &str,
    env: &HashMap<String, String>,
) -> Vec<InstallCell> {
    let mut cell_env = env.clone();
    cell_env.remove("WAIT_ON_NATIVE_LIBRARY_PATH");
    let npm_install = [
        npm_exec_path,
        "install",
        "--ignore-scripts",
        "--no-audit",
        "--no-fund",
    ];
    let pnpm = format!("pnpm@{PNPM_VERSION}");
    let cells: [(&'static str, Vec<&str>); 3] = [
        ("npm", [&npm_install[..], &[tgz]].concat()),
        (
            "npm-omit-optional",
            [&npm_install[..], &["--omit=optional", tgz]].concat(),
        ),
        (
            "pnpm",
            vec![
                npm_exec_path,
                "exec",
                "--yes",
                "--package",
                &pnpm,
                "--",
                "pnpm",
                "add",
                tgz,
                "--ignore-scripts",
            ],
        ),
    ];
    cells
        .into_iter()
        .map(|(name, args)| InstallCell {
            name,
            cmd: host::node_exe(),
            args: args.into_iter().map(String::from).collect(),
            env: cell_env.clone(),
        })
        .collect()
}

/// `canonicalize` on Windows returns `\\?\C:\...`; Node's realpath does not.
// ponytail: drive paths only; `\\?\UNC\` shares never host a temp project.
pub fn strip_verbatim(path: &str) -> String {
    path.strip_prefix(r"\\?\").unwrap_or(path).to_string()
}

pub fn assert_installed_addon(
    realpath: &str,
    project_root: &Path,
    dir: &str,
) -> Result<(), String> {
    let sep = std::path::MAIN_SEPARATOR_STR;
    let root = project_root
        .canonicalize()
        .map_err(|e| format!("{}: {e}", project_root.display()))?;
    let root = strip_verbatim(&root.to_string_lossy()) + sep;
    let tail = Path::new("prebuilds").join(dir).join(ADDON);
    let tail = tail.to_string_lossy();
    let real = strip_verbatim(realpath);
    if !real.starts_with(&root) || !real.ends_with(&format!("{sep}{tail}")) {
        return Err(format!(
            "loaded {realpath}, expected {tail} inside the installed package under {root}"
        ));
    }
    Ok(())
}

const STUB_PACKAGE_JSON: &str = "{\"name\":\"wait-on-probe\",\"private\":true}\n";

/// The JS probe run inside install cells and containers (KTD1).
pub fn probe_path(root: &Path) -> PathBuf {
    root.join("xtask").join("assets").join("prebuild-probe.js")
}

#[derive(Debug)]
pub struct ContainerCell {
    pub name: String,
    pub tag: String,
    pub expected_dir: String,
    pub expect_ready: bool,
    pub dockerfile: String,
    pub npmrc: &'static str,
    pub run_args: Vec<String>,
}

/// AE1: install the tarball at image build time with ignore-scripts=true, then run with a
/// read-only root and no network. The probe serves its own tcp listener inside the container;
/// the timeout cells wait on a closed port so the check must poll and time out.
pub fn container_cells(arch: &str) -> Vec<ContainerCell> {
    // ponytail: trixie (glibc 2.41) because the gnu addons need GLIBC_2.39 (built on
    // ubuntu-24.04); lowering that floor is a napi build change, see docs/guides/releasing.md.
    let images = [
        ("glibc", "node:24-trixie-slim", format!("linux-{arch}")),
        ("musl", "node:24-alpine", format!("linux-{arch}-musl")),
    ];
    images
        .into_iter()
        .flat_map(|(libc, image, expected_dir)| {
            let tag = format!("wait-on-ae1-{libc}");
            let dockerfile = [
                format!("FROM {image}").as_str(),
                "WORKDIR /app",
                "COPY .npmrc package.json wait-on.tgz prebuild-probe.js ./",
                "RUN npm install --omit=optional --no-audit --no-fund ./wait-on.tgz",
                "",
            ]
            .join("\n");
            let ready: Vec<String> = [
                "run",
                "--rm",
                "--read-only",
                "--network",
                "none",
                "-e",
                "WAIT_ON_ENGINE=rust-strict",
                &tag,
                "node",
                "/app/prebuild-probe.js",
            ]
            .map(String::from)
            .to_vec();
            let timeout = [
                &ready[..],
                &["--no-listener", "--timeout", "1000"].map(String::from),
            ]
            .concat();
            [(true, "ready", ready), (false, "timeout", timeout)].map(
                |(expect_ready, kind, run_args)| ContainerCell {
                    name: format!("{libc}-{kind}"),
                    tag: tag.clone(),
                    expected_dir: expected_dir.clone(),
                    expect_ready,
                    dockerfile: dockerfile.clone(),
                    npmrc: "ignore-scripts=true\n",
                    run_args,
                },
            )
        })
        .collect()
}

pub fn write_docker_context(
    cell: &ContainerCell,
    tgz: &Path,
    probe: &Path,
    context: &Path,
) -> Result<(), String> {
    let io = |e: std::io::Error| format!("{}: {e}", context.display());
    std::fs::write(context.join("Dockerfile"), &cell.dockerfile).map_err(io)?;
    std::fs::write(context.join(".npmrc"), cell.npmrc).map_err(io)?;
    std::fs::write(context.join("package.json"), STUB_PACKAGE_JSON).map_err(io)?;
    std::fs::copy(tgz, context.join("wait-on.tgz")).map_err(io)?;
    std::fs::copy(probe, context.join("prebuild-probe.js")).map_err(io)?;
    Ok(())
}

#[derive(Debug, PartialEq)]
pub enum DockerDecision {
    Run,
    Skip(String),
    Fail(String),
}

pub fn docker_decision(docker_found: bool, ci: bool) -> DockerDecision {
    let reason = "docker not found: read-only container cells (AE1)";
    match (docker_found, ci) {
        (true, _) => DockerDecision::Run,
        (false, true) => DockerDecision::Fail(format!("{reason} must run in CI")),
        (false, false) => DockerDecision::Skip(format!("{reason} skipped")),
    }
}

/// JS `String(v)`: strings unquoted, everything else as JSON (`true`, `null`, `1`).
fn js_string(v: &Value) -> String {
    v.as_str().map_or_else(|| v.to_string(), String::from)
}

/// Judge one probe JSON line. Ready: the API and the CLI both succeeded with the addon from
/// `expected_dir`. Timeout: the API and the CLI each timed out on their own (the CLI's stderr,
/// not the API's, carries the CLI's message). Returns `None` on pass, else the reason.
pub fn probe_verdict(line: &Value, expect_ready: bool, expected_dir: &str) -> Option<String> {
    const TIMED_OUT: &str = "Timed out waiting for";
    let realpath = js_string(&line["realpath"]);
    if !realpath
        .replace('\\', "/")
        .ends_with(&format!("/prebuilds/{expected_dir}/{ADDON}"))
    {
        return Some(format!(
            "loaded {realpath}, expected prebuilds/{expected_dir}"
        ));
    }
    let (api, cli, cli_error) = (&line["api"], &line["cli"], js_string(&line["cliError"]));
    if expect_ready {
        if api != &Value::Bool(true) {
            return Some(format!("api failed: {}", js_string(api)));
        }
        if cli.as_i64() != Some(0) {
            return Some(format!("cli exited {}: {cli_error}", js_string(cli)));
        }
        return None;
    }
    if !js_string(api).contains(TIMED_OUT) {
        return Some(format!("api did not time out: {}", js_string(api)));
    }
    if cli.as_i64() == Some(0) || cli.is_null() || !cli_error.contains(TIMED_OUT) {
        return Some(format!(
            "cli did not time out (exit {}): {cli_error}",
            js_string(cli)
        ));
    }
    None
}

/// A fresh `std::env::temp_dir()/wait-on-<cell>-<pid>-<n>`.
fn fresh_temp_dir(cell: &str) -> Result<PathBuf, String> {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static N: AtomicUsize = AtomicUsize::new(0);
    let n = N.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("wait-on-{cell}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    Ok(dir)
}

/// The subcommand: returns the process exit code.
pub fn run(args: &[String]) -> i32 {
    match package(args) {
        Ok(()) => 0,
        Err(err) => {
            eprintln!("{err}");
            1
        }
    }
}

fn package(args: &[String]) -> Result<(), String> {
    let root = host::repo_root();
    let host_only = parse_args(args);
    let npm = std::env::var("npm_execpath")
        .ok()
        .filter(|v| !v.is_empty())
        .ok_or("run this through npm: npm run ci:rs:package [-- --host-only]")?;

    let dirs = required_dirs(host_only)?;
    let missing = missing_prebuilds(&root.join("prebuilds"), &dirs);
    if !missing.is_empty() {
        return Err(format_missing(&missing));
    }

    let io = |e: std::io::Error| format!("{}: {e}", root.display());
    for entry in std::fs::read_dir(&root).map_err(io)? {
        let name = entry.map_err(io)?.file_name().to_string_lossy().to_string();
        if name.starts_with("wait-on-") && name.ends_with(".tgz") {
            std::fs::remove_file(root.join(&name)).map_err(io)?;
        }
    }
    let node = host::node_exe();
    let packed = Command::new(&node)
        .args([npm.as_str(), "pack", "--json"])
        .current_dir(&root)
        .output()
        .map_err(|e| format!("npm pack failed:\n{e}"))?;
    if !packed.status.success() {
        return Err(format!(
            "npm pack failed:\n{}",
            String::from_utf8_lossy(&packed.stderr)
        ));
    }
    let pack_json: Value = serde_json::from_slice(&packed.stdout).map_err(|e| e.to_string())?;
    let pack = Pack::from_json(&pack_json[0])?;
    let tgz = root.join(&pack.filename);

    let manifest = std::fs::read_to_string(root.join("package.json")).map_err(io)?;
    let manifest: Value = serde_json::from_str(&manifest).map_err(|e| e.to_string())?;
    let problems = [check_pack(&pack, &dirs), check_manifest(&manifest)].concat();
    if !problems.is_empty() {
        return Err(format!(
            "tarball check failed:\n  {}",
            problems.join("\n  ")
        ));
    }

    let sizes = size_report(&pack);
    println!("size report for {}", pack.filename);
    println!("  packed           {}", sizes.packed);
    println!("  unpacked         {}", sizes.unpacked);
    println!("  js-only unpacked {}", sizes.js_only_unpacked);
    for (dir, size) in &sizes.targets {
        println!("  {dir:<16} {size}");
    }

    std::fs::write(root.join("SHA256SUMS"), sha256sums_line(&tgz)?).map_err(io)?;
    println!("wrote {} and SHA256SUMS", pack.filename);

    let probe = probe_path(&root);
    let host = host::host_dir()?;
    let env: HashMap<String, String> = std::env::vars_os()
        .map(|(k, v)| (k.to_string_lossy().into(), v.to_string_lossy().into()))
        .collect();
    for cell in install_cells(&tgz.to_string_lossy(), &npm, &env) {
        let project = fresh_temp_dir(cell.name)?;
        let pio = |e: std::io::Error| format!("{}: {e}", project.display());
        std::fs::write(project.join("package.json"), STUB_PACKAGE_JSON).map_err(pio)?;
        if host::run(&cell.cmd, &cell.args, &project, Some(&cell.env)) != 0 {
            return Err(format!("{}: install failed", cell.name));
        }
        let out = Command::new(&node)
            .arg(&probe)
            .current_dir(&project)
            .env_clear()
            .envs(&cell.env)
            .env("WAIT_ON_ENGINE", "rust-strict")
            .output()
            .map_err(|e| format!("{}: probe failed to start: {e}", cell.name))?;
        let stdout = String::from_utf8_lossy(&out.stdout);
        if !out.status.success() {
            return Err(format!(
                "{}: probe exited {}\n{stdout}{}",
                cell.name,
                out.status.code().map_or("null".into(), |c| c.to_string()),
                String::from_utf8_lossy(&out.stderr)
            ));
        }
        let line: Value =
            serde_json::from_str(stdout.trim()).map_err(|e| format!("{}: {e}", cell.name))?;
        if let Some(problem) = probe_verdict(&line, true, &host) {
            return Err(format!("{}: {problem}", cell.name));
        }
        let realpath = js_string(&line["realpath"]);
        assert_installed_addon(&realpath, &project, &host)?;
        println!("{}: loaded {realpath}", cell.name);
        let _ = std::fs::remove_dir_all(&project);
    }

    let ci = std::env::var_os("CI").is_some_and(|v| !v.is_empty());
    let docker_found = Command::new("docker").arg("--version").output().is_ok();
    match docker_decision(docker_found, ci) {
        DockerDecision::Fail(reason) => return Err(reason),
        DockerDecision::Skip(reason) => {
            println!("{reason}");
            return Ok(());
        }
        DockerDecision::Run => {}
    }
    let (_, arch) = host::node_name(std::env::consts::OS, std::env::consts::ARCH)?;
    let cells = container_cells(arch);
    let mut unpacked: Vec<&str> = cells
        .iter()
        .map(|c| c.expected_dir.as_str())
        .filter(|dir| !pack.has(&format!("prebuilds/{dir}/{ADDON}")))
        .collect();
    unpacked.dedup();
    if !unpacked.is_empty() {
        let reason = format!("no {} prebuild: container cells (AE1)", unpacked.join(", "));
        if ci {
            return Err(format!("{reason} must run in CI"));
        }
        println!("{reason} skipped");
        return Ok(());
    }

    let docker = Path::new("docker");
    for cell in cells.iter().filter(|c| c.expect_ready) {
        let context = fresh_temp_dir(&cell.tag)?;
        write_docker_context(cell, &tgz, &probe, &context)?;
        let build = ["build", "-t", &cell.tag, &context.to_string_lossy()].map(String::from);
        if host::run(docker, &build, &root, None) != 0 {
            return Err(format!("{}: docker build failed", cell.tag));
        }
        let _ = std::fs::remove_dir_all(&context);
    }
    for cell in &cells {
        let out = Command::new(docker)
            .args(&cell.run_args)
            .output()
            .map_err(|e| format!("{}: {e}", cell.name))?;
        let stdout = String::from_utf8_lossy(&out.stdout);
        let output = format!("{stdout}{}", String::from_utf8_lossy(&out.stderr));
        let status = out.status.code().map_or("null".into(), |c| c.to_string());
        let problem = match serde_json::from_str::<Value>(stdout.trim()) {
            Ok(line) => probe_verdict(&line, cell.expect_ready, &cell.expected_dir),
            Err(_) => Some("no probe result".to_string()),
        };
        if let Some(problem) = problem {
            return Err(format!(
                "{}: {problem} (exit {status})\n{output}",
                cell.name
            ));
        }
        println!("{}: ok (exit {status})\n{}", cell.name, output.trim());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::path::PathBuf;

    const PO4: [&str; 8] = [
        "darwin-arm64",
        "darwin-x64",
        "linux-x64",
        "linux-arm64",
        "linux-x64-musl",
        "linux-arm64-musl",
        "win32-x64",
        "win32-arm64",
    ];

    fn po4() -> Vec<String> {
        PO4.map(String::from).to_vec()
    }

    pub(super) fn tmp(tag: &str) -> PathBuf {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static N: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "wait-on-pkg-{tag}-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn touch(root: &Path, rel: &str) {
        let file = root.join(rel);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, "x").unwrap();
    }

    fn pack_of(paths: &[String]) -> Pack {
        Pack {
            files: paths.iter().map(|p| (p.clone(), 1)).collect(),
            ..Pack::default()
        }
    }

    fn prebuild_paths(dirs: &[&str]) -> Vec<String> {
        dirs.iter()
            .map(|d| format!("prebuilds/{d}/wait-on.node"))
            .collect()
    }

    #[test]
    fn expected_prebuild_dirs_are_the_eight_po4_dirs_in_table_order() {
        assert_eq!(expected_prebuild_dirs(), po4());
    }

    #[test]
    fn required_dirs_is_the_host_dir_under_host_only_else_all_eight() {
        assert_eq!(required_dirs(true), Ok(vec![host::host_dir().unwrap()]));
        assert_eq!(required_dirs(false), Ok(po4()));
    }

    #[test]
    fn missing_prebuilds_names_every_missing_dir() {
        let root = tmp("missing");
        for dir in ["darwin-x64", "linux-x64-musl", "win32-arm64"] {
            touch(&root, &format!("{dir}/wait-on.node"));
        }
        let missing = missing_prebuilds(&root, &po4());
        assert_eq!(
            missing,
            [
                "darwin-arm64",
                "linux-x64",
                "linux-arm64",
                "linux-arm64-musl",
                "win32-x64"
            ]
        );
        let message = format_missing(&missing);
        for dir in &missing {
            assert!(
                message.contains(&format!("prebuilds/{dir}/wait-on.node")),
                "{message}"
            );
        }
        for dir in PO4 {
            touch(&root, &format!("{dir}/wait-on.node"));
        }
        assert!(missing_prebuilds(&root, &po4()).is_empty());
    }

    #[test]
    fn check_pack_passes_all_eight_prebuilds_and_nothing_else() {
        assert!(check_pack(&pack_of(&prebuild_paths(&PO4)), &po4()).is_empty());
    }

    #[test]
    fn check_pack_names_a_missing_prebuild_and_a_shipped_intermediate() {
        let dirs: Vec<&str> = PO4.into_iter().filter(|d| *d != "win32-x64").collect();
        let mut paths = prebuild_paths(&dirs);
        paths.push("scripts/prebuild-probe.js".into());
        let problems = check_pack(&pack_of(&paths), &po4()).join("\n");
        assert!(
            problems.contains("prebuilds/win32-x64/wait-on.node"),
            "{problems}"
        );
        assert!(problems.contains("scripts/prebuild-probe.js"), "{problems}");
    }

    #[test]
    fn check_pack_never_ships_xtask_cargo_config_or_cargo_lock() {
        let mut paths = prebuild_paths(&PO4);
        paths.extend(["xtask/src/main.rs", ".cargo/config.toml", "Cargo.lock"].map(String::from));
        let problems = check_pack(&pack_of(&paths), &po4());
        assert_eq!(
            problems,
            [
                "must not ship: xtask/src/main.rs",
                "must not ship: .cargo/config.toml",
                "must not ship: Cargo.lock"
            ]
        );
    }

    #[test]
    fn npm_pack_dry_run_ships_every_prebuild_and_no_intermediates() {
        // ponytail: needs npm; set when run through npm run (R6)
        let Some(npm) = std::env::var_os("npm_execpath") else {
            eprintln!("skipped: npm_execpath unset");
            return;
        };
        let root = tmp("pack");
        std::fs::copy(
            host::repo_root().join("package.json"),
            root.join("package.json"),
        )
        .unwrap();
        for rel in [
            "lib/wait-on.js",
            "bin/wait-on",
            "exampleConfig.js",
            "index.d.ts",
        ] {
            touch(&root, rel);
        }
        for dir in PO4 {
            touch(&root, &format!("prebuilds/{dir}/wait-on.node"));
        }
        for rel in [
            "target/x",
            "crates/x",
            "scripts/x",
            "docs/x",
            "test/x",
            "benchmarks/x",
            "xtask/x",
            ".cargo/x",
            "Cargo.toml",
        ] {
            touch(&root, rel);
        }
        let out = Command::new(host::node_exe())
            .arg(npm)
            .args(["pack", "--dry-run", "--json"])
            .current_dir(&root)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let v: Value = serde_json::from_slice(&out.stdout).unwrap();
        let pack = Pack::from_json(&v[0]).unwrap();
        assert_eq!(check_pack(&pack, &po4()), Vec::<String>::new());
        for p in prebuild_paths(&PO4) {
            assert!(pack.files.iter().any(|(f, _)| *f == p), "{p} not packed");
        }
    }

    #[test]
    fn pack_from_json_reads_npm_pack_fields() {
        let v = json!({"filename": "wait-on-1.0.0.tgz", "size": 7, "unpackedSize": 9,
            "files": [{"path": "package.json", "size": 3, "mode": 420}]});
        let pack = Pack::from_json(&v).unwrap();
        assert_eq!(pack.filename, "wait-on-1.0.0.tgz");
        assert_eq!((pack.size, pack.unpacked_size), (7, 9));
        assert_eq!(pack.files, [("package.json".to_string(), 3)]);
        assert!(Pack::from_json(&json!({})).is_err());
    }

    #[test]
    fn check_manifest_rejects_lifecycle_scripts_and_optional_deps_but_not_the_repo() {
        let joined = |v: Value| check_manifest(&v).join("\n");
        assert!(joined(json!({"scripts": {"postinstall": "x"}})).contains("postinstall"));
        assert!(joined(json!({"scripts": {"prepare": "x"}})).contains("prepare"));
        assert!(joined(json!({"optionalDependencies": {}})).contains("optionalDependencies"));
        let repo = std::fs::read_to_string(host::repo_root().join("package.json")).unwrap();
        assert!(check_manifest(&serde_json::from_str(&repo).unwrap()).is_empty());
    }

    #[test]
    fn size_report_splits_js_only_from_per_target_addons() {
        let mut files: Vec<(String, u64)> = prebuild_paths(&PO4)
            .into_iter()
            .map(|p| (p, 600_000))
            .collect();
        files.push(("lib/wait-on.js".into(), 150_000));
        files.push(("package.json".into(), 50_000));
        let pack = Pack {
            size: 700_000,
            unpacked_size: 5_000_000,
            files,
            ..Pack::default()
        };
        assert_eq!(
            size_report(&pack),
            Sizes {
                packed: 700_000,
                unpacked: 5_000_000,
                js_only_unpacked: 200_000,
                targets: po4().into_iter().map(|d| (d, 600_000)).collect(),
            }
        );
    }

    #[test]
    fn sha256sums_line_is_sha256sum_compatible() {
        let file = tmp("sha").join("wait-on-1.0.0.tgz");
        std::fs::write(&file, "abc").unwrap();
        assert_eq!(
            sha256sums_line(&file).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad  wait-on-1.0.0.tgz\n"
        );
    }

    #[test]
    fn parse_args_reads_host_only() {
        assert!(parse_args(&["--host-only".to_string()]));
        assert!(!parse_args(&[]));
    }
    #[test]
    fn install_cells_plan_npm_omit_optional_and_pnpm_with_scripts_off_and_override_scrubbed() {
        let env: HashMap<String, String> = [
            ("PATH", "p"),
            ("WAIT_ON_NATIVE_LIBRARY_PATH", "/elsewhere/wait-on.node"),
        ]
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .into();
        let cells = install_cells("/t/wait-on.tgz", "/npm-cli.js", &env);
        let names: Vec<&str> = cells.iter().map(|c| c.name).collect();
        assert_eq!(names, ["npm", "npm-omit-optional", "pnpm"]);
        for cell in &cells {
            assert_eq!(cell.cmd, host::node_exe());
            assert_eq!(cell.args[0], "/npm-cli.js");
            assert!(cell.args.iter().any(|a| a == "--ignore-scripts"));
            assert!(cell.args.iter().any(|a| a == "/t/wait-on.tgz"));
            assert!(!cell.env.contains_key("WAIT_ON_NATIVE_LIBRARY_PATH"));
            assert_eq!(cell.env["PATH"], "p");
        }
        let omit = |c: &InstallCell| c.args.iter().any(|a| a == "--omit=optional");
        assert!(!omit(&cells[0]));
        assert!(omit(&cells[1]));
        let pnpm = cells[2].args.join(" ");
        assert!(
            pnpm.contains("exec --yes --package pnpm@10.34.6 -- pnpm add"),
            "{pnpm}"
        );
        assert!(
            env.contains_key("WAIT_ON_NATIVE_LIBRARY_PATH"),
            "caller's env untouched"
        );
    }

    #[test]
    fn strip_verbatim_drops_the_windows_verbatim_prefix_only() {
        assert_eq!(strip_verbatim(r"\\?\C:\p\x"), r"C:\p\x");
        assert_eq!(strip_verbatim("/p/x"), "/p/x");
    }

    #[test]
    fn assert_installed_addon_accepts_the_installed_prebuild_only() {
        let host = host::host_dir().unwrap();
        let project = tmp("installed");
        let real = strip_verbatim(&project.canonicalize().unwrap().to_string_lossy());
        let inside = Path::new(&real)
            .join("node_modules/wait-on/prebuilds")
            .join(&host)
            .join(ADDON);
        let inside = inside
            .to_string_lossy()
            .replace('/', std::path::MAIN_SEPARATOR_STR);
        assert_eq!(assert_installed_addon(&inside, &project, &host), Ok(()));

        let repo_addon = host::repo_root().join("prebuilds").join(&host).join(ADDON);
        let repo_addon = repo_addon.to_string_lossy().to_string();
        let err = assert_installed_addon(&repo_addon, &project, &host).unwrap_err();
        assert!(err.contains(&repo_addon), "{err}");

        let wrong = inside.replace(&host, "other");
        let err = assert_installed_addon(&wrong, &project, &host).unwrap_err();
        assert!(err.contains(&host), "{err}");
    }
    #[test]
    fn container_cells_cover_glibc_and_musl_read_only_without_network() {
        let cells = container_cells("x64");
        let names: Vec<&str> = cells.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(
            names,
            ["glibc-ready", "glibc-timeout", "musl-ready", "musl-timeout"]
        );
        let images: Vec<&str> = cells
            .iter()
            .map(|c| {
                c.dockerfile
                    .lines()
                    .next()
                    .unwrap()
                    .trim_start_matches("FROM ")
            })
            .collect();
        assert_eq!(
            images,
            [
                "node:24-trixie-slim",
                "node:24-trixie-slim",
                "node:24-alpine",
                "node:24-alpine"
            ]
        );
        let dirs: Vec<&str> = cells.iter().map(|c| c.expected_dir.as_str()).collect();
        assert_eq!(
            dirs,
            ["linux-x64", "linux-x64", "linux-x64-musl", "linux-x64-musl"]
        );
        for cell in &cells {
            let run = cell.run_args.join(" ");
            assert!(run.contains("--read-only"), "{run}");
            assert!(run.contains("--network none"), "{run}");
            assert!(run.contains("-e WAIT_ON_ENGINE=rust-strict"), "{run}");
            assert!(
                run.contains(&format!("{} node /app/prebuild-probe.js", cell.tag)),
                "{run}"
            );
            assert_eq!(cell.npmrc, "ignore-scripts=true\n");
            assert!(cell.dockerfile.contains("prebuild-probe.js"));
            assert!(cell.dockerfile.contains("--omit=optional"));
        }
        let ready: Vec<&str> = cells
            .iter()
            .filter(|c| c.expect_ready)
            .map(|c| c.name.as_str())
            .collect();
        assert_eq!(ready, ["glibc-ready", "musl-ready"]);
        for cell in cells.iter().filter(|c| !c.expect_ready) {
            assert_eq!(
                cell.run_args[cell.run_args.len() - 3..],
                ["--no-listener", "--timeout", "1000"]
            );
        }
    }

    #[test]
    fn container_cells_derive_dirs_from_the_runner_arch() {
        let cells = container_cells("arm64");
        let dirs: Vec<&str> = cells.iter().map(|c| c.expected_dir.as_str()).collect();
        assert_eq!(
            dirs,
            [
                "linux-arm64",
                "linux-arm64",
                "linux-arm64-musl",
                "linux-arm64-musl"
            ]
        );
    }

    #[test]
    fn docker_context_carries_the_probe_asset() {
        let probe = probe_path(&host::repo_root());
        assert!(probe.is_file(), "{} missing", probe.display());
        let tgz = tmp("ctx-tgz").join("wait-on-1.0.0.tgz");
        std::fs::write(&tgz, "tgz").unwrap();
        let context = tmp("ctx");
        let cell = &container_cells("x64")[0];
        write_docker_context(cell, &tgz, &probe, &context).unwrap();
        let read = |f: &str| std::fs::read_to_string(context.join(f)).unwrap();
        assert_eq!(
            read("prebuild-probe.js"),
            std::fs::read_to_string(&probe).unwrap()
        );
        assert_eq!(read("wait-on.tgz"), "tgz");
        assert_eq!(read("Dockerfile"), cell.dockerfile);
        assert_eq!(read(".npmrc"), "ignore-scripts=true\n");
        assert_eq!(read("package.json"), STUB_PACKAGE_JSON);
    }

    #[test]
    fn docker_decision_skips_locally_fails_in_ci_and_runs_when_present() {
        match docker_decision(false, false) {
            DockerDecision::Skip(reason) => {
                assert!(reason.contains("docker"), "{reason}");
                assert!(!reason.contains('\n'), "{reason}");
            }
            other => panic!("expected skip, got {other:?}"),
        }
        match docker_decision(false, true) {
            DockerDecision::Fail(reason) => assert!(reason.contains("CI"), "{reason}"),
            other => panic!("expected fail, got {other:?}"),
        }
        assert_eq!(docker_decision(true, true), DockerDecision::Run);
        assert_eq!(docker_decision(true, false), DockerDecision::Run);
    }

    fn with(mut v: Value, key: &str, val: Value) -> Value {
        v[key] = val;
        v
    }

    #[test]
    fn probe_verdict_ready_passes_only_when_api_and_cli_succeed_from_the_expected_dir() {
        let ready = json!({"realpath": "/app/node_modules/wait-on/prebuilds/linux-x64-musl/wait-on.node",
            "api": true, "cli": 0, "cliError": ""});
        let verdict = |v: &Value, dir: &str| probe_verdict(v, true, dir).unwrap_or_default();
        assert_eq!(probe_verdict(&ready, true, "linux-x64-musl"), None);
        assert!(
            verdict(&with(ready.clone(), "api", json!("boom")), "linux-x64-musl").contains("api")
        );
        assert!(verdict(&with(ready.clone(), "cli", json!(1)), "linux-x64-musl").contains("cli"));
        assert!(
            verdict(&with(ready.clone(), "cli", Value::Null), "linux-x64-musl").contains("cli")
        );
        assert!(verdict(&ready, "linux-x64").contains("linux-x64"));
        let windows = with(
            ready,
            "realpath",
            json!(r"C:\p\node_modules\wait-on\prebuilds\linux-x64-musl\wait-on.node"),
        );
        assert_eq!(probe_verdict(&windows, true, "linux-x64-musl"), None);
    }

    #[test]
    fn probe_verdict_timeout_passes_only_when_api_and_cli_each_time_out() {
        let timed_out = json!({"realpath": "/app/node_modules/wait-on/prebuilds/linux-x64/wait-on.node",
            "api": "Timed out waiting for: tcp:127.0.0.1:1", "cli": 1,
            "cliError": "Timed out waiting for: tcp:127.0.0.1:1"});
        let verdict = |v: &Value| probe_verdict(v, false, "linux-x64").unwrap_or_default();
        assert_eq!(probe_verdict(&timed_out, false, "linux-x64"), None);
        let no_text = with(
            timed_out.clone(),
            "cliError",
            json!("Error: Cannot find module"),
        );
        assert!(verdict(&no_text).contains("cli"));
        assert!(verdict(&with(timed_out.clone(), "api", json!(true))).contains("api"));
        assert!(verdict(&with(timed_out, "cli", json!(0))).contains("cli"));
    }
}
