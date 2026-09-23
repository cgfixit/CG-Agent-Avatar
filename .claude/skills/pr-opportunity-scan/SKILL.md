---
name: pr-opportunity-scan
description: Time-boxed (~3 minute), strictly read-only scan of this repo that ends with 3-4 focused, independently mergeable PR proposals - bug fixes, dependency drift, CI hardening, contract-test gaps, Direct Ollama misconfigurations, performance, or security hardening. Manual only; run it when you want a prioritized "what should the next few PRs be" plan, not an edit.
disable-model-invocation: true
argument-hint: "[focus area, e.g. ci | ollama | security | perf | tests | deps]"
allowed-tools: Read Grep Glob Bash(git log:*) Bash(git diff:*) Bash(git show:*) Bash(git ls-files:*) Bash(git status:*) Bash(cargo tree:*) Bash(cargo metadata:*) Bash(wc:*)
---

# PR opportunity scan

Goal: in about **3 minutes of work**, find the 3-4 changes that would most improve
this repo. Each one must be small, verifiable, and mergeable on its own. The output
is a plan. **Change nothing**: no edits, no commits, no branches, no pushes, and no
`cargo build`/`test` that would take minutes. The only exception is when `target/`
is already warm and a single `cargo clippy` finishes in seconds.

If `$ARGUMENTS` names a focus area, spend about 2/3 of the budget there. Still run a
quick pass over the rest so that a higher-severity issue elsewhere isn't missed.

## Time box

| Minute | Do |
|---|---|
| 0:00-0:30 | Orient: run `git log --oneline -15` and `git status`, then skim `AGENTS.md` "Hard rules". Note what changed recently; fresh code is where regressions hide. |
| 0:30-2:15 | Run the probes below, fastest first. Record each hit with `file:line` and a one-line reason. Stop probing a category once it has one strong hit. |
| 2:15-3:00 | Rank the hits, merge related ones, cut down to 3-4 PRs, and write the report. |

When time runs out, report what you have. A short, well-supported list beats a
long, speculative one.

## Probes (verify each one; any of them may already be fixed)

**Bugs / correctness**
- Readiness vs request drift: `ollama.rs::tags_ok` must keep matching the exact
  `MODEL` that `chat_body` sends (a prefix match hides a 404 until first send).
- Dead error paths: enum variants that are matched but never constructed.
- Error mapping that loses meaning, e.g. an I/O read error surfaced as `Json`
  (`response_bytes` in `ollama.rs`, and the same pattern in `client.rs`).
- `unwrap()`/`expect()` outside `#[cfg(test)]` on data that comes from the network,
  home files, or the environment: `grep -n "unwrap()\|expect(" src/*.rs`.

**Direct Ollama understanding**
- `resources/direct-ollama-system.md` is sent verbatim as the system prompt to a
  **tool-free** model. Flag any instruction the model cannot follow without tools
  (reading files, appending to a memory log). Web text only arrives through the
  explicit-lookup block, so also check `web_intent.rs` for phrases that trigger a
  lookup by accident (false positives send text off the machine).
- The model tag must be identical in `ollama.rs`, the `app.rs` menu/status strings,
  `README.md`, `docs/CONTROLS.md`, `docs/BUILD.md`, `SECURITY.md`, and the
  `ollama_relay_is_loopback_openai_compat_only` contract test.
- `discover.rs` must keep excluding `OLLAMA_PORT` from Harness discovery.

**CI / supply chain**
- Every `actions/checkout` in `.github/workflows/*.yml` should set
  `persist-credentials: false` unless a job actually pushes:
  `grep -n -A3 "actions/checkout" .github/workflows/*.yml`.
- Every `uses:` has to be pinned to a full SHA with a version comment. Check for
  `permissions:` broader than `contents: read` outside the release job.
- Check that the tool versions in `docs/BUILD.md` match the workflow YAML (cargo-audit,
  cargo-deny, Rust 1.88).

**Dependencies**
- Run `cargo tree -d --locked` to find duplicate major versions worth consolidating.
- Look at the features in `Cargo.toml`: flag any that nothing uses, or a default
  feature that pulls in TLS/native code the crate doesn't need.
- Look for `deny.toml` exceptions (`ignore`, `skip`, `allow`) that lack a reason or
  an expiry.

**Tests**
- `check-security-invariants` has a section called "Invariants without a dedicated
  contract test". Each invariant listed there is a candidate for a cheap
  `include_str!` test.
- Look for public functions in `validate.rs`, `csrf.rs`, `origin.rs`, `display.rs` without
  a boundary-value test (exact limit, limit+1, empty, NUL).

**Performance**
- Blocking I/O reachable from AppKit callbacks in `app.rs` outside the worker-thread
  pattern (`std::thread::spawn`). See the `rust-optimize` skill for the priorities.
- A `reqwest` client rebuilt per request, or per timer tick, where one could be reused.

**Security hardening**
- Anything that weakens a `SECURITY.md` control, or adds a path, host, header, or
  process spawn without a matching allowlist entry and test. Use the
  `check-security-invariants` table as the checklist.

## Ranking

Rank by severity × confidence ÷ size. Put security regressions and correctness bugs
first, then CI and supply chain, then tests and deps, then perf. A proposal only
makes the list if **all** of these hold:
- the evidence is a real `file:line` you read in this session, not memory;
- it fits one reviewable PR (roughly ≤ 150 changed lines and one concern);
- it has a concrete validation command.

## Report format

```
## PR opportunity scan — <date>, HEAD <short sha>[, focus: <area>]

### 1. <type>: <imperative title>          [category · severity · size S/M]
Evidence: path/to/file.rs:123 — <what is wrong, one line>
Why it matters: <user-visible or security impact>
Change: <2-4 bullet sketch; files touched>
Validate: <exact commands, e.g. cargo test --locked --test source_contracts>
Invariants touched: <none | which AGENTS.md hard rule / SECURITY.md row>
Platform note: <"Linux-verifiable" or "needs macOS CI (touches app.rs)">

### 2. ...

### Considered and dropped
- <finding> — <why: too big, speculative, already tracked, not worth the risk>
```

Finish by asking which proposal to implement. Put each accepted proposal on its own
branch and draft PR. For anything that touches networking, auth, discovery,
`Info.plist`, or workflows, also run `check-security-invariants` before pushing.
