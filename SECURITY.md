# Security

Companion for a **running** loopback CG-Agent-Harness. It is not the harness.
It must not weaken harness I6, CSRF, or write gates.

CISA [Secure by Design](https://www.cisa.gov/securebydesign): default-deny
network, fail closed, no secrets in this tree.

## Asset → threat → control

| Asset | Threat (STRIDE) | Control | Residual |
|---|---|---|---|
| Chat POST | SSRF / spoofed origin | `LoopbackOrigin` allows only `http://127.0.0.1` and `http://[::1]`. `localhost` rejected. Credentials, query, fragment, extra path rejected. | Local process can still hit loopback. Single-operator box. |
| Chat POST | CSRF | Token from `GET /` meta tag; charset `[A-Za-z0-9_-]`, 8–128 chars. Header `X-CyClaw-CSRF`. Never logged (`Debug` redacts). | Token is in console HTML; any local process can scrape it (same as curl). |
| Chat POST | Redirect SSRF | `reqwest` `redirect::Policy::none()`. 3xx → `Redirect` error. | None for this client. |
| Chat POST | Write-gate bypass | Allowlist: `GET /`, `/api/status`, `/api/sessions`; `POST /api/sessions`, `/api/chat`. No `/api/agent/*`, no `"loop"` field. CI greps the client source. | New methods must extend the allowlist and tests. |
| Secrets | Info disclosure | Never reads `~/.CGagentHarness/.env`. No API key prompt. 401 → "use the console". | Operator may still type secrets into chat; that is harness policy. |
| Model text | XSS / URL open (CWE-1022) | `NSTextField` preview and `NSTextView` scroll pane are plain strings, not HTML. `display` strips C0/ANSI; preview caps at 400 chars and expanded text at 8,000. No `open` of URLs. | A future WebView would need a new review. |
| Backend response | Memory exhaustion (CWE-400) | Every response stream is capped at 1 MiB before text or JSON parsing; `Content-Length` is only an early rejection. | A local backend can still spend the full request timeout sending the capped prefix. |
| Home files | Planted `harness.json` | Regular file only (no symlink), ≤64 KiB, not world-writable, port 1024–65535. | Owner-writable plant still works (same uid). |
| CI | Supply chain | Actions pinned to full SHAs. `permissions: contents: read`. No `pull_request_target`. `cargo deny`. | Pin drift; Dependabot recommended. |

## Desktop sidecar discovery

The bundled `CG Agent Harness.app` binds `127.0.0.1:0`, not 8790. The avatar does **not** scan the port range. If `:8790` fails it runs argv-only `lsof` for the current user's `cgagentharness` LISTEN sockets on `127.0.0.1`, then `GET /api/status` must look like harness JSON (`model` + `api_key_optional`). Ollama `:11434` is excluded. CSRF is still taken from `GET /` on that origin. The focus Unix socket is not used.

## Direct Ollama override

Optional menu path. Same fail-closed HTTP rules as harness chat: `127.0.0.1:11434` only, no redirects, no forwarding headers, no API key, model tag is a **constant** (`qwen3.8:27b-mlx`), path allowlist `/v1/chat/completions` and `/api/tags`. The bundled Soul Markdown is sent as the first `system` message on Direct Ollama chat requests only. It is application data, not tool authority: the request has no `tools`, web-search, web-fetch, filesystem, process, or write capability. Does not call `/api/generate`, Ollama cloud APIs, or any harness write route.

## Darwin

- `LSUIElement`: menu extra, no Dock. No Accessibility / Mic / Camera entitlements.
- `NSAllowsLocalNetworking` only. Not `NSAllowsArbitraryLoads`.
- Text input uses the AppKit field editor (in-process). No CGEvent tap.
- Ad-hoc signature. Hardened Runtime / App Sandbox wait for notarize.

## Explicit non-goals

- Finding the Tauri desktop's ephemeral port
- Sending `loop: true`
- Calling agent run/jobs/push/publish
- Storing GitHub or LLM API keys
- Following HTTP redirects, even to loopback
