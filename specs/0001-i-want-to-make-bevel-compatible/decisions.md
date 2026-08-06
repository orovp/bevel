# Decisions — 0001 I want to make Bevel compatible with agents other than Claude Code. We can start with Open Code.

The why. `git blame` records what changed; this records which alternative was
rejected and on what grounds. Appended during the interview, one entry per
answered question.

## 2026-08-05 — The blind spot pass was not isolated

**Answer:** Run it inline, in the interview's own context.

**Rejected:** the three scouts in parallel, because they could not run —
`domain-scout`, `risk-scout` and `unknowns-scout` each terminated on
`You're out of usage credits`. Waiting for credits was offered and declined by
proceeding.

**Cost, stated so a later reader can weigh it:** the isolation that the fan-out
exists for was lost. Everything the exploration turned up — including the
`dc3bbef` archaeology and the opencode research — sat in context while the
interview happened, which is precisely the pollution `src/sync.rs`-style
isolation is meant to prevent. Treat the findings as sound but the *framing* of
the questions as having been anchored by them.

## 2026-08-05 — How far up the tier ladder does opencode go?

**Answer:** Tier ~1.5 — the seven subagents rendered to
`~/.config/opencode/agents/*.md`, plus `AGENTS.md`. Hooks are out.

Skills needed no decision: opencode already scans `~/.claude/skills/`, so the
five skills `bevel sync` installs today load in opencode untouched. The work is
exactly the resource that has no shared path.

**Rejected — full tier 2 with hooks,** because opencode expresses hooks as
JavaScript plugins in `~/.config/opencode/plugins/`. That would make a Rust
binary the generator and maintainer of JS source, and the three hooks it would
carry (`bevel fmt --hook`, `bevel status --brief`, `bevel pending`) are all
explicitly non-blocking conveniences. New surface, in a new language, for the
least load-bearing resource.

**Rejected — tier 0, `AGENTS.md` only,** because that is what `dc3bbef` already
deleted, and because it concedes context isolation. DESIGN.md §9 says the
degradation is survivable, which is true and not the same as free.

## 2026-08-05 — A renderer abstraction, or opencode hardcoded?

**Answer:** A trait per resource kind, with two implementations — Claude Code
and opencode. The inbox's own proposal, kept.

**Rejected — `sync_opencode` written flat beside the Claude logic,** and
rejected against the grain of `src/sync.rs:1-11`, which asks for the split to be
known rather than guessed. The counter-argument that carried: the two agents now
differ *per resource kind* rather than wholesale — skills share a path, subagents
do not, instructions differ by filename — and that is a shape a flat function
would obscure rather than reveal.

**Rejected — restoring the bare `enum Agent` from `dc3bbef`,** because a `match`
inside every resource function reproduces the same dispatch without naming the
resource kinds, which is the part of the inbox proposal actually worth having.

**The risk, recorded because it was raised and accepted:** with N=2 the trait
will be shaped by the two agents in front of us, and a third may not fit it. The
mitigation is that the trait is internal — no plugin API, no stability promise —
so the third agent may reshape it freely.

## 2026-08-05 — opencode v1 or v2 frontmatter?

**Answer:** Pin v2. Emit `permissions` only, and state the requirement.

Field-by-field, only one thing actually diverges. `description` and
`mode: subagent` are valid in both; `name` comes from the filename in both;
`model` is dropped by the assumption above. The tool restriction is the whole
question.

**Rejected — emitting `tools:` and `permissions:` side by side** so either
version finds its own. It works, and it was rejected for being the kind of
hedge that is never removed: the redundant field outlives the version it was
for, and nobody dares delete it because nobody remembers which half is live.

**Rejected — running `opencode --version` during sync,** because `sync` writes
files and does not execute other people's binaries. Making it shell out puts a
foreign process on the path of a command that must work on a fresh machine.

**Rejected — a `schema = "v2"` key in `config.toml`,** because it hands the user
a decision they have no basis to make and must then keep current.

**Accepted consequence:** on opencode v1 the seven subagents load with
unrestricted tools, and a read-only reviewer could edit what it reviews. Two
cheap guards, neither of which reopens the decision: the generated file carries
a YAML comment naming the requirement, and `bevel doctor` reports the pin so the
answer is discoverable without reading generated frontmatter.

## 2026-08-05 — Which file carries a project's instructions?

**Answer:** `AGENTS.md` carries the body; `CLAUDE.md` shrinks to a pointer at
it. Exactly what this repository already does to itself.

**Rejected — leaving `CLAUDE.md` as the only generated file,** even though
opencode's fallback chain would find it. The fallback is opencode's courtesy,
not a contract, and it ranks `CLAUDE.md` last; naming the shared file after one
of its two readers is how the next agent inherits a misnomer.

**Rejected — dropping `CLAUDE.md` entirely,** because it is the file a user is
most likely to have edited by hand, and deleting it to prove a point about
naming is not worth the one person whose local notes vanish.

**Migration, and it is the delicate part:** an existing generated `CLAUDE.md`
has to become `AGENTS.md` plus a pointer. `LEGACY_DENY_RULE` (`src/sync.rs:68`)
is the precedent for what happens when a rename forgets to clean up: both
generations sit on disk and neither is wrong enough to notice.

> **Corrected 2026-08-05 after `spec-critic`.** This entry first claimed the
> marker in `write_generated` (`src/sync.rs:371-393`) already distinguishes a
> generated `CLAUDE.md` from a hand-edited one. It does not. `CLAUDE.md` is
> written by `write_if_absent` (`src/sync.rs:151`), which appends no marker, and
> the `CLAUDE_MD` constant contains none — so *every* `CLAUDE.md` bevel has ever
> produced is markerless, and the migration would have been a no-op on every
> existing project. See the entry below.

## 2026-08-05 — What happens to a hand-edited `CLAUDE.md`?

**Answer:** Adopt and merge. The file moves wholesale to `AGENTS.md` and a
pointer is left in its place, whatever it contains.

First, the rule that makes "hand-edited" mean anything at all, since the marker
does not exist for this file: **a `CLAUDE.md` byte-identical to the `CLAUDE_MD`
constant is treated as generated** and may be replaced. Anything else is the
user's.

**Rejected — writing `AGENTS.md` and leaving the edited `CLAUDE.md` in place,**
because the two would then both be live: the user's notes serving Claude Code,
bevel's generated body serving opencode. That is the drift the whole
one-instruction-one-place rule exists to prevent, arrived at by a route that
looks polite.

**Rejected — skipping the instructions resource entirely** when `CLAUDE.md` is
edited, because opencode would then read nothing in that project and the only
notice is a line of `sync` output nobody re-reads.

**Accepted consequence, and it is the uncomfortable one:** this is the only
place in the design where bevel moves content a user wrote. Two guards, both
required rather than advisory: it must never overwrite an existing `AGENTS.md`
— if one is already there, the whole thing is reported and skipped — and the
move must be reported as a move, not as a write.

## 2026-08-05 — Findings from `spec-critic`, and what changed

The critique ran isolated and found four things the inline blind spot pass had
missed. Recorded because the pattern is the lesson, not the individual bugs.

**Two rabbit holes in the first draft were aimed at non-risks.** The spec warned
that rendering subagents twice might double the context budget; `context.rs:167`
iterates `sync::method_sources`, which returns *source* paths, so a second
destination cannot double anything. It warned that `method_names().len() == 12`
would need updating; that count is `SKILLS.len() + AGENT_DEFS.len()`, read from
the one method tree, and this spec adds no method file.

**Meanwhile the real breakage in that same file went unfound.**
`context.rs:144-152` hardcodes `CLAUDE.md` as the only `Load::Always` item at a
50-line budget, and the loop below it walks `project.config.packages` only — so
the root `AGENTS.md` is never audited. Moving the body without moving the
measurement silently retires the budget check that DESIGN.md §13 calls "the
piece most likely to save this project long-term". That is now tier A criterion
eight.

**And the frontmatter mapping was a schema, not a mapping.** The seven subagents
carry four distinct tool sets, not one: `Read, Write` (mockup-builder),
`Read, Grep, Glob` (spec-critic, unknowns-scout), `Read, Grep, Glob, Bash`
(domain-scout, risk-scout, diff-reviewer) and
`Read, Grep, Glob, Bash, WebFetch, Write` (context-packer). A "read-only
translation" would have corrupted the two that legitimately write.

**The lesson worth keeping:** every fact the inline pass *recalled* was right —
the `dc3bbef` archaeology, the README claims, the gate mechanics all verified
clean. Every fact it *inferred without reopening the file* was wrong. Isolation
was not what the fan-out was buying here; re-derivation was.

