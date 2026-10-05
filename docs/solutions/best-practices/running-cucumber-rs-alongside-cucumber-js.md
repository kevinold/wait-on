---
title: Running the same features in cucumber-rs and cucumber-js needs tag inheritance and fail-loud guards
date: 2026-10-05
category: best-practices
module: crates/wait-on-features
problem_type: best_practice
component: testing_framework
severity: medium
applies_when:
  - "Adding a Rust runner for Gherkin features another runner already executes"
  - "Adding or bumping cucumber-rs and its dev-only crates"
related_components: [crates/wait-on-features/tests/features.rs, supply-chain/config.toml, deny.toml, docs/guides/testing.md]
tags: [cucumber-rs, cucumber-js, gherkin, cargo-vet, cargo-deny, spike-next-rs]
---

# Running the same features in cucumber-rs and cucumber-js needs tag inheritance and fail-loud guards

## Context

`features/*.feature` is run by cucumber-js (the consumer contract) and its `@engine` scenarios
also by cucumber-rs against `waiter::wait`. The two runners disagree by default.

## Guidance

- **Tags:** cucumber-js lets a scenario inherit Feature and Rule tags. cucumber-rs's
  `filter_run` closure sees them separately, so it must check `feature.tags`, the rule's tags and
  `scenario.tags` together, or an `@engine` Feature runs nothing.
- **Fail loud:** use `fail_on_skipped()` so an undefined step fails, and exit non-zero when zero
  scenarios passed. Otherwise a tag typo or a filter bug reports green with nothing run.
- **Outlines:** cucumber-rs expands every `<name>` in a Scenario Outline, so a non-column
  placeholder like `<resource 1>` is a parse error for the whole file, even in scenarios it never
  runs. Keep such placeholders in plain Scenarios.
- **Supply chain:** cucumber-rs brought about 70 new crates to vet (`safe-to-run` exemptions).
  The `synthez*` crates are BlueOak-1.0.0, which is not on the `deny.toml` license allow-list.
  This works because `cargo deny` builds its graph without dev-dependencies. Moving cucumber-rs
  out of dev-dependencies would fail the license gate.
- **No skips:** the contract allows no skipped scenario, so a step that needs a scarce resource
  picks an alternative instead of skipping (the Fetch bad-port step tries the next bad-list port
  when 6000 is taken).
