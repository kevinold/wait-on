//! `cargo xtask dependents [--only <name>] [--include-optional] [--tgz <path>] [--keep]`
//! Runs published dependents' own suites against the local tarball (KTD8, KTD9): clone each
//! `assets/dependents.json` entry at its pinned tag, run it on the published wait-on
//! (baseline), swap in the tarball, prove the swap, then run it under `js` and `rust-strict`.

use std::collections::HashMap;
use std::path::Path;

use serde_json::Value;

use crate::{contract, host, package};

const MANIFEST: &str = include_str!("../assets/dependents.json");

#[derive(Debug, PartialEq)]
pub struct Entry {
    pub name: String,
    pub repo: String,
    pub tag: String,
    /// The dependent package inside the clone; R20's `require.resolve` runs from here.
    pub subdir: String,
    /// `install` (`npm install --no-save <tgz>`) or `overrides` (root `overrides` + install).
    pub swap: String,
    /// Install and swap may run lifecycle scripts (Chromium download for jest-puppeteer).
    pub scripts: bool,
    /// Node `process.platform` names this entry runs on; empty means every OS.
    pub os: Vec<String>,
    pub optional: bool,
    /// Commands after install, before the baseline run.
    pub build: Vec<String>,
    /// The commands judged three ways.
    pub run: Vec<String>,
}

pub fn parse_manifest(text: &str) -> Result<Vec<Entry>, String> {
    let json: Value = serde_json::from_str(text).map_err(|e| format!("dependents.json: {e}"))?;
    let items = json
        .as_array()
        .ok_or("dependents.json: expected an array")?;
    items.iter().map(entry).collect()
}

fn entry(v: &Value) -> Result<Entry, String> {
    let name = v["name"].as_str().unwrap_or("?");
    let missing = |field: &str| format!("dependents.json: entry {name} needs {field}");
    let text = |field: &str| {
        v[field]
            .as_str()
            .map(String::from)
            .ok_or_else(|| missing(field))
    };
    let flag = |field: &str| v[field].as_bool().ok_or_else(|| missing(field));
    let list = |field: &str| {
        v[field]
            .as_array()
            .and_then(|a| a.iter().map(|s| s.as_str().map(String::from)).collect())
            .ok_or_else(|| missing(field))
    };
    Ok(Entry {
        name: text("name")?,
        repo: text("repo")?,
        tag: text("tag")?,
        subdir: text("subdir")?,
        swap: text("swap")?,
        scripts: flag("scripts")?,
        os: list("os")?,
        optional: flag("optional")?,
        build: list("build")?,
        run: list("run")?,
    })
}

fn strings(args: &[&str]) -> Vec<String> {
    args.iter().map(|s| s.to_string()).collect()
}

/// `git` args cloning `entry` at its tag into `dest`.
pub fn clone_args(entry: &Entry, dest: &Path) -> Vec<String> {
    let mut args = strings(&[
        "-c",
        "core.longpaths=true",
        "clone",
        "--depth",
        "1",
        "--branch",
    ]);
    args.extend([
        entry.tag.clone(),
        entry.repo.clone(),
        dest.to_string_lossy().to_string(),
    ]);
    args
}

/// npm args installing the entry's locked dependencies.
pub fn install_args(entry: &Entry) -> Vec<String> {
    let mut args = strings(&["ci"]);
    if !entry.scripts {
        args.push("--ignore-scripts".into());
    }
    args
}

/// npm arg lists that swap the tarball in, run in the clone root.
pub fn swap_plan(entry: &Entry, tgz: &Path) -> Vec<Vec<String>> {
    let tgz = tgz.to_string_lossy();
    let mut install = strings(&["install"]);
    if entry.swap == "install" {
        install.push("--no-save".into());
    }
    if !entry.scripts {
        install.push("--ignore-scripts".into());
    }
    if entry.swap == "install" {
        install.push(tgz.to_string());
        vec![install]
    } else {
        let set = format!("overrides.wait-on=file:{tgz}");
        vec![strings(&["pkg", "set", &set]), install]
    }
}

/// R20: every `wait-on` in `npm ls wait-on --all --json` stdout is at `version`. npm exits 1
/// (ELSPROBLEMS, the node marked `invalid`) when the swap breaks a pinned range, so only the
/// versions are judged, never the exit status or the `invalid`/`problems`/`error` fields.
pub fn ls_verdict(npm_ls: &str, version: &str) -> Result<(), String> {
    fn walk(node: &Value, path: &str, found: &mut Vec<(String, String)>) {
        let Some(deps) = node["dependencies"].as_object() else {
            return;
        };
        for (name, dep) in deps {
            let here = if path.is_empty() {
                name.clone()
            } else {
                format!("{path} > {name}")
            };
            if name == "wait-on" {
                found.push((
                    here.clone(),
                    dep["version"].as_str().unwrap_or("?").to_string(),
                ));
            }
            walk(dep, &here, found);
        }
    }
    let tree: Value = serde_json::from_str(npm_ls).map_err(|e| format!("npm ls output: {e}"))?;
    let mut found = Vec::new();
    walk(&tree, "", &mut found);
    if found.is_empty() {
        return Err("npm ls found no wait-on to swap".into());
    }
    let stale: Vec<String> = found
        .iter()
        .filter(|(_, v)| v != version)
        .map(|(path, v)| format!("{path}@{v}"))
        .collect();
    if stale.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "swap incomplete, expected {version}: {}",
            stale.join(", ")
        ))
    }
}

/// R13 for one rust-strict entry run: some process dlopened `addon` (the realpath of the
/// swapped-in `wait-on.node`) and none loaded `lib/engine-js.js`. `records` is the proof
/// preload's JSON-lines file.
pub fn proof_verdict(records: &str, addon: &str) -> Result<(), String> {
    let records: Vec<Value> = records
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(serde_json::from_str)
        .collect::<Result<_, _>>()
        .map_err(|e| format!("proof records: {e}"))?;
    if records.iter().any(|r| r["engineJs"] == true) {
        return Err("proof: a process loaded lib/engine-js.js under rust-strict".into());
    }
    let dlopened = records
        .iter()
        .filter_map(|r| r["dlopened"].as_array())
        .flatten()
        .filter_map(Value::as_str)
        .any(|p| package::strip_verbatim(p) == addon);
    if dlopened {
        Ok(())
    } else {
        Err(format!(
            "proof: no process dlopened {addon} under rust-strict"
        ))
    }
}

/// One run's env: the parent scrubbed of proxies and the addon override (R21), and
/// `npm_config_ignore_scripts` so npm never runs a dependent's `pre`/`post` hooks (R19:
/// start-server-and-test's `pretest` rewrites sources). `None` is the baseline: the published
/// wait-on, no engine switch, no preload. An engine also gets the proof preload.
pub fn run_env(
    engine: Option<&str>,
    proof: &Path,
    preload: &Path,
    parent: &HashMap<String, String>,
) -> HashMap<String, String> {
    let mut env = match engine {
        Some(engine) => {
            let mut env = contract::cell_env(engine, preload, parent);
            host::env_set(&mut env, "WAIT_ON_PROOF_FILE", &proof.to_string_lossy());
            env
        }
        None => {
            let mut env = contract::scrubbed(parent);
            host::env_remove(&mut env, "WAIT_ON_ENGINE");
            host::env_remove(&mut env, "NODE_OPTIONS");
            env
        }
    };
    host::env_set(&mut env, "npm_config_ignore_scripts", "true");
    env
}

/// The three-way result of one command.
#[derive(Debug, Clone)]
pub struct Row {
    pub command: String,
    pub baseline: bool,
    pub js: bool,
    pub rust: bool,
}

/// KTD9 / R21: a table and the exit code. A command failing under an engine but passing on
/// baseline is a regression (1); failing on baseline too is pre-existing (0); a rust-strict
/// run whose proof failed is 1 whatever its commands did.
pub fn verdict(name: &str, rows: &[Row], proof: &Result<(), String>) -> (String, i32) {
    let mark = |ok: bool| if ok { "pass" } else { "FAIL" };
    let mut code = 0;
    let mut table = format!(
        "\n{name}\n  {:<64} {:<8} {:<8} {:<11} verdict\n",
        "command", "baseline", "js", "rust-strict"
    );
    for r in rows {
        let label = match (r.baseline, r.js && r.rust) {
            (_, true) => "ok",
            (true, false) => {
                code = 1;
                "regression"
            }
            (false, false) => "pre-existing",
        };
        table.push_str(&format!(
            "  {:<64} {:<8} {:<8} {:<11} {label}\n",
            r.command,
            mark(r.baseline),
            mark(r.js),
            mark(r.rust)
        ));
    }
    match proof {
        Ok(()) => table.push_str("  rust-strict proof: addon dlopened, engine-js never loaded\n"),
        Err(e) => {
            code = 1;
            table.push_str(&format!("  rust-strict {e}\n"));
        }
    }
    (table, code)
}

#[derive(Debug, Default, PartialEq)]
pub struct Options {
    pub only: Option<String>,
    pub include_optional: bool,
    pub tgz: Option<String>,
    pub keep: bool,
}

pub fn parse_args(args: &[String]) -> Result<Options, String> {
    let mut opts = Options::default();
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        let mut value = || it.next().cloned().ok_or(format!("{arg} needs a value"));
        match arg.as_str() {
            "--only" => opts.only = Some(value()?),
            "--tgz" => opts.tgz = Some(value()?),
            "--include-optional" => opts.include_optional = true,
            "--keep" => opts.keep = true,
            other => return Err(format!("unknown argument {other}")),
        }
    }
    Ok(opts)
}

/// The entries to run on `platform`, in manifest order.
pub fn select<'a>(
    entries: &'a [Entry],
    opts: &Options,
    platform: &str,
) -> Result<Vec<&'a Entry>, String> {
    if let Some(only) = &opts.only
        && !entries.iter().any(|e| &e.name == only)
    {
        let known: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        return Err(format!(
            "--only {only}: expected one of {}",
            known.join(", ")
        ));
    }
    Ok(entries
        .iter()
        .filter(|e| match &opts.only {
            Some(only) => &e.name == only,
            None => !e.optional || opts.include_optional,
        })
        .filter(|e| e.os.is_empty() || e.os.iter().any(|os| os == platform))
        .collect())
}

/// Node args for a manifest command: `npm ...` runs npm's own script, `node ...` runs as is.
pub fn node_args(command: &str, npm: &str) -> Result<Vec<String>, String> {
    let mut words = command.split_whitespace().map(String::from);
    match words.next().as_deref() {
        Some("npm") => Ok(std::iter::once(npm.to_string()).chain(words).collect()),
        Some("node") => Ok(words.collect()),
        _ => Err(format!(
            "dependents.json: {command}: commands start with npm or node"
        )),
    }
}

/// The subcommand: returns the process exit code.
pub fn run(args: &[String]) -> i32 {
    host::exit_code(dependents(args))
}

fn dependents(args: &[String]) -> Result<i32, String> {
    let opts = parse_args(args)?;
    let entries = parse_manifest(MANIFEST)?;
    let (platform, _) = host::node_name(std::env::consts::OS, std::env::consts::ARCH)?;
    let picked = select(&entries, &opts, platform)?;
    let npm = host::npm_execpath("npm run dependents [-- --only <name>]")?;
    let root = host::repo_root();
    let tgz = match &opts.tgz {
        Some(t) => std::path::absolute(t).map_err(|e| format!("{t}: {e}"))?,
        None => contract::pack(&root, &npm)?,
    };
    let manifest = std::fs::read_to_string(root.join("package.json")).map_err(|e| e.to_string())?;
    let manifest: Value = serde_json::from_str(&manifest).map_err(|e| e.to_string())?;
    let version = manifest["version"]
        .as_str()
        .ok_or("package.json has no version")?;
    let mut code = 0;
    for entry in picked {
        code = code.max(run_entry(entry, &npm, &tgz, version, &root, opts.keep)?);
    }
    Ok(code)
}

/// Run each command under `env` in `cwd`; true per command that exited 0.
fn run_all(
    entry: &Entry,
    npm: &str,
    cwd: &Path,
    label: &str,
    env: &HashMap<String, String>,
) -> Result<Vec<bool>, String> {
    entry
        .run
        .iter()
        .map(|cmd| {
            println!("> {} [{label}] {cmd}", entry.name);
            Ok(host::run(&host::node_exe(), &node_args(cmd, npm)?, cwd, Some(env)) == 0)
        })
        .collect()
}

fn npm_step(npm: &str, args: Vec<String>, cwd: &Path, what: &str) -> Result<(), String> {
    let args: Vec<String> = std::iter::once(npm.to_string()).chain(args).collect();
    if host::run(&host::node_exe(), &args, cwd, None) == 0 {
        Ok(())
    } else {
        Err(format!("{what} failed"))
    }
}

/// R20: `wait-on` resolves from the dependent's dir into the swapped-in copy.
fn resolve_check(clone: &Path, subdir: &Path) -> Result<(), String> {
    let out = std::process::Command::new(host::node_exe())
        .args([
            "-p",
            "require('fs').realpathSync(require.resolve('wait-on/package.json'))",
        ])
        .current_dir(subdir)
        .output()
        .map_err(|e| e.to_string())?;
    let resolved = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let expected = clone
        .join("node_modules")
        .join("wait-on")
        .join("package.json");
    let expected =
        std::fs::canonicalize(&expected).map_err(|e| format!("{}: {e}", expected.display()))?;
    let expected = package::strip_verbatim(&expected.to_string_lossy());
    if resolved == expected {
        Ok(())
    } else {
        Err(format!(
            "wait-on resolves to {resolved}, expected {expected}"
        ))
    }
}

fn run_entry(
    entry: &Entry,
    npm: &str,
    tgz: &Path,
    version: &str,
    root: &Path,
    keep: bool,
) -> Result<i32, String> {
    let dir = package::fresh_temp_dir(&format!("dependents-{}", entry.name))?;
    let clone = dir.join("repo");
    println!(
        "> {}: clone {} {} into {}",
        entry.name,
        entry.repo,
        entry.tag,
        clone.display()
    );
    if host::run(Path::new("git"), &clone_args(entry, &clone), &dir, None) != 0 {
        return Err(format!(
            "{}: git clone failed (kept {})",
            entry.name,
            dir.display()
        ));
    }
    npm_step(
        npm,
        install_args(entry),
        &clone,
        &format!("{}: npm ci", entry.name),
    )?;
    for cmd in &entry.build {
        let args = node_args(cmd, npm)?;
        if host::run(&host::node_exe(), &args, &clone, None) != 0 {
            return Err(format!("{}: {cmd} failed", entry.name));
        }
    }
    let parent = host::env_map();
    let preload = contract::preload_path(root);
    let none = Path::new("");
    let baseline = run_all(
        entry,
        npm,
        &clone,
        "baseline",
        &run_env(None, none, &preload, &parent),
    )?;

    for args in swap_plan(entry, tgz) {
        npm_step(npm, args, &clone, &format!("{}: swap", entry.name))?;
    }
    let ls = std::process::Command::new(host::node_exe())
        .args([npm, "ls", "wait-on", "--all", "--json"])
        .current_dir(&clone)
        .output()
        .map_err(|e| e.to_string())?;
    ls_verdict(&String::from_utf8_lossy(&ls.stdout), version)
        .and_then(|()| resolve_check(&clone, &clone.join(&entry.subdir)))
        .map_err(|e| format!("{}: {e} (kept {})", entry.name, dir.display()))?;

    let mut results = Vec::new();
    for engine in contract::ENGINES {
        let proof = dir.join(format!("proof-{engine}.jsonl"));
        let env = run_env(Some(engine), &proof, &preload, &parent);
        results.push((proof, run_all(entry, npm, &clone, engine, &env)?));
    }
    let addon = clone
        .join("node_modules")
        .join("wait-on")
        .join("prebuilds")
        .join(host::host_dir()?)
        .join(package::ADDON);
    let addon = std::fs::canonicalize(&addon).unwrap_or(addon);
    let records = std::fs::read_to_string(&results[1].0).unwrap_or_default();
    let proof = proof_verdict(&records, &package::strip_verbatim(&addon.to_string_lossy()));

    let rows: Vec<Row> = entry
        .run
        .iter()
        .enumerate()
        .map(|(i, cmd)| Row {
            command: cmd.clone(),
            baseline: baseline[i],
            js: results[0].1[i],
            rust: results[1].1[i],
        })
        .collect();
    let (table, code) = verdict(&format!("{} {}", entry.name, entry.tag), &rows, &proof);
    print!("{table}");
    if keep || code != 0 {
        println!("  kept {}", dir.display());
    } else {
        remove_with_retry(&dir);
    }
    Ok(code)
}

/// Windows holds `node_modules` files briefly after their processes exit (EBUSY); cleanup
/// retries and never fails the run (R21).
fn remove_with_retry(dir: &Path) {
    for _ in 0..5 {
        if std::fs::remove_dir_all(dir).is_ok() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
    eprintln!("could not remove {}; remove it by hand", dir.display());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parent() -> HashMap<String, String> {
        HashMap::from(
            [
                ("PATH", "/bin"),
                ("HTTP_PROXY", "http://p:1"),
                ("http_proxy", "http://p:1"),
                ("HTTPS_PROXY", "http://p:2"),
                ("https_proxy", "http://p:2"),
                ("NO_PROXY", "localhost"),
                ("no_proxy", "localhost"),
                ("WAIT_ON_NATIVE_LIBRARY_PATH", "/x.node"),
                ("WAIT_ON_ENGINE", "rust"),
                ("NODE_OPTIONS", "--inspect"),
            ]
            .map(|(k, v)| (k.to_string(), v.to_string())),
        )
    }

    const SCRUBBED: [&str; 7] = [
        "HTTP_PROXY",
        "http_proxy",
        "HTTPS_PROXY",
        "https_proxy",
        "NO_PROXY",
        "no_proxy",
        "WAIT_ON_NATIVE_LIBRARY_PATH",
    ];

    #[test]
    fn engine_runs_are_scrubbed_and_carry_the_engine_proof_and_preload() {
        let env = run_env(
            Some("rust-strict"),
            Path::new("/t/proof.jsonl"),
            Path::new("/r/p.js"),
            &parent(),
        );
        for key in SCRUBBED {
            assert!(!env.contains_key(key), "{key} scrubbed");
        }
        let get = |k: &str| env.get(k).map(String::as_str);
        assert_eq!(get("WAIT_ON_ENGINE"), Some("rust-strict"));
        assert_eq!(get("WAIT_ON_PROOF_FILE"), Some("/t/proof.jsonl"));
        assert_eq!(get("NODE_OPTIONS"), Some("--require \"/r/p.js\""));
        assert_eq!(get("npm_config_ignore_scripts"), Some("true"));
        assert_eq!(get("PATH"), Some("/bin"));
    }

    #[test]
    fn baseline_runs_published_wait_on_without_engine_or_preload() {
        let env = run_env(None, Path::new("/t/p"), Path::new("/r/p.js"), &parent());
        for key in SCRUBBED
            .iter()
            .chain(&["WAIT_ON_ENGINE", "NODE_OPTIONS", "WAIT_ON_PROOF_FILE"])
        {
            assert!(!env.contains_key(*key), "{key} absent");
        }
        assert_eq!(
            env.get("npm_config_ignore_scripts").map(String::as_str),
            Some("true")
        );
    }

    fn row(command: &str, baseline: bool, js: bool, rust: bool) -> Row {
        Row {
            command: command.into(),
            baseline,
            js,
            rust,
        }
    }

    #[test]
    fn green_everywhere_with_rust_proven_exits_0() {
        let (table, code) = verdict("sst", &[row("npm run demo", true, true, true)], &Ok(()));
        assert_eq!(code, 0);
        assert!(
            table.contains("npm run demo") && table.contains("ok"),
            "{table}"
        );
    }

    #[test]
    fn failing_under_js_but_passing_on_baseline_is_a_regression() {
        let (table, code) = verdict("sst", &[row("npm run demo", true, false, true)], &Ok(()));
        assert_eq!(code, 1);
        assert!(table.contains("regression"), "{table}");
    }

    #[test]
    fn failing_under_rust_strict_but_passing_on_baseline_is_a_regression() {
        let (_, code) = verdict("sst", &[row("npm run demo", true, true, false)], &Ok(()));
        assert_eq!(code, 1);
    }

    #[test]
    fn failing_on_baseline_too_is_pre_existing_and_exits_0() {
        let (table, code) = verdict("sst", &[row("npm run demo", false, false, false)], &Ok(()));
        assert_eq!(code, 0);
        assert!(table.contains("pre-existing"), "{table}");
    }

    #[test]
    fn green_commands_with_an_unproven_rust_run_exit_1_naming_the_proof() {
        let proof = Err("proof: no process dlopened /a.node under rust-strict".to_string());
        let (table, code) = verdict("sst", &[row("npm run demo", true, true, true)], &proof);
        assert_eq!(code, 1);
        assert!(table.contains("proof: no process dlopened"), "{table}");
    }

    fn s(args: &[&str]) -> Vec<String> {
        args.iter().map(|a| a.to_string()).collect()
    }

    #[test]
    fn flags_parse() {
        let opts = parse_args(&s(&[
            "--only",
            "jest-dev-server",
            "--include-optional",
            "--tgz",
            "x.tgz",
            "--keep",
        ]))
        .unwrap();
        assert_eq!(
            opts,
            Options {
                only: Some("jest-dev-server".into()),
                include_optional: true,
                tgz: Some("x.tgz".into()),
                keep: true,
            }
        );
        assert!(parse_args(&s(&["--frob"])).unwrap_err().contains("--frob"));
        assert!(
            parse_args(&s(&["--tgz"]))
                .unwrap_err()
                .contains("--tgz needs a value")
        );
    }

    fn names(picked: &[&Entry]) -> Vec<String> {
        picked.iter().map(|e| e.name.clone()).collect()
    }

    #[test]
    fn optional_entries_run_only_when_included_and_on_their_os() {
        let e = entries();
        let all = Options::default();
        assert_eq!(
            names(&select(&e, &all, "linux").unwrap()),
            ["start-server-and-test"]
        );
        let opt = Options {
            include_optional: true,
            ..Options::default()
        };
        assert_eq!(
            names(&select(&e, &opt, "linux").unwrap()),
            ["start-server-and-test", "jest-dev-server"]
        );
        assert_eq!(
            names(&select(&e, &opt, "darwin").unwrap()),
            ["start-server-and-test"]
        );
    }

    #[test]
    fn only_picks_one_entry_and_names_an_unknown_one() {
        let e = entries();
        let only = |name: &str| Options {
            only: Some(name.into()),
            ..Options::default()
        };
        assert_eq!(
            names(&select(&e, &only("jest-dev-server"), "linux").unwrap()),
            ["jest-dev-server"]
        );
        let err = select(&e, &only("nope"), "linux").unwrap_err();
        assert!(
            err.contains("nope") && err.contains("start-server-and-test"),
            "{err}"
        );
    }

    #[test]
    fn manifest_commands_run_through_node() {
        assert_eq!(
            node_args("npm run demo2", "/n/npm-cli.js").unwrap(),
            ["/n/npm-cli.js", "run", "demo2"]
        );
        assert_eq!(
            node_args("node node_modules/mocha/bin/mocha.js src/*-spec.js", "/n").unwrap(),
            ["node_modules/mocha/bin/mocha.js", "src/*-spec.js"]
        );
        assert!(node_args("curl x", "/n").unwrap_err().contains("curl"));
    }

    fn tree(nested_version: &str) -> String {
        serde_json::json!({
            "name": "start-server-and-test",
            "problems": ["invalid: wait-on@10.0.0-rc.1"],
            "error": {"code": "ELSPROBLEMS"},
            "dependencies": {
                "wait-on": {"version": "10.0.0-rc.1", "invalid": "\"9.1.0\" from the root project"},
                "other": {"version": "1.0.0", "dependencies": {
                    "wait-on": {"version": nested_version}
                }}
            }
        })
        .to_string()
    }

    #[test]
    fn ls_accepts_every_copy_at_the_tarball_version_despite_elsproblems() {
        assert_eq!(ls_verdict(&tree("10.0.0-rc.1"), "10.0.0-rc.1"), Ok(()));
    }

    #[test]
    fn ls_names_a_nested_copy_left_at_the_published_version() {
        let err = ls_verdict(&tree("9.1.0"), "10.0.0-rc.1").unwrap_err();
        assert!(err.contains("other > wait-on@9.1.0"), "{err}");
    }

    #[test]
    fn ls_without_any_wait_on_fails() {
        let err = ls_verdict(r#"{"name":"x"}"#, "10.0.0-rc.1").unwrap_err();
        assert!(err.contains("no wait-on"), "{err}");
    }

    const ADDON: &str = "/t/sst/node_modules/wait-on/prebuilds/darwin-arm64/wait-on.node";

    #[test]
    fn proof_passes_when_some_process_dlopened_the_addon_and_none_ran_engine_js() {
        let records = format!(
            "{{\"pid\":1,\"dlopened\":[],\"engineJs\":false}}\n{{\"pid\":2,\"dlopened\":[\"{ADDON}\"],\"engineJs\":false}}\n"
        );
        assert_eq!(proof_verdict(&records, ADDON), Ok(()));
    }

    #[test]
    fn proof_accepts_a_windows_verbatim_dlopen_path() {
        let addon = r"C:\t\node_modules\wait-on\prebuilds\win32-x64\wait-on.node";
        let records =
            serde_json::json!({"pid": 1, "dlopened": [format!(r"\\?\{addon}")], "engineJs": false});
        assert_eq!(proof_verdict(&format!("{records}\n"), addon), Ok(()));
    }

    #[test]
    fn proof_fails_naming_the_proof_when_nothing_dlopened_the_addon() {
        let err =
            proof_verdict("{\"pid\":1,\"dlopened\":[],\"engineJs\":false}\n", ADDON).unwrap_err();
        assert!(err.contains("proof") && err.contains(ADDON), "{err}");
    }

    #[test]
    fn proof_fails_when_any_process_loaded_engine_js() {
        let records = format!(
            "{{\"pid\":1,\"dlopened\":[\"{ADDON}\"],\"engineJs\":false}}\n{{\"pid\":2,\"dlopened\":[],\"engineJs\":true}}\n"
        );
        let err = proof_verdict(&records, ADDON).unwrap_err();
        assert!(err.contains("engine-js"), "{err}");
    }

    fn entries() -> Vec<Entry> {
        parse_manifest(MANIFEST).unwrap()
    }

    #[test]
    fn manifest_has_the_anchor_first_and_jest_dev_server_optional_on_linux() {
        let e = entries();
        assert_eq!(e.len(), 2);
        let (sst, jds) = (&e[0], &e[1]);
        assert_eq!(sst.name, "start-server-and-test");
        assert!(!sst.optional && sst.os.is_empty() && !sst.scripts);
        assert_eq!(sst.swap, "install");
        assert_eq!(jds.name, "jest-dev-server");
        assert!(jds.optional && jds.scripts);
        assert_eq!(jds.os, vec!["linux".to_string()]);
        assert_eq!(jds.swap, "overrides");
        assert_eq!(jds.build, vec!["npm run build".to_string()]);
    }

    #[test]
    fn anchor_runs_mocha_and_the_demos_but_never_demo4() {
        let run = &entries()[0].run;
        assert_eq!(
            run[0],
            "node node_modules/mocha/bin/mocha.js test/helper src/*-spec.js"
        );
        assert!(run.contains(&"npm run demo-multiple".to_string()));
        assert!(!run.contains(&"npm run demo4".to_string()));
    }

    #[test]
    fn a_manifest_entry_missing_a_field_is_named() {
        let err = parse_manifest(r#"[{"name": "x"}]"#).unwrap_err();
        assert!(err.contains('x') && err.contains("repo"), "{err}");
    }

    #[test]
    fn clone_is_shallow_at_the_tag_with_long_paths() {
        let args = clone_args(&entries()[0], Path::new("/t/sst"));
        assert_eq!(
            args,
            [
                "-c",
                "core.longpaths=true",
                "clone",
                "--depth",
                "1",
                "--branch",
                "v3.0.12",
                "https://github.com/bahmutov/start-server-and-test.git",
                "/t/sst"
            ]
        );
    }

    #[test]
    fn install_ignores_scripts_unless_the_entry_allows_them() {
        let e = entries();
        assert_eq!(install_args(&e[0]), ["ci", "--ignore-scripts"]);
        assert_eq!(install_args(&e[1]), ["ci"]);
    }

    #[test]
    fn install_swap_pins_the_tarball_without_saving() {
        let plan = swap_plan(&entries()[0], Path::new("/p/wait-on.tgz"));
        assert_eq!(
            plan,
            [vec![
                "install",
                "--no-save",
                "--ignore-scripts",
                "/p/wait-on.tgz"
            ]]
        );
    }

    #[test]
    fn overrides_swap_points_the_root_override_at_the_tarball_then_installs() {
        let plan = swap_plan(&entries()[1], Path::new("/p/wait-on.tgz"));
        assert_eq!(
            plan,
            [
                vec!["pkg", "set", "overrides.wait-on=file:/p/wait-on.tgz"],
                vec!["install"]
            ]
        );
    }
}
