# CG-Agent-MacOS-Avatar

**A little creature. A conversation within reach.**

Avatar brings your local AI onto your desktop: a walking companion for **Apple
Silicon macOS 13+**, written in Rust. Click the creature, ask a question, and read
its answer right above it. Its moods show when it is resting, thinking, talking,
or needs attention.

- **Stay in the flow.** Open a stationary chat panel, send with Return or **Send**,
  and close it without losing your draft or reply.
- **Room to read.** Padded replies, larger text, and a separate toolbar keep the
  conversation clear; **See More** opens a scrollable plain-text pane.
- **Choose your companion's engine.** Start with Direct Ollama or connect to
  [CG-Agent-Harness](https://github.com/cgfixit/CG-Agent-Harness).

![Classic theme showing a reply above the chat composer](docs/screenshots/classic-reply.jpg)

*Native macOS app. [Screenshot provenance](docs/screenshots/README.md).*

## Get started

Build with Rust installed through rustup:

```sh
./scripts/package-app.sh
open dist/CG-Agent-MacOS-Avatar.app
```

The repository selects Rust 1.88. The app is ad-hoc signed and **not notarized**;
macOS may require **Open Anyway**. [Build and verification](docs/BUILD.md) covers
prerequisites, toolchain setup, and package validation.

Download builds from [Releases](https://github.com/cgfixit/CG-Agent-Avatar/releases).
Merges to `main` also produce an Actions → **bundle** artifact containing
`CG-Agent-MacOS-Avatar.zip`, retained for 14 days. Scheduled builds publish a
prerelease at noon America/New_York when commits have changed. Manual runs from
`main` publish a full release, optionally marked Latest; other branches produce
prereleases. Downloaded builds are also ad-hoc signed, not notarized.

## Choose a backend

| Backend | Setup and behavior |
|---|---|
| **Direct Ollama (default)** | Run a local service at `http://127.0.0.1:11434` exposing `/api/tags` and `/v1/chat/completions`, with the exact model tag `qwen3.8:27b-mlx` available. Avatar sends the bundled [Soul prompt](resources/direct-ollama-system.md) and your current message, asking for a direct answer without a hidden reasoning pass. It never starts Ollama, installs models, or sends previous turns. |
| **Harness** | Select **Harness (127.0.0.1:8790)** from the menu. Avatar opens the installed Harness desktop app and discovers its listener. A headless `cgagentharness serve` instance also works. Harness manages its model and conversation session; Avatar reuses or creates a session titled `CG-Agent`. |

Ollama has its own [macOS requirements](https://docs.ollama.com/macos), including
macOS 14 or newer. A responding daemon does not guarantee the fixed model tag is
available. The chat panel identifies the selected backend; Harness setup appears
in the reply area when needed.

### Web lookups (Direct Ollama)

Ask explicitly when you want information from the web:

- "search the web for best pizza in Atlanta", "look up …", or "google …"
- "read https://…", or "summarize https://…"

Avatar requests one search (up to five results) or one page through the local
Ollama service, passes the response to the model as untrusted reference text,
and marks the answer `[via web ×1]`. Other messages stay local; bare `search …`
does not trigger a lookup. See [all supported phrases](docs/CONTROLS.md#web-lookups-direct-ollama).

Lookups require a daemon supporting Ollama's experimental web routes, signed in
with `ollama signin`, with cloud features enabled. **The query or link leaves
your Mac through Ollama's cloud service.** Avatar connects only to loopback and
holds no API key. Private hosts, link-local addresses, `.local` names, single-word
hosts, and links with embedded credentials are refused.

## Talk, read, carry on

Click the creature or choose **Talk** from its menu-bar menu. The panel stays in
place while you type and read. **Return** or **Send** submits your message;
`…thinking` remains until the non-streaming request completes. You can prepare a
draft even when the backend is unavailable.

![Expanded native reply pane with a separate reply toolbar](docs/screenshots/expanded-reply.jpg)

**See More** expands the reply within the screen; **See Less** restores the
compact bubble. Longer replies scroll. **Close** hides the conversation and lets
the creature roam again; click it to return to your draft and reply. Empty overlay
space passes clicks through to the application underneath.

Both backends share the renderer: previews show up to 400 characters and expanded
replies up to 8,000, with an ellipsis at the limit. Input accepts 32,768 characters
after trimming, which is a character limit, not a model-context guarantee.

Classic pairs dark text with a translucent white surface. The optional Fable
Protocol theme uses warm text on a dark surface and a calmer gait.
[Controls and themes](docs/CONTROLS.md) covers theme selection, menu actions, and
troubleshooting messages.

## Harness TLS and login

Avatar reads the selected Harness home's port and leaf certificate. The default
is `~/.CGagentHarness`; `CGAGENTHARNESS_HOME` may select another absolute path
without `..` components. Avatar never reads `.env`.

For TLS homes, Avatar pins `tls/server.pem` without changing Keychain trust or
disabling certificate validation. Keep both apps pointed at the same home.
Legacy HTTP homes remain supported; a home with a certificate never falls back
to plain HTTP.

For an account-gated home:

1. Select **Harness (127.0.0.1:8790)**. Avatar opens Harness and waits for a
   confirmed loopback status response before showing the next step.
2. When prompted, choose **Harness Login…**, enter your Harness account, and
   wait for the result. Login alone does not select the backend.
3. If a bootstrap password change is required, choose **Harness Password
   Reset…**. Enter the current password and a new one of at least 12 characters.
   **OK** submits; **Cancel** leaves it unchanged. Avatar checks status again
   before enabling chat.

Configure missing models, providers, or keys in Harness, then wait for the next
status check. Drafts stay in the field while setup is incomplete. Password Reset
changes only the authenticated account; it is not forgotten-password recovery.
Credentials and cookies stay in memory, so restart requires another login.

### Harness port and launch

Headless serving normally uses **8790**, or the port in `harness.json`; the desktop
sidecar uses an ephemeral port. Avatar probes the configured port, discovers
candidates with argv-only `lsof`, and validates `GET /api/status`. It never scans
port ranges or uses the desktop focus socket. Do not run desktop and headless
Harness against the same home.

Launch Services opens bundle ID `com.cgfixit.agent-harness`, activating an existing
instance. An unregistered app produces installation or headless-serving guidance.
Avatar never spawns Harness, passes launch arguments, or hardcodes an install path.

## Clear boundaries

Avatar stays a client: it never runs agent jobs or administers Harness accounts.

- Avatar connects only to literal loopback addresses (`127.0.0.1` and `::1`),
  rejecting `localhost` names and HTTP redirects.
- Direct Ollama receives no tool definitions, API keys, or agent jobs. Avatar
  never fetches web pages itself.
- Harness may use configured providers and tools beyond your Mac. A reported
  `[via web ×N]` note counts its web-tool calls; loopback connectivity does not
  mean those capabilities operate offline.
- Replies stay plain text: Avatar never executes HTML or opens reply URLs.

[SECURITY.md](SECURITY.md) details route allowlists, TLS trust, response limits,
credential handling, and residual risks.

## Contribute and explore

[AGENTS.md](AGENTS.md) is the shared contributor contract for architecture,
security, verification, and repo skills. Run changed code with meaningful inputs;
prefer focused linting and let GitHub Actions cover broad regression checks.
Claude Code imports the guide through [CLAUDE.md](CLAUDE.md); Codex reads it directly.

- [Build and verification](docs/BUILD.md)
- [Controls, themes, and troubleshooting](docs/CONTROLS.md)
- [Native screenshot provenance](docs/screenshots/README.md)

MIT licensed. See [LICENSE](LICENSE).
