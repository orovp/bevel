# Decisions — 0002 bevel close counts phantom pending markers: validate::pending_markers

The why. `git blame` records what changed; this records which alternative was
rejected and on what grounds. Appended during the interview, one entry per
answered question.

<!-- ## <date> — <question>
     **Answer:** ...
     **Rejected:** ... because ... -->
## 2026-08-06 — Where does the pending count come from?

**Answer:** From the declared tier A criteria. For each `test:` name in the
frontmatter, decide whether that criterion is live; the count is over those N
names and nothing else.

This makes `remaining` and `total` come from one source. Today they come from
two — `total` counts declared criteria, `remaining` counts free-text matches
across the repo — so `remaining` can exceed `total` and "4/8 criteria live" can
be not merely inflated but incoherent.

It also fixes both classes of false positive at once, which the obvious fix does
not: a fixture in `src/board.rs` and a sentence of spec prose are neither of
them named `opencode_subagents_land_where_opencode_reads_them`.

**Rejected — text scan, excluded and narrowed:** keep counting occurrences but
skip `specs/` and only count in files that also hold a tier A name. Smaller
change, and it inherits the defect: the number still comes from how many times a
string appears, so two markers on one criterion count twice.

**Rejected — exclude `specs/` and renumber the fixtures:** the minimum change,
and the one the inbox item proposed. It fixes this repository rather than bevel.
Any project whose documentation quotes the marker syntax hits the same bug, and
the three fixtures would be one careless edit away from returning.

## 2026-08-06 — A tier A criterion whose test is nowhere in the repo

**Answer:** Its own blocker, reported separately from the inert ones. "No test"
and "test still switched off" are two failures with two different repairs —
write it, or take the marker off — and one number confuses them.

This closes a hole that is reachable today and has nothing to do with phantom
counting: a spec whose tier A tests were never written reports every criterion
live, because `pending_markers` finds no markers and `close` never runs
`validate` (`src/main.rs:1220`), which is the only thing that checks a named
test exists. Deriving the count from the declared criteria puts every name
through `locate` anyway, so the check costs nothing to add.

It makes `close` stricter. A spec that would close today with its tests
unwritten will stop closing, which is the point.

**Rejected — fold it into the count:** one rule, one number, easier to explain,
and the report loses the distinction exactly where a reader needs it most.

**Rejected — leave it live:** keeps the scope on the reported bug, and knowingly
leaves a false green in the command whose whole job is refusing to let one
through.

## 2026-08-06 — What signals that a located test is still inert

**Answer:** The marker `acceptance: NNNN pending` near the name, and nothing
else. It is the documented convention, its id is what binds the marker to this
spec rather than to some other, and every marker already written in every
repository keeps working with no change to the method tree.

**Known hole, accepted:** removing the marker while leaving `#[ignore]` in place
closes a spec on a test that never runs. Narrow, and visible in a diff.

**Rejected — any inert idiom:** counting `#[ignore]`, `test.todo(`, `xit(` and
`it.skip(` with or without a marker would close that hole, and would attribute
an `#[ignore]` added for flakiness to this spec — blocking a close for a reason
nobody would connect to the spec they are trying to close.

**Rejected — marker blocks, idiom warns:** covers the hole without hijacking
anyone's `#[ignore]`, at the cost of a third state to explain in every report.
Kept in mind if the hole ever bites.

## Assumptions recorded rather than asked

- **Proximity:** a marker binds to a test name if it is on the same line or in
  the contiguous non-blank lines immediately above it. No line-count constant —
  a blank line ends the block, which is what `#[ignore]` above `fn` and
  `test.todo('… name')` on one line both already respect.
- **A stray marker for a criterion no longer declared stops blocking.** Today it
  blocks. It is not a criterion, so it is not this command's business.
- **`locate` is reused unchanged.** It already searches the whole repo, already
  excludes `specs/`, and already documents why.

## 2026-08-06 — Where the one implementation lives

**Context that arrived late.** `spec-critic` found that `review.rs` has shipped
the three-state model since `f7db08a`: `Evidence` (`src/review.rs:46`) and
`evidence()` (`src/review.rs:187`) already do `locate` → `None` is missing, else
`marker_near`. The interview designed a model that already existed in one of the
six call sites. The decisions above survive unchanged — that `bevel review`
already works this way is the strongest evidence they were right — but the work
is not to invent it. It is to make it exist once.

**Answer:** Extract it from `review.rs` into `validate.rs`, beside `locate`.
`Evidence` stays in `review.rs` as the report's own view, built on top.

**Rejected — `review.rs` owns it and the others call in:** less code moved and
the report is untouched, at the price of `lifecycle`, `summary` and `board`
depending on the module that renders HTML for humans.

**Rejected — do not unify:** a new function for the count, `review.rs` left
alone, two commands knowingly disagreeing about the same criterion. Smallest
change and a second spec owed.

## 2026-08-06 — Two assumptions the critique overturned

**Proximity: the shipped three-line window, not the blank-line rule.** The
assumption recorded above was invented during the interview and is ambiguous in
ways `marker_near`'s window is not: "blank" is undefined for a whitespace-only
line, and a closing `}` is not blank, so for two adjacent Rust tests the block
above the second walks up through the first and picks up its marker. That is not
hypothetical — `src/board.rs:307` is exactly that shape. `marker_near`'s
`WINDOW: usize = 3` is shipped, tested and unambiguous. It wins.

**All occurrences, not `locate`'s first hit.** `locate` returns the first match
in sort order, and `walk` visits root-level `AGENTS.md`, `DESIGN.md` and
`INBOX.md` before `src/`. A criterion name mentioned in prose outside `specs/`
would become the located "test", the marker check would run against markdown,
find nothing, and report the criterion live — the same bug this spec exists to
fix, re-entering by the door left open. A criterion is inert if **any**
occurrence of its name carries the marker. Failing toward blocking is the right
direction for a gate, and one walk over all criteria replaces N walks, which the
`Stop` and `SessionStart` hooks pay for on every turn.
