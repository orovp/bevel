# Notes — 0002

Deviations, discoveries and anything the spec did not cover, logged as they
happen.

## No context pack

This spec touches no external framework — stdlib and bevel's own modules only.
`context-packer` was not dispatched because there was nothing to pin. Not an
`[offline]` case: documentation was not unreachable, it was not needed.

## Discovered while planning

`review.rs` keeps two counts, not one. Per-criterion `Evidence` (the correct
model) *and* a `Dossier::pending_markers` field from the text scan, with
`render` comparing them at `src/review.rs:582` to warn the human when they
disagree:

> "{n} pending markers found in the repo, {attached} of them sitting on a named
> criterion. The rest are on helpers, or on another spec's work."

That note is this bug, admitted in the UI and explained away. The spec said the
field goes; it did not know the field had a rendered consequence. Both go.

## Deviation — short criterion names are found in ordinary prose

`summary.rs`'s fixture declared criteria named `one`, `two` and `three`. Under
the new model `one` resolves to `INBOX.md` line 3 — "Raw ideas, **one** per
line" — so the criterion reports `Live` on the strength of a word in a
scaffolded document.

The search is a substring match by No-go, and `locate` and
`check_tier_a_tests_exist` have always had this property. What changes is the
consequence: it used to mislabel a row in a report, and now it feeds the close
gate, in the **false-green** direction — a criterion with no test looks done.

Chose not to invent a rule mid-implementation, which would be reopening a
design decision. The three fixtures now use names of realistic length, which is
what the spec's No-go protects (the *ids* stay `0001`; the names were never the
point). Filed as an inbox item.

## Deviation — the three-line window bleeds onto the next test

`marker_near`'s window is positional: `at - 3 ..= at`, whatever those lines
contain. Two one-line tests in a row therefore both read as marked, and a blank
line between them does not help — it costs one of the three.

`spec-critic` predicted this shape for the blank-line rule that was withdrawn
during shaping; it is true of the shipped window too, and neither the spec nor
`decisions.md` says so. It fails toward blocking, which is the safe direction
for a gate, and real Rust tests carry doc comments and bodies so they sit far
further apart than three lines. `board.rs`'s fixture now has a body, which is
what real code looks like.

Kept the shipped window as the spec directs. Filed as an inbox item.

## Step 6 — the adversarial review did not run

`diff-reviewer` was dispatched and terminated on a session limit before
producing a single finding. Not retried: the same limit would very likely cut
the retry too. **This diff has not had an independent review**, which is the one
step of the method whose value comes precisely from not being me. Worth a second
pass before this is trusted.

Reviewed it myself against the questions the agent was given, and found one
defect worth the name:

**`evidence()` called `criteria_state` once per criterion**, and each call walks
the repository. A spec with six tier A criteria walked it six times to answer
one question about each — the exact cost the spec's Solution shape identified
("One walk, not one per criterion") and that `criteria_state` was built to
avoid. Solved in `validate.rs`, reintroduced one module over. `build` now
computes the states once and passes them down.

Also checked, and clean: `Kind::Approval` still short-circuits to
`Named`/`Missing` before relocation; `marker_near`'s slice is bounds-safe
because `at` comes from `enumerate`; the per-name filter over `texts` is
O(names × matching files), and `texts` holds only files that contain a declared
name.

**Known gap in the coverage:** `every_command_reports_the_same_state_for_the_same_criterion`
pins `progress`, `blockers`, `summary`, `review` and `board`, but not
`cmd_pending` or `cmd_pause` — those live in `main.rs`, outside the library, and
are not reachable from a lib test. Both call `validate::progress` directly, so
they cannot disagree by construction, but the criterion does not prove it.
