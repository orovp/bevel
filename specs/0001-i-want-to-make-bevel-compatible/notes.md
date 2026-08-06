# Notes — 0001

Deviations from the spec, and edge cases it did not cover. Each is either fixed,
filed, or stated here as a decision taken conservatively at build time.

## Resolved during the build

**opencode honours `XDG_CONFIG_HOME` and `OPENCODE_CONFIG_DIR`.** The spec said
`~/.config/opencode`. Deriving that from `$HOME` alone would, for anyone who
sets either variable, write into a directory opencode never reads — the exact
"looks identical to success" failure the spec elevates to a rabbit hole for the
`agents/` vs `agent/` question. `Layers` gained an `opencode` field resolved by
opencode's own order, and `BEVEL_HOME` deliberately does not capture it: that
variable moves *bevel's* layers, and dragging another tool's configuration along
would hide opencode's files from opencode.

**The `permissions` array is emitted block-style, not flow-style.** The spec's
worked example showed `- { action: read, resource: "**", effect: allow }`.
`serde_yaml` 0.9 has no flow-style option anywhere in its API, so the same data
comes out multi-line. The spec anticipated this class of difference — "the shape
above is the contract, the spellings are not" — and the shape is unchanged.

**`Write` maps to opencode's `edit` permission, not to `write`.** opencode gates
its `write` tool behind the `edit` permission, so granting `write` would leave
`mockup-builder` and `context-packer` unable to write. Confirmed against
opencode's tool documentation rather than inferred from the name.

**An unrecognised tool name is a hard error.** The spec did not say what happens
to a tool with no opencode equivalent. Silently dropping it would narrow a
subagent's capabilities in a way indistinguishable from the translation working,
which is the failure the whole criterion exists to prevent. `sync` is run
deliberately, so a loud error is one someone can act on.

**The `CLAUDE.md` pointer is written *generated*, with the marker.** The first
implementation reused `write_if_absent`, which appends no marker — and that is
precisely what made the original `CLAUDE.md` unmigratable in the first place.
Writing a second markerless file would have recreated the bug this spec exists
to fix, one file over. Caught by the idempotence criterion, which failed with
`skipped as already present` rather than `unchanged`.

## Found by `diff-reviewer`, and fixed

Three of these were data loss. All three landed on the migration — the part the
spec fenced hardest — which is the argument for the adversarial pass in one
paragraph.

**A `CLAUDE.md` that is not valid UTF-8 was destroyed.** `ensure_agents_md`
classified a failed `read_to_string` as "absent" and overwrote the file with the
pointer. A single Latin-1 byte was enough. This was a *regression*: the code it
replaced used `write_if_absent`, which tested `path.exists()`, so those bytes
had been untouchable. Both classifications now turn on existence — via
`symlink_metadata`, so a broken symlink counts as present too — and unreadable
content is treated as the user's, because the only safe assumption about bytes
you cannot decode is that someone wants them.

**The same fault bypassed the "`AGENTS.md` is never overwritten" fence**, which
is a guarantee the spec states in prose (lines 190-192).

**A user's edits to a generated `AGENTS.md` were reset on the next sync.**
`write_generated` overwrites anything carrying the marker that is not byte-equal
to what it would write — so adding gotchas *inside* the template's placeholder,
which is the one thing that file exists for, lost them. `AGENTS.md` is now
seeded once and never rewritten; the marker identifies our seed rather than
licensing us to replace what grew out of it. The spec's own table said "ours |
updated as usual", so this is a deliberate departure from it, taken because the
alternative silently destroys the file the design invites people to maintain.

**The pointer was written even when `AGENTS.md` belonged to the user**, against
spec line 190 ("the instructions resource is reported and skipped entirely").
`ensure_agents_md` now returns whether the resource is bevel's to manage.

**A `CLAUDE.md` symlinked to `AGENTS.md` was written through**, replacing the
body with a pointer to itself and oscillating on every subsequent run. Symlinks
are now detected and left alone.

**A subagent with no `tools:` rendered as deny-everything.** An omitted `tools:`
means "inherit all" in the source format, so the closed allow-list stripped
every capability — the same unannounced narrowing that makes an unmapped tool
name a hard error. The `permissions` key is now omitted entirely in that case.

**`doctor` carried a second copy of the subagent path mapping**, so changing the
renderer would have left it confidently printing the old directory — in the one
command that exists to make misplacement visible. It now asks the renderer.

Regression tests were added for every one of these, plus for `detect()` and
`Agent::parse`, which the reviewer correctly noted no test touched: every other
test passes its agents explicitly, so detection could have broken silently.

## Filed to the inbox, not fixed here

**`bevel close` counts phantom pending markers** — inbox item 2.

`validate::pending_markers` text-searches the whole repo for
`acceptance: <id> pending`. In bevel's own repository that matches three test
fixtures in `src/summary.rs`, `src/lifecycle.rs` and `src/board.rs` that use
`0001` as sample data, plus one line of this spec's own prose describing the
mechanism. `bevel status` therefore reports `7/8 criteria live` when all eight
are live, and `bevel close 0001` will refuse.

Its sibling `validate::locate` already excludes `specs/` and documents why in
terms that apply identically here: a search including the spec folder "would
find the *declaration* of the name and report the contract as its own evidence".

Not fixed inside this spec for two reasons. It would change validation
behaviour for every spec in every repository, which is a decision of its own and
not one to slip into an unrelated implementation. And the obvious half-fix —
excluding `specs/` — would still leave the three source-literal false positives,
so it is not even a complete repair. An attempt to move the fixtures off id
`0001` was made and reverted: those tests construct a spec whose id really is
`0001`, so the marker has to match it.

**Consequence for closing this spec:** `bevel status` reads **4/8** — three
source fixtures plus one line of spec prose are each subtracted from the eight —
although all eight criteria are live and passing. `cargo test --workspace`
showing 163 passed and 0 ignored is the honest count. The two tier C criteria
already require a human at the terminal, so this does not change who closes it,
but the number they see understates the work.

## For whoever ticks the tier C criteria

The second criterion asked for DESIGN.md §9's tier table and §15's Phase 4 line.
The "Single source" paragraph in §9 was also rewritten, because it described
`bevel sync` projecting into `.claude/` paths that are no longer the whole story
and promised Linux symlinks that are not used — a subagent rendered for opencode
is a translation of its source, not a copy, so there is nothing for a symlink to
point at. That edit is beyond what the criterion named, and is flagged here so
it is accepted knowingly rather than waved through.

## Not done, and deliberately

Nothing in the spec's No-gos was implemented: no opencode hooks or plugins, no
deny rule for opencode, no slash commands for any agent, nothing written to a
project-local `.opencode/`, no third agent, and skills are not re-rendered into
`~/.config/opencode/skills`.
