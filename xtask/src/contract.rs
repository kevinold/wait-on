//! `cargo xtask contract [--tgz <path>] [--fixture cjs|esm|ts] [--engine js|rust-strict]`
//! The library-consumer contract (KTD1): pack (unless --tgz), install the tarball into a fresh
//! copy of each `features/fixtures/<f>/` project, then run cucumber-js once per engine and
//! fixture with the proof preload on `NODE_OPTIONS` so every child records which engine ran.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

use crate::{host, package};

pub const FIXTURES: [&str; 3] = ["cjs", "esm", "ts"];
pub const ENGINES: [&str; 2] = ["js", "rust-strict"];

/// Env a contract run must not inherit: a proxy would reroute http checks, and the addon
/// override would stand in for the installed prebuild.
const SCRUBBED: [&str; 8] = [
    "HTTP_PROXY",
    "http_proxy",
    "HTTPS_PROXY",
    "https_proxy",
    "NO_PROXY",
    "no_proxy",
    "WAIT_ON_NATIVE_LIBRARY_PATH",
    "WAIT_ON_PROOF_FILE",
];

#[derive(Debug, PartialEq)]
pub struct Cell {
    pub fixture: &'static str,
    pub engine: &'static str,
    pub env: HashMap<String, String>,
}

#[derive(Debug, PartialEq)]
pub struct Plan {
    /// `None`: pack the working tree first.
    pub tgz: Option<String>,
    pub cells: Vec<Cell>,
}

fn pick(
    all: &[&'static str],
    flag: &str,
    value: Option<&String>,
) -> Result<Vec<&'static str>, String> {
    match value {
        None => Ok(all.to_vec()),
        Some(v) => all
            .iter()
            .find(|a| **a == v.as_str())
            .map(|a| vec![*a])
            .ok_or(format!("{flag} {v}: expected one of {}", all.join(", "))),
    }
}

/// Parse the flags and lay out one cell per fixture × engine, fixture-major so each install
/// runs both engines back to back.
pub fn plan(
    args: &[String],
    parent: &HashMap<String, String>,
    preload: &Path,
) -> Result<Plan, String> {
    let mut flags: HashMap<&str, &String> = HashMap::new();
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        let name = match arg.as_str() {
            "--tgz" | "--fixture" | "--engine" => arg.as_str(),
            other => return Err(format!("unknown argument {other}")),
        };
        let value = it.next().ok_or(format!("{name} needs a value"))?;
        flags.insert(name, value);
    }
    let fixtures = pick(&FIXTURES, "--fixture", flags.get("--fixture").copied())?;
    let engines = pick(&ENGINES, "--engine", flags.get("--engine").copied())?;
    let cells = fixtures
        .iter()
        .flat_map(|f| engines.iter().map(move |e| (*f, *e)))
        .map(|(fixture, engine)| Cell {
            fixture,
            engine,
            env: cell_env(engine, preload, parent),
        })
        .collect();
    Ok(Plan {
        tgz: flags.get("--tgz").map(|s| s.to_string()),
        cells,
    })
}

/// The cucumber tag expression for one fixture (KTD2): behavior scenarios run on cjs only,
/// `@consumer` scenarios on every fixture, and `@fixture:<f>` pins a scenario to one fixture.
pub fn tags(fixture: &str) -> String {
    let others: Vec<String> = FIXTURES
        .iter()
        .filter(|f| **f != fixture)
        .map(|f| format!("not @fixture:{f}"))
        .collect();
    let others = others.join(" and ");
    if fixture == "cjs" {
        others
    } else {
        format!("@consumer and {others}")
    }
}

/// The parent env, scrubbed, with the engine and the proof preload set.
pub fn cell_env(
    engine: &str,
    preload: &Path,
    parent: &HashMap<String, String>,
) -> HashMap<String, String> {
    let mut env = parent.clone();
    for key in SCRUBBED {
        env.remove(key);
        host::env_remove(&mut env, key);
    }
    host::env_set(&mut env, "WAIT_ON_ENGINE", engine);
    // quoted so a temp path with spaces survives Node's NODE_OPTIONS parsing
    host::env_set(
        &mut env,
        "NODE_OPTIONS",
        &format!("--require \"{}\"", preload.display()),
    );
    env
}

/// The subcommand: returns the process exit code.
pub fn run(args: &[String]) -> i32 {
    host::exit_code(contract(args).map(|()| 0))
}

fn npm_execpath() -> Result<String, String> {
    std::env::var("npm_execpath")
        .ok()
        .filter(|v| !v.is_empty())
        .ok_or("run this through npm: npm run contract [-- --engine js]".to_string())
}

fn preload_path(root: &Path) -> PathBuf {
    root.join("features")
        .join("support")
        .join("proof-preload.js")
}

fn contract(args: &[String]) -> Result<(), String> {
    let root = host::repo_root();
    let npm = npm_execpath()?;
    let plan = plan(args, &host::env_map(), &preload_path(&root))?;
    let tgz = match plan.tgz {
        Some(t) => std::path::absolute(&t).map_err(|e| format!("{t}: {e}"))?,
        None => pack(&root, &npm)?,
    };
    run_cells(&root, &npm, &tgz, &plan.cells)
}

/// Run every cell against an already packed tarball (`cargo xtask package` after its
/// install cells, so the contract also covers the full eight-target tarball).
pub fn run_with(tgz: &Path, npm: &str) -> Result<(), String> {
    let root = host::repo_root();
    let plan = plan(&[], &host::env_map(), &preload_path(&root))?;
    run_cells(&root, npm, tgz, &plan.cells)
}

fn pack(root: &Path, npm: &str) -> Result<PathBuf, String> {
    let dest = package::fresh_temp_dir("contract-pack")?;
    let out = Command::new(host::node_exe())
        .args([npm, "pack", "--json", "--pack-destination"])
        .arg(&dest)
        .current_dir(root)
        .output()
        .map_err(|e| format!("npm pack failed:\n{e}"))?;
    if !out.status.success() {
        return Err(format!(
            "npm pack failed:\n{}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    let json: Value = serde_json::from_slice(&out.stdout).map_err(|e| e.to_string())?;
    let name = json[0]["filename"]
        .as_str()
        .ok_or("npm pack printed no filename")?;
    Ok(dest.join(name))
}

/// The `@types/node` version the repo locks; the ts fixture installs it because
/// `index.d.ts` references Node types.
fn locked_types_node(root: &Path) -> Result<String, String> {
    let lock =
        std::fs::read_to_string(root.join("package-lock.json")).map_err(|e| e.to_string())?;
    let lock: Value = serde_json::from_str(&lock).map_err(|e| e.to_string())?;
    lock["packages"]["node_modules/@types/node"]["version"]
        .as_str()
        .map(String::from)
        .ok_or("package-lock.json has no @types/node".to_string())
}

fn install(root: &Path, npm: &str, tgz: &Path, fixture: &str) -> Result<PathBuf, String> {
    let project = package::fresh_temp_dir(&format!("contract-{fixture}"))?;
    let src = root.join("features").join("fixtures").join(fixture);
    let io = |e: std::io::Error| format!("{}: {e}", src.display());
    for entry in std::fs::read_dir(&src).map_err(io)? {
        let entry = entry.map_err(io)?;
        std::fs::copy(entry.path(), project.join(entry.file_name())).map_err(io)?;
    }
    let mut args: Vec<String> = [
        npm,
        "install",
        "--ignore-scripts",
        "--no-audit",
        "--no-fund",
    ]
    .map(String::from)
    .to_vec();
    args.push(tgz.to_string_lossy().to_string());
    if fixture == "ts" {
        args.push(format!("@types/node@{}", locked_types_node(root)?));
    }
    let mut env = host::env_map();
    for key in SCRUBBED {
        env.remove(key);
        host::env_remove(&mut env, key);
    }
    if host::run(&host::node_exe(), &args, &project, Some(&env)) != 0 {
        return Err(format!("contract {fixture}: npm install failed"));
    }
    Ok(project)
}

fn run_cells(root: &Path, npm: &str, tgz: &Path, cells: &[Cell]) -> Result<(), String> {
    let cucumber = root
        .join("node_modules")
        .join("@cucumber")
        .join("cucumber")
        .join("bin")
        .join("cucumber.js");
    let host_dir = host::host_dir()?;
    let mut failed = Vec::new();
    let mut projects: HashMap<&str, PathBuf> = HashMap::new();
    for cell in cells {
        let project = match projects.get(cell.fixture) {
            Some(p) => p.clone(),
            None => {
                let p = install(root, npm, tgz, cell.fixture)?;
                projects.insert(cell.fixture, p.clone());
                p
            }
        };
        let world = serde_json::json!({
            "project": project, "fixture": cell.fixture, "engine": cell.engine, "hostDir": host_dir,
        });
        let args = vec![
            cucumber.to_string_lossy().to_string(),
            "--strict".to_string(),
            "--tags".to_string(),
            tags(cell.fixture),
            "--world-parameters".to_string(),
            world.to_string(),
        ];
        println!("> contract {} under {}", cell.fixture, cell.engine);
        if host::run(&host::node_exe(), &args, root, Some(&cell.env)) != 0 {
            failed.push(format!("{} under {}", cell.fixture, cell.engine));
        }
    }
    for project in projects.values() {
        let _ = std::fs::remove_dir_all(project);
    }
    if failed.is_empty() {
        Ok(())
    } else {
        Err(format!("contract failed: {}", failed.join(", ")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(args: &[&str]) -> Vec<String> {
        args.iter().map(|a| a.to_string()).collect()
    }

    fn parent() -> HashMap<String, String> {
        HashMap::from(
            [
                ("PATH", "/bin"),
                ("HTTP_PROXY", "http://p:1"),
                ("https_proxy", "http://p:2"),
                ("NO_PROXY", "localhost"),
                ("WAIT_ON_NATIVE_LIBRARY_PATH", "/x.node"),
                ("NODE_OPTIONS", "--inspect"),
            ]
            .map(|(k, v)| (k.to_string(), v.to_string())),
        )
    }

    #[test]
    fn no_flags_packs_and_runs_every_fixture_under_both_engines() {
        let preload = Path::new("/r/features/support/proof-preload.js");
        let plan = plan(&[], &parent(), preload).unwrap();
        assert_eq!(plan.tgz, None);
        let cells: Vec<(&str, &str)> = plan.cells.iter().map(|c| (c.fixture, c.engine)).collect();
        assert_eq!(
            cells,
            vec![
                ("cjs", "js"),
                ("cjs", "rust-strict"),
                ("esm", "js"),
                ("esm", "rust-strict"),
                ("ts", "js"),
                ("ts", "rust-strict"),
            ]
        );
        for cell in &plan.cells {
            assert_eq!(
                cell.env.get("WAIT_ON_ENGINE").map(String::as_str),
                Some(cell.engine)
            );
            assert_eq!(
                cell.env.get("NODE_OPTIONS").map(String::as_str),
                Some("--require \"/r/features/support/proof-preload.js\"")
            );
            assert_eq!(cell.env.get("PATH").map(String::as_str), Some("/bin"));
            for key in [
                "HTTP_PROXY",
                "https_proxy",
                "NO_PROXY",
                "WAIT_ON_NATIVE_LIBRARY_PATH",
            ] {
                assert!(!cell.env.contains_key(key), "{key} scrubbed");
            }
        }
    }

    #[test]
    fn flags_narrow_to_one_cell_and_name_the_tarball() {
        let plan = plan(
            &s(&["--tgz", "x.tgz", "--fixture", "cjs", "--engine", "js"]),
            &parent(),
            Path::new("/p.js"),
        )
        .unwrap();
        assert_eq!(plan.tgz.as_deref(), Some("x.tgz"));
        let cells: Vec<(&str, &str)> = plan.cells.iter().map(|c| (c.fixture, c.engine)).collect();
        assert_eq!(cells, vec![("cjs", "js")]);
    }

    #[test]
    fn behavior_scenarios_run_on_cjs_and_consumer_scenarios_on_every_fixture() {
        assert_eq!(tags("cjs"), "not @fixture:esm and not @fixture:ts");
        assert_eq!(
            tags("esm"),
            "@consumer and not @fixture:cjs and not @fixture:ts"
        );
        assert_eq!(
            tags("ts"),
            "@consumer and not @fixture:cjs and not @fixture:esm"
        );
    }

    #[test]
    fn bad_flags_are_named() {
        let p = Path::new("/p.js");
        let err = plan(&s(&["--fixture", "mjs"]), &parent(), p).unwrap_err();
        assert!(err.contains("mjs") && err.contains("cjs, esm, ts"), "{err}");
        let err = plan(&s(&["--engine", "rust"]), &parent(), p).unwrap_err();
        assert!(err.contains("js, rust-strict"), "{err}");
        assert!(
            plan(&s(&["--tgz"]), &parent(), p)
                .unwrap_err()
                .contains("--tgz needs a value")
        );
        assert!(
            plan(&s(&["--frob"]), &parent(), p)
                .unwrap_err()
                .contains("--frob")
        );
    }
}
