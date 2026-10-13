---
name: add-avatar-feature
description: Add a feature, menu item, mood or theme state, or backend capability to this macOS menu-bar avatar so it lands in the right layer and the allowlist entry, SECURITY.md row, docs, and contract tests move with it. Use when asked to add a menu item or button, wire up chat or status behavior, add a theme or mood, extend what the avatar talks to, or "can we make it also do X".
argument-hint: "[what to add, e.g. \"menu item that shows the model name\"]"
---

# Add an avatar feature

This crate is layered so security-relevant code stays small and portable, and UI
code stays dumb. New functionality almost always crosses several layers. Find where
each piece belongs instead of dropping everything into `app.rs`; AGENTS.md states
the rule: put new code in the lowest layer that can hold it.

## 1. The layering (bottom to top)

1. `origin.rs`, `paths.rs`, `validate.rs`, `csrf.rs`, `http.rs`, `web_intent.rs`:
   loopback/allowlist enforcement, input validation, CSRF, response-size caps,
   web-lookup intent parsing. Pure logic, no AppKit, builds and tests on any OS.
2. `tls.rs`, `client.rs`, `home.rs`, `discover.rs`, `ollama.rs`, `launch.rs`: the actual
   backend I/O built on layer 1. This is where a new HTTP call lives.
3. `harness_setup.rs`, `mood.rs`, `theme.rs`, `display.rs`: portable UI *state* (what to
   show), still no AppKit dependency. These have their own tests that run on any
   platform.
4. `app.rs` (`#[cfg(target_os = "macos")]` only): the AppKit wiring: `NSStatusItem`,
   `NSMenu`/`NSMenuItem`, drawing, event handling. This is where layer 3's state
   becomes pixels, and nothing else should live here.

`app.rs` also holds some AppKit-free logic (the worker threads, `harness_probe`,
`ollama_health`) whose tests run only on macOS CI. Don't add to that pile: new logic
of that kind goes in a lower layer where Linux CI can test it.

"Add a menu item that shows X" touches layer 4 for the `NSMenuItem`, layer 3 if X is
new UI state, and layer 2 if X comes from a network call. Identify which first.

| If the feature... | It lives in | The same change must also touch |
|---|---|---|
| calls a new Harness route | `paths.rs` + `client.rs` | `SECURITY.md` row, a contract test, README/`docs/CONTROLS.md` |
| calls a new Ollama route, or changes the chat body | `ollama.rs` (`ALLOWED`, `chat_body`) | `SECURITY.md` "Direct Ollama" rows, `source_contracts.rs` `ollama_*` tests |
| takes new user text | `validate.rs` | boundary tests (exact limit, limit+1, empty, NUL) |
| adds a menu item or bubble string | `app.rs` | `docs/CONTROLS.md` |
| adds a mood or theme | `mood.rs` / `theme.rs` | tests that run on Linux |
| needs a macOS permission (microphone, speech, notifications) | `resources/Info.plist` | `tests/plist_contract.rs`; `SECURITY.md` "Darwin" (it currently says no Mic/Camera) |
| adds a dependency | `Cargo.toml` + `Cargo.lock` | `cargo deny --locked check`; the license allowlist in `deny.toml` |

## 2. The hard rule: new HTTP capability must extend the allowlist, deliberately

Harness routes live in `paths.rs` (`ALLOWED_GET`/`ALLOWED_POST`), with a documented
`FORBIDDEN` list. Ollama routes live in `ollama.rs`'s own `ALLOWED`. `origin.rs`
enforces both structurally: a client cannot address a path that isn't in its
allowlist. So a new backend call is never just "add a function to `client.rs`":

1. Add the path constant to the right allowlist.
2. Add a row to `SECURITY.md`'s asset -> threat -> control table. It's the threat
   model this repo reviews against; a new route without a row is an undocumented
   capability.
3. Add or extend a contract test in `tests/source_contracts.rs`.
   `forbidden_paths_are_documented` walks `paths::FORBIDDEN` and asserts each entry is
   unreachable; the per-backend tests (`client_never_mentions_agent_routes`,
   `ollama_relay_is_loopback_openai_compat_only`,
   `ollama_web_lookups_stay_on_the_loopback_daemon`) lock the shape of each backend's
   requests with a literal source-string check. A new backend or route earns the same
   treatment.

Never call anything in `paths::FORBIDDEN`. If a feature seems to need one, stop and
ask: that's a product and security decision.

A new *host or port* (a third backend, not a new path on an existing one) follows
`discover.rs`'s pattern instead: it distinguishes the harness port from
`OLLAMA_PORT` and never scans a range.

## 3. If it touches session, login, or CSRF

Read `client.rs` and `csrf.rs` first, and the "Chat POST" / "Harness login and
password change" rows in `SECURITY.md`. Not every route is CSRF-guarded the same
way: `/api/auth/login` intentionally isn't (harness design), but `/api/auth/password`
and the chat/session routes are. Getting this backwards breaks the feature or
removes a control.

## 4. If it's UI-only (menu item, bubble state, theme, mood)

- New mood/state logic goes in `mood.rs` (pure derivation from status + in-flight
  state, no AppKit); follow its `Mood` enum and `MoodInput` struct.
- A new visual identity or token goes in `theme.rs`, as a new `Theme` value or field,
  not as inline constants in `app.rs`; `CG_AGENT_THEME` selects between shipped
  themes at runtime.
- Only the `NSMenuItem`/view wiring goes in `app.rs`, following the existing
  `NSMenuItem::initWithTitle_action_keyEquivalent` pattern. Network calls run on the
  worker threads, never on the `NSApplication` run loop.

## 5. New user-supplied input

Route it through `validate.rs` (see `message()`/`session_id()` for the fail-closed
style: trim, length-bound, charset-check, reject before any bytes leave the process),
never an ad hoc check at the call site. Model output is untrusted text: it goes
through `display.rs` before it reaches the screen.

## 6. Before calling it done

1. Run the changed code with meaningful inputs, then `cargo fmt --all -- --check`,
   `cargo clippy --locked --all-targets --all-features -- -D warnings`, and the tests
   tied to the change (`cargo test --locked --test source_contracts`, plus
   `--test lsof_argv` / `--test plist_contract` if you touched `discover.rs` or bundle
   metadata). AGENTS.md reserves `./scripts/ci.sh` for when it is explicitly requested.
   The contract tests are cheap `include_str!` source-grep assertions and the
   enforcement mechanism for this repo's invariants; if your feature adds an invariant
   worth protecting the same way, add a test in that style.
2. Docs follow code (AGENTS.md hard rule 8): rewrite the owning doc in place, per its
   doc map. `/docs-sync --check` lists what drifted.
3. If the change touches networking, auth, discovery, `Info.plist`, or workflows, run
   `/check-security-invariants` before pushing.
4. If you touched `app.rs`, say in the PR that Linux could not compile it, and don't
   claim macOS CI passed until it has.
