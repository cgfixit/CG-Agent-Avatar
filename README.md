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
- Not a cloud client. Loopback IPv4/IPv6 only (`127.0.0.1` / `::1`). The name
  `localhost` is rejected.

## Menu

| Item | Effect |
|---|---|
| **Talk** | Show the strip and focus the field |
| **Harness (127.0.0.1:8790)** | Optional. `POST /api/chat` with CSRF from `GET /` |
| **Direct Ollama (qwen3.8:27b-mlx)** | Default. Sends the bundled Soul prompt as a system message to `POST http://127.0.0.1:11434/v1/chat/completions` |
| **Quit CG-Agent-MacOS-Avatar** | Exit |

Click the creature for Talk. Return sends. While a turn is in flight the bubble
shows `…thinking`. If the chosen backend is down the field still types; the
bubble says `harness asleep` or `ollama asleep`.

Ollama supports web search and fetch, but this app does not expose those tools
or send an Ollama cloud API key; Direct Ollama remains local and tool-free.

## Harness port

Headless `cgagentharness serve` listens on **8790** (or `harness.json` `port`).

The bundled **CG Agent Harness.app** does **not**. Its sidecar binds
`127.0.0.1:0` (ephemeral). If `:8790` fails, this avatar looks up a
current-user `cgagentharness` LISTEN socket on `127.0.0.1` via argv-only `lsof`,
then checks `GET /api/status` looks like harness JSON. It does not scan the
port range and does not use the desktop focus socket.

You cannot run the desktop `.app` and `serve` on the same home at once (home lock).

## Build

Apple Silicon, macOS 13+.

```sh
./scripts/ci.sh
./scripts/package-app.sh
open dist/CG-Agent-MacOS-Avatar.app
```

The unsigned `.app` is **not** in git. Recipients build it. Ad-hoc `codesign`.
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
