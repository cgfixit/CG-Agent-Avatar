# Controls

Left-click the menu-bar icon (top toolbar). Typing does **not** require Harness or Ollama; those are only needed for a model reply.

| Input | Action |
|---|---|
| **Talk** | Show the strip, focus the text field (works offline) |
| **Harness (127.0.0.1:8790)** | Optional. Tries `:8790`, then finds the desktop `.app` sidecar on `127.0.0.1`; HTTPS with a pinned certificate on fresh homes, plain HTTP on legacy homes |
| **Direct Ollama (qwen3.8:27b-mlx)** | Default. Sends the bundled Soul prompt as a system message via `http://127.0.0.1:11434/v1/chat/completions` |
| **Harness Login…** | Prompts for a harness account username and password (native secure text entry), then logs in for later Harness chats |
| **Quit CG-Agent-MacOS-Avatar** | Terminate |
| Click the creature | Same as Talk |
| Type, Return | Send. Reply appears in the bubble **above** the avatar |
| **See More** | Expand a reply into a scrollable pane; **See Less** collapses it |
| Harness/Ollama down | Field still types. Bubble says `harness asleep` / `ollama asleep` |
| Harness found, not logged in | Bubble says `login required — use Harness Login… in the menu` |
| Harness needs a password change | Bubble says `bootstrap password must be changed — use the harness console` |
| Harness certificate changed | Bubble says `harness certificate changed — verify it, then re-check trust` |
| Harness reply used web search | Bubble appends a compact `[via web ×N]` note — this app only displays that, it never fetches pages itself |

Chat is not streaming. While a turn is in flight the bubble says `…thinking`.

The overlay follows the creature but only covers the creature, bubble, and text field. Clicks elsewhere in that horizontal band go to the app underneath.

The initial reply bubble is 640×112 points. Expanded replies remain plain text and can scroll when longer than the available panel height.
