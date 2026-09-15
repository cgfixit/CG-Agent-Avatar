# Controls

Left-click the menu-bar icon (top toolbar). Typing does **not** require Harness or Ollama; those are only needed for a model reply.

| Input | Action |
|---|---|
| **Talk** | Show the strip, focus the text field (works offline) |
| **Harness (127.0.0.1:8790)** | Optional. Tries `:8790`, then finds the desktop `.app` sidecar on `127.0.0.1` |
| **Direct Ollama (qwen3.8:27b-mlx)** | Default. Sends the bundled Soul prompt as a system message via `http://127.0.0.1:11434/v1/chat/completions` |
| **Quit CG-Agent-MacOS-Avatar** | Terminate |
| Click the creature | Same as Talk |
| Type, Return | Send. Reply appears in the bubble **above** the avatar |
| **See More** | Expand a reply into a scrollable pane; **See Less** collapses it |
| Harness/Ollama down | Field still types. Bubble says `harness asleep` / `ollama asleep` |

Chat is not streaming. While a turn is in flight the bubble says `…thinking`.

The overlay follows the creature but only covers the creature, bubble, and text field. Clicks elsewhere in that horizontal band go to the app underneath.

The initial reply bubble is 640×112 points. Expanded replies remain plain text and can scroll when longer than the available panel height.
