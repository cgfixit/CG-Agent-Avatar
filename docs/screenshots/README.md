# Native screenshots

These JPEGs replace the README's earlier screenshots. They are unedited
app-window captures taken through Computer Use (`@oai/sky`), with no desktop,
account details, credentials, or unrelated windows included.

| Property | Capture |
|---|---|
| Date | September 27, 2026 |
| Source revision | `a8fa9bcfd3aa4c4cdcc1e253d63d9b6e392a92db` (main) plus the reply-button contrast change in this branch |
| Platform | Apple Silicon, macOS 27.0 (26A428) |
| Build | `./scripts/package-app.sh`, Rust 1.88 toolchain, ad-hoc signed |
| Executable SHA-256 | `6ed614e4e598dbc80ec02df057daa0e50c324adafeb4e867ed9254645bb8fec5` |
| Theme/backend | Classic / live Direct Ollama, fixed model tag `qwen3.8:27b-mlx` |
| Harness home | Empty, isolated temporary directory |
| Prompts | A five-step coding checklist in the compact capture; an eight-step debugging checklist in the expanded capture |

- [`classic-reply.jpg`](classic-reply.jpg): completed five-step reply in the compact bubble.
- [`expanded-reply.jpg`](expanded-reply.jpg): an eight-step reply after **See More**.

The reply is live model output, so wording varies between runs. These images
show native rendering and a working local chat request; they do not establish
Harness authentication, remote-provider behavior, notarization, or support for
every macOS version.

## Reproduce a capture

1. Run the [local checks and package validation](../BUILD.md), and record the
   source revision and executable hash before launching.
2. Launch that exact `.app` with an isolated `CGAGENTHARNESS_HOME`. Use public demo
   text and the intended backend. Avoid screenshots of login/password dialogs.
3. Click the creature, enter the prompt, verify the text field, and press Return.
   Confirm the thinking state and completed reply.
4. Capture only the app window. Expand with **See More**, capture the second
   view, then check **See Less** restores the preview. For a longer reply,
   also verify scrolling and text selection after several animation frames.
5. Review the pixels for legibility and private content, copy the original captures
   into this directory, and update the provenance table and README captions.

Do not describe fixture responses as live-provider results. Screenshots and
native interaction checks complement the automated tests; neither proves the
other passed.
