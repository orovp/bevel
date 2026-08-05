---
name: shape
description: Turn an undeveloped INBOX.md idea into a spec a human can approve. Use when the user runs /shape, names an inbox item number, or wants to develop a vague idea before any code is written.
---

# Shape

Produce a spec that can be implemented without asking the user anything else.
The success metric is literal: **zero questions during `/implement`**.

## 1. Judge the depth, then say so

Read the inbox item and the repo before deciding anything. State the depth you
propose and one sentence of why, then let the user confirm or redirect.

| Depth | What you do | When |
|---|---|---|
| chore | no spec; implement directly | mechanical, no decisions to make |
| shallow | interview + spec | one obvious approach |
| full | everything below | real unknowns |

Bias shallow. Escalating mid-interview costs one step; unwinding three passes
you did not need costs what they already burned. If the interview reveals the
idea is bigger than it looked, say so and offer to widen.

## 2. Reserve the artifacts

```
bevel shape <n>
```

This assigns the id, creates `specs/NNNN-slug/` and links the inbox item. Do
not create these by hand — the id has to be reserved, not chosen.

## 3. Blind spot pass (full depth only)

Dispatch `domain-scout`, `risk-scout` and `unknowns-scout` **in parallel, in a
single message**. They are independent read-only searches, and the isolation is
the point: three repo explorations must not pollute the context where the
interview happens next.

Synthesise into `open-questions.md`, deduplicated and **ranked by decision
impact**. Then cut it. Only questions whose answer changes the design get
asked; everything else becomes an explicit assumption recorded in the spec. A
forty-question interrogation is how this practice gets abandoned.

## 4. Interview, one question at a time

One question. Wait. Record the answer in `decisions.md` with its date. Then the
next. Never batch.

Record rejected alternatives and why. `git blame` will tell them what changed;
this is the only place that will tell them what they decided against.

## 5. Shape the solution

Two or three approaches with their appetite, tradeoffs and rabbit holes. The
user picks or blends. For the riskiest part, offer a throwaway prototype in a
scratch directory — twenty minutes of spike kills two-day plans often enough to
be worth offering. Discard the prototype, keep the findings in `decisions.md`.

If the shape is visual, breadboard first: places, affordances, connections, in
plain text. That usually settles it. When it does not, dispatch
`mockup-builder`. It returns numbered states — `§1 empty`, `§2 conflict` — and
those numbers are what tier C criteria point at below.

## 6. Name the acceptance criteria

This is the highest-yield step. Write the criteria as **named, empty, failing
tests** in `acceptance.<ext>`, and declare them in the frontmatter.

```yaml
acceptance:
  - tier: A          # executable: a named test
    test: conflict_prefers_local_when_remote_is_older
  - tier: B          # commanded: anything with an exit code
    cmd: "npm run build --workspace=web"
  - tier: C          # judged: a human decides, never you
    text: "the conflict banner matches mockup.html §2"
```

```rust
#[test]
#[ignore = "acceptance: 0007 pending"]
fn conflict_prefers_local_when_remote_is_older() { todo!() }
```

Rules that matter:

- **A name must survive a refactor.** `test_sync_manager_resolve` names a
  class; `conflict_prefers_local_when_remote_is_older` names a behaviour.
- Emit them **inert** — `#[ignore]`, `test.todo()`, `xit()` — so an approved
  but unimplemented spec never reddens CI.
- Three to seven tier A criteria is healthy. Above ten, the spec is too big;
  say so and propose splitting it.
- At least one tier A or B is mandatory. If you cannot name a single one, the
  spec is still vague — that is the signal, surface it rather than inventing a
  criterion to satisfy the check.
- A tier C criterion that cites the mockup cites a section that exists:
  `mockup.html §2`, resolved by `bevel validate` against the mockup's anchors.
  A pointer that dangles is found by the human at close, far too late.

## 7. Critique your own spec, then hand it over

Dispatch `spec-critic`. It reads in a fresh context and did not write the spec,
which is the only reason its opinion is worth more than your own second look.
Address what it reports.

```
bevel validate <id>
```

Then stop. **You cannot approve.** `bevel approve` requires a terminal and
you do not have one — that is the gate working, not a problem to route around.
Tell the user what to run and what changed.
