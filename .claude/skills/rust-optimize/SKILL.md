---
name: rust-optimize
description: Tighten and validate Rust code in this crate before it ships - a clippy-driven idiom/perf pass plus a check that changes don't quietly break this app's blocking-I/O-off-the-main-thread and bounded-response-size design. Use this whenever the user asks to optimize, speed up, clean up, tighten, or review Rust code or performance in this repo, before merging a perf-sensitive change to client.rs/http.rs/discover.rs/app.rs, or when asked "is this efficient" / "can this be faster" / "reduce allocations" - even if they just paste a diff and ask what to improve.
---

# Rust optimize

This crate is a low-QPS macOS menu-bar client, not a hot-loop service. "Optimize"
here rarely means micro-allocations - it means: don't freeze the UI, don't blow past
the deliberate size/time bounds, and clear `cargo clippy -D warnings` (CI's actual
bar). Read the diff or module with that ordering in mind before suggesting anything.

## 1. Establish a clean baseline first

```sh
./scripts/ci.sh
```

This runs `cargo fmt --all -- --check`, `cargo clippy --all-targets --all-features -- -D warnings`,
`cargo test --all-targets` (all with `--locked`), and `cargo deny check` (if
installed) - the same gate CI runs. If it's not green before you start, fix that
first: a before/after perf comparison against a broken baseline is meaningless.
`rust-toolchain.toml` pins Rust 1.88; if `cargo clippy` reports a different version,
put rustup's binaries first on `PATH` so the pinned toolchain's Clippy is used (see
`docs/BUILD.md`). On Linux (Claude Code on the web) `app.rs` is compiled out, so the
gate there covers the portable modules only - say so when a change touches `app.rs`.

## 2. What actually matters in this codebase, in priority order

1. **Main-thread blocking.** `client.rs`, `ollama.rs`, and `discover.rs` use
   *blocking* `reqwest` on purpose (see `Cargo.toml`: `reqwest` has no `tokio`
   feature). Any network call reachable from `app.rs` (macOS-only, AppKit) must run
   the way the existing chat/status calls do - off the thread that owns the
   `NSApplication` run loop - not inline in a menu action handler. A "faster"
   change that moves a call onto the main thread is a regression dressed as an
   optimization; check `app.rs` for the existing dispatch pattern before adding a
   new call site.
2. **Bounded response handling.** `http.rs` caps every response body at `MAX_BODY`
   before it's parsed as text or JSON - this is CWE-400 memory-exhaustion mitigation,
   documented in `SECURITY.md`, not an arbitrary limit. Don't read a body before that
   cap, and don't raise the cap "for performance" without that being an explicit,
   separately-justified decision.
3. **Timeouts are asymmetric on purpose.** `client.rs` sets `GET_TIMEOUT` (8s),
   `CHAT_TIMEOUT` (720s), and `CONNECT_TIMEOUT` (2s) separately because chat
   generation is slow and everything else should fail fast. Don't unify them to
   "simplify."
4. **`cargo clippy -D warnings` findings.** This is the highest-leverage, lowest-risk
   category - CI already enforces it, so fixing a real clippy finding can't
   regress the gate. Prefer these over speculative rewrites.
5. **Obvious algorithmic waste** (repeated parsing of the same JSON/HTML, O(n^2)
   string building, redundant clones of data that's immediately dropped). This app's
   traffic is one operator's chat turns, not a request firehose - don't chase
   micro-allocation wins that clippy doesn't already flag; they're not worth the
   risk of introducing a bug in security-sensitive code (`origin.rs`, `csrf.rs`,
   `validate.rs`).

## 3. Explicitly out of scope for "optimize"

- Introducing `async`/a runtime, threads, or `unsafe` to chase throughput. This
  codebase is deliberately synchronous and safe; that tradeoff is an architecture
  decision, not something to relitigate under a perf request. If it genuinely needs
  revisiting, say so and ask - don't just do it.
- Re-tuning the release profile (`Cargo.toml`: `strip = true`, `lto = "thin"`,
  `codegen-units = 1`) without a measured reason.
- Loosening anything `SECURITY.md` documents as a control (redirect refusal, path
  allowlist, cert pinning, response cap) in the name of speed. If a security control
  is genuinely the bottleneck, that's a finding to report, not something to quietly
  work around - see the `check-security-invariants` skill.

## 4. Report, then change

List concrete findings (file:line, what's wasteful or blocking, why) before editing.
After making changes, re-run `./scripts/ci.sh` and note whether the clippy warning
count changed. A change that "optimizes" but turns CI red isn't done.
