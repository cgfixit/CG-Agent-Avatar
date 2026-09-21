---
name: add-avatar-feature
description: Scaffold a new feature, menu item, mood/theme state, or backend capability in this macOS menu-bar avatar app so it lands in the right architectural layer and the security allowlist/tests move with it instead of drifting out of sync. Use this whenever the user asks to add a menu item, wire up a new chat/status behavior, add a theme or mood state, or extend what the avatar can talk to - even phrased casually as "can we make it also do X" or "add a button that...".
---

# Add an avatar feature

This crate is layered specifically so security-relevant code stays small and
portable, and UI code stays dumb. New functionality almost always crosses several
of these layers - find where each piece actually belongs instead of dropping
everything into `app.rs`.

## 1. The layering (bottom to top)

1. `origin.rs`, `paths.rs`, `validate.rs`, `csrf.rs`, `http.rs` - loopback/allowlist
   enforcement, input validation, CSRF, response-size caps. Pure logic, no AppKit,
   builds and tests on any OS.
2. `client.rs`, `discover.rs`, `home.rs`, `ollama.rs` - the actual backend I/O built
   on layer 1. This is where a new HTTP call lives.
3. `mood.rs`, `theme.rs`, `display.rs` - portable UI *state* (what to show), still no
   AppKit dependency - these have their own tests that run on any platform.
4. `app.rs` (`#[cfg(target_os = "macos")]` only) - the AppKit wiring: `NSStatusItem`,
   `NSMenu`/`NSMenuItem`, drawing, event handling. This is where layer 3's state
   becomes pixels, and nothing else should live here.

A feature request like "add a menu item that shows X" touches layer 4 for the
`NSMenuItem` itself, layer 3 if X is new UI state, and layer 2 if X comes from a
network call. Identify which before writing code.

## 2. The hard rule: new HTTP capability must extend the allowlist, deliberately

`paths.rs` is the exhaustive list of routes this process may call
(`ALLOWED_GET`/`ALLOWED_POST`), plus a documented `FORBIDDEN` list. `origin.rs`
enforces it structurally - `client.rs` cannot address a path that isn't in the
allowlist. So a new backend call is never just "add a function to `client.rs`":

1. Add the path constant to `paths.rs` and into `ALLOWED_GET`/`ALLOWED_POST`.
2. Add a row to `SECURITY.md`'s asset -> threat -> control table. It's the living
   threat model this repo reviews against, not background reading - a new route
   without a row there is an undocumented capability.
3. Check whether the change needs a new contract test, or extends an existing one,
   in `tests/source_contracts.rs`. For example `forbidden_paths_are_documented`
   walks `paths::FORBIDDEN` and asserts each entry is actually unreachable; the
   existing per-backend tests (`client_never_mentions_agent_routes`,
   `ollama_relay_is_loopback_openai_compat_only`) lock the shape of each backend's
   requests with a literal source-string check. A new backend or route earns the
   same treatment.

Adding a new *host or port* (a third backend, not a new path on an existing one)
follows `discover.rs`'s pattern instead - see how it distinguishes the harness port
from `OLLAMA_PORT` and never scans a range.

## 3. If it touches session, login, or CSRF

Read `client.rs` and `csrf.rs` first, and the "Chat POST" / "Harness login and
password change" rows in `SECURITY.md`. Not every route is CSRF-guarded the same
way - `/api/auth/login` intentionally isn't (harness design), but `/api/auth/password`
and the chat/session routes are. Getting this backwards either breaks the feature or
removes a control; it's not a detail to guess at.

## 4. If it's UI-only (menu item, bubble state, theme, mood)

- New mood/state logic goes in `mood.rs` (pure derivation from status + in-flight
  state, no AppKit) - see its existing `Mood` enum and `MoodInput` struct for the
  pattern.
- A new visual identity or token goes in `theme.rs`, as a new `Theme` value or field,
  not as inline constants in `app.rs` - that module exists precisely so a new look
  doesn't require hunting `draw` calls for magic numbers. `CG_AGENT_THEME` selects
  between shipped themes at runtime.
- Only the actual `NSMenuItem`/view wiring goes in `app.rs`, following the existing
  `NSMenuItem::initWithTitle_action_keyEquivalent` construction pattern already used
  there.

## 5. New user-supplied input

Route it through `validate.rs` (see `message()`/`session_id()` for the fail-closed
style: trim, length-bound, charset-check, reject before any bytes leave the process)
rather than writing an ad hoc check inline at the new call site.

## 6. Before calling it done

Run `./scripts/ci.sh`. Then specifically re-read `tests/source_contracts.rs` and, if
you touched `discover.rs` or bundle metadata, `tests/lsof_argv.rs` /
`tests/plist_contract.rs` - these are cheap `include_str!` source-grep assertions,
and this repo leans on them as the actual enforcement mechanism for its invariants,
not as optional style checks. If your feature adds a new invariant worth protecting
the same way, add a test in that style rather than only relying on review to catch
regressions later.
