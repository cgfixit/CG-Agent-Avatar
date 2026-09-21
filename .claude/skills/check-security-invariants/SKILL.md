---
name: check-security-invariants
description: Check a diff or the current working tree against this repo's specific hard security invariants - loopback-only origins, the HTTP path allowlist, CSRF handling, no dotenv/API-key reads, certificate pinning, redirect refusal, response size caps, argv-only lsof - before it's committed or pushed. Use this whenever the user asks for a security check of changes here, before pushing to the PR, when touching client.rs/origin.rs/paths.rs/discover.rs/home.rs/csrf.rs/http.rs/ollama.rs, or asks "is this safe to merge" / "did I break anything security-wise". This is the repo-specific complement to a generic security review: it's derived directly from SECURITY.md and the tests/*.rs contract files, not OWASP-top-10 in the abstract, so run it alongside a generic review rather than instead of one.
---

# Check security invariants

This repo's threat model isn't just documentation - `SECURITY.md`'s asset -> threat
-> control table is enforced by literal source-grep tests in `tests/`. Walk the diff
against the table below (verified against current source), then actually run the
tests rather than eyeballing it - they're fast and don't need a full build.

```sh
cargo test --test source_contracts --test lsof_argv --test plist_contract --test ci_workflows
```

## The invariants and what enforces them

| Invariant | Enforced by |
|---|---|
| `client.rs` never mentions `/api/agent` or sends `loop_turn` | `source_contracts.rs::client_never_mentions_agent_routes` |
| Chat request JSON is exactly `{message, session_id}` - no `loop` field can sneak in | `source_contracts.rs::chat_serializer_cannot_grow_a_loop_field` |
| `client.rs` never sets `X-Forwarded-For`/`-Host`/`-Proto`, `X-Real-Ip`, or `Forwarded` | `source_contracts.rs::client_never_sets_forwarding_headers` |
| `home.rs` production code (outside `#[cfg(test)]`) never mentions `.env` or `CGAGENTHARNESS_API_KEY` | `source_contracts.rs::home_never_reads_dotenv` |
| Every path in `paths::FORBIDDEN` is actually unreachable via `is_allowed_get`/`is_allowed_post` | `source_contracts.rs::forbidden_paths_are_documented` |
| `discover.rs` production code never scans a range, never touches the focus socket (`cgah-desktop`, `/_desktop/ready`), never binds/matches `0.0.0.0` | `source_contracts.rs::discover_does_not_scan_or_touch_focus_socket` |
| `discover.rs` uses argv-only `/usr/sbin/lsof -i4TCP@127.0.0.1 -sTCP:LISTEN`, never `sh -c`/`bash -c`, never `localhost` | `lsof_argv.rs::discover_lsof_is_argv_only_loopback` |
| Discovery skips Ollama's port and privileged ports | `lsof_argv.rs::discover_skips_ollama_and_privileged_ports` |
| `ollama.rs` production code only calls `/v1/chat/completions`, never `/api/generate`; model tag is the constant `qwen3.8:27b-mlx`; request JSON has no `loop` key | `source_contracts.rs::ollama_relay_is_loopback_openai_compat_only` |
| `launch.rs` launches the harness only by `CFBundleIdentifier` (`com.cgfixit.agent-harness`), never a hardcoded `/Applications` path, never `Command::new` | `source_contracts.rs::launch_uses_bundle_identifier_never_a_hardcoded_path` |
| Bundle id `com.cgfixit.cg-agent`, `LSUIElement` true (no Dock) | `plist_contract.rs::bundle_identity`, `::accessory_no_dock` |
| ATS: `NSAllowsLocalNetworking` present, `NSAllowsArbitraryLoads` absent | `plist_contract.rs::ats_local_networking_only` |
| `audit.yml` doesn't use `rustsec/audit-check` (installs unpinned cargo-audit); installs `cargo-audit --locked --version 0.22.2` with `RUSTUP_TOOLCHAIN` set | `ci_workflows.rs::audit_does_not_use_rustsec_audit_check`, `::audit_installs_cargo_audit_locked_and_versioned` |
| No workflow uses `pull_request_target:` | `ci_workflows.rs::audit_never_uses_pull_request_target` (+ bundle equivalent) |
| `bundle.yml` keeps `workflow_dispatch` + the noon America/New_York schedule, isn't push-only | `ci_workflows.rs::bundle_has_noon_eastern_and_manual_dispatch`, `::bundle_release_is_not_on_push_only` |

## Invariants without a dedicated contract test - check these by reading

- `origin.rs`: `LoopbackOrigin` accepts only `127.0.0.1`/`[::1]`; `localhost` is
  rejected (it can resolve off-loopback); construction rejects a URL carrying a
  username, password, query, or fragment, and any path other than `/` or empty.
- `client.rs`: `reqwest` is built with `redirect::Policy::none()` - any 3xx becomes
  an error, never a followed redirect.
- `http.rs`: every response stream is capped at `MAX_BODY` (1 MiB) before it's
  treated as text or JSON - `Content-Length` alone is only an early rejection, not
  the real cap.
- Harness TLS: the HTTPS client pins exactly one certificate, read fresh from the
  harness's own home (`tls/server.pem`) on each probe, with `tls_built_in_root_certs(false)`
  and never `danger_accept_invalid_certs`. A certificate mismatch must surface as its
  own distinct error - never a silent fallback to plain HTTP.
- `home.rs`: `CGAGENTHARNESS_HOME` is honored only when it's an absolute path with no
  `..` components (`is_safe_home`); `harness.json` and `tls/server.pem` reads both
  reject symlinks, oversized files, and world-writable files.
- Credentials and session cookies are never written to disk; password prompts use
  `NSSecureTextField`, not a plain text field.

## Process

1. `git diff` (or the PR diff) against the table above, file by file for anything
   touching `client.rs`, `origin.rs`, `paths.rs`, `discover.rs`, `home.rs`,
   `csrf.rs`, `http.rs`, `ollama.rs`, `resources/Info.plist`, or `.github/workflows/`.
2. Run the contract-test command above and read actual pass/fail, not a guess.
3. Report findings before proposing fixes - security-relevant code doesn't get
   silently patched.
4. If a change genuinely needs to extend an invariant (a new allowlisted path, a new
   backend host), that's a deliberate, visible edit to `paths.rs` + `SECURITY.md` +
   the matching test - see the `add-avatar-feature` skill for that flow. Never edit
   a test to make a regression pass instead of fixing the regression.
5. This skill doesn't replace `/security-review` - that one catches generic
   injection/XSS/OWASP-pattern issues this one has no way to know about. Run both on
   a PR that touches networking or auth code.
