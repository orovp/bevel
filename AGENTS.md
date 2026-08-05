# Agent notes

Non-obvious things only. Anything derivable from the file tree does not belong
here, and nothing here is repeated in another file.

## The loop

Ideas live in `INBOX.md`. Shaping turns one into a spec under `specs/`; a human
approves it; implementation builds it against that spec.

```
bevel status                   where things stand
bevel shape <n>                reserve an id, scaffold specs/NNNN-slug/
bevel validate <id>            deterministic rules; draft -> review
bevel start <id>                 claim an approved spec (checks the gate)
bevel close <id>                 finish it; enforces markers and verification
bevel verify --affected        only what changed, plus its dependents
bevel docs <lib> --spec <id>   version-pinned docs from the lockfile
```

`bevel approve` is missing from that list on purpose. It requires a terminal,
so an agent cannot run it. Ask the human, and say what changed.

## Full instructions

```
bevel method shape
bevel method implement
```

Those print the same text a `/shape` or `/implement` command would load, so the
pipeline works in any agent whether or not it has slash commands.

## Gotchas

<!-- Conventions and traps a competent newcomer to this repo would get wrong.
     Not a tutorial, and not anything the file tree already says.
     Keep this file under 50 lines. -->

`bevel review`, `board`, `doctor --context --html` and `index --html` write
HTML into `.bevel/cache/`. **Never read one back.** They exist for a human's
eyes; a page costs several times the tokens of the markdown behind it, and
every fact in one is available to you from `--json` or the source file. Point
the user at the path and move on.
