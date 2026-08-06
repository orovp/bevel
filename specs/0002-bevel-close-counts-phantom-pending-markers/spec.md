---
id: '0002'
title: 'bevel close counts phantom pending markers: validate::pending_markers'
status: done
schema_version: 1
created: 2026-08-06
packages:
- bevel
acceptance:
- tier: A
  test: a_marker_inside_prose_or_a_fixture_is_not_a_pending_criterion
- tier: A
  test: a_criterion_is_pending_only_while_its_own_test_carries_the_marker
- tier: A
  test: a_criterion_whose_test_has_not_been_relocated_yet_is_not_missing
- tier: A
  test: a_tier_a_criterion_with_no_test_anywhere_blocks_the_close_on_its_own
- tier: A
  test: every_command_reports_the_same_state_for_the_same_criterion
- tier: A
  test: pending_can_never_exceed_the_declared_criteria
- tier: B
  cmd: cargo test --workspace
- tier: B
  cmd: cargo clippy --all-targets -- -D warnings
- tier: C
  text: DESIGN.md no longer describes progress as a count of markers in the repo — §5's pausing note, §8's list of what close refuses, and §11's two answer rows all name the declared criteria instead
---
# bevel close counts phantom pending markers: validate::pending_markers

> From the inbox: bevel close counts phantom pending markers: validate::pending_markers text-searches the whole repo, so it matches the string inside spec.md prose and inside test fixtures in src/*.rs that use a real spec id. Its sibling locate() already excludes specs/ for exactly this reason and documents why. Surfaced while implementing 0001 in bevel's own repo.

## Problem

`bevel close` refuses to close spec 0001 in this repository, reporting four
pending acceptance criteria. There are none. All eight tier A tests exist and
pass.

`validate::pending_markers` (`src/validate.rs:295`) counts how many times the
string `acceptance: NNNN pending` appears anywhere under the project root. The
four matches for 0001 are two different kinds of not-a-marker:

| Where | What it actually is |
|---|---|
| `specs/0001-…/spec.md` | the spec quoting the marker syntax while explaining the rule |
| `src/board.rs`, `src/lifecycle.rs`, `src/summary.rs` | test fixtures using a real spec id as sample data |

**The same question already has a second, better answer in this codebase.**
`bevel review` does not count occurrences. `evidence()` (`src/review.rs:187`)
takes each declared tier A criterion, finds its test with `locate`, and asks
`marker_near` whether that test still carries the marker — three states, named
in `Evidence` (`src/review.rs:46`). It has been right since `f7db08a` and it is
right for the reason the text scan is wrong: it is bounded by what the spec
declares, not by what the repository happens to contain.

So bevel answers "is this criterion done?" twice, differently, and the wrong
answer is the one wired into the gate. Six call sites read the text count —
`close`, `status`, `pending`, `pause`, the board and, inconsistently, `review`
itself, which uses both.

Three consequences follow from the split:

**The count is incoherent, not merely inflated.** `remaining` counts string
occurrences while `total` counts declared criteria, so `remaining` can exceed
`total`. Four call sites carry a `total.saturating_sub(remaining)` to stop that
underflowing into `18446744073709551612` — a load-bearing workaround for a
condition nobody wrote down.

**It hides the opposite failure.** A spec whose tier A tests were never written
has no markers, so it reports every criterion live and closes clean. `validate`
catches it; `close` never runs `validate` (`src/main.rs:1220`). The command whose
job is refusing a premature close cannot see the most obvious way to deserve one.

**And the two answers can already disagree in production.** `bevel review 0007`
and `bevel close 0007` can classify the same criterion differently today. Nobody
has hit it because the text count is usually zero or the spec has no prose about
markers — which is to say, nobody has hit it yet.

## Appetite

Hours, not days, and the ceiling is real: this is one function extracted, one
blocker added, six call sites pointed at it. No new concept — the concept is
already written and under test in `review.rs`.

## Solution shape

**`review.rs`'s model moves to `validate.rs`, beside `locate`, and becomes the
only one.** `Evidence` stays in `review.rs` as the report's own view, built on
top of the shared answer rather than beside it.

For each `test:` name the spec frontmatter declares, one of three states:

| State | When | Consequence |
|---|---|---|
| live | the test is found and carries no marker | counts toward `N/M live` |
| inert | the test is found and its marker is still there | blocks the close, as today |
| missing | the name is nowhere the search can see | blocks the close, **separately** |

That alone fixes both classes of false positive without a rule about either:
neither a sentence of prose nor a fixture in `src/board.rs` is named
`opencode_subagents_land_where_opencode_reads_them`.

**Where it looks depends on whether the plan has moved the file yet, and this
is the part that is easy to get fatally wrong.** `locate` excludes `specs/` — on
purpose, and it documents why. A spec is `implementing` from the moment `bevel
start` runs, which is *before* task zero relocates `acceptance.*` into its
package. Searching only with `locate` would report every criterion `missing` for
that whole window and refuse the close with "no test anywhere in the repo" while
the tests sit in the spec folder. `evidence()` already solves this with
`in_spec_folder` (`src/review.rs:192`), and `check_tier_a_tests_exist` solves it
with two filters rather than one (`src/validate.rs:204-207`). The extracted
function inherits that: **the spec folder is consulted first, then the rest of
the repo.** Commit `7f2dedf` fixed the mirror image of this bug in `validate`;
this must not reintroduce it in `close`, where it is a gate rather than a
warning.

**A criterion is inert if any occurrence of its name carries the marker**, not
if `locate`'s first hit does. `walk` sorts by name, so root-level `AGENTS.md`,
`DESIGN.md` and `INBOX.md` are visited before `src/`; a criterion name mentioned
in prose there would become the located "test", the marker check would run
against markdown and find nothing, and the criterion would report live. That is
this spec's own bug coming back through the door it left open. Considering every
occurrence also collapses N walks into one, which matters because `bevel pending`
is the `Stop` hook and `bevel status --brief` the `SessionStart` hook.

**Proximity stays `marker_near`'s shipped three-line window** (`WINDOW: usize =
3`, `src/review.rs:230`). See `decisions.md` for the blank-line rule that was
specified first and withdrawn as ambiguous.

**`missing` is its own blocker.** "Never written" and "still switched off" need
different repairs, and one number confuses them.

## No-gos

- **No parser per language.** The search stays a substring match. Telling a real
  `#[ignore]` from one inside a Rust string literal is parsing, and it is the
  wrong fix: the right one makes the fixtures irrelevant rather than
  recognisable.
- **The fixture ids are not renumbered.** Changing sample data to dodge a
  scanner leaves the scanner broken for every other repository. `0001` stays in
  `src/board.rs`, `src/lifecycle.rs` and `src/summary.rs`. Their *assertions*
  necessarily change — those three tests assert outcomes this spec redefines,
  and `pending_markers_are_counted_across_the_repo` (`src/validate.rs:639`)
  tests a function that is going away. Rewriting them is in scope; keeping the
  ids is the point.
- **`locate` is not modified.** It is called by `validate` and `review` and its
  `specs/` exclusion is correct for both. The spec-folder case is handled by the
  new caller, not by changing what `locate` means.
- **`is_texty` and `SKIP_DIRS` are not extended** (`src/validate.rs:332`, `:359`).
  Nine extensions, none of them `.py`, `.go` or `.rb`. This is a real limit and
  it is pre-existing: `check_tier_a_tests_exist` already reports
  `acceptance/missing-test` for those projects once relocated. This spec
  promotes that from a `validate` warning to a `close` blocker, which is a
  narrowing, and widening `EXT` changes `validate` and `review` for every
  project at once. It is a separate decision with its own inbox item.
- **`close` still does not run `validate`.** Adding the missing-test check is not
  the start of folding one command into the other.
- **The marker convention does not change.** No edit to the method tree; every
  marker already written keeps working.

## Rabbit holes

- **`pending_markers` is deleted, and one shipped report loses a feature.**
  `review.rs:582` renders "{n} pending markers found in the repo, {attached} of
  them sitting on a named criterion" — a sentence that exists only because the
  two counts could disagree. Once they cannot, it says nothing. Remove it with
  the function rather than leaving a line that reports 0 of 0.
- **The four `saturating_sub` calls stop protecting anything** —
  `src/summary.rs:61`, `src/board.rs:69`, `src/main.rs:751`, `src/main.rs:1299`.
  Leaving them documents a fear that is no longer true; removing them is correct
  only once `pending_can_never_exceed_the_declared_criteria` proves the bound.
  Prove first, then remove, and remove all four — the easy one to miss is
  `board.rs`, the only call site this spec calls cosmetic.
- **Spec 0001 must still close after this.** Checked: all eight of its tier A
  tests exist outside `specs/` (`src/sync.rs:1090, 1381, 1403, 1465, 1505, 1582,
  1612` and `src/context.rs:702`), none carries a marker, and none is a substring
  of another. It goes from a false 4/8 to a true 8/8. The answer could easily
  have been no, which is why it is written down.
- **0001's appetite argument retires with this change.** It argued its migration
  was inseparable because a marker "anywhere under the repo root" blocks the
  close (`specs/0001-…/spec.md:75-81`). After this, only a marker bound to a
  declared, located test blocks. Historically true, prospectively false — leave
  0001's text alone and know why it reads oddly.
- **TypeScript and Angular have no marker convention, so `inert` is unreachable
  there.** DESIGN.md:822-826 gives `test.todo('name')` and `xit('name')` with no
  marker; only the Rust row has one. Those projects get `live` or `missing` and
  never block on unfinished acceptance work. Not a regression — today's count is
  also zero — but this spec must not claim a cross-language proximity rule it
  has validated in one language.
- **This spec's own markers count against it under the code being replaced.**
  `acceptance.rs` carries `acceptance: 0002 pending`, and so does the prose
  above. Expected, and the smoke test: the numbers settle when the change lands.
- **A spec with zero tier A criteria.** `tier_a_tests()` returns empty and every
  count is zero. `validate` already refuses a spec with no tier A or B criterion,
  so it is unreachable through the pipeline — but the function must not divide,
  index or `unwrap` its way into a panic on it.
