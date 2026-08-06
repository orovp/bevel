# Plan — 0002

Written against the code as of `4850f82`, not as of approval. One package:
`bevel`, a single crate. No context pack — this spec touches no external
framework, only stdlib and bevel's own modules.

The shape found in the code that the spec did not name: `review.rs` keeps
**two** counts. `Evidence` per criterion (right) *and* a `pending_markers`
field on the `Dossier` (wrong), and `render` compares them at line 582 to warn
the human when they disagree. That warning is the bug, admitted in the UI. It
goes with the field.

## Task 0 — Relocate the acceptance tests

The convention in this repo is inline `mod tests`, not a `tests/` directory:
0001's acceptance tests live in `src/sync.rs` and `src/context.rs`. So the six
tests move to the module that owns the behaviour each one pins.

| Test | Lands in |
|---|---|
| `a_marker_inside_prose_or_a_fixture_is_not_a_pending_criterion` | `src/validate.rs` |
| `a_criterion_is_pending_only_while_its_own_test_carries_the_marker` | `src/validate.rs` |
| `a_criterion_whose_test_has_not_been_relocated_yet_is_not_missing` | `src/validate.rs` |
| `pending_can_never_exceed_the_declared_criteria` | `src/validate.rs` |
| `a_tier_a_criterion_with_no_test_anywhere_blocks_the_close_on_its_own` | `src/lifecycle.rs` |
| `every_command_reports_the_same_state_for_the_same_criterion` | `src/lifecycle.rs` |

Markers stay on until each body is filled in (task 4 onward). Delete
`specs/0002-…/acceptance.rs` once they are moved.

**Verify:** `cargo test --workspace` (all six inert, nothing red) and
`bevel validate 0002` still ok — the names must remain locatable, which after
this move means found in `src/`.

## Task 1 — `criteria_state` in `validate.rs`

The one implementation, beside `locate`. Signature carries what all six callers
need without any of them recomputing:

```rust
pub enum Live { Live, Inert, Missing }
pub fn criteria_state(project: &Project, spec: &Spec) -> Vec<(String, Live)>
```

Rules, in order:
1. Spec folder first — `acceptance_files()` contains the name → found there.
2. Otherwise the repo, via **one** walk collecting every file containing any
   declared name. Not `locate` per criterion: N walks on the `Stop` hook path,
   and first-hit resolution lets root-level `AGENTS.md` shadow `src/`.
3. Found and any occurrence carries the marker within `marker_near`'s 3-line
   window → `Inert`. Found and none does → `Live`. Not found → `Missing`.

`locate` itself is untouched (No-go).

**Verify:** `cargo test --workspace validate::`

## Task 2 — `review.rs` consumes it

`Evidence` stays as the report's view. `evidence()`'s `Criterion::A` arm calls
`criteria_state` instead of doing its own `locate` + `marker_near`; it keeps
`locate` for the *label* (`path:line`), which is presentational and needs no
agreement. Delete `Dossier::pending_markers`, the `render` note at line 582, and
`marker_near` once nothing calls it.

**Risk:** `Kind::Approval` currently short-circuits to `Named`/`Missing` before
relocation. That branch must survive — it is the same spec-folder case task 1
handles, and dropping it changes what `bevel review` shows pre-approval.

**Verify:** `cargo test --workspace review::`

## Task 3 — The five counting call sites

`lifecycle.rs:107`, `summary.rs:57`, `board.rs:64`, `main.rs:745`,
`main.rs:1294`. Each derives its numbers from `criteria_state`. Add
`Blocker::MissingTest(Vec<String>)` with an `explain` that names the repair —
write the test — distinctly from `Pending`'s take the marker off.

Delete `pending_markers` and its test at `validate.rs:639`. Rewrite the three
fixture-bearing tests (`summary.rs:208`, `board.rs:295`, `lifecycle.rs:231`)
against the new behaviour, **keeping their `0001` ids**: they are the regression
test for this spec.

**Verify:** `cargo test --workspace`

## Task 4 — Remove the four `saturating_sub`

`summary.rs:61`, `board.rs:69`, `main.rs:751`, `main.rs:1299`. Only after
`pending_can_never_exceed_the_declared_criteria` is green, and all four — the
easy miss is `board.rs`.

**Verify:** `cargo test --workspace && cargo clippy --all-targets -- -D warnings`

## Task 5 — DESIGN.md (tier C)

§5's pausing note, §8's "all four" refusals, §11's two answer rows. A human
confirms this one.

**Verify:** `bevel review 0002`, then a human.

## Risks

- **Task 2 is the one that can silently regress a shipped report.** `bevel
  review` works today; the only proof it still works after is its own tests plus
  reading the rendered page.
- **`Live` as an enum name collides with `Evidence::Live` in `review.rs`.** Not
  a conflict — different modules — but confusing at the call site. Consider
  `CriterionState` if the diff reads badly.
- **Task 3 touches every command that reports progress.** The failure mode is
  arithmetic that looks plausible: verify against a fixture with a known,
  hand-checked answer rather than eyeballing the number.
