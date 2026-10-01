---
title: Holding a Rust crate at 100% cargo-llvm-cov lines and regions without ignore regexes or coverage attributes
date: 2026-10-01
category: best-practices
module: crates/wait-on-core
problem_type: best_practice
component: rust-engine
severity: medium
applies_when:
  - Gating a Rust crate on cargo-llvm-cov --fail-under-lines 100 --fail-under-regions 100
  - Writing inline #[cfg(test)] modules in src/*.rs that the coverage report measures
  - Testing a generic function (closure or callback type parameter) that the gate must score at 100%
  - Red-proving a coverage gate by deleting a test
  - Adding a skip-on-missing-capability test (IPv6 loopback, a tool, a platform feature)
related_components: [package.json, xtask, docs/guides/testing.md]
tags: [cargo-llvm-cov, coverage, regions, generics, inline-tests, red-proof, spike-next-rs]
retire_when: "cargo-llvm-cov changes its default ignore regex (src/report.rs ignore_filename_regex) or llvm-cov starts scoring generic functions by the union of instantiations; check the cargo-llvm-cov changelog and rerun the gate after a toolchain or cargo-llvm-cov bump"
---

# Holding a Rust crate at 100% cargo-llvm-cov lines and regions without ignore regexes or coverage attributes

## Context

Lane L13 (#76) of the `spike-next-rs` spine, branch `rs-76-rust-first-tests`, made
`crates/wait-on-core` the place where Rust behavior is tested first. It put a hard coverage gate
at the end of `npm run ci:rs` (`package.json:30`):

```sh
cargo vet --locked && cargo xtask ci && \
  cargo xtask cov --exclude xtask --fail-under-lines 100 --fail-under-regions 100
```

The rule was 100% with no `--ignore-filename-regex` and no `#[coverage(off)]` (that attribute is
nightly-only anyway). When the U1 baseline first ran on darwin (Rust 1.98.1, cargo-llvm-cov
0.9.1), it exited 1 at 98.61% regions (34 of 2448 missed). Some misses were real gaps in `src/`.
The rest came from how llvm-cov counts test code and generics. Four of those findings are not
obvious, and each one cost time to find (evidence: `docs/plans/2026-10-01-spike-rs-l13-rust-first-tests-plan.md`,
`## Resume notes`, U1, U3 and U7/U8 entries; finding 4 comes from the code-review fix on the
same branch, `fix(review): skip the ipv6 loopback test on hosts without ::1`). The rules now in force are in
`docs/guides/testing.md:144-159` (`## Rust coverage`). Bare `src/...` and `tests/...` paths below
are relative to `crates/wait-on-core/`.

## Guidance

### 1. Know what the report measures: `tests/` is excluded, but it still covers `src/`

cargo-llvm-cov 0.9.1 drops `tests/`, `examples/`, `benches/` directories and files named
`tests.rs` / `*_tests.rs` / `*-tests.rs` from the report by default. The regex is built in
`ignore_filename_regex` (`~/.cargo/registry/src/*/cargo-llvm-cov-0.9.1/src/report.rs:893`),
and the default pattern is at `report.rs:939-948`. It is applied unless
`--no-default-ignore-filename-regex` is passed (`report.rs:921`); the pattern below is the one
used without `--remap-path-prefix`:

```text
...|^{workspace_root}(/.*)?/(tests|examples|benches)/|^{workspace_root}(/.*)?/(tests\.rs|[0-9a-zA-Z_-]+[_-]tests\.rs)$
```

So `crates/wait-on-core/tests/**` and `src/waiter/tests.rs` never appear as report rows. The
`src/` code they execute still counts as covered. Inline `#[cfg(test)] mod tests` blocks inside
`src/*.rs` are a different case: they are part of the measured file, so every region in the test
body must run too.

As a result, **a red-proof that deletes one test only proves something if no other test, inline or
integration, reaches the same arm.**

### 2. Generic functions score by their best single instantiation

llvm-cov's per-file region summary scores a generic function by its **best single
instantiation, not the union** of its instantiations. `--show-missing-lines` merges the
instantiations, so it can show no `^0` line while the summary still reports missed regions. To
find them, read `cargo llvm-cov report --json` (per-instantiation function records). Instrumented
artefacts are in `target/xtask-inner`:

```sh
CARGO_TARGET_DIR=target/xtask-inner cargo llvm-cov report --json > cov.json
```

Fix: pick one instantiation and make it reach every arm. In `src/http.rs`, the alias
`NoValidate` (`http.rs:44`) is the type every `None::<NoValidate>` call uses. The generic
`check`/`send` (`http.rs:116`, `http.rs:135`) then needed that same instantiation to run:

- the `Some(f)` validate arm: `validate_fn_pointer_overrides_the_2xx_rule` (`http.rs:743`)
  passes a `NoValidate` fn pointer (`|s| std::future::ready(Ok(s == 401))`);
- the `let Some(client) = .. else { return cancelled() }` arm: `cancel_settles_in_flight_and_later_checks`
  (`http.rs:714`) ends with a direct `send` after `cancel` (`http.rs:739`).

The `http.rs` lines whose coverage changed from run to run were caused by this, **not** by a
`tokio::select!` race. Which line looked uncovered depended on which instantiation the summary
picked as "best".

### 3. Inline test bodies: no expressions in assertion messages, no `matches!` in async asserts

Inside measured `src/*.rs` test modules:

| Form | Cost when the assertion passes |
|---|---|
| `assert!(cond)`, `assert_eq!(a, b)` | none |
| `assert!(cond, "{x:?}")` (inline capture) | none |
| `assert!(cond, "{:?}", start.elapsed())` | message-argument region at count 0 |
| `assert!(matches!(r, Err(NotReady::Io(_))), ..)` in an `async` test | `matches!` region at count 0 |
| the same `matches!` in a sync test | none |
| `match` with an arm this host never takes | that arm at count 0 |

Bind first, then use an inline capture:

```rust
let elapsed = start.elapsed();
assert!(elapsed < Duration::from_millis(500), "{elapsed:?}");
```

In async tests, replace `matches!` with `is_err()`, a `let Err(..) = .. else` with no dead arm,
or `assert_eq!` on a value taken from the result. `tcp.rs:48-62` `times_out_within_bound`
replaced a three-arm `match` on host-dependent errors with a single `assert_eq!` relating the
reason to the elapsed bound:

```rust
let reason = result.unwrap_err().to_string();
assert_eq!(reason == "timed out", elapsed >= Duration::from_millis(200), "{reason}");
```

Outcomes that differ by host get one `cfg`-gated test per host, not a `match` covering all
of them.

### 4. Skip-on-missing-capability branches belong in `tests/`, not inline

A `let Ok(..) = .. else { return; }` skip inside an inline `src` test is itself a region. On
every host that *has* the capability, the skip never runs, so the gate fails. Put skips in
integration files, which are outside the report. The inline `ipv6_literal` twin in `src/tcp.rs`
was removed. `tests/tcp.rs:41-47` `tcp_forward_ipv6_literal_ready` keeps the skip:

```rust
let Ok(listener) = TcpListener::bind("[::1]:0").await else {
    return;
};
```

### Unreachable production code: restructure, don't ignore

If a `src/` arm cannot be reached by any test, it is either dead code or an invariant. Delete it,
or state the invariant with `expect`. Example: the `http.rs` client-build fallback became
`.expect("a client with an empty trust set always builds")` (`http.rs:103`), because only TLS
material can fail a build.

## Why This Matters

- A 100% gate with an ignore regex or `coverage(off)` decays: each exemption hides the next
  untested arm. Without exemptions, a red result always means "a missing test or a branch to
  delete" (`docs/guides/testing.md:152`).
- Without findings 2 and 3, the gate looks random. A summary miss with no `^0` line, or a
  miss on a line holding a passing `assert!`, leads people to blame races or to add an ignore.
  Both are wrong fixes.
- Without finding 1, a red-proof can pass by accident. In U7/U8, deleting
  `not_ready_timed_out_reads_timed_out` (`src/lib.rs:259`) **kept** 100%, because
  integration tests also reach the `NotReady::TimedOut` Display arm. Deleting
  `invalid_method_fails_construction` (`src/http.rs:758`) dropped regions to 99.88% and the gate
  exited 1. Only the second deletion proves the gate works.

## When to Apply

- Any crate gated by `cargo llvm-cov --fail-under-regions` (at any threshold; the effects are
  only easiest to see at 100).
- Writing or reviewing inline `#[cfg(test)]` modules in a measured `src/` file.
- A coverage summary that disagrees with `--show-missing-lines`, or that changes between runs on
  a generic function.
- Choosing which test to delete when red-proving a coverage gate.
- After a cargo-llvm-cov upgrade: re-check `report.rs` `ignore_filename_regex`, because finding 1
  depends on that default.

## Examples

**Message argument (before / after)**, `src/http.rs` timeout tests:

```rust
// before (U1 form at http.rs:451/469): start.elapsed() in the message is a region that never runs when the assert passes
assert!(start.elapsed() < Duration::from_millis(500), "{:?}", start.elapsed());

// after (http.rs:465-466)
let elapsed = start.elapsed();
assert!(elapsed < Duration::from_millis(500), "{elapsed:?}");
```

**`matches!` in an async test (before / after)**, `src/socket.rs` / `src/tcp.rs`:

```rust
// before: matches! region at count 0 even though the assert passes
assert!(matches!(result, Err(NotReady::Io(_))), "{result:?}");

// after (tcp.rs:86-87): assert on a value taken from the result
let reason = ready("127.0.0.1", port, 5000).await.unwrap_err().to_string();
assert_ne!(reason, "timed out");
```

**Generic instantiation (before / after)**, `src/http.rs`:

```rust
// before (illustrative): the Some(f) arm ran only under a closure type, so the NoValidate
// instantiation (used by every None call) still had the arm at 0, and the summary
// scored the file by one partial instantiation
c.check(Some(|s: u16| async move { Ok::<_, ()>(s == 401) })).await;

// after (http.rs:743-755): same NoValidate instantiation reaches the Some arm
let accept_401: NoValidate = |s| std::future::ready(Ok(s == 401));
checker(base, "GET", true, Some(1000)).check(Some(accept_401)).await;
```

**Skip placement (before / after)**:

```rust
// before: src/tcp.rs inline ipv6_literal test; the `else { return }` is 0 on IPv6 hosts
// after: inline twin deleted; tests/tcp.rs:41-47 keeps the let-else skip outside the report
```
