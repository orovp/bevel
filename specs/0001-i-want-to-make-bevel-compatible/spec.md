---
id: '0001'
title: Render the method for opencode, not only for Claude Code
status: implementing
schema_version: 1
created: 2026-08-05
packages:
- bevel
acceptance:
- tier: A
  test: opencode_subagents_land_where_opencode_reads_them
- tier: A
  test: a_subagents_tool_set_survives_translation_into_opencode_permissions
- tier: A
  test: the_claude_only_model_shorthand_is_dropped_rather_than_translated
- tier: A
  test: the_project_notes_are_printed_for_a_human_to_apply
- tier: A
  test: sync_never_writes_or_moves_the_projects_instruction_files
- tier: A
  test: the_always_loaded_context_budget_follows_the_body_to_agents_md
- tier: A
  test: pruning_for_one_agent_never_reaches_another_agents_directory
- tier: A
  test: a_second_sync_for_both_agents_skips_everything_as_unchanged
- tier: B
  cmd: cargo test --workspace
- tier: B
  cmd: cargo clippy --all-targets -- -D warnings
- tier: C
  text: DESIGN.md §9's tier table is replaced with the four-tier table in this spec, and §15's Phase 4 no longer promises Codex/Cursor adapters
- tier: C
  text: README.md describes only the agents and flags that exist, with no `--agent cursor,codex` example
---
# Render the method for opencode, not only for Claude Code

> From the inbox: I want to make Bevel compatible with agents other than Claude
> Code. We can start with Open Code.

## Problem

`bevel sync` installs the method for one agent. A user running opencode gets the
CLI and the artifacts on disk — enough to run the pipeline, and short of what
the pipeline is for.

Three things are wrong today, and they are not the same thing:

1. **The seven subagents are invisible to opencode.** It scans
   `~/.config/opencode/agents/` and `.opencode/agents/`, with no fallback to
   `.claude/agents`. Context isolation — three scouts that must not pollute the
   interview, two reviewers that must not have written what they review — is the
   part of the method that most needs a second process, and it is exactly the
   part opencode cannot see.

2. **`README.md:24` promises a flag that does not exist.** It advertises
   `bevel sync --agent cursor,codex --hooks` and a tier 0/1/2 model. `SyncArgs`
   (`src/main.rs:172`) carries `--hooks` and nothing else. The feature was real
   once and `dc3bbef` removed it; the prose was never updated.

3. **A project's instructions are written to the file named after one agent.**
   `sync` generates `CLAUDE.md` and no `AGENTS.md` (`src/sync.rs:531`), while
   this very repository does the opposite — body in `AGENTS.md`, `CLAUDE.md` a
   two-line pointer "so the two cannot drift apart".

What is *not* wrong: skills. opencode scans `~/.claude/skills/<name>/SKILL.md`
among its global skill paths, so the five skills `bevel sync` already installs
load in opencode untouched. This spec must not touch them — but it must not
assume them silently either; see *The skills claim* below.

## Appetite

Days, not weeks. One agent added, one resource kind genuinely rendered, one
naming migration.

The migration is **not** separable from the rest, despite looking it:
`src/lifecycle.rs:107` blocks `bevel close` while any `acceptance: 0001 pending`
marker survives anywhere under the repo root, so deferring three of the eight
tier A criteria would mean either never closing or editing the acceptance list
and reopening the frozen hash. If the migration has to move, it moves by
superseding this spec (`src/validate.rs:220-272`), not by quietly leaving
criteria unmet.

## Solution shape

**A trait per resource kind, implemented once per agent.** The kinds are the
ones already present in `sync.rs`, and they differ enough per agent to be worth
naming separately:

| Resource | Claude Code | opencode |
|---|---|---|
| Skills | `~/.claude/skills/<n>/SKILL.md` | *the same path* — already read, not re-rendered |
| Subagents | `~/.claude/agents/<n>.md` | `~/.config/opencode/agents/<n>.md`, frontmatter translated |
| Project instructions | pointer at `AGENTS.md` | `AGENTS.md` |
| Hooks | `.claude/settings.json` | not rendered — see No-gos |
| Approve deny rule | `.claude/settings.json` | not rendered — see No-gos |

Skills sharing a path is a finding, not a coincidence to design around: the
opencode renderer declares that kind already satisfied rather than writing a
second copy into `~/.config/opencode/skills/`. Two copies of one SKILL.md is
precisely the drift this design exists to prevent.

### The subagent frontmatter mapping

The only real translation in the spec, so it is written out rather than
described.

| ours | opencode | note |
|---|---|---|
| `name:` | *the filename* | opencode derives the id from the file |
| `description:` | `description:` | unchanged, required by both |
| — | `mode: subagent` | added; these are never primary agents |
| `tools:` | `permissions:` | see below |
| `model: opus` \| `fable` | *dropped* | Claude shorthands, not opencode model ids |

**`permissions` is an allow-list closed by a terminal deny**, never a permissive
default with explicit denies bolted on. The two differ exactly on the property
the criterion exists to prove: a subagent that gained a tool nobody granted is
the failure, and only the closed form makes that impossible.

**The seven are not uniform.** There are four tool sets, and a blanket
"read-only" translation corrupts two of them:

| tool set | subagents |
|---|---|
| `Read, Grep, Glob` | spec-critic, unknowns-scout |
| `Read, Grep, Glob, Bash` | domain-scout, risk-scout, diff-reviewer |
| `Read, Grep, Glob, Bash, WebFetch, Write` | context-packer |
| `Read, Write` | mockup-builder |

Worked example — `domain-scout.md`, whose `tools: Read, Grep, Glob, Bash`
becomes:

```yaml
---
# generated by bevel — requires opencode v2 or newer
description: Blind spot pass, domain half. Finds what the codebase already does…
mode: subagent
permissions:
  - { action: read,  resource: "**", effect: allow }
  - { action: grep,  resource: "**", effect: allow }
  - { action: glob,  resource: "**", effect: allow }
  - { action: bash,  resource: "*",  effect: allow }
  - { action: "*",   resource: "*",  effect: deny }
---
```

The implementer confirms opencode v2's exact action names against its schema
before writing them; the shape above is the contract, the spellings are not.

### The skills claim, and what happens if it is wrong

The scope reduction rests on opencode reading `~/.claude/skills/`. If that is
wrong or version-dependent, the failure is silent in the worst way: sync reports
success, subagents and `AGENTS.md` land correctly, and the two skills that *are*
the pipeline are invisible.

So it must not be assumed silently. **`bevel doctor` reports, per detected
agent, where each resource kind resolves** — the `method_sources`/`pick` shape
at `src/sync.rs:572-609` already does exactly this for layers. If skills turn
out not to resolve for opencode, the remedy is a follow-up spec rendering into
`~/.config/opencode/skills/`; it is explicitly not an acceptable silent
degradation to tier 0.

### Selection

`--agent <list>`, overriding home-directory detection (`.claude`,
`.config/opencode`), defaulting to Claude Code when nothing is detected.
Detection is **additive**: both present means both rendered.

An unknown or unsupported name — `--agent cursor` — is an **error**, not a
silent no-op. Accepting it and writing nothing reproduces `dc3bbef`'s
`Agent::Codex | Agent::Opencode => {}`, which is the exact mistake this spec
exists to correct. `README.md:24`'s example changes accordingly.

**Scope stays per machine.** The method is the same text in every project
(`src/sync.rs:112-116`); a second agent does not change that reasoning.

### The instructions are printed, not installed

*Amended after approval — see "Amendment" below.*

`sync` writes neither `AGENTS.md` nor `CLAUDE.md`. `bevel notes` prints the
markdown to stdout and the user redirects it if they want it:

```
bevel notes > AGENTS.md
bevel notes claude > CLAUDE.md
```

This is not a smaller version of the migration it replaces; it is the reason the
migration is unnecessary. `CLAUDE.md` had no generated marker, so classifying it
required a byte comparison against a frozen copy of what bevel used to write —
which had to stay frozen, which froze the seed with it. Getting the comparison
wrong meant relocating a user's own prose. A `CLAUDE.md` symlinked at `AGENTS.md`
had to be detected before it was followed. All three were correct answers to
"who owns this file?", a question that only exists while bevel writes it.

Stdout carries markdown and nothing else, so the redirect is the whole workflow.
`bevel sync` names the command on every run, since that is where someone who
expected the old behaviour finds out about the new one.

### The context budget must follow the body

`src/context.rs:144-152` hardcodes `CLAUDE.md` as the only `Load::Always` item
at a 50-line budget, and the loop beneath it walks `project.config.packages`
only, so a root `AGENTS.md` is never audited. Putting the body there without
moving the measurement retires the check DESIGN.md §13 calls "the piece most
likely to save this project long-term" — silently, and while every test still
passes. Both files are audited instead.

The amendment makes this *more* load-bearing, not less: bevel no longer authors
either file, so nothing upstream constrains its length and this audit is the
only thing that ever counts the lines.

### The replacement for DESIGN.md §9

The current tier table describes commands rendered per adapter, which bevel does
for nobody. After this spec:

| Tier | What it is | Agents |
|---|---|---|
| 0 — Universal | `AGENTS.md` + file artifacts + the `bevel` CLI | Any |
| 1 — Method loaded | the skills load as skills | Claude Code, opencode — one tree, two readers |
| 2 — Context isolation | the seven subagents | Claude Code, opencode — rendered per agent |
| 3 — Hooks | format-on-write, session start, stop | Claude Code |

## No-gos

- **Hooks are not rendered for opencode.** They would be JavaScript in
  `~/.config/opencode/plugins/`, making a Rust binary the author and maintainer
  of JS source, and all three are non-blocking conveniences. Out of scope, not
  deferred to later in this spec.
- **No slash commands are rendered for any agent.** DESIGN.md §9's current tier
  1 implies opencode's `commands/` directory is in scope. It is not: the skills
  already carry `shape` and `implement`, and a command file would be a third
  copy of the same instruction.
- **Nothing is written to project-local `.opencode/`.** The machine-scope
  argument is about the method being one text; this makes it a prohibition, so
  the opencode side gets the assertion the Claude side already has at
  `src/sync.rs:667-668`.
- **The `bevel approve` deny rule is not rendered for opencode.** The gate is
  `stdin().is_terminal()` (`src/gate.rs:194`) and the only call site passes
  `assume_yes: false` (`src/main.rs:1065`). The deny rule stops an agent trying;
  it is not what stops it succeeding.
- **No third agent.** Not Cursor, Codex or Gemini, however tempting once a trait
  exists. The trait earns its shape from two real cases; a third added blind is
  the speculative surface `dc3bbef` already removed once.
- **Skills are not re-rendered into opencode's own directory.**
- **No opencode in CI**, and no test asserting that opencode loaded anything.
  Tests assert placement and parseable frontmatter.
- **opencode v1 is not supported.** Pinned to v2; see Rabbit holes.

## Rabbit holes

- **`agents/` vs `agent/`.** opencode's current convention is plural, with
  singular kept for backwards compatibility. Write plural. Getting this wrong
  produces the one failure with no symptom, which `src/paths.rs:49-53` already
  names: a file installed where nothing reads it looks like success.
- **The v1 silent degradation.** On opencode v1 the `permissions` array is not
  understood and the seven subagents load with unrestricted tools — a read-only
  reviewer able to edit what it reviews. Accepted, with two cheap guards: the
  YAML comment shown in the worked example above, and `bevel doctor` reporting
  the pin.
- **Pruning across two trees.** `prune_skill` deletes, and it will now run
  against a directory holding agents the user wrote themselves. It must stay
  inside what this sync installed and never cross from one agent's tree into
  another's.
- **The rename precedent.** `LEGACY_DENY_RULE` (`src/sync.rs:68`) exists because
  a rename once left both generations installed side by side. The `CLAUDE.md` →
  `AGENTS.md` move is the same hazard: finish it, or two files disagree and
  neither is obviously the stale one.
- **Idempotence has two green states and only one is right.** A second sync
  reporting "nothing changed" is also what happens when everything is skipped as
  hand-edited. The criterion asserts the *reason* is `unchanged`, because the
  weaker assertion would go green on precisely the migration bug this spec was
  written to fix.
- **Tests that break on the move.** `src/context.rs:594`
  (`an_oversized_agents_file_is_caught`), `src/context.rs:522`, `src/sync.rs:707`
  (`claude_md_carries_the_runnable_pipeline_and_stays_in_budget`) and the help
  string at `src/main.rs:514` all encode `CLAUDE.md` as the body. They are
  corrected, not loosened.

## Amendment — 2026-08-06, after approval

**`bevel sync` no longer writes `AGENTS.md` or `CLAUDE.md`. `bevel notes` prints
them and the user applies what they want.** Requested directly, with the break to
existing projects accepted explicitly.

Two tier A criteria named tests that this removes, so they are replaced rather
than dropped — an acceptance list naming a deleted test never passes `validate`
again, and silently shrinking the list would lower the bar the spec was approved
against:

| Was | Is now |
|---|---|
| `the_project_body_lives_in_agents_md_with_claude_md_pointing_at_it` | `the_project_notes_are_printed_for_a_human_to_apply` |
| `a_hand_edited_claude_md_is_moved_whole_and_never_silently_discarded` | `sync_never_writes_or_moves_the_projects_instruction_files` |

The second replacement is stronger than what it replaces. The old test proved
bevel relocated a user's `CLAUDE.md` *correctly*; the new one proves it does not
touch either file at all — including one holding bytes no UTF-8 decoder accepts,
and one holding the seed applied verbatim, which is precisely the case the
deleted byte comparison existed to classify.

`the_always_loaded_context_budget_follows_the_body_to_agents_md` is unaffected
and unchanged: both files are still audited, and now nothing else constrains
their length.

**This invalidates the approval hash.** Re-run `bevel approve 0001` before
`bevel close`.

### What this removes

`ensure_agents_md`, the `Instructions` struct, the `InstructionTarget` trait and
both its impls, `GENERATED_CLAUDE_MD`, `replace`, `exists`, `is_symlink`,
`generated`, and `Action::Moved`. That last one is now an invariant rather than
an omission: sync only touches files it generates, so it never holds a user's
content to relocate.

The Rabbit holes entry "The rename precedent" is settled by this and not by a
finished migration — there is no second generation of the file to leave behind,
because bevel writes no generation of it.
