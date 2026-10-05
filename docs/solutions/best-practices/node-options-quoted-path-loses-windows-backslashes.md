---
title: A quoted path in NODE_OPTIONS loses its Windows backslashes
date: 2026-10-02
category: best-practices
module: xtask/src/contract.rs
problem_type: best_practice
component: tooling
severity: high
applies_when:
  - Tooling builds a NODE_OPTIONS value that carries a file path (--require, --import, --env-file)
  - The path can contain spaces, so it is wrapped in double quotes
  - The tooling also runs on Windows
tags: [node-options, windows, quoting, xtask, preload, consumer-contract, spike-next-rs]
---

# A quoted path in NODE_OPTIONS loses its Windows backslashes

## Context

`cargo xtask contract` (lane L16, #97) loads a proof preload into every Node child process through `NODE_OPTIONS=--require "<path>"`. The quotes are there so a temp path with spaces survives. On macOS the contract ran green under both engines. The defect was found only in code review: the PR's `windows-latest` `rust` row (matrix at `.github/workflows/node.js.yml:56`) runs `ci:rs` (`:75`), which runs the contract (`xtask/src/ci.rs:59`).

Node's `NODE_OPTIONS` parser reads a backslash inside a double-quoted value as an escape character and drops it. So on Windows, `--require "D:\a\wait-on\features\support\proof-preload.js"` asks Node for `D:await-onfeaturessupportproof-preload.js`. Every child, cucumber itself included, would then fail at startup (inferred from the mechanism; the fix landed before a Windows run). The reviewer reproduced this on macOS (Node 26.3.1) with a file literally named `a\b.js`. Quoted and unescaped, `--require "<dir>/a\b.js"` loaded `<dir>/ab.js`.

## Guidance

When you quote a path inside `NODE_OPTIONS`, escape `\` and `"` before you quote it:

```rust
// xtask/src/contract.rs, cell_env
let quoted = preload
    .display()
    .to_string()
    .replace('\\', r"\\")
    .replace('"', r#"\""#);
host::env_set(&mut env, "NODE_OPTIONS", &format!("--require \"{quoted}\""));
```

Pin the escape with a unit test that uses a Windows-shaped path. `Path::new(r"C:\r\p.js").display()` keeps the backslashes on every host, so the test is meaningful on macOS and Linux CI too:

```rust
#[test]
fn node_options_escapes_backslashes_in_a_windows_preload_path() {
    let env = cell_env("js", Path::new(r"C:\r\p.js"), &HashMap::new());
    assert_eq!(env.get("NODE_OPTIONS").map(String::as_str), Some(r#"--require "C:\\r\\p.js""#));
}
```

To check the round trip end to end, write a file whose name contains a backslash. Spawn `node -e 0` with the escaped value through `child_process.spawnSync`'s `env`, not through a shell, and confirm the file loads. That check printed `loaded backslash file` on macOS.

## Why This Matters

- The failure is total and invisible locally: every contract cell on Windows dies before a scenario runs, and no POSIX run can show it.
- A unit test with a POSIX path (`/r/features/support/proof-preload.js`) passes either way, so the existing test gave no signal.
- Dropping the quotes is not a fix, because then a path with a space splits into two options.

## When to Apply

Apply this to any `NODE_OPTIONS` value assembled from a path. Later lanes that reuse the proof preload, such as the dependents harness (L19, `run_env`), must use the same escaping or call `cell_env`. A path passed as an argv element (`Command::arg`) needs no escaping. Only `NODE_OPTIONS`, which Node re-parses as a string, has the quoting rules.

## Examples

| Built value | What Node loads on Windows |
|---|---|
| `--require "D:\a\x.js"` | `D:ax.js` (backslashes eaten) |
| `--require D:\a\x.js` | `D:\a\x.js`, but breaks on `D:\my dir\x.js` |
| `--require "D:\\a\\x.js"` | `D:\a\x.js` |

Related: [trimmed-pr-matrix-lets-platform-bugs-escape-to-the-base.md](trimmed-pr-matrix-lets-platform-bugs-escape-to-the-base.md) covers the same class of Windows-only bug, and [proving-the-rust-engine-ran-when-verifying-an-installed-napi-prerelease.md](proving-the-rust-engine-ran-when-verifying-an-installed-napi-prerelease.md) covers the route proof this preload carries.
