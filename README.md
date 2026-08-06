# bevel

A spec-driven development harness for coding agents. Ideas go into `INBOX.md`,
`/shape` turns one into a spec, a human approves it, `/implement` builds it.

The design and the reasoning behind every decision are in [DESIGN.md](DESIGN.md).
The one-line version: **judgments an agent would otherwise make are exit codes
here.** Approval, validation and completion are decided by a binary, not by a
sentence in a markdown file that an enthusiastic agent walks straight past.

```
npm i -g @orovp/bevel        # primary
cargo install bevel          # when npm is unreachable
```

Both install a binary called `bevel`. Two channels because one network may be
closed: the crate has to exist anyway to build the npm binaries, so the fallback
costs one `cargo publish`.

## Claude Code and opencode

```
bevel sync                     # renders for whichever agents it detects
bevel sync --agent opencode --hooks
```

Two agents, named because they are the two that are rendered for. An unknown
name is an error rather than a silent no-op — a flag that accepts `cursor` and
then writes nothing for it reads exactly like success.

Both get the five skills and the seven subagents. Claude Code additionally gets
three non-blocking hooks; opencode expresses hooks as JavaScript plugins, and a
Rust binary generating JS for three conveniences is not a trade worth making.

Any *other* agent still runs the pipeline through `AGENTS.md`, the artifacts on
disk and this binary, because every phase writes a file and the next one reads
it. What it loses is context isolation, not function.

**`sync` does not write `AGENTS.md` or `CLAUDE.md`.** What a repository says
about itself is your writing, so bevel prints a starting point and stops:

```
bevel notes > AGENTS.md        # the body: the loop, and your own gotchas
bevel notes claude > CLAUDE.md # a two-line pointer at it, for Claude Code
```

Run them once, edit freely, and no later `bevel sync` will disagree with what
you wrote. `bevel doctor --context` still counts the lines against the 50-line
budget, because that file enters every turn whoever authored it.

**No instruction text is copied into anyone's prompt format.** The skills
install once into `~/.claude/skills`, which opencode scans too, and `bevel
method show shape` prints the same text from the one method tree. Only a
subagent's *frontmatter* is translated — opencode has no `.claude/agents`
fallback, so the seven definitions are rendered into `~/.config/opencode/agents`
with their tool grants mapped to opencode's `permissions`. That needs opencode
v2 or newer, and `bevel doctor` says so.

## The loop

```
bevel project init --monorepo
bevel inbox add "let documents sync between devices"
bevel shape 1                  # reserves an id, scaffolds specs/0001-.../
                                 # → run /shape in your agent
bevel validate 1               # deterministic rules; promotes draft → review
bevel review 1                 # the dossier you approve from, as a page
bevel approve 1                # you, in a terminal. Freezes a hash
                                 # → run /implement in your agent
bevel verify --affected        # only what changed, plus its dependents
```

## What makes it different

**The gate is a hash, not a boolean.** `bevel approve` records a SHA-256 over
the spec body and its acceptance criteria. Edit the spec afterwards and the
gate reopens by itself. It requires a terminal, so an agent's non-interactive
shell cannot approve anything — that is the gate working, not a bug.

**Acceptance criteria are named tests, written while shaping.** Not TDD: no
test bodies, just named failing stubs. It is a naming exercise, and it converts
*"am I done?"* from an agent's optimistic judgment into `cargo test`. Criteria
that genuinely cannot be tested are tier B (any command with an exit code) or
tier C (a human decides, and an agent may never tick one). A spec with only
tier C criteria fails validation.

**`verify --affected` expands dependents.** Changing a core crate verifies
everything downstream of it. A file-to-package map without that expansion
produces false greens, and a verification tool that reports false greens is
worse than none. Anything ambiguous — an unknown path, a lockfile change, most
of the workspace affected — widens to a full run.

**Documentation is pinned to your lockfile, not to "latest".** `bevel docs
tokio --topic "graceful shutdown"` reads the resolved version out of
`Cargo.lock`, asks Context7 for *that* version, and says so:

```
/websites/rs_tokio_1_49_0 — fetched, version-pinned to 1.49.0
```

When no version-specific documentation exists it says that too, rather than
quietly serving the wrong version. When the network is closed it serves a stale
cache, and when there is nothing to serve it exits 0 and writes an `[offline]`
marker into the spec's `notes.md` — because code written without pinned
documentation is the code most likely to use an API that moved, and the marker
is the only thing that tells it apart afterwards.

**Framework packs activate from the lockfile.** Not from a config file you
maintain, and not from a manifest that needs feature resolution:

```
rust/tokio     method  tokio@1.49.0          0 checks
ts/angular     method  @angular/core@19.2.1  1 checks
```

The shipped packs deliberately have **no** `gotchas.md`. A shared pack cannot
know your lint configuration, your test runner or your error-handling
convention, and inventing framework lore an agent would then treat as
authoritative is worse than leaving the file absent. `bevel doctor` tells you
where to put yours.

**The method is not inside the binary.** Skills, subagents, packs and artifact
templates live in this repository and are fetched into a cache, so editing a
markdown file takes effect on the next command — no build, no release, no
version bump.

```
bevel method where     which layer every file resolved from
bevel method fetch     download the method for the configured ref
bevel method show shape
```

`[method] ref` in `~/.config/bevel/config.toml` takes a branch, a tag or a
commit SHA: a branch while you iterate, a tag when two machines must agree.
`method where` prints a **content hash of the tree** rather than a commit —
because when two machines behave differently, the question is whether the
instructions differ, not whether the commit does.

The cost, stated plainly: a machine that has never fetched and cannot reach
GitHub has no method at all. The cache is permanent once warm, npm's
`postinstall` fills it while the network is demonstrably reachable, and
`[method] path` pointing at a local checkout needs no network ever. This
repository uses that last mode on itself.

**Nothing about your project lives in the global layers.** `~/.config/bevel`
holds your overrides; specs, decisions and the inbox live in the repository and
go into git, because they have a lifecycle and belong in code review.

**The harness has a budget for itself.** `bevel doctor --context` measures
what it injects and fails past the limits in DESIGN.md §13. The likeliest way
this project dies is becoming three thousand lines of markdown arguing with the
model, so that number is a test rather than a note. It currently sits at ~351
tokens per turn unconditionally.

**Four reports are HTML, and only humans read them.** Everything an agent
consumes is markdown or `--json`; a page costs several times the tokens of the
text it came from, so nothing here is ever read back into a prompt.

```
bevel review <id>              # the dossier behind approving, or closing
bevel board                    # the pipeline enumeration status refuses to be
bevel doctor --context --html  # the budget, with the trend a table cannot show
bevel index --html             # every decision ever made, and what superseded what
```

None of them can act. Each ends in the command to type, because the terminal
check on `approve` is the only thing standing between an agent and its own
approval — and the agent is what generated the page.

## Status

All four phases of the roadmap in [DESIGN.md §15](DESIGN.md) are implemented.

The full artifact lifecycle, the gate, validation, workspace detection for cargo
and npm workspaces, scoped verification with dependency expansion, eight packs
with three-layer overrides and lockfile-driven activation, version-pinned
Context7 retrieval with an offline contract, seven subagents, three hooks,
adapters for five agents, migrations with directional pin diagnostics, the
context budget linter, and the npm plus crates.io distribution pipeline.

**Never exercised against a real project.** The pipeline has unit and
end-to-end coverage, but no feature has been shaped and built through it in
anger yet. DESIGN.md §16 lists what to watch for while dogfooding — chiefly
whether the depth proposal in `/shape` under-shoots often enough that
escalation becomes the normal path rather than the exception.

`bevel migrate` exists and updates the version pin, but its artifact
migration registry is empty: schema 1 is the only schema so far. The plumbing
is there so the first real migration is a one-line addition rather than a
redesign under time pressure.

## Development

```
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```
