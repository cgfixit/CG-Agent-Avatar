# CG-Agent-MacOS-Avatar

Menu-bar creature for Apple Silicon macOS. It talks to a **running** local
[CG-Agent-Harness](https://github.com/cgfixit/CG-agent-harness) and/or
[Ollama](https://ollama.com). It does not start either of them.

[![Screenshots: local AI](https://i.imgur.com/0QKJhgK.png)](https://github.com/cgfixit/cg-agent-avatar/assets/)

## What it is

- A status-item extra: left-click the icon in the menu bar.
- A tight transparent overlay around the walking avatar and chat controls; empty space passes clicks through.
- A text field (typing works even if backends are down).
- A speech bubble **above** the avatar for replies.

## What it is not

- Not CG-Agent-Harness. Not a second copy of `CG Agent Harness.app`.
- Not an agent runner. It never calls `/api/agent/*`, never sends `"loop": true`,
  never reads `~/.CGagentHarness/.env`.
- Not a cloud client. Loopback IPv4/IPv6 only (`127.0.0.1` / `::1`), whether
  plain HTTP or HTTPS. The name `localhost` is rejected.
- Not a web-search client of its own. It never calls `/api/web/*`. When
  Harness mode's `/api/chat` reply already used the harness's own web search
  (see below), this app only displays that fact — it does not fetch, render,
  or open any of the pages the harness looked at.

## Menu

| Item | Effect |
|---|---|
| **Talk** | Show the strip and focus the field |
| **Harness (127.0.0.1:8790)** | Optional. `POST /api/chat` with CSRF, over HTTPS with a pinned certificate on fresh harness homes, or plain HTTP on legacy (`tls.enabled: false`) homes |
| **Direct Ollama (qwen3.8:27b-mlx)** | Default. Sends the bundled Soul prompt as a system message to `POST http://127.0.0.1:11434/v1/chat/completions` |
| **Harness Login…** | Prompts for a harness account username and password, then logs in so Harness-mode chat can proceed on an account-gated home |
| **Harness Password Reset…** | After Harness Login reports a required bootstrap change, securely prompts for the current and replacement password for that same account |
| **Quit CG-Agent-MacOS-Avatar** | Exit |

Click the creature for Talk. Return sends. While a turn is in flight the bubble
shows `…thinking`. If a backend is down the field still types; the bubble
says `harness asleep` / `ollama asleep`, or, for a harness that answered but
needs more from you: `login required — use Harness Login… in the menu`,
`bootstrap password must be changed — use Harness Password Reset…`, or
`harness certificate changed…`.

Ollama's own web-search/web-fetch tools are a *cloud* feature (a free
ollama.com account + API key; the lookup itself leaves loopback) and this app
still does not use them — Direct Ollama stays local and tool-free.
**Harness mode is different**: cg-agent-harness has its own free, entirely
local-to-remote-fetch web search (Google via an optional SerpAPI key, or an
unauthenticated public-Google fallback) built into ordinary `POST /api/chat`
once an administrator has granted a URL pattern with `/web allow` in the
harness's own console or CLI — this app doesn't call any `/api/web/*` route
itself, it only reads the `web_tools` field the harness's reply already
carries and appends a short "via web ×N" note in the bubble when present.

Replies start in a 640×112 point bubble. **See More** expands that reply into a
scrollable plain-text pane; **See Less** returns to the bubble. This works the
same with Harness and Direct Ollama. The default Classic theme pairs fixed
dark reply text with a translucent-white background so replies stay readable
in both light and dark macOS appearance.

## Harness TLS and login

Fresh `cgagentharness` homes default to `tls.enabled: true` and
`auth.enabled: true` (see cg-agent-harness's `docs/SECURE_RESEARCH.md`) —
HTTPS-only, with a self-signed certificate, and a required account login.
This app follows that instead of only ever speaking plain HTTP:

- **Certificate**: this app never asks you to trust a certificate, never
  changes system/keychain trust, and never disables certificate validation.
  It reads the harness's own already-generated leaf certificate from its
  home directory (`~/.CGagentHarness/tls/server.pem` by default — the exact
  file `cgagentharness tls certificate` exports) and pins that one
  certificate as the only trust root for that connection. If the harness
  ever rotates its certificate, the bubble reports it distinctly
  (`harness certificate changed…`) instead of looking "asleep".
- **Login**: use **Harness Login…** and enter the account you set up in the
  harness's own console (default `admin` / `admin`, which the harness forces
  you to replace on first use). If that replacement is required, use
  **Harness Password Reset…** to change only that authenticated account's
  password. This app cannot administer accounts or change other users.
- A legacy home with `auth.enabled: false` / `tls.enabled: false` still
  works exactly as before, over plain HTTP, no login needed.

For an account-gated home:

1. Select **Harness (127.0.0.1:8790)** in the menu. Login alone does not switch
   the selected chat backend.
2. Choose **Harness Login…**, enter the username and password, and wait for
   the login result in the bubble.
3. If a bootstrap replacement is required, choose **Harness Password Reset…**.
   Enter the current password, then a new password of at least 12 characters.
   **OK** in the new-password prompt submits the change; **Cancel** leaves it
   unchanged. Wait for `password changed — ready to chat` before sending a message.

The reset requires the current password and an authenticated session; it is
not forgotten-password recovery. Use the Harness's own recovery tools if the
current password is unknown. Credentials and session cookies are not saved
to disk by Avatar, so log in again after restarting the app. Guarded requests
fetch their CSRF token from the Harness console after login and password change.

## Harness port

Headless `cgagentharness serve` listens on **8790** (or `harness.json` `port`).

The bundled **CG Agent Harness.app** does **not**. Its sidecar binds
`127.0.0.1:0` (ephemeral). If `:8790` fails, this avatar looks up a
`cgagentharness` LISTEN socket on `127.0.0.1` via argv-only `lsof`,
then checks `GET /api/status` looks like harness JSON (including the thin,
pre-login shape a fresh home answers with). It does not scan the port range
and does not use the desktop focus socket.

You cannot run the desktop `.app` and `serve` on the same home at once (home lock).

## Build

Apple Silicon, macOS 13+.

```sh
./scripts/ci.sh
./scripts/package-app.sh
open dist/CG-Agent-MacOS-Avatar.app
```

The locally built `.app` is **not** in git. Recipients build it. Ad-hoc `codesign`.
Gatekeeper may require Open anyway.

## CI artifacts and nightlies

Merges to `main` upload `CG-Agent-MacOS-Avatar.zip` as a workflow artifact
(Actions → bundle, 14-day retention).

A GitHub Release is cut daily at 12:00 America/New_York (`nightly-YYYY-MM-DD`)
and can be run by hand (Actions → bundle → Run workflow). Nightlies are
prereleases, ad-hoc signed, not notarized. Duplicate daily tags are skipped
when there are no new commits.

## Docs

- [docs/BUILD.md](docs/BUILD.md)
- [docs/CONTROLS.md](docs/CONTROLS.md)
- [SECURITY.md](SECURITY.md)

## License

MIT. See [LICENSE](LICENSE).
