---
title: The codebase-memory graph is missing in freshly started agent sessions
date: 2026-10-01
category: best-practices
module: .mcp.json, .claude/settings*.json
problem_type: best_practice
component: tooling
severity: medium
applies_when:
  - A worker agent is started in this repo and is told to use codebase-memory-mcp
  - ToolSearch for "codebase-memory" returns nothing in that session
retire_when: "`claude mcp list` in a fresh session in this repo lists a codebase-memory server"
tags: [codebase-memory-mcp, mcp, plugins, overdrive, workers, spike-next-rs]
---

# The codebase-memory graph is missing in freshly started agent sessions

## Symptom

A worker told to use the graph first reports that ToolSearch finds no `codebase-memory` tools, even by keyword, and falls back to LSP + grep. The PM session that was started earlier does have `plugin:overdrive:codebase-memory-mcp`.

## Root cause

- `.claude/settings.json` enables `overdrive@overdrive`, and the `overdrive` marketplace is registered, but the plugin is not in `~/.claude/plugins/installed_plugins.json`, so a fresh session cannot load it.
- The repo's own `.mcp.json` server `codebase-memory-mcp` is listed under `disabledMcpjsonServers` in the gitignored `.claude/settings.local.json` (to avoid a duplicate of the plugin's copy).
- Result: `claude mcp list` in a fresh session shows no codebase-memory server at all.

The graph itself is fine: it indexes `lib/`, `bin/`, `test/` and `crates/` (Rust included), and `scripts/reindex-codebase-memory.sh` re-indexes by repo path at session start.

## Fix (operator, pick one)

- Install the plugin: `claude plugin install overdrive@overdrive`; or
- Enable the project server: move `codebase-memory-mcp` from `disabledMcpjsonServers` to `enabledMcpjsonServers` in `.claude/settings.local.json`.

Agents must not make either change themselves (it edits agent configuration). Sessions started before the fix need a restart. Verify with `claude mcp list`. Until then, workers use LSP (tsserver, rust-analyzer) and grep for literals only.
