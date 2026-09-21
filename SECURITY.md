# Security

Companion for a **running** loopback CG-Agent-Harness. It is not the harness.
It must not weaken harness I6, CSRF, or write gates.

CISA [Secure by Design](https://www.cisa.gov/securebydesign): default-deny
network, fail closed, no secrets in this tree.

## Asset → threat → control

| Asset | Threat (STRIDE) | Control | Residual |
|---|---|---|---|
| Chat POST | SSRF / spoofed origin | `LoopbackOrigin` allows only IPv4/IPv6 loopback (`127.0.0.1` / `[::1]`) over HTTP or explicitly constructed HTTPS. `localhost` rejected. Credentials, query, fragment, extra path rejected. | Local process can still hit loopback. Single-operator box. |
| Chat POST | CSRF | Token from `GET /` meta tag; charset `[A-Za-z0-9_-]`, 8–128 chars. Header `X-CyClaw-CSRF`. Never logged (`Debug` redacts). | Token is in console HTML; any local process can scrape it (same as curl). |
| Chat POST | Redirect SSRF | `reqwest` `redirect::Policy::none()`. 3xx → `Redirect` error. | None for this client. |
| Chat POST | Write-gate bypass | Allowlist: `GET /`, `/api/status`, `/api/sessions`; `POST /api/sessions`, `/api/chat`, `/api/auth/login`, `/api/auth/password`. Password change is limited by Harness to the authenticated account and requires its current password plus console CSRF. No `/api/agent/*`, no `"loop"` field, no `/api/auth/logout` or `/api/auth/users`. CI checks the allowlist and client source. | New methods must extend the allowlist and tests. |
| Secrets | Info disclosure | Never reads `~/.CGagentHarness/.env`. No API key prompt. `AUTH_REQUIRED` directs the user to Harness Login; legacy API-key errors direct them to the console. | Operator may still type secrets into chat; that is harness policy. |
| Model text | XSS / URL open (CWE-1022) | `NSTextField` preview and `NSTextView` scroll pane are plain strings, not HTML. `display` strips C0/ANSI; preview caps at 400 chars and expanded text at 8,000. No `open` of URLs. | A future WebView would need a new review. |
| Backend response | Memory exhaustion (CWE-400) | Every response stream is capped at 1 MiB before text or JSON parsing; `Content-Length` is only an early rejection. | A local backend can still spend the full request timeout sending the capped prefix. |
| Home files | Planted `harness.json` | Regular file only (no symlink), ≤64 KiB, not world-writable, port 1024–65535. | Owner-writable plant still works (same uid). |
| Harness TLS | MITM on loopback / spoofed harness | HTTPS client pins exactly the harness's own leaf certificate read from its home (`tls/server.pem`, same hardening as `harness.json`: no symlink, ≤64 KiB, not world-writable); `tls_built_in_root_certs(false)` — no other CA is trusted. Never `danger_accept_invalid_certs`. A mismatch (rotation or spoof) surfaces as a distinct `CertMismatch` error, never silently retried as plain HTTP. | Owner-writable plant of `tls/server.pem` still works (same uid) — same residual class as `harness.json`. |
| Harness login and password change | Credential handling | The username uses a native text field; passwords use `NSSecureTextField` prompts. The worker sends login to `POST /api/auth/login` (no CSRF by Harness design), or current/new passwords to guarded `POST /api/auth/password`. Avatar does not persist credentials or log request bodies; errors carry Harness error codes. Session cookies, including the replacement cookie after a password change, stay in the worker's in-memory cookie jar. | Password strings are dropped after use, not securely zeroized. A compromised local process with debugger access could read process memory. |
| Chat reply | Trusting harness-reported web use | `web_tools` from `/api/chat` is only ever displayed as a short "via web ×N" count — this app never fetches, renders, or opens the underlying pages itself, and never calls `/api/web/*`. | The harness's own web-fetch trust boundary (its URL allowlist, SerpAPI/public-Google fallback) is unchanged by this app either way. |
| CI | Supply chain | Actions pinned to full SHAs. `permissions: contents: read`. No `pull_request_target`. `cargo deny`. | Pin drift; Dependabot recommended. |

## Desktop sidecar discovery

The bundled `CG Agent Harness.app` binds `127.0.0.1:0`, not 8790. The avatar does **not** scan the port range. If `:8790` fails it runs argv-only `lsof` for `cgagentharness` LISTEN sockets on `127.0.0.1`, then `GET /api/status` must look like harness JSON (`model` + `api_key_optional`, or the thin pre-login shape: `auth_enabled: true` with no `model` at all). Ollama `:11434` is excluded. Guarded POSTs take CSRF from `GET /` on that origin. Login's session-scoped `csrf_token` is validated but not used for these guarded routes; login and successful password change clear the cached console token. The focus Unix socket is not used.

## HTTPS discovery and certificate pinning

A fresh harness home has no HTTP fallback (`tls.enabled: true` by default), so every discovery probe tries HTTPS first — pinning the harness's own `tls/server.pem` — before falling back to plain HTTP for a legacy (`tls.enabled: false`) home. The pinned certificate is read fresh on each probe, never cached across a rotation. IPv6 loopback (`[::1]`) is tried alongside `127.0.0.1` for the HTTPS path, since the harness's cert SANs cover both; the legacy plain-HTTP path stays IPv4-only, matching its existing, narrower `lsof -i4TCP@127.0.0.1` scan.

## Direct Ollama

Default chat backend. Same fail-closed HTTP rules as harness chat: `127.0.0.1:11434` only, no redirects, no forwarding headers, no API key, model tag is a **constant** (`qwen3.8:27b-mlx`), path allowlist `/v1/chat/completions` and `/api/tags`. The bundled Soul Markdown is sent as the first `system` message on Direct Ollama chat requests only. It is application data, not tool authority: the request has no `tools`, web-search, web-fetch, filesystem, process, or write capability. Does not call `/api/generate`, Ollama cloud APIs, or any harness write route.

## Darwin

- `LSUIElement`: menu extra, no Dock. No Accessibility / Mic / Camera entitlements.
- `NSAllowsLocalNetworking` only. Not `NSAllowsArbitraryLoads`.
- Text input uses the AppKit field editor (in-process). No CGEvent tap.
- Ad-hoc signature. Hardened Runtime / App Sandbox wait for notarize.

## Explicit non-goals

- Scanning the ephemeral port range (discovery uses existing loopback listeners)
- Sending `loop: true`
- Calling agent run/jobs/push/publish
- Storing GitHub or LLM API keys
- Following HTTP redirects, even to loopback
