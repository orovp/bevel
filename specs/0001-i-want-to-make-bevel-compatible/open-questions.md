# Open questions — 0001

Ranked by decision impact. Only questions whose answer changes the design belong
here; everything else is an assumption, recorded in the spec.

> The blind spot pass ran **inline, not isolated**: `domain-scout`, `risk-scout`
> and `unknowns-scout` all terminated on `You're out of usage credits`. The
> findings below are therefore mine, gathered in the same context the interview
> then happened in. See `decisions.md`.

## Findings that reframe the question

**Skills are already free.** opencode scans `~/.claude/skills/<name>/SKILL.md`
among its global skill paths. The five skills `bevel sync` installs today are
already loadable by opencode with zero changes. Tier 0 is not the work.

**Subagents are not.** opencode scans only `~/.config/opencode/agents/` and
`.opencode/agents/`. There is no `.claude/agents` fallback, so the seven
subagent definitions are invisible to it today.

**Prior art exists and was deleted.** `dc3bbef` removed an `enum Agent` with
five variants, home-directory detection, and an `AGENTS.md` writer — and in it,
`Agent::Codex | Agent::Opencode => {}` wrote *nothing at all* for opencode,
because both read `AGENTS.md` natively. It was cut as "speculative surface with
no real usage behind it yet".

**The gate does not depend on any of this.** `gate::approve` refuses on
`!stdin().is_terminal()` (`src/gate.rs:194`), and the only call site passes
`assume_yes: false` (`src/main.rs:1065`). The `Bash(bevel approve*)` deny rule
is convenience that stops an agent trying, not the boundary itself.

## Asked

1. **How far up the tier ladder does opencode go?** Tier 0 is already achieved
   by accident. DESIGN.md §9 places opencode at tier 1. Subagents are tier ~1.5
   and hooks-as-plugins are tier 2, and each is a different appetite.
2. **General mechanism, or opencode hardcoded?** The inbox proposes "mapping by
   resource type, plus a mapper per agent". `src/sync.rs:1-11` warns against
   exactly that abstraction before evidence, and N=2 is where premature
   generality is cheapest to mistake for good design.
3. **Which opencode config generation?** v1 frontmatter (`mode`, `tools`,
   `temperature`, `permission`) and v2 (`permissions` array of
   `action`/`resource`/`effect`, with the v1 fields explicitly deprecated) are
   not compatible. Emitting the wrong one installs files nothing loads.
4. **Does `AGENTS.md` become the body and `CLAUDE.md` the pointer?** This
   repository already does exactly that, but `sync` writes a full `CLAUDE.md`
   and no `AGENTS.md` (`src/sync.rs:531`).

## Cut — recorded as assumptions instead

- **Selection is `--agent` plus home-directory detection.** Both the deleted
  code and `README.md:24` already promise this shape; restoring it is not a
  decision, it is honouring a documented claim.
- **Machine scope, not project.** The method is the same text in every project
  (`src/sync.rs:112-116`). A second agent does not change that reasoning.
- **The `model:` field is dropped, not translated.** `model: opus` and
  `model: fable` are Claude Code shorthands, not opencode model ids. Omitting
  the field lets opencode use its own default; inventing a mapping would pin
  someone else's model choice into our generated file.
- **Pruning stays scoped exactly as today.** `prune_skill` never removes a
  directory it did not install into, which is the property that keeps it from
  eating a user's own work in a shared config tree.
- **The deny rule for opencode is polish.** See the gate finding above.
- **Tests assert placement and parseable frontmatter, never that opencode
  loaded anything.** There is no opencode in CI and there should not be.
