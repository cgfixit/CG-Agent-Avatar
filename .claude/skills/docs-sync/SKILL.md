---
name: docs-sync
description: Bring every doc in this repo (README.md, SECURITY.md, docs/BUILD.md, docs/CONTROLS.md, AGENTS.md, CLAUDE.md, and the .claude/skills playbooks) back in line with the current source, workflows, and toolchain. It rewrites stale statements in place instead of appending changelog-style notes. Manual only; run it after a feature/CI change or before a release.
disable-model-invocation: true
argument-hint: "[scope: all | readme | security | build | controls | agents | skills] [--check]"
allowed-tools: Read Grep Glob Bash(git log:*) Bash(git diff:*) Bash(git show:*) Bash(git ls-files:*) Bash(cargo test --locked:*)
---

# Docs sync

Docs here are part of the security posture. `SECURITY.md` is the threat model that
reviews are checked against, and the skills are playbooks that agents execute
literally. A stale doc is a bug. This skill makes the docs describe **what the code
does now**.

`$ARGUMENTS`: a scope (default `all`), plus an optional `--check`. With `--check`,
report the drift and don't edit anything.

## Rules

1. **Source wins.** When a doc and the code disagree, the code is the truth. The
   exception is when the code violates a documented `SECURITY.md` control. Then
   **stop and report it**; that's a regression for `check-security-invariants`, not
   a doc edit.
2. **Rewrite, don't append.** Fix the sentence or row that is wrong, where it is.
   No "Update:" paragraphs, dated notes, or "as of" asides. Git history is the
   changelog.
3. **One owner per fact.** Each fact has a home doc (see the map below). Other docs
   should link to that home instead of restating the fact. When the same fact is
   duplicated and has drifted, collapse it to one copy plus links.
4. **Quote real identifiers.** Test names, constants, env vars, menu titles, and
   bubble strings must be copied from source, not paraphrased from memory.
5. **Don't invent.** If you can't verify something from the tree (for example, an
   external app's behavior or a release date), keep the existing wording and list
   it under "unverified" in the report.

## Fact → home doc map

| Fact | Home | Verify against |
|---|---|---|
| User-facing behavior, backends, setup | `README.md` | `src/app.rs`, `src/ollama.rs`, `src/launch.rs` |
| Menu items, bubble strings, themes, troubleshooting | `docs/CONTROLS.md` | `app.rs` `ns_string!`/`last_reply` strings, `theme.rs`, `display.rs` limits |
| Toolchain, CI matrix, audit tool versions, packaging | `docs/BUILD.md` | `rust-toolchain.toml`, `.github/workflows/*.yml`, `scripts/*.sh` |
| Routes, controls, residual risk | `SECURITY.md` | `paths.rs`, `origin.rs`, `client.rs`, `http.rs` (`MAX_BODY`), `home.rs`, `ollama.rs`, `resources/Info.plist` |
| Agent rules, architecture, contract-test map, skills index | `AGENTS.md` | `src/lib.rs` module list, `tests/*.rs` fn names, `.claude/skills/*/SKILL.md` frontmatter |
| Claude-only wiring | `CLAUDE.md` | `.claude/settings.json`, `.claude/hooks/*` |
| Each skill's claims | that `SKILL.md` | whatever files the skill cites |

## Procedure

1. **Diff scope.** Run `git log --oneline -20` and `git diff origin/main...HEAD --stat`
   (or the last release tag) to see what changed. Changed areas get a line-by-line
   check; the rest get a spot check.
2. **Extract ground truth** (grep; don't read everything):
   - constants: `grep -n "pub const\|^const" src/*.rs`;
   - allowlists: `paths.rs` `ALLOWED_GET`/`ALLOWED_POST`/`FORBIDDEN`, and `ollama.rs` `ALLOWED`;
   - env vars: `grep -n "env::var" src/*.rs`;
   - user-visible strings: `grep -n 'ns_string!\|last_reply.lock' src/app.rs`;
   - tests: `grep -n "^fn " tests/*.rs`, and the `#[test]` names in `src/*.rs`;
   - CI: `uses:` pins, `toolchain:` values, installed tool versions in `.github/workflows/*.yml`;
   - skills: frontmatter `name`, `description`, `disable-model-invocation` for each `SKILL.md`.
3. **Check each doc against the map.** For every mismatch, record
   `doc:line — says X, source says Y (file:line)`.
4. **Edit** (skip this step with `--check`). Apply the rules above. Keep each doc's
   existing voice and structure. Fix tables in place.
5. **Cross-links.** Make sure the README "Documentation" list, the AGENTS.md skills
   table, and the CLAUDE.md pointer all cover every doc and every skill that exists,
   and nothing that doesn't.
6. **Validate.** Docs have no build step, and no test reads the Markdown docs.
   Contract tests do `include_str!` workflows, scripts, `Info.plist`, and source
   files, so run them whenever a doc fix also required touching one of those. Also
   run them to confirm that the doc's claims about those files still hold:

   ```sh
   cargo test --locked --test source_contracts --test ci_workflows --test plist_contract
   ```

## Report format

```
## Docs sync — HEAD <sha>, scope <scope>[, check-only]
Fixed (or would fix): <n>
- README.md:42 — model tag said X; ollama.rs:13 says Y
- ...
Collapsed duplicates: <fact → now owned by doc>
Unverified (left as is): <statement — why it can't be checked from the tree>
Stopped on: <any code-vs-SECURITY.md regression, handed to /check-security-invariants>
```

Suggested commit subject: `docs: sync <scope> with current source`.
