---
name: rust-optimize
description: Tighten and validate Rust code in this crate before it ships - a clippy-driven idiom and performance pass, plus a check that changes don't quietly break the blocking-I/O-off-the-main-thread and bounded-response-size design. Use when asked to optimize, speed up, clean up, or review Rust code or performance here, before merging a perf-sensitive change to client.rs, http.rs, discover.rs, ollama.rs, or app.rs, or for "is this efficient" / "reduce allocations" - even if the user just pastes a diff.
argument-hint: "[file, module, or diff to review]"
---

# Rust optimize

This crate is a low-QPS macOS menu-bar client, not a hot-loop service. "Optimize"
here rarely means micro-allocations. It means: don't freeze the UI, don't blow past
the deliberate size/time bounds, and clear `cargo clippy -D warnings` (CI's actual
bar). Read the diff or module with that ordering in mind before suggesting anything.
`$ARGUMENTS` narrows the scope; with none, review the branch's diff.

For reuse and duplication cleanups use `/simplify`, and for correctness bugs use
`/code-review`. This skill covers what is specific to this repo's performance and
bounds.

## 1. Establish a clean baseline first

```sh
./scripts/ci.sh
```

This runs `cargo fmt --all -- --check`, `cargo clippy --locked --all-targets --all-features -- -D warnings`,
`cargo test --locked --all-targets`, and `cargo deny --locked check` (if installed),
the same gate CI runs. If it's not green before you start, fix that first: a
before/after comparison against a broken baseline is meaningless. `rust-toolchain.toml`
pins Rust 1.88; if `cargo clippy` reports a different version, put rustup's binaries
first on `PATH` so the pinned toolchain's Clippy is used (see `docs/BUILD.md`). On
Linux (Claude Code on the web) `app.rs` is compiled out, so the gate there covers the
portable modules only. Say so when a change touches `app.rs`.

## 2. What actually matters in this codebase, in priority order

1. **Main-thread blocking.** `client.rs`, `ollama.rs`, and `discover.rs` use
   *blocking* `reqwest` on purpose (see `Cargo.toml`: no async runtime, `blocking`
   feature only). Any network call reachable from `app.rs` (macOS-only, AppKit) must
   run the way the existing chat/status calls do, off the thread that owns the
   `NSApplication` run loop, not inline in a menu action handler. A "faster" change
   that moves a call onto the main thread is a regression dressed as an optimization;
   check `app.rs` (`spawn_workers`) for the existing dispatch pattern before adding a
   new call site.
2. **Worker-thread stalls.** The `cg-agent-status` thread runs the blocking health and
   discovery probe *and* derives the creature's mood every 250 ms, so a slow probe
   freezes the mood. The bounds are `GET_TIMEOUT` (8 s) per request, plus
   `discover::lsof_stdout`, which runs `lsof` with no deadline. Don't add slow or
   unbounded calls to that loop; give new blocking work its own thread or a deadline.
   Also remember every `reqwest` client owns a runtime thread, so reuse a client rather
   than building one per request or per port (`discover.rs` builds one per pass and
   re-points it).
3. **Bounded response handling.** `http.rs` caps every response body at `MAX_BODY`
   before it's parsed as text or JSON. This is CWE-400 memory-exhaustion mitigation,
   documented in `SECURITY.md`, not an arbitrary limit. Don't read a body before that
   cap, and don't raise the cap "for performance" without that being an explicit,
   separately justified decision.
4. **Timeouts are asymmetric on purpose.** `client.rs` and `ollama.rs` set `GET_TIMEOUT`
   (8 s), `CHAT_TIMEOUT` (720 s), and `CONNECT_TIMEOUT` (2 s) separately, and `ollama.rs`
   adds `WEB_TIMEOUT` (30 s), because chat generation is slow and everything else should
   fail fast. Don't unify them to "simplify".
5. **`cargo clippy -D warnings` findings.** This is the highest-leverage, lowest-risk
   category. CI already enforces it, so fixing a real clippy finding can't regress the
   gate. Prefer these over speculative rewrites.
6. **Obvious algorithmic waste** (repeated parsing of the same JSON/HTML, O(n^2)
   string building, redundant clones of data that's immediately dropped). This app's
   traffic is one operator's chat turns, not a request firehose. Don't chase
   micro-allocation wins that clippy doesn't already flag; they're not worth the risk
   of introducing a bug in security-sensitive code (`origin.rs`, `csrf.rs`,
   `validate.rs`).

## 3. Explicitly out of scope for "optimize"

- Introducing `async`/a runtime, threads, or `unsafe` to chase throughput. This
  codebase is deliberately synchronous and safe; that tradeoff is an architecture
  decision, not something to relitigate under a perf request. If it genuinely needs
  revisiting, say so and ask. Don't just do it.
- Re-tuning the release profile (`Cargo.toml`: `strip = true`, `lto = "thin"`,
  `codegen-units = 1`) without a measured reason.
- Loosening anything `SECURITY.md` documents as a control (redirect refusal, path
  allowlist, cert pinning, response cap) in the name of speed. If a security control
  is genuinely the bottleneck, that's a finding to report, not something to quietly
  work around. See the `check-security-invariants` skill.

## 4. Report, then change

List concrete findings (file:line, what's wasteful or blocking, why) before editing.
After making changes, re-run `./scripts/ci.sh` and note whether the clippy warning
count changed. A change that "optimizes" but turns CI red isn't done.
