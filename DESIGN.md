# bevel — Design Draft v0.6

> Status: **draft for discussion**. Nothing here is final.
> Every decision comes with its argument and, where one exists, the rejected alternative.
> Sections marked 🔶 are the ones I most want your input on.
>
> **Changes in v0.6:** renamed from `harness` to **bevel** (§1). `harness` was taken on
> crates.io, and prefixing around it was a worse answer than choosing a free word: crate,
> repository and binary are now one string.
>
> **Changes in v0.5:** the method is no longer compiled into the binary. It is fetched from the
> GitHub repository and cached, so editing a markdown file needs no release (§2). Artifact
> templates became method files for the same reason.
>
> **Changes in v0.4:** the project has a name (§1); one active spec at a time, which turned out
> to need no new state (§5); amendment is edit-and-re-approve, with the mid-flight case
> specified (§5); shaping depth is judged rather than flagged (§8); `status` is a fixed-size
> summary (§11); supersession is a line-by-line reckoning with the old contract (§5).
> No open question blocks Phase 1.
>
> **Changes in v0.3:** no task runner — `verify --affected` is derived from `cargo metadata`
> and npm workspaces (§3); dual distribution via npm *and* `cargo install` (§2);
> degrade-offline as a design principle rather
> than an error path (§10); the foundation-spec question decided (§4).
>
> **Scope note:** every path and identifier in this document is an illustrative placeholder.
> This document designs the harness, not any project built with it.
>
> **Earlier:** v0.2 introduced the monorepo layout, greenfield adaptations, the three-tier
> acceptance model (§7), HTML mockups (§6) and the XDG three-layer global layout (§2).

---

## 0. Where the principles come from

This design is anchored in four Anthropic engineering articles. They are not cited as
decoration: almost every decision below traces back to one of these rules, and when the design
gets complicated, these are the rules that should win.

**"Harnessing Claude's intelligence"** — a harness is *"the software scaffolding around a
model: the loop, tools, context management, and guardrails that turn raw intelligence into a
working agent."* Three patterns: lean on capabilities the model already has (bash, file
editing), **minimize harness assumptions** (*"agent harnesses encode assumptions about what
Claude can't do on its own"* — and those expire), and set deliberate boundaries only where they
earn their place (security, UX, observability). The article's recurring question:
***"What can I stop doing?"***

**"The new rules of context engineering"** — Anthropic removed **over 80% of Claude Code's
system prompt** with no loss in performance: *"we were overconstraining Claude."* From rules to
judgment. From examples to interface design. Progressive disclosure instead of front-loading.
CLAUDE.md as a *brief repo description plus non-obvious gotchas*, not an encyclopedia. And one
idea that matters a lot here: **code-based specs** (test suites, function signatures) **over
markdown descriptions**.

**"A field guide to Claude Fable"** — the taxonomy of unknowns (known knowns, known unknowns,
**unknown knowns** = the obvious things you never articulate, and unknown unknowns), the
*blind spot pass*, *"Interview me one question at a time about anything ambiguous"*, the
prototype as cheap validation before expensive implementation, the `implementation-notes.md`
where deviations get logged, and the pattern of **compiling artifacts and handing them to a
fresh session**. Also the single most useful warning of the four: *"If you are too specific,
Claude will follow your instructions even when a pivot may be more appropriate. If you are too
vague, Claude will make choices based on industry best practices that may not fit."*

**"A harness for every task"** — context isolation between subagents counters three concrete
failure modes: **agentic laziness** (stopping before completion), **self-preferential bias**
(preferring its own findings) and **goal drift**. Composition patterns: classify-and-act,
fan-out-and-synthesize, adversarial verification, generate-and-filter, tournament,
loop-until-done. And the brake: ***"parallelism and specialization have to earn their
coordination cost."***

### The seven rules I will apply

| # | Rule | Practical consequence |
|---|---|---|
| R1 | The harness is thin by default | Explicit, measurable token budget (§13) |
| R2 | Progressive disclosure | Short `SKILL.md` + `references/` on demand |
| R3 | Acceptance criteria must be **machine-verifiable wherever possible** | Three-tier model (§7) |
| R4 | Determinism belongs to a binary, not the model | IDs, hashes, gates and verification in the CLI |
| R5 | Artifacts **are** the portability layer | A file on disk works in any agent |
| R6 | Parallelize only when it earns its cost | Fan-out in two places, sequential everywhere else |
| R7 | Adversarial verification in a fresh context | Whoever writes does not review |

---

## 1. Locked-in decisions

| Decision | Chosen | Source |
|---|---|---|
| Agents | Claude Code tier-1 + portable `AGENTS.md` layer | v0.1 |
| Artifacts | Visible, versioned `specs/` + `.bevel/` for state only | v0.1 |
| Gate | Local human approval + integrity hash, hard block | v0.1 |
| Tooling | **Rust CLI distributed via npm** | v0.1 |
| Repo model | **Monorepo, always** | v0.2 |
| Codebase age | **Greenfield projects** | v0.2 |
| Acceptance criteria | **Three tiers**; `/shape` performs the naming exercise | v0.2 ✓ |
| Tauri | **Deferred**, not in scope for now | v0.2 |
| HTML mockups | **Yes**, as a shaping artifact | v0.2 |
| Machines | **Several** (home, work) — must survive drift | v0.2 |
| Monorepo tooling | **None** — cargo + npm side by side, no nx/turbo | v0.3 |
| Distribution | **npm primary + `cargo install` fallback** | v0.3 |
| Method delivery | **Fetched from GitHub and cached; nothing embedded** | v0.5 |
| Network | **May be restricted** — degrade, never fail hard | v0.3 |
| Foundation spec | **Yes**, shaped at shallow depth (§4) | v0.3 |
| Shaping depth | **Proposed by the model, not a flag** (§8) | v0.4 |
| `status` output | **Fixed size, independent of spec count** (§11) | v0.4 |
| Supersession | **A reckoning with the old contract, not a status flip** (§5) | v0.4 |
| Name | npm `@orovp/bevel` · crate `bevel` · binary **`bevel`** | v0.4 |
| Concurrency | **One active spec at a time** | v0.4 |
| Amendment | **Edit the file and re-approve** — no `amend` command | v0.4 |

### Naming

```
crate          bevel                repo   github.com/orovp/bevel
npm package    @orovp/bevel         binary bevel
config / cache ~/.config/bevel      project dir  .bevel/
```

One word everywhere. That is worth recording because it took three attempts to get here.

v0.4 proposed `orovp-harness` for the crate and `harness` for the binary, on the reasoning that a
registry needs uniqueness while `$PATH` needs brevity — **long name where uniqueness is required,
short name where ergonomics matter.** The principle is sound and still applies; what was wrong was
accepting a squatted bare word as inevitable. `harness` is taken on crates.io (v0.0.8), and rather
than prefix around it the better move was to pick a word nobody had taken.

`bevel` is free on crates.io, so the crate, the repository and the binary are all one string, and
only npm carries a scope — because npm scopes are conventional there, not because anything forces
it. Five characters, pronounceable, spellable from hearing, and no collision in `$PATH`.

The name earns its meaning too: a **bevel gauge** is the tool that transfers an exact angle from a
drawing to the work, which is close to literally what this does with a spec.

Registry state at the time of the rename, worth re-checking before the first publish:

| Name | Status |
|---|---|
| `bevel` (crates.io) | free |
| `@orovp/bevel` (npm) | free; the `@orovp` scope is free for public packages |
| `bevel` (`$PATH`) | no known collision |

**Renaming was nearly free only because nothing had been published.** The change touched the crate,
the npm packages, the binary, `~/.config`, `~/.cache`, the project state directory, the deny rule,
all three hooks, every doc, and the default method repository. The one thing that needed real care
was the state directory: `.harness/` → `.bevel/`. Discovery recognises the old name and prints the
exact `git mv` rather than claiming no project exists, and `sync` strips hook and permission entries
installed under the previous name instead of leaving both generations in place. After a publish
that same change would have been a migration to support indefinitely.

### Note on the Rust CLI

You picked Rust over my TypeScript recommendation. It is defensible, and in your specific case
it is **cheaper than usual**, for one reason: Linux is the only target. The usual pain of
shipping a Rust binary through npm is the platform matrix (macOS Intel/ARM, Windows, glibc,
musl…); with Linux only, it collapses to 2–3 triples.

The standard pattern (used by esbuild, swc, Biome, turbo):

```
@orovp/bevel                      -> main package, 20-line JS shim in bin/
  optionalDependencies:
    @orovp/bevel-linux-x64-gnu    -> binary named `bevel`
    @orovp/bevel-linux-arm64-gnu  -> binary
    @orovp/bevel-linux-x64-musl   -> binary (Alpine / containers)
```

npm resolves only the matching package via `os`/`cpu`, and the shim `execve`s the binary. This
can be generated with `dist` (formerly `cargo-dist`), which already emits npm packages, or with
a GitHub Actions matrix. **Realistic cost: one day of CI the first time, zero afterwards.**

What you gain, and it is not trivial: ~5 ms startup (the CLI runs from *hooks*, i.e. on the
critical path of every tool call), a single binary with no runtime dependency, and consistency
with Rust being one of your target languages — you get to dogfood your own packs.

### Two distribution channels, because one network may be closed

You said the work machine may not reach everything it needs. That turns distribution from a
one-line decision into a design constraint, and it is why Rust now looks like the better call
rather than merely a defensible one:

| Channel | Command | Works when |
|---|---|---|
| **Primary** | `npm i -g @orovp/bevel` | npm registry reachable |
| **Fallback** | `cargo install bevel` | crates.io reachable, npm is not |
| **Locked down** | `cargo install --git <your-host>/harness` | only your own git host is reachable |
| **Last resort** | drop the release binary in `~/.local/bin` | nothing is reachable; copy by hand |

The crate has to exist anyway to build the npm binaries, so the second channel costs one
`cargo publish` in CI. A TypeScript CLI would have had exactly one channel.

### The method lives in the repository, not in the binary

v0.1 said the method (skills, subagents, packs, templates) **must not** be compiled in, because
changing one sentence would mean republishing three binaries. v0.2 then softened that to "embed as
a fallback", because `cargo install` delivers only a binary and a strict rule would leave the work
machine with a CLI and no method.

**v0.5 goes back to the original rule and holds it.** Nothing is embedded. The method is fetched
from the GitHub repository and cached:

```
BEVEL_METHOD_DIR            ← a checkout, for working on the method itself
.bevel/project.toml         method_path, so a repo can host its own method
~/.config/bevel/config.toml [method] path
~/.cache/bevel/method/<repo>/<ref>/   ← fetched from GitHub, cached permanently
```

On top of whichever of those resolves, the usual override chain still applies: a file in
`.bevel/method/` beats one in `~/.config/bevel/method/`, which beats the tree.

**What this buys:** editing a skill or a pack takes effect on the next command, with no build, no
release, and no version bump. That is the whole reason for the change.

**What it costs, stated rather than buried:** a machine that has never fetched and cannot reach
GitHub has *no method at all*. That is a missing install, not a degraded mode, and it is a real
regression against the "may be restricted" constraint in §1. Three things keep it from biting:

| | |
|---|---|
| The cache is permanent | Fetch once per ref; never needed again on that machine |
| npm `postinstall` fetches | If npm just resolved a package, the network is demonstrably reachable |
| `[method] path` needs no network | Point at a checkout, or an rsync'd directory, and nothing is downloaded ever |

Every command that needs the method says so precisely when it is absent, and prints both routes —
`bevel method fetch`, or the `path` setting. `bevel fmt` is the one exception: a missing method
must never fail a file write, so it exits 0 silently.

**Pinning.** `[method] ref` takes a branch, a tag or a commit SHA. A branch is right while
iterating; a tag is right when two machines must agree. `bevel method where` prints a **content
hash of the method tree** — chosen over a commit SHA because it needs no API call and answers the
question that actually matters when two machines behave differently: whether the *instructions*
differ, not whether the commit does.

`bevel doctor` reports which layer every method file resolved from. Without that, "why is my
skill edit not taking effect?" is an afternoon.

## 2. Global layout, and how it survives two machines

You work from a home machine and a work machine. That single answer changed this section more
than anything else in v0.2, because it forces a clean split between *what must be identical
across machines*, *what must be reinstallable*, and *what must never travel*.

### Three layers, three lifetimes

```
── Layer 0: EMBEDDED ── compiled into the binary, fallback only (cargo-install path)

── Layer 1: BUILT-IN ── ships with the npm package, never edited, never synced
$(npm root -g)/@orovp/bevel/
├── method/          skills, agents, templates, schemas, rubrics
└── packs/           rust, ts, angular, milkdown, …

── Layer 2: USER ── small, syncable across machines (dotfiles / chezmoi / stow)
~/.config/bevel/
├── config.toml      model routing, budgets, preferences   ← NO SECRETS
├── packs/           your own packs, or overrides of built-ins
└── method/          optional overrides of individual skills or agents

── Layer 3: MACHINE ── regenerable, never synced, safe to delete
~/.cache/bevel/
├── context7/        docs cached by (library, version, topic)
└── registry.json    projects known on THIS machine
```

Resolution order for any method file or pack: **built-in → user → project (`.bevel/`)**.
Last one wins. This is what makes "add my own framework" a first-class operation instead of a
fork of the harness.

**Why XDG instead of a single `~/.bevel/`?** In v0.1 I proposed one directory. Multi-machine
kills that: you would either sync a 200 MB documentation cache to your work laptop or hand-craft
ignore rules. The XDG split gives you a sync boundary that needs no rules at all — *`~/.config`
syncs, everything else does not*. `BEVEL_HOME` remains as an override that collapses all three
into one directory for anyone who prefers it.

**Why the method is not synced but reinstalled:** it is a versioned artifact. Syncing it would
mean two machines drifting into states neither package manager nor git can reconcile.
`npm i -g @orovp/bevel@0.4.2` and `cargo install bevel --version 0.4.2` are both
reproducible; an rsync'd directory is not.

**On dotfile managers:** the design assumes none. Layer 2 is deliberately small enough
(`config.toml` plus whatever packs you have written) that copying it, committing it to a private
repo, or retyping it are all viable. `harness config export` is therefore **not** planned for
v1 — if it turns out you want it later, it is an afternoon's work, and building it now would be
scaffolding for a workflow that does not exist yet ("what can I stop doing?").

### Secrets never touch config.toml

The Context7 API key resolves in this order:

1. `CONTEXT7_API_KEY` in the environment
2. `key_command` in `config.toml` — e.g. `key_command = "pass show context7/api"`
3. Interactive prompt (TTY only)
4. No key: run against Context7's unauthenticated tier, degraded

**`key_command` is the point of this design.** It is the pattern git credential helpers and
neomutt use, and it is what makes `~/.config/bevel/config.toml` safe to commit to a *private
dotfiles repo* without ever writing a secret to disk. Without it, you would end up with a
gitignore rule you eventually forget.

### Version drift is the real multi-machine risk

Home is on harness 0.4, work is still on 0.2, the project's specs use schema v3. Silent
misbehavior on the work laptop is the worst possible outcome. Mitigation, three parts:

- `.bevel/project.toml` pins `harness = "^0.4"` per project.
- Every artifact carries `schema_version` in its frontmatter.
- `bevel doctor` **hard-fails** on mismatch and prints the exact `npm i -g` line to fix it.

Failing loudly is the whole point. A harness that behaves subtly differently depending on which
laptop you opened is worse than no harness.

---

## 3. Monorepo model

Monorepo always. Concretely:

```
monorepo/
├── INBOX.md                  # ONE inbox, at the root
├── AGENTS.md                 # root: cross-cutting gotchas only — yours, from `bevel notes`
├── CLAUDE.md                 # 3-line stub pointing at AGENTS.md
├── docs/
│   └── architecture.md       # the design record (see §4)
├── specs/
│   ├── README.md             # generated index: id, title, status, gate, packages
│   └── 0007-example-feature/ # a spec may span several packages
├── apps/
│   └── web/                  # TypeScript → optional package-local AGENTS.md
├── crates/
│   └── core/                 # Rust       → optional package-local AGENTS.md
└── .bevel/
    ├── project.toml          # workspace map + per-package pack overrides
    ├── gates.lock            # approval hashes  ← goes into git
    └── cache/                # context packs  ← gitignored
```

### Three decisions worth arguing

**One `INBOX.md` at the root, not one per package.** An idea rarely knows which package it
belongs to before it has been shaped — that is precisely what shaping determines. Forcing you to
file it correctly at capture time adds friction at the exact moment friction is most damaging:
the moment you have an idea and eight seconds of attention. Items may carry an optional
`[scope: web]` tag, but it is a hint, never a requirement.

**Global, monotonic spec IDs.** In a monorepo a feature routinely spans packages: one that
changes a data format touches the Rust crate that writes it and the TypeScript app that reads it,
in the same piece of work. Per-package IDs would make cross-package specs either unrepresentable
or duplicated, and duplicated specs desynchronize within a week. `spec.md` frontmatter carries
`packages = ["crates/core", "apps/web"]` so the index can still be filtered per package.

**Package-local `AGENTS.md` is optional and additive.** The root file holds only cross-cutting
rules; a package file holds that package's gotchas. Claude Code reads nested context files when
working inside a subtree, so this composes naturally, and it keeps the root file inside its
50-line budget (§13) as the repo grows.

### Scoped verification without a task runner

No nx, no turbo — cargo and npm side by side. That was the open question gating Phase 1, and the
answer is better news than I expected when I asked it.

**Why this is load-bearing rather than an optimization:** in a monorepo a naive full
`bevel verify` becomes a multi-minute run. The implement loop (§8, Phase 3) verifies *after
every task*. At that cost per task the loop is unusable, you disable verification, and the whole
determinism argument of this design collapses. Scoped verification is what keeps R4 affordable.

**The good news: cargo already ships the affected graph.** `cargo metadata --no-deps` returns the
complete workspace dependency graph as JSON, offline, in a few hundred milliseconds. I had
budgeted for parsing `[dependencies]` sections by hand; that is not necessary. npm workspaces
need slightly more work, but only slightly: inter-package edges are the entries in each
`package.json`'s `dependencies`/`devDependencies` whose names match another workspace package.

Algorithm for `bevel verify --affected`:

```
1. CHANGED   git diff --name-only <merge-base>...HEAD  +  git status --porcelain
2. MAP       longest-prefix match of each path against the package map
3. EXPAND    add every package that depends on a changed one (transitively)
               rust → edges from `cargo metadata --no-deps`
               npm  → edges from workspace-internal dependency entries
4. RUN       one batched invocation per ecosystem, not one per package
               cargo nextest run -p core -p api -p cli
               npm test --workspace=a --workspace=b
```

**Step 3 is the one that is easy to get wrong.** Changing a core crate must verify everything
that depends on it, not just the crate itself. A file-to-package map without the dependency
expansion produces false greens, and a verification tool that reports false greens is worse than
no verification tool — it converts a caught bug into a trusted one.

**Step 4 batches deliberately.** N separate `cargo nextest` invocations pay N times for
dependency resolution; one invocation with N `-p` flags does not.

Conservative defaults, because a false green costs more than a slow run:

- Any change to a lockfile, a root manifest, CI config, or `.bevel/` → **full verify**.
- A path matching **no** package → **full verify**. Unknown means unknown.
- Affected set exceeds ~60% of packages → **full verify**; scoping has stopped paying.
- `--all` and `--since <ref>` as manual escape hatches.

The package map is cached in `.bevel/cache/`, keyed by a hash of every manifest file, so
`cargo metadata` runs on manifest change rather than on every verify. Total overhead of the
affected computation: well under a second, replacing minutes.

**Detection.** `bevel doctor` builds the map from `Cargo.toml [workspace].members` (globs
expanded) and `package.json` `workspaces`, writing the result into `project.toml`. Detected, not
hand-maintained — hand-maintained maps drift, and a drifted map is exactly the false-green
scenario above.

---

## 4. Greenfield adaptations

New projects, no legacy. Two consequences.

**`domain-scout` inverts its priority order.** In an existing codebase it scans code. In an
empty repo there is no code to scan, so it reads, in order: `docs/architecture.md`, the `specs/`
index, then whatever code exists. Early on this makes it very cheap; as the repo grows it
naturally shifts toward code without any configuration change.

**The specs directory *becomes* the design record.** With no legacy to consult, prior specs are
the only institutional memory. This raises the value of `decisions.md` considerably: it is not
documentation of a system, it is the system's reasoning history.

### The foundation spec — decided

You left this one to me. **Yes to `specs/0000-foundation/`, but shaped at shallow depth, not the
full pipeline.** The split:

| Concern | Who handles it | Why |
|---|---|---|
| Directory skeleton, manifests, harness wiring, `.gitignore` | `bevel project init` | Mechanical. Has a right answer. Needs no conversation. |
| Module boundaries, error strategy, test approach, layering rules | `specs/0000-foundation/` | Genuine decisions. Need the reasoning recorded. |

The failure mode being prevented is specific to greenfield: with no architectural baseline,
*every* spec silently re-litigates where modules live and how errors are modeled. That is a slow
leak that runs for the life of the project, and its cost is invisible because it never shows up
as a single bad decision.

**Why shallow rather than the full pipeline, and this is the part that decided it for me:** the
blind-spot fan-out (§6, Phase 2) is *useless* on an empty repository. `domain-scout` has no code
to scan, no prior specs to collide with, and no forgotten decisions to surface. Running three
parallel subagents to search an empty directory is pure coordination cost with nothing to
coordinate — precisely what R6 forbids. `0000` is an interview plus a written record.

This is also the cleanest illustration of why depth is judged rather than passed (§8): the reason
to go shallow here is a *fact about the repository* that the model can observe directly — it is
empty — not a preference you have to remember to declare.

Its implementation output is `docs/architecture.md`, which then becomes the first thing
`domain-scout` reads for every subsequent spec (§4, above). So the cost is one short session and
the return starts with spec `0001`.

---

## 5. Lifecycle

```
INBOX.md
  │  bevel shape 3      (CLI reserves ID, creates folder, links the item)
  ▼
specs/0007/  status: draft
  │  /shape                    (blind spot → interview → shapes → mockup → spec → critique)
  ▼
status: review
  │  bevel approve 0007 ← HUMAN, requires a TTY. Freezes the hash.
  ▼
status: approved  +  gates.lock
  │  /implement 0007           (gate → context pack → plan → build → verify → review)
  ▼
status: done  + commit SHA in gates.lock
  │
  └─→ unresolved deviations from notes.md → back into INBOX.md
```

The loop closes. That matters: what you learn while implementing is exactly the raw material
for the next `/shape`, and today that knowledge evaporates.

| State | Who sets it | What it unlocks |
|---|---|---|
| `draft` | `bevel shape` | free editing |
| `review` | `/shape` on completion + `bevel validate` green | request approval |
| `approved` | **human** via `bevel approve` (TTY) | `/implement` may start |
| `implementing` | `/implement` after passing the gate | writing code |
| `done` | `/implement` on close | archived |
| `superseded` | the superseding spec, at its own `done` | nothing — see below |

### One active spec at a time

The pleasant consequence of this answer: **it needs no new state at all.** A spec whose status is
`implementing` *is* the active spec, and the rule is simply that at most one may hold that status.

I had assumed this would need a lock file in `.bevel/`. It does not, and avoiding one is
strictly better:

- No second source of truth to fall out of sync with the specs themselves.
- It travels between your machines for free, because `spec.md` is already in git — you can start
  at home and `git pull` into the same state at work.
- It is visible in `git status` and in a diff, rather than hidden in a state file.
- Nothing to clean up after a crashed session.

Enforcement is one check: `bevel gate <id>` fails if a *different* spec is already
`implementing`, and names it. That is the same exit code the pipeline already depends on, so it
costs nothing new. What shipped folds the two moves together: `bevel start <id>` runs that check
*and* writes the status, so there is no window in which a spec has passed the gate but not yet
claimed the slot, and no way to claim one without passing. `gate` remains on its own for anything
that wants to ask without claiming.

**Pausing.** `bevel pause <id>` returns the spec to `approved`. The hash is untouched, so
resuming needs no re-approval. Nothing is lost by pausing because **progress was never stored in
the status** — it is the state of each tier A criterion the spec declares, read back from the
code (§7). A half-finished spec reports `4/7 criteria live` whether it is active or paused, and
that number is recomputed rather than remembered.

The question is asked of the spec, not of the repository, and that is load-bearing. Counting how
many times the marker string appears anywhere under the root counted a spec's own prose
explaining the convention, and test fixtures using a real id as sample data — bevel did this to
itself and refused to close spec 0001 over four markers that were not markers. Bounding the
search by what the frontmatter declares makes both impossible rather than merely unlikely, and
makes `live + pending + missing` equal the declared total, which two counts from two sources
never did.

**What the rule does not block.** The constraint binds `/implement`, not your keyboard. The
`chore` fast path (§8) stays available while a spec is active — otherwise a one-line fix
discovered mid-implementation would require pausing the very work that found it, which is the
kind of rule people route around within a week.

### Amending an approved spec

Edit the file and re-approve. No `amend` command, no ceremony — the hash mechanism (below)
already produces the correct behavior on its own, which is the strongest argument for not
building anything.

The case worth specifying is amendment **during** implementation. `/implement` re-checks the gate
at each phase boundary, so a mid-flight edit halts the pipeline at the next phase rather than
letting code drift from a contract that no longer says what it said. You re-approve, you resume.
Halting is correct here: you changed the agreement while the work was being done against it.

One thing recovered for free, since you declined the command that would have provided it: on
detecting a broken hash, `/implement` appends the diff summary of `spec.md` to `decisions.md`
before halting. An amendment made *after* implementation started is exactly the kind of decision
worth a record, and this gets that record without a new command or a new habit.

### Superseding a spec, and what happens to its tests

You asked what becomes of a superseded spec's acceptance tests. They are passing tests asserting
behavior that has just been redefined: deleting them loses coverage of whatever the new spec did
*not* redefine, and keeping them freezes an obsolete contract that the new implementation will
break at the worst moment.

**Decision: supersession is not a status you set. It is a line-by-line reckoning with the old
contract, performed by the superseding spec.**

Spec 0031's frontmatter declares `supersedes = ["0007"]`. `bevel validate 0031` then requires a
disposition for **every Tier A criterion of 0007** — no exceptions, no defaults:

| Disposition | Meaning | What happens to the test |
|---|---|---|
| `inherited` | the behavior still holds | test moves to 0031's ownership, unchanged |
| `replaced` | 0031 covers the same concern differently | old test deleted, new named test required |
| `dropped` | the behavior is intentionally gone | test deleted, **reason recorded in `decisions.md`** |

0007 flips to `superseded` only when 0031 reaches `done`. Until then the old contract is still the
contract, which is correct — the replacement does not exist yet.

**Why this rather than something lighter.** The entire value of Tier A criteria is that they form
a contract a machine can check (§7). If a status flip could dissolve that contract without
accounting for it, the contract was never binding in the first place — it was a comment with a
test runner attached. Supersession is the only transition that destroys prior guarantees, so it is
the only one that earns a validation requirement.

The `dropped` case is the one that pays for the whole mechanism. Deleting a passing test is
exactly the change that should require a written reason, and it is exactly the change that never
gets one. Here it cannot happen silently.

Cost: a checklist over three to seven items, generated automatically from the old spec's
frontmatter. Cheap, because §7 already made the criteria a structured list rather than prose.

### The gate, made real

Most "spec-driven development frameworks" lie here: they leave the gate as a markdown
instruction (*"do not implement until the human approves"*) and the agent, in good faith, walks
straight past it. **An instruction is not a guardrail** (R4).

1. **Approval is a hash, not a boolean.** `bevel approve 0007` computes `sha256` over the
   canonicalized bytes of `spec.md` (body, plus the criteria declared in frontmatter, excluding
   gate-managed fields) and writes:

   ```toml
   [spec.0007]
   hash = "sha256:9f2c…"
   approved_at = "2026-08-02T15:40:00Z"
   approved_by = "orovp"
   bevel_version = "0.1.0"
   ```

   Edit the spec later and the hash stops matching, so **the gate reopens by itself**. That
   property is what keeps this from decaying.

   *Not hashed:* `decisions.md`, `open-questions.md` and `mockup.html`, which are logs and
   references — annotating a log must not invalidate an approval.

   **Also not hashed, and this is a correction:** the *bytes* of `acceptance.*`. Earlier drafts
   included them, which was self-contradictory — implementation exists precisely to fill in the
   test bodies that shaping left as `todo!()`, so the first thing any real `/implement` run did
   was reopen the gate it had just passed. Running the pipeline end to end is what surfaced it.
   The contract is the **named behaviours**, declared in frontmatter and hashed there; a body is
   how you *prove* one, which is implementation. `validate` separately enforces that every named
   test exists, so nothing is lost.

2. **`bevel approve` requires a TTY.** An agent's bash is non-interactive, so it cannot
   self-approve by accident.

3. **A deny rule in the Claude Code adapter** on `Bash(bevel approve*)`. Second layer.

4. **The gate is the first thing `/implement` runs**, and it is an exit code — in the shipped
   pipeline via `bevel start <id>`, which checks it and claims the slot in one step.

Because `gates.lock` is in git, an approval made at home is present at work after a pull. That
is the multi-machine payoff of having versioned it.

> **Honest threat model:** this is a speed bump against accidental self-approval, **not a
> security boundary**. The agent runs as your user and could edit `gates.lock`. It protects
> against the real failure mode — an enthusiastic agent pushing ahead — not against an
> adversary. GPG-signing `gates.lock` is the escalation path if you ever need one; today it
> would be over-engineering.

---

## 6. The `/shape` pipeline

Goal: turn a line of `INBOX.md` into a spec an agent can implement without asking you anything
else. The success metric is literally that: **zero questions during `/implement`**.

🔶 **On the name "shape":** it matches Basecamp's *Shape Up*, and adopting its vocabulary seems
worth it because it maps onto exactly what you described — *appetite* (how much time you are
willing to spend, fixed before designing), *rabbit holes* (holes identified and patched up
front), *no-gos* (explicitly out of scope), *breadboarding* (sketching flow without designing
UI). If the reference is new to you, say so and I will justify it properly.

**Phase 0 — Bookkeeping (CLI, deterministic).** `bevel shape 3` reads item 3 from `INBOX.md`,
reserves the next monotonic ID, derives the slug, creates `specs/0007-slug/` from a template,
and links the item back in `INBOX.md`. *Why the CLI:* assigning IDs and not clobbering folders
is exactly where a model fails 2% of the time and `mkdir` fails 0%.

**Phase 1 — Framing (main agent).** Reads the item, `project.toml`, the `specs/` index,
`docs/architecture.md`, and the workspace map. Cheap, no subagents.

**Phase 2 — Blind spot pass (fan-out ×3, read-only).** ← *here the coordination cost is earned*

- `domain-scout` — architecture doc and prior specs first (§4), then code: existing patterns,
  colliding specs, decisions already made that you have forgotten.
- `risk-scout` — failure modes, data migration, security, performance, operations.
- `unknowns-scout` — Fable's taxonomy applied explicitly, focused on **unknown knowns**: what
  you take for granted and therefore never write down.

*Why parallel here:* three independent read-only searches with cheap synthesis. Isolation also
keeps three repo explorations from polluting the context where the interview happens next — the
part that most needs to stay clean.

Synthesis lands in `open-questions.md`, **deduplicated and ranked by decision impact**. That
filter is critical: without it you get a 40-question interrogation and abandon the harness in
week two. Only questions that change the design get asked; the rest become explicit assumptions.

**Phase 3 — Interview (main agent, you in the loop).** One question at a time. Each answer is
written to `decisions.md` as a timestamped Q&A record.

> **Architectural constraint:** subagents **cannot talk to you**. Anything needing a human runs
> on the main thread. This rules out, at the root, any design with an "interviewer subagent."

*Why `decisions.md` is first-class:* it is the "why", and the "why" is what git does not
preserve. In six months `git blame` tells you *what* changed; `decisions.md` tells you *which
alternative you rejected and on what grounds*. In a greenfield project it is the only
institutional memory that exists.

**Phase 4 — Solution shapes (generate-and-filter) + optional spike.** Two or three approaches
with their appetite, tradeoffs and rabbit holes. You pick or blend. For the highest technical
risk, a **throwaway prototype** in a separate worktree: cheap validation before expensive
implementation. Twenty minutes of spike kills two-day plans with uncomfortable frequency. The
prototype is discarded; the findings survive in `decisions.md`.

### Phase 4b — Breadboard, then mockup (new in v0.2)

You said HTML mockups are interesting. They are, and they are also the easiest artifact in this
whole design to turn into a liability. Two rules keep them useful.

**Breadboard first, always.** Shape Up's breadboarding — *places*, *affordances*,
*connections* — as plain text or a mermaid diagram. It costs almost nothing and resolves most
UI ambiguity on its own:

```
[Editor]  --(toolbar: Sync)-->  [Sync status panel]
          --(conflict banner)-->  [Conflict resolver]  --(Keep local / Keep remote)--> [Editor]
```

**Mockup only when the visual shape is genuinely uncertain.** A `mockup-builder` subagent
produces `specs/NNNN/mockup.html`: one self-contained file, no external requests, no build step,
opened with `xdg-open`. Subagent because HTML generation is token-heavy and entirely
self-contained — a textbook isolation case.

**The rule that keeps it from becoming a liability: the mockup is a reference, not a
deliverable.** At `status: done` it is frozen and marked historical. It is never maintained
against the implementation. Without this rule you acquire a second UI to keep in sync forever,
and the harness starts costing more than it returns. Its job is to make the review conversation
concrete *before* code exists, and it retires the moment code exists.

Its second job is anchoring Tier C acceptance criteria (§7): "matches the conflict banner in
`mockup.html`" is reviewable by you in ten seconds; three paragraphs of prose are not.

**Phase 5 — Writing the spec.** `spec.md` + `acceptance.*` per §7.

**Phase 6 — Adversarial critique (subagent, fresh context).** (R7) `spec-critic` scores against
`rubrics/spec-quality.md`: ambiguity, testability of each criterion, contradictions with
existing specs, unbounded scope, missing no-gos, and — new in v0.2 — whether the spec has at
least one Tier A/B criterion. **It reports only, it does not fix.** Fresh context precisely to
avoid the self-preferential bias the workflows article describes.

**Phase 7 — Close.** `bevel validate 0007` → `status: review`. You read. You approve.

### Model routing 🔶

The workflows article notes you can choose *"which models an agent uses."* Proposal,
configurable in `config.toml`:

| Phase | Model | Reason |
|---|---|---|
| Blind spot + interview | **Fable** | Literally its use case: long horizon, discovering unknowns |
| `spec-critic` | Opus | Judgment, and different from whoever wrote it |
| `mockup-builder` | Sonnet | Bounded, mechanical, token-heavy |
| `context-packer` | Haiku/Sonnet | Mechanical, token-heavy |
| Implementation | Opus | Code |

---

## 7. Acceptance criteria — the three tiers

This replaces v0.1's binary choice between "failing tests" and "prose", which was the wrong
framing.

**First, what R3 is not: it is not TDD.** There is no red-green-refactor cycle and `/shape`
never writes a test *body*. It writes named, empty, failing test functions — a **naming
exercise**, not a testing exercise. `/implement` fills them in.

```rust
// acceptance.rs — emitted by /shape, must fail until /implement is done
#[test] #[ignore = "acceptance: pending"]
fn conflict_prefers_local_when_remote_is_older() { todo!() }
```

**Why it is the highest-yield rule:** it turns *"am I done?"* from a model judgment into an exit
code. Without it, `/implement` finishes when the agent decides it has finished, and agents are
optimistic. **Secondary benefit:** if you cannot name the test, the spec is still vague — and
that discovery costs two minutes during shaping instead of three days during implementation.

**The honest objection:** not everything is nameable as a test. Hence three tiers.

| Tier | Form | Verified by | Example |
|---|---|---|---|
| **A — Executable** | Named failing test | `bevel verify` exit code | `conflict_prefers_local_when_remote_is_older()` |
| **B — Commanded** | Any command with an exit code | `bevel verify` exit code | bundle < 500 kB, `ng build` clean, axe a11y pass |
| **C — Judged** | Prose + a pointer into `mockup.html` | **The human, at review time** | "the toolbar stays responsive while typing" |

Rules:

1. Tier A and B criteria are the **stop condition**. `/implement` cannot report done while any
   of them is red.
2. Tier C criteria are **never** marked satisfied by the agent. They render as a checklist you
   tick. This is the honest version of what every other harness gets wrong: they let the model
   self-grade subjective quality, and it always awards itself a pass.
3. **A spec with zero Tier A/B criteria fails `bevel validate`.** Subjective criteria are
   allowed; *only* subjective criteria are not.

Tier B is what makes this workable for Angular and Milkdown, where much of the value is not unit
testable but is still commandable: build succeeds, bundle budget holds, no a11y regression, no
`tsc` errors, Lighthouse over a threshold. Those are exit codes, and exit codes are R4.

### The naming rule

**A test name must survive a refactor of the implementation.** If renaming a struct or a service
forces you to rename the test, the name was describing implementation, not behavior.

```
✅  conflict_prefers_local_when_remote_is_older
✅  empty_document_syncs_without_creating_a_revision
❌  test_sync_manager_resolve                    ← names a class
❌  sync_works                                   ← names nothing
```

Read as a sentence: subject, verb, condition. That form is hard to write while your thinking is
still vague, which is the point — the difficulty *is* the signal.

**Count as a scope signal.** Three to seven Tier A criteria is the healthy range. Below three,
the spec is probably still hand-waving. Above ten, the spec is too big and should be split — it
is the same information Shape Up's *appetite* is trying to give you, arriving from a different
direction and much harder to argue with.

### Lifecycle of a single acceptance test

This is the part that needs to be exact, because a named-but-uncompiled test is not a stop
condition — it is a comment.

```
/shape       specs/0007-example-feature/acceptance.rs
             named + #[ignore = "acceptance: 0007 pending"] + todo!()
                │
approve        hashed and frozen alongside spec.md          ← §5
                │
/implement     RELOCATED into the real package by the plan:
             crates/core/tests/acceptance_0007.rs      still ignored
                │
per task       body filled, #[ignore] removed, must pass
                │
done           zero remaining "acceptance: 0007" ignores    ← enforced by verify
```

**Why it starts in the spec folder and moves later.** In a greenfield monorepo, the crate that
will host the test frequently does not exist when the spec is written. Emitting into the spec
folder keeps the artifact versioned and hashable next to `spec.md`; relocation is a planned task
in `plan.md` (§8, Phase 2), which is also where the target package finally gets decided.

**Why it is inert until implementation.** An approved-but-unimplemented spec must not break CI —
otherwise you accumulate red builds proportional to your backlog and immediately stop approving
specs. Rust's `#[ignore]` is skipped by `cargo nextest run` and included by
`--run-ignored all`, so `bevel verify` can target exactly this spec's tests while the normal
suite stays green. `/implement` removes the marker as each body is filled, and closing the spec
requires zero markers left. The gap between "approved" and "done" is therefore *countable*.

Per-language emission comes from the pack's `acceptance.tmpl`:

| Pack | Pending form | Run-anyway flag |
|---|---|---|
| `rust` | `#[test] #[ignore = "acceptance: NNNN pending"] fn name() { todo!() }` | `--run-ignored all` |
| `ts` (vitest) | `test.todo('name')` | reported as todo, counted by `verify` |
| `ts/angular` (jasmine) | `xit('name', () => pending())` | filtered by spec tag |

### The frontmatter contract

Tiers live in `spec.md` frontmatter so that `bevel validate` is a lookup rather than a parse of
prose:

```yaml
acceptance:
  - tier: A
    test: conflict_prefers_local_when_remote_is_older
  - tier: B
    cmd: "npm run build --workspace=web && node scripts/bundle-budget.mjs 500"
  - tier: C
    text: "The conflict banner matches mockup.html §2"
```

`bevel validate` then checks three things, all deterministically: at least one A or B exists;
every A has a matching test name present in `acceptance.*`; every C has non-empty prose. No model
judgment anywhere in that check — which is the whole reason the tiers are declared rather than
inferred.

---

## 8. The `/implement` pipeline

Goal: execute an approved spec without reopening design decisions.

**Phase 0 — Claim.** `bevel start 0007`. Exit code. If it fails, stop. Gate and slot are taken
together (§5), so the three failure modes it reports — not approved, hash mismatch, another spec
active — are the same three the gate has always had, plus the one the slot adds.

**Phase 1 — Context pack (isolated subagent).** Detects which frameworks the spec touches,
reads **the lockfiles** for exact versions, queries Context7 for *those* versions, writes
`.bevel/cache/context-pack-0007.md` with only the relevant API surface.
*Why isolated:* pulling docs burns enormous token counts on material the implementer never needs
raw. Textbook context isolation — the implementer reads the distilled pack, not 40k tokens of
documentation.

**Phase 2 — Plan.** Approved spec + today's code → `plan.md`: ordered tasks, **the package each
task belongs to**, files touched, risks, and **the verification command per task**. The last two
fields are what make the plan executable rather than decorative.

Task zero is always the same and is generated, not invented: relocate `acceptance.*` from the
spec folder into the target package's test directory (§7). Deciding that target is the moment
the plan commits to a package, which is why it cannot happen during shaping.

**Phase 3 — Implementation loop.** Sequential in the main context **by default** (R6). Parallel
worktrees only when the plan marks tasks `parallel = true` and there are at least three.
*Why sequential by default:* coordinating worktrees, resolving conflicts and synthesizing diffs
costs more than it saves for most features. A monorepo makes parallelism *more* attractive than
usual — tasks in `crates/core` and `apps/web` genuinely do not collide — so this is the
default most likely to be relaxed once there is real data.

Each task: implement → run its verification command (`--affected`, §3) → fix → next. Every
deviation goes into `notes.md`, following Fable's rule: *"If you hit an edge case, pick the
conservative option, log it, and keep going."*

**Phase 4 — Deterministic verification.** `bevel verify --affected` runs the active packs'
commands, plus every Tier B criterion from the spec. Loop-until-done **with an attempt budget**;
once exhausted, stop and report rather than thrash. The explicit budget prevents the three-hour
session burning tokens against a test that was never going to pass.

**Phase 5 — Adversarial diff review (subagent, fresh context).** (R7) `diff-reviewer` checks
`git diff` against the spec's acceptance criteria and the pack rubric. It did not write the
code, so it cannot prefer its own decisions.

**Phase 6 — Reconciliation and human checklist.** Tier C criteria are presented to you as a
checklist — this is the second human touchpoint, and the only place subjective quality is
judged. Deviations in `notes.md` resolve one of three ways: fixed, or they amend the spec —
which **breaks the hash and forces re-approval**, the correct behavior — or they become new
`INBOX.md` items.

Closing requires **every declared tier A criterion to be live** (§7): its test found, and no
`acceptance: 0007 pending` marker on it. An agent cannot
declare done while a named criterion is still ignored, and because the criteria are countable,
`bevel status` can show partial progress as `5/7 criteria live` instead of a self-reported
percentage. Then `status: done` and the commit SHA lands in `gates.lock`.

That last step is `bevel close <id>`, and it is a command rather than an edit for the same reason
approval is: **the status is the enforcement point, so nothing that the status gates may be free
to write it.** `close` re-runs verification, refuses while a declared criterion still carries its
marker, refuses when a criterion names a test that exists nowhere, refuses if the spec was amended
after approval, and refuses on Tier C until a human has confirmed the checklist in a terminal. An
agent editing `status: done` by hand would bypass all five at once.

The second of those is the one the marker count could not see at all: a spec whose tier A tests
were never written has no markers, so it read as nothing left to do. No test is not a passing
test — it is a criterion that passes by never running, which is the failure this whole tier
exists to prevent.

### Depth, and why there is no `--quick` flag

Applying the full pipeline to a ten-line change is absurd and would be the number one reason you
abandon the harness. So depth has to vary. The open question was whether that variation is a flag
you pass or a judgment the model makes. **Decision: no flag.**

Three arguments, in increasing order of how much they convinced me:

**A flag is a rule where judgment works.** This is the whole thesis of the context engineering
article — Anthropic deleted 80% of a system prompt by trusting judgment over rules. A depth flag
is a rule I would be writing into every skill, every example and every piece of documentation, to
encode a decision the model can make by reading the item.

**A flag asks you to decide at the worst possible moment.** You would have to choose the depth
*before* anything has looked at the idea. But whether an idea needs a blind-spot pass is precisely
what you cannot know until something has looked at it. The classification belongs after the read,
not before it.

**A flag prevents the pivot.** The field guide's warning is exact here: *"If you are too specific,
Claude will follow your instructions even when a pivot may be more appropriate."* `--quick` is
about as specific as an instruction gets. Pass it on an idea that turns out to be load-bearing and
you get a shallow spec, delivered obediently.

What replaces it:

```
depth is proposed, not passed:

  /shape 3
    → reads the item, states a depth and a one-line reason
    → "This looks like a chore: renaming a config key. No spec needed. OK?"
    → you confirm, or redirect in plain language ("no, this touches the wire format")
```

**Start shallow, escalate on evidence.** The bias is deliberate and asymmetric: escalating costs
one extra step, while de-escalating after three subagents have already run costs the tokens they
burned. So the interview can stop and say *"this is bigger than it looked — three questions in,
I have found two decisions that conflict with spec 0004. Run the blind-spot pass?"* That
conversation is impossible with a flag, because the flag already settled it.

The three depths still exist as *behaviors*; they simply are not switches:

| Depth | What runs | Typical trigger |
|---|---|---|
| chore | nothing — implement directly | mechanical change, no decisions |
| shallow | interview + spec, no fan-out, no mockup, no spike | one obvious approach |
| full | everything in §6 | real unknowns, or escalated into |

---

## 9. Multi-agent compatibility

| Tier | What it is | Agents |
|---|---|---|
| **0 — Universal** | `AGENTS.md` + file artifacts + the `bevel` CLI | Any |
| **1 — Method loaded** | the skills load as skills | Claude Code, opencode — one tree, two readers |
| **2 — Context isolation** | the seven subagents | Claude Code, opencode — rendered per agent |
| **3 — Hooks** | format-on-write, session start, stop | Claude Code |

The earlier version of this table promised a tier of *commands rendered per adapter* for five
agents. Nothing rendered commands for anyone, and the row survived a release in which the
multi-agent code was deleted (`dc3bbef`) — which is the exact failure mode §13 is about, in
prose rather than in markdown budget. Tiers now name what the code does.

The tiers are also no longer a ladder each agent climbs wholesale: opencode reaches 2 without
1 needing any work, because it scans `~/.claude/skills` already. Agents differ **per resource
kind**, which is why `sync` is one trait per kind rather than one renderer per agent.

**The idea holding this up (R5): artifacts are the portability layer.** Because every phase
*writes a file* and the next one *reads a file*, the pipeline still works when the "subagent" is
just "the same agent in a new session." An agent without subagents runs the phases sequentially
over the same `.md` files; it loses context isolation, not functionality. This is precisely the
field guide's pattern: *compile artifacts and hand them to a fresh session*.

| Missing capability | Degradation |
|---|---|
| Subagents | Sequential phases; open a new session between heavy phases |
| Hooks | The gate stops auto-blocking; still `bevel start` at step 0 |
| MCP (Context7) | `bevel docs` over HTTP from the CLI (§10) |
| Worktrees | Everything in the main tree |

**Single source:** content lives in the built-in/user method layers; `bevel sync` projects it
into `~/.claude/skills/`, `~/.claude/agents/`, `~/.config/opencode/agents/` and
`.claude/settings.json`. Every generated file carries a marker comment, and a file without one
is treated as the user's and never clobbered.

Symlinks were considered and are not used: a subagent rendered for opencode is a *translation*
of the source, not a copy of it, so there is nothing for a symlink to point at.

**`sync` does not write `AGENTS.md` or `CLAUDE.md`. `bevel notes` prints them and the user
applies what they want.** That list above is configuration — directories under `~`, one JSON
file — and those two are prose a project writes about itself. Treating them as generated output
was the design error, and every sharp edge in that code was a symptom of it: `CLAUDE.md` shipped
without a marker, so classifying it needed a byte comparison against a frozen copy of what
bevel used to write; the seed and that fingerprint were the same constant, so improving the seed
silently reclassified every file already on disk; getting it wrong meant *relocating a user's
own writing*; and a `CLAUDE.md` symlinked at `AGENTS.md` had to be detected before it was
followed or the body was destroyed on alternate runs. Four hazards, one cause.

Printing costs a redirect and removes all four:

```
bevel notes > AGENTS.md
bevel notes claude > CLAUDE.md
```

There is nothing to classify, because bevel never reads these files back; nothing to migrate,
because it never wrote them; and nothing to clobber. The budget in §13 is what keeps them
honest afterwards, and it applies to a hand-written file exactly as it did to a generated one.

**This breaks projects initialised before the change** — their `AGENTS.md` and `CLAUDE.md` carry
a generated marker that now means nothing, and a stale command list that no `sync` will refresh.
The fix is the same one line: `bevel notes > AGENTS.md`, then re-add your own gotchas. Accepted
deliberately, because a migration path here would be one more piece of code reaching into files
this section has just established are not bevel's to touch.

**`CLAUDE.md` is a three-line stub pointing at `AGENTS.md`.** Anti-duplication rule: the same
instruction must never exist in two places. The article lists this as an explicit anti-pattern,
and in practice it is how a harness starts contradicting itself. That is why `bevel notes claude`
prints a pointer and not a second copy of the body — but it is a recommendation now, and a user
who wants one file, or neither, is not fighting the tool to get it.

---

## 10. Packs + Context7

```toml
# packs/rust/pack.toml
id      = "rust"
detect  = ["Cargo.toml"]
version = { from = "Cargo.lock" }

[context7]
library = "/rust-lang/rust"          # ⚠ IDs to confirm with resolve-library-id

[[verify]]
name = "fmt"  ; cmd = "cargo fmt --check" ; fix = "cargo fmt"
[[verify]]
name = "lint" ; cmd = "cargo clippy --all-targets --all-features -- -D warnings"
[[verify]]
name = "test" ; cmd = "cargo nextest run" ; scoped = "cargo nextest run -p {package}"

[acceptance]
template = "acceptance.rs.tmpl"
```

The `scoped` field is what `verify --affected` uses in a monorepo (§3).

Plus `gotchas.md` (≤80 lines), loaded **only** when the pack is active *and* the task touches it.

**What belongs in `gotchas.md` — and what does not.** A pack is **not** a tutorial. The model
knows Tokio better than either of us. What it does not know: that you use `nextest` rather than
`cargo test`, that your `clippy.toml` forbids `unwrap()` in production code, that errors use
`thiserror` in libraries and `anyhow` in binaries, or the specific friction between Diesel and
`async` and how you resolve it. **Conventions, verification, and version-specific traps.** If a
sentence could have been written by the model from memory, it does not belong (R1).

### Planned packs

```
v1:     rust/  { tokio, diesel, reqwest, tower }
        ts/    { angular, milkdown }
later:  rust/tauri            (deferred — no verification story yet, §16)
```

Inheritance: a framework pack inherits from its language pack and adds only detection, Context7
library ID, gotchas and extra commands. Layering across the three global layers (§2) means your
own packs live in `~/.config/bevel/packs/` and travel between machines with your dotfiles.

### Context7 — four decisions

1. **Pin the version, never ask for "latest."** The real version comes from the lockfile and the
   docs *for that version* are requested. Always fetching newest reintroduces the problem in
   reverse: docs for a version you do not run.
2. **Access through the CLI, not only MCP.** `bevel docs tokio --topic "graceful shutdown"`
   hits the API and caches. Portability: MCP is not available in every agent, so this keeps
   behavior identical everywhere. If Context7 MCP is present the agent may use it directly.
   Final fallback: `WebFetch` of the official docs.
3. **Two-level cache.** Global by `(library, version, topic)` with a TTL in `~/.cache/bevel/`,
   materialized per task in `.bevel/cache/context-pack-NNNN.md`. Determinism within a task
   (two phases see identical docs) and lower cost. Machine-local by design (§2).
4. **Privacy:** these queries leave your machine. Library names and search topics form an
   information channel — minor, but worth knowing, particularly on a work machine.
5. **Offline is a supported mode, not an error path.** See below.

### Degrading when the network is closed

You said the work machine may not reach everything, and that failure should be graceful. That is
worth stating as a principle rather than a set of `try/catch` blocks, because it constrains where
network calls are allowed to appear at all:

> **No network call sits on the critical path of `gate`, `validate` or `verify`.**
> Those three must work identically with the cable unplugged. Context7 enrichment is the only
> network-dependent step in either pipeline, and it is optional by construction.

The contract, concretely:

| Situation | Behavior |
|---|---|
| Context7 unreachable | 3 s timeout, one retry, then continue |
| Cache has a stale entry | Serve it. Stale beats absent; TTL is ignored while offline |
| Cache has nothing | Continue with **no** pack, and record the fact (below) |
| `--offline` passed | Never touch the network at all |
| Reachability probing | Once per `bevel doctor`, not per call |

**The part that matters most is the record.** When a context pack cannot be built, `/implement`
does not fail and does not silently pretend everything is fine. It writes into `notes.md`:

```
[offline] No version-pinned docs for tokio@1.42 — implemented from model knowledge.
          Review async APIs in this diff against the real docs before merging.
```

That line is the entire point. Code written without pinned documentation is exactly the code most
likely to use an API that changed, and without the marker it is indistinguishable from code
written with full context. This makes "written blind" a searchable property of the repository,
and it feeds the Phase 6 reconciliation (§8) rather than disappearing.

The same graceful-failure rule applies to the fallback chain itself: Context7 MCP → `bevel docs`
over HTTP → `WebFetch` of official docs → nothing, with a note. Each step is allowed to fail
quietly; only the last one leaves a trace.

---

## 11. The CLI

```
bevel project init --monorepo  # scaffold .bevel/, INBOX.md, specs/
bevel sync                     # install the method into ~/.claude and .claude/settings.json
bevel notes [agents|claude]    # print the project's notes; applying them is yours (§9)
bevel doctor                   # versions, workspace map, packs, Context7, broken gates
bevel doctor --context         # ← the harness token budget (§13)
bevel status                   # fixed-size summary — never a list (see below)
bevel list [--status …]        # the list, when you actually want one
bevel inbox add "…"            # capture cheaply; precision comes later
bevel shape <n|"text">         # reserve an ID and create specs/NNNN-slug/
bevel validate <id>            # schema + rubrics + Tier A/B presence + name↔test match
bevel approve <id>             # TTY-only. Freezes the hash in gates.lock
bevel gate <id>                # exit 0/1; fails if another spec is implementing
bevel start <id>               # the gate, plus the active slot, in one step (§5)
bevel pause <id>               # implementing → approved, hash untouched
bevel close <id>               # markers + verification + Tier C; writes done and the commit SHA
bevel pending                  # advisory: unfinished work on the active spec. Always exits 0
bevel verify [--affected]      # active-pack commands + Tier B criteria
bevel verify --spec <id>       # that spec's acceptance tests, ignored ones included
bevel docs <lib> [--topic]     # Context7 pinned to the lockfile version, cached
bevel method show <name>       # print one body, for an agent without slash commands
bevel fmt --file <path>        # format using the pack that owns the file
bevel index [--html]           # regenerate specs/README.md
bevel review <id>              # the dossier a human approves or closes from
bevel board                    # the whole pipeline on one page, for a human
bevel migrate                  # migrate projects across method versions
```

**The commands an agent branches on carry `--json`** — `status`, `list`, `validate`, `approve`,
`gate`, `start`, `pause`, `close` and `doctor` — so a decision is a parse rather than a guess at
prose. That set is exactly the state machine of §5, which is the point: every transition is
machine-readable, and everything else is output a human or a tool owns. `shape`, `verify`, `docs`
and `migrate` have no `--json` because none of them is a branch — `verify` passes a toolchain's
own diagnostics through, and reformatting those would discard the part that makes a failure
actionable.

**The four HTML reports are the sharper exception.** `review`, `board`, `index --html` and
`doctor --context --html` take `--open`, never `--json`, because they are the half of the
pipeline pointed at a human. A machine format would invite an agent to read back a page costing
several times the tokens of the markdown it was rendered from, to learn facts already available
to it from the source file. The absent flag is the affordance.

### `status` at a hundred specs

I asked how this behaves at scale and you rightly bounced it back. The answer turns out to be a
constraint rather than a feature: **`bevel status` output size must be independent of the number
of specs.** It is a summary, never a list.

```
$ bevel status
  inbox      7 items
  active     0007 example-feature      4/7 criteria live
  review     2   0011, 0012            ← waiting on you
  approved   1   0009
  done       94
```

Six lines at a hundred specs, six lines at a thousand. That matters because `SessionStart` injects
this into every session (§12) against a fifteen-line budget (§13) — a `status` that grows with the
repo would quietly eat the context budget the whole design depends on. Making it fixed-size is not
polish; it is what keeps §13 true over time. `bevel list --status review` exists for when you
genuinely want the enumeration.

**The real scaling pressure is elsewhere, and worth naming:** at a hundred specs the expensive
thing is `domain-scout` (§6, Phase 2) reading prior specs to find collisions. That is a token
problem, not a display problem, and it gets the same answer the rest of the design gets —
progressive disclosure (R2). The generated `specs/README.md` carries a one-line summary per spec,
so the scout reads the index and opens only the handful that look relevant. The project's own
artifacts get the same treatment as everything else.

**No archival.** `done` specs stay where they are. Moving them into `specs/archive/` would break
every cross-reference in `decisions.md` for the sake of a directory listing nobody reads
sequentially. The index does the organizing; the filesystem stays boring.

### The human channel: four HTML reports

Everything above is written for an agent to parse. Four things in this design are not, and they
are the four places where a person has to hold several documents in their head at once:

| Command | The question it answers | Why a terminal cannot |
|---|---|---|
| `bevel review <id>` | *Is this contract worth freezing?* — or, past implementation, *did the work meet the contract that was frozen?* | It is a cross-read of five files: spec, criteria, `decisions.md`, `open-questions.md`, `notes.md`. And a tier C criterion points into `mockup.html §2`, which a terminal cannot show you. |
| `bevel board` | Where is everything, how old, and which gates reopened? | `status` is fixed-size *by design* (§13). The constraint belongs to the channel, not to the question. |
| `bevel doctor --context --html` | Is the harness growing? | The failure §13 names is *slow*. A table shows one instant; only a slope shows accretion. Reconstructed from `git log --numstat`. |
| `bevel index --html` | What did we ever decide, and what did we reject? What superseded what? | `decisions.md` is per spec, so the cross-spec question has nowhere to be asked. Supersession is a graph with per-criterion edges. |

Five rules keep these from becoming the liability the mockup rule (§6) warns about. The first four
are that rule, generalised; the fifth is new and is the one that matters most:

1. **One file, no network.** Opened from `file://`, sometimes over SSH.
2. **Regenerated, never edited.** They live in `.bevel/cache/`, which is gitignored.
3. **Only a human reads one.** No agent ever opens one back — every fact in a report is already
   available from `--json` or from the markdown it was rendered from, and HTML costs several times
   the tokens of that markdown. A report read back into a prompt is a §13 regression.
4. **The markdown stays canonical.** These render `decisions.md`; they do not replace it.
5. **No control that changes state.** No approve button, no close button, no form. Every report
   ends in the command to type. `approve` is TTY-gated precisely so an agent cannot cross it, and
   *the agent is what generated the page* — a button there would make the gate decorative.

Rendered by the CLI rather than by an agent, which follows from the same split as everything else:
the data already exists behind `--json`, so rendering it costs zero tokens, is identical on every
run, is unit-testable, and works on a tier 0 agent (§9). `mockup-builder` stays the one exception,
because a mockup is invention rather than a query.

### Where determinism actually enters

Your intuition that "these tools help the agent be more reliable" is right, but the mechanism is
not the CLI's language — it is **turning judgments into exit codes**:

| Before (model judgment) | After (deterministic) |
|---|---|
| "is this spec approved?" | `bevel gate 0007` → 0/1 |
| "is anything missing from the spec?" | `bevel validate` against JSON Schema |
| "is the code good?" | `bevel verify` → clippy + nextest + tsc |
| "what ID should I use?" | The CLI reserves it |
| "which version of Tokio?" | Read from `Cargo.lock` |
| "which packages did I touch?" | `git diff` → workspace map |
| "am I done?" | Every declared tier A criterion live + Tier A/B green |
| "how far along am I?" | `5/7 criteria live`, counted from the declared criteria |

Every row is one fewer opportunity for the model to hallucinate something plausible.

---

## 12. Hooks (Claude Code only)

Few and cheap — they run on the critical path:

| Hook | What it does |
|---|---|
| `PreToolUse(Bash)` | Denies `bevel approve*` |
| `PostToolUse(Edit\|Write)` | If the file belongs to a pack, runs that pack's `fix` (fmt) |
| `Stop` | If an `implementing` task is open, flags pending verify / reconciliation |
| `SessionStart` | Injects `bevel status --json --brief` (≤15 lines) |

🔶 I deliberately do **not** propose a hook that blocks all writes outside `specs/` when no spec
is approved. Tempting, but it would break normal coding outside the pipeline and is exactly the
kind of "harness assumption" the article warns against baking in.

---

## 13. Context budget — the guardrail against ourselves

The most likely failure of this project is not technical: it is that in six months the harness
is 3,000 lines of markdown fighting the model. Anthropic removed 80% of their own system prompt.
We need to be able to do the same, which means **measuring it** (R1):

| Content | Budget | Loaded |
|---|---|---|
| Root `AGENTS.md` | ≤ 50 lines | always |
| Package-local `AGENTS.md` | ≤ 30 lines | when working in that subtree |
| `SKILL.md` (shape / implement) | ≤ 120 lines | on invocation |
| `references/*.md` | no hard limit | on demand |
| Pack `gotchas.md` | ≤ 80 lines | pack active *and* task touches it |
| `SessionStart` injection | ≤ 15 lines | always |

`bevel doctor --context` sums the tokens the harness injects *unconditionally* and fails past
budget. It is a linter for the harness itself, and it is the piece most likely to save this
project long-term.

Maintenance rule, straight from the article: each release, the question is not *"what else do I
add"* but ***"what can I stop doing?"*** — every new model makes some scaffolding obsolete.

---

## 14. Risks

| Risk | Severity | Mitigation |
|---|---|---|
| Over-process: the pipeline costs more than the feature | **High** | Fast path §8, entry classification |
| Harness bloat over time | **High** | `doctor --context`, budgets, pruning per release |
| Full-workspace verify makes the loop unusable | **High** | `verify --affected` (§3) |
| Method version drift between home and work | Medium | Pin in `project.toml`, `doctor` hard-fails (§2) |
| Affected-set misses a dependent → false green | Medium | Dependency expansion from `cargo metadata`; unknown path ⇒ full verify (§3) |
| Restricted network on the work machine | Medium | Dual distribution + embedded method + offline contract (§2, §10) |
| Code written without pinned docs, unmarked | Medium | `[offline]` marker in `notes.md`, feeds reconciliation (§10) |
| Mockups becoming a second UI to maintain | Medium | Frozen at `done`, reference-only (§6) |
| Specs rotting against the code | Medium | Hash reopens the gate; reconciliation Phase 6 |
| Context7 outage or API change | Medium | Cache + fallback to official docs |
| Agent self-approves | Medium | TTY + deny rule + hash (§5, limits stated) |
| Secret leaked into synced dotfiles | Medium | `key_command`, never a literal key (§2) |
| Maintaining 3 Rust targets on npm | Low | Linux-only → small matrix, `dist` automates it |
| Drift between method and rendered output | Low | Idempotent `sync` + generated header + `doctor` |

---

## 15. Roadmap

**Phase 1 — The skeleton that already pays for itself (1–2 weeks).**
Rust CLI: `init`, `project init --monorepo`, `shape`, `validate`, `approve`, `gate`, `status`,
`verify --affected` (with dependency expansion — §3). Workspace detection from `cargo metadata`
and npm workspaces. `/shape` and `/implement` **without subagents**, sequential. Basic `rust` and
`ts` packs. Claude Code adapter. Dual distribution (npm + `cargo install`) with the embedded
method fallback and version pinning.
*Success criterion: an INBOX idea reaches code through a real gate, on both machines, with the
work machine offline.*

**Phase 2 — The subagents that earn their cost.**
Blind spot pass ×3, `spec-critic`, `diff-reviewer`, `context-packer`, `mockup-builder`. Context
isolation arrives, and with it the real quality gain.

**Phase 3 — Framework packs + Context7.**
Tokio, Diesel, reqwest, tower, Angular, Milkdown. `bevel docs`, caching, lockfile version
pinning.

**Phase 4 — Portability and polish.**
Full `AGENTS.md`, the opencode renderer, `doctor --context`, migrations, model routing, hooks.
Codex and Cursor are deliberately not on this list: the two agents in use are the two that are
rendered for, and a third adapter written blind is what Phase 4 already had to delete once.

**Deliberately out of v1:** Tauri pack, dynamic JS workflows (wait for data on where it hurts),
solution tournaments, dashboards, telemetry, anything multi-repo.

---

## 16. Decided without asking, and what is still open

### Decided (cheap, reversible, not worth a round trip)

**`bevel inbox add "…"` exists.** The entire pipeline starts at `INBOX.md`, so capture friction
is the highest-leverage friction in the system — an empty inbox makes everything downstream
worth nothing. One command, runnable from any directory inside the repo, appending a timestamped
line. Perhaps thirty lines of Rust.

**The harness repository uses the harness.** It gets its own `INBOX.md` and `specs/`, and
features get shaped before they get built. Two reasons: it is the only way to find out whether
the pipeline is tolerable in daily use before committing to it, and every pack rule written from
real friction beats one written from imagination. The caveat is a bootstrap order — Phase 1 has
to be built without the harness, and dogfooding starts at Phase 2.

### Decided on your behalf, v0.4

You handed all three back. Each is now settled in the body of the document; the reasoning in one
line apiece:

| Question | Decision | Why |
|---|---|---|
| `/shape --quick` flag? | **No flag** (§8) | A flag makes you choose depth before anything has read the idea, and then prevents the pivot. Depth is proposed and confirmed; escalation is cheap, de-escalation is not. |
| `status` at a hundred specs? | **Fixed-size summary** (§11) | It is injected at `SessionStart` against a 15-line budget. Output that grows with the repo would eat the context budget the design rests on. |
| Superseded acceptance tests? | **Line-by-line disposition** (§5) | A contract a status flip can dissolve without accounting was never binding. `dropped` now requires a written reason. |

The common thread is worth stating, because it will keep coming up: in all three, the answer was
to **remove a decision from the user and give it either to the model (depth) or to a machine check
(status size, supersession)**. That is the same move as R4, applied to the harness's own ergonomics
rather than to the code it produces.

### Still open

Nothing blocks Phase 1. The remaining unknowns are the kind that only real use can answer, and
guessing at them now would be scaffolding for workflows that do not exist yet:

- Whether the depth proposal in §8 is accurate enough in practice, or whether it under-shoots so
  often that the escalation step becomes the normal path rather than the exception.
- Whether the 15-line `SessionStart` injection earns its place at all, or whether `bevel status`
  on demand is enough.
- Whether `decisions.md` stays readable past a dozen Q&A rounds, or needs its own summary head.

All three are observations to make while dogfooding (§16), not decisions to take in advance.
