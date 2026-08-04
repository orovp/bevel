---
name: rust-architecture
description: Architecture and conventions for a layered Rust service workspace whose typed contract is the vertex everything else derives from. Use when adding an entity, endpoint, crate or layer to such a workspace, when deciding which crate a change belongs in, when choosing between an enum and a trait object, or when reviewing whether a Rust change respects the layering.
---

# Rust architecture

The shape of a Rust workspace where **one crate defines every type that crosses
the process boundary and every other artefact derives from it**. Applies when
there is a contract crate, a domain crate linking no web framework, and a
generated client-facing schema. Every rule below cost a bug that compiled.

**The order is a ranking, not a reading order.** Rules are sorted by what being
wrong costs: how likely a competent implementer is to get it wrong, how silent
the failure is, and how expensive it is to undo — minus whatever reading one
neighbouring file would have taught anyway. So the first rules are the ones no
local context can teach and no passing test will catch; the last are the ones a
good neighbouring file already half-teaches. Read top-down under a budget, and
if two rules ever collide, the earlier one wins.

The same test admits a new rule: **does it turn a silent failure into a loud
one** — a compile error, a rolled-back transaction, a non-zero exit code? The
top of this list does; that is why it is the top.

## 1. The connection is a parameter, never an ambient

`enum RequestConn { Pool(DataSource), Tx(Transaction) }` is an argument on every
repository method — the whole mechanism keeping one request on one connection.

- Middleware wraps the router: mutating methods get a transaction injected, and
  after the handler **2xx commits, anything else rolls back**.
- **Reads build `Pool`; writes build `Tx`.** A write handler that builds `Pool`
  compiles, passes its test and silently escapes the transaction.
- **Never open a transaction inside a repository** — the request already is one.
  Closures handed to the connection are `move`.

## 2. Architecture is asserted, and the assertion is an xtask task

A rule a command can fail on outlives a rule written in a document, so prefer
the assertable form: crate boundaries, contract freshness, naming, the presence
of the standard test block. **The assertion is a Rust binary in the workspace** —
`xtask`, aliased in `.cargo/config.toml`, so `cargo xtask <name>` runs from a
clean checkout with nothing installed beyond the toolchain.

**Never a `.sh`, never a `.ps1`.** Two scripts are two implementations of one
rule and they drift: a boundary proven on Linux is unproven on Windows, and the
agent that ran one of them learned nothing about the other. A task is one
implementation, it compiles, and a broken check is then a build error rather
than a surprise on the OS nobody ran. That is the property an agentic loop is
buying — agent, human and CI issue the same command and get the same bytes, so
a green run means one thing.

- **Parse machine-readable output.** `cargo metadata` and `--message-format
  json`, not `cargo tree` text piped through `grep`; locale, coreutils version
  and shell quoting have no business inside a fact about the dependency graph.
- **Shelling out reintroduces the platform** the task existed to remove.
  `Command::new("grep")` is the regression; invoking the toolchain itself is the
  exception, because cargo is present by definition.
- **Determinism is written, not hoped for**: sort what you walk, serialise
  through a map that orders its keys, and keep directory order out of anything
  a diff or an agent reads.
- **The exit code is the interface** — zero passes, non-zero names the file and
  the fix in one line, since that line is what the agent acts on.
- **What CI does outside a task cannot be reproduced locally.** CI holds
  `cargo xtask` lines and no logic of its own; the bar below is one of them.

## 3. A closed set of alternatives is an enum, not a trait object

`RequestConn` above is the canonical instance: **when the variants are known at
compile time, model them as an enum and match.** Static dispatch, no allocation
and free `Clone` / `PartialEq` / `Copy` are the small reasons — a trait object
needs a `clone_box` dance to be cloneable at all. The load-bearing one is that
**a forgotten case is a compile error**, which `Box<dyn Trait>` cannot detect.

- **Never add a `_ =>` arm to a match over a domain enum.** It converts that
  compile error into a runtime surprise, and the error is the whole thing being
  bought: a new variant of the domain error enum must refuse to compile at the
  one mapping site until someone decides what it means over HTTP. Otherwise it
  silently becomes a 500 nobody hears about until production.
- **Dispatching over a parallel index is the same bug in disguise.** `match idx`
  over an array built in variant order makes both the ordering and the catch-all
  load-bearing and unchecked; a new variant lands in the last arm, so one case
  runs twice and the new one never runs. Match the variant instead.
- **A trait can be a shape contract without ever being a trait object.**
  Implemented by every repository, held as `dyn` by none, it forces one shape
  while dispatch stays static — that is its intended use here. `#[async_trait]`
  still boxes a future per call; moving to native `async fn` in traits is a
  workspace-wide change or none.
- **The exception is open set *and* cold path**, both halves. A boxed error trait
  accepting any dependency's error type qualifies: the set cannot be enumerated,
  and an allocation on the way to a 500 costs nothing. If either half is false,
  use an enum.

Sum types cross the contract boundary too: an internally tagged enum
(`#[serde(tag = "…")]`) reaches the generated schema as a discriminated union
the client narrows on without a second request.

## 4. One crate is the vertex

The contract crate holds DTOs, validation rules and schema derives. Handlers,
the typed client and the schema generator read from it; the generated schema
then generates the client models.

- **The generated schema is never hand-edited**, there is one copy, and its
  output is deterministic or it appears in diffs that changed nothing.
- **To change a field, change the Rust type**, then regenerate. Downstream stops
  compiling — that is the mechanism, not a side effect.
- **The check regenerates in memory and compares; it never runs the writer**,
  which would overwrite the hand-edit the check exists to catch.
- **List what stays hand-written and why**, or the generator's boundary erodes.

## 5. Resolve configuration once; set pragmas per connection

**defaults → file → environment → CLI flags**, later winning, recording which
layer each value came from. **Never read the environment outside that layer** —
the config would describe what the process *should* do — and **never store a
relative path**, which is not a resolved value at all.

**Connection pragmas belong in the pool's post-create hook**: they are per
connection, so one statement at startup configures exactly one connection of N —
the difference between foreign keys being guarantees and being documentation.

## 6. A crate is defined by what it may not link

| Crate | Must never link |
| --- | --- |
| contract — DTOs, validation, schema derives | web framework, ORM |
| storage — schema, pool, request-connection, migrations | web framework |
| runtime — layered config, per-OS paths, telemetry | web framework, ORM |
| core — entities, repositories, services | web framework |
| desktop shell | **every** first-party crate |

Checked, not aspirational: a task walks the resolved dependency graph and fails
naming the crate and the edge that broke the rule (the xtask rule above is why
it is a task and not a `grep`). Two smells catch a misplaced type first — **a
serialization derive on a domain struct** means it belongs in the contract
crate, and **a service that validates its own input** means framework-shaped
code leaked inward. Validation rules travel with the DTO; the extractor running
them stays at the edge.

Composition lives in the binary and its order is load-bearing: API routes match
first, and the fallback **splits by prefix**, so an unmatched `/api/...` keeps
the API's JSON 404 instead of getting `index.html` and a 200.

## 7. The standard test block, and the bar

Storage modules end with `#[cfg(test)] mod tests` over an in-memory database,
`setup()` called **per test**. Eight cases: create returns a generated id; find
by id; find by id missing errors; **update only** the target; **delete only** the
target; find all; find all respects limit and offset (assert *which* rows); count.
The `only` cases are what catch a missing `WHERE`.

The bar runs **contract verification first** — a desynchronised contract makes
every later result meaningless — then cheap file checks, `fmt --check` (never
`fmt`: a bar that rewrites while it looks changes the answer), lint with warnings
denied, tests. A *missing* build artefact fails the build; a **stale** one does
not, so ordering constraints belong in the file read while building.

## 8. Pagination, and the one error mapping site

Lists take `limit`/`offset`; by-id and mutations take neither. **Clamp silently**
— a default, a maximum, negatives to zero; a 400 for `limit=1000` is a client bug
you chose to create. Push it into SQL and make `total` a separate count **scoped
identically** to the page query: a count that forgets the parent scope reports a
wrong total and nothing fails.

The domain error enum knows no HTTP; the api crate owns the only conversion.
`NotFound`→404, `Conflict`→409, pool/interact→503, rest→500 logged at `error!`.
**`InvalidReference`→422 is the one that is not obvious**: when a service checks
a parent exists, convert that lookup's `NotFound` at the check, or creating a
child with a bad parent id returns 404 — read by the client as "no such endpoint".

## 9. Three entity shapes

**Catalog** — plain CRUD, no service. **Complex** — adds a service implementing
the same trait, delegating reads through and validating foreign keys; the
*service* is what the store registers. **Join-table** — **does not implement the
CRUD trait**: scoped methods carrying the parent (`find_all(ctx, parent_id,
limit, offset)`, `delete(parent, child)`), read via `inner_join`, routed
`/{parent}/{id}/{child}/{id}`, service checks the parent exists.

**`delete` takes an id, never an entity**; never fetch first in order to delete;
zero rows affected is `NotFound`, not `Ok`.

## 10. Naming, which never varies

`{Entity}Id`, `{Entity}` (domain model, **no derives**), `{Entity}Entity` /
`{Entity}New` / `{Entity}Update` (ORM rows, **all private**, with private
`create()` / `update()`), `{Entity}Repository` (the only public item of a storage
module), `Create{Entity}Request` / `Update{Entity}Request` / `{Entity}Response`.
Conversions are `From` impls both ways.

**DTOs derive both `Serialize` and `Deserialize`.** The server needs one and the
typed client the other; deriving only what today's caller uses is what makes the
client crate a rewrite later.
