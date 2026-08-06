# Open questions — 0002

Shaped at **shallow** depth, so there was no blind spot pass: the bug is one
function with six readers and the repository is small enough to read directly.

Nothing is open. Four questions were asked and answered — see `decisions.md`.
The finding that would have justified full depth arrived from `spec-critic`
instead, which is that `review.rs` had already implemented the model the
interview designed. A domain scout would have found it sooner and one turn of
interview was spent designing something that existed.

## Deliberately not asked, and assumed

- **Proximity uses `marker_near`'s shipped three-line window.** The blank-line
  rule specified during the interview was withdrawn as ambiguous; the reasoning
  is in `decisions.md`.
- **A stray marker for a criterion the frontmatter no longer declares stops
  blocking the close.** Today it blocks. It is not a criterion.
- **`locate` keeps its `specs/` exclusion.** The spec-folder case belongs to the
  new caller.

## Deferred to their own inbox items

- **`is_texty` covers nine extensions and none of them is `.py`, `.go` or
  `.rb`.** After relocation, a project in those languages reports every tier A
  criterion missing. Pre-existing in `validate`; this spec promotes it to a
  `close` blocker. Widening `EXT` changes `validate` and `review` for every
  project at once and is a decision of its own.
- **TypeScript and Angular emit no marker**, so `inert` is unreachable there and
  those projects never block on unfinished acceptance work.
