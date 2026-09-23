---
name: ollama-doctor
description: Diagnose the Direct Ollama backend end to end - the fixed model tag, the /api/tags readiness check versus what chat actually requests, the bundled Soul system prompt versus a tool-free model, loopback/port rules, and (on a Mac with Ollama running) a live loopback probe - and explain "ollama asleep", "ollama http 404", or "pull <model>" symptoms. Manual only; diagnoses and reports, never edits.
disable-model-invocation: true
argument-hint: "[symptom, e.g. \"ollama http 404\" | \"replies ignore persona\" | audit]"
allowed-tools: Read Grep Glob Bash(git log:*) Bash(cargo test --locked ollama:*) Bash(curl -sS --max-time 5 http://127.0.0.1:11434/api/tags) Bash(uname:*)
---

# Ollama doctor

Direct Ollama is the **default** backend. It is also the one users misconfigure
most often, because the app deliberately gives them no settings for it. This skill
explains what the code actually does, checks it against the docs, and, when
possible, compares it with the live local service. It reports only; fixes go
through a normal PR (`add-avatar-feature` / `check-security-invariants`).

`$ARGUMENTS` is either a user-reported symptom or `audit` (the default).

## 1. Ground truth from source (read it; don't recall it)

Read `src/ollama.rs` top to bottom and record:
- `PORT`, `MODEL`, `POST_CHAT`, `GET_TAGS`, `POST_WEB_SEARCH`, `POST_WEB_FETCH`,
  `ALLOWED`, and the timeouts (including `WEB_TIMEOUT`);
- `tags_ok`: exactly which model names it accepts (exact match? prefix?);
- `chat_body`: the exact `model` it sends, `stream`, and the message array
  (system prompt + current message only, no history);
- the `OllamaError` variants and their `Display` strings; note any variant that
  nothing constructs.

Then grep `src/app.rs` for `tags_ok`, `OllamaError::`, and `11434` to see how each
error becomes user-facing text, and check that `discover.rs` excludes `OLLAMA_PORT`.

## 2. Consistency checks (each one is pass/fail with `file:line`)

| Check | Where |
|---|---|
| The model tag is identical everywhere | `ollama.rs`, `app.rs` strings, `README.md`, `docs/CONTROLS.md`, `docs/BUILD.md`, `SECURITY.md`, `tests/source_contracts.rs` |
| The readiness check agrees with what chat sends | If `tags_ok` accepts a tag that `chat_body` won't request, "ready" can still end in `ollama http 404` |
| Only `/v1/chat/completions`, `/api/tags`, and `/api/experimental/web_search`/`web_fetch` are reachable; never `/api/generate`, `/api/chat`, or `/api/pull` | `ALLOWED`, `ollama_relay_is_loopback_openai_compat_only`, `ollama_web_lookups_stay_on_the_loopback_daemon` |
| No `tools`, `loop`, API key, proxy, or redirect-following | `chat_body` test `body_has_fixed_model_no_loop_no_tools`, the `.no_proxy()` and `redirect::Policy::none()` builder calls |
| Every response is bounded by `MAX_BODY` | `response_bytes` → `http::read_bounded` |

Run the module's own tests; they are fast and portable:

```sh
cargo test --locked ollama
```

## 3. System prompt fitness

`resources/direct-ollama-system.md` is compiled in with `include_str!` and sent as
the only `system` message to a model that has **no tools and no history**. The only
outside text it ever sees is a `<web_results>`/`<web_page>` block the app adds for
an explicit lookup. Read the prompt and flag every instruction the model cannot
follow in that setup:
- reading or browsing files (`SOUL.md`, `STYLE.md`, `examples/`, `data/`);
- writing or appending anything (such as a `MEMORY.md` log);
- personas, vocabularies, or examples it points to that aren't actually included
  in the prompt text.

Every instruction like that wastes context. It can also make the model hallucinate
file contents or apologize for missing files. Report them; don't rewrite the
prompt here. The prompt's content belongs to the owner, and changing it is a
product decision.

## 4. Live probe (optional; only on macOS with the service running)

Only if `uname` reports `Darwin` and the user agrees:

```sh
curl -sS --max-time 5 http://127.0.0.1:11434/api/tags
```

Compare the listed `name` values with `MODEL`, character for character. Never
probe any host other than literal `127.0.0.1:11434`, never call `/api/pull`,
`/api/generate`, or any write route, and never start or install Ollama. The app
itself never does these things, and this diagnosis doesn't either.

## 5. Symptom map

Bubble strings come from the chat worker in `app.rs` (grep `BACKEND_OLLAMA`). Check
them again before quoting, because they change.

| What the user sees | Most likely cause | Confirm by |
|---|---|---|
| `ollama asleep` | Nothing listening on `127.0.0.1:11434`, a connect timeout (2s), or a service bound only to `localhost`/IPv6 | Live probe; `lsof -nP -iTCP:11434 -sTCP:LISTEN` on the Mac |
| `ollama: ollama http 404` | Model tag not installed under the **exact** name `chat_body` sends, or an old Ollama without the `/v1` API | Live probe tag list compared with `MODEL`; the Ollama version |
| `pull <model>` | Chat got a 404 and `/api/tags` confirms the exact tag is absent (`OllamaError::ModelMissing`) | Live probe tag list compared with `MODEL` |
| `ollama: web lookup unavailable (http N)` | 404: Ollama older than 0.18.1. 401/403: not signed in (`ollama signin`) or cloud features disabled | `ollama --version`; the Ollama app's sign-in state |
| `ollama: can't read that link…` | `web_intent` refused a private, `.local`, single-label, or credentialed link | Expected; only public http(s) pages |
| Creature looks unwell but chat works | `tags_ok` returned `Ok(false)` (the status loop), or chat succeeded despite a tag the readiness rule rejects | Compare `tags_ok` matching with the live tag list |
| `ollama: response too large` | Reply over 1 MiB (`MAX_BODY`) | Expected behavior; this is not a bug |
| Replies ignore the persona or mention missing files | The system prompt asks for tool actions (§3) | Read the prompt |

## Report format

```
## Ollama doctor — HEAD <sha>, mode: <audit | symptom>
Ground truth: model=<tag> port=<n> paths=<list> readiness=<exact|prefix>
Checks: <n pass / n fail>, each fail with file:line and one-line impact
Prompt fitness: <tool-dependent instructions found, quoted briefly>
Live probe: <skipped (reason) | installed tags …>
Likely cause of "<symptom>": <answer + confidence>
Suggested PRs: <0-2 focused fixes; hand off to /pr-opportunity-scan format>
```
