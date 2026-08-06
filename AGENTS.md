# Agent notes

Non-obvious things only. Anything derivable from the file tree does not belong
here, and nothing here is repeated in another file.

## The loop

Ideas live in `INBOX.md`. Shaping turns one into a spec under `specs/`; a human
approves it; implementation builds it against that spec. Two skills drive it,
and each runs the commands indented under it — invoke the skill, not the list:

```
bevel status                    where things stand — start here
bevel inbox add "<idea>"        capture it; precision comes later

/shape <n>                      inbox item -> a spec a human can approve
  bevel shape <n>               reserve an id, scaffold specs/NNNN-slug/
  bevel validate <id>           deterministic rules; draft -> review
  bevel review <id>             the dossier the human approves from

/implement <id>                 an approved spec -> code
  bevel start <id>              claim it; checks the gate first
  bevel docs <lib> --spec <id>  version-pinned docs from the lockfile
  bevel verify --affected       only what changed, plus its dependents
  bevel close <id>              markers, verification, then done
```

`bevel approve <id>` belongs between the two and is absent on purpose: it
requires a terminal, so an agent cannot run it. Ask the human, and say what
changed. `bevel pause <id>` hands the slot back without losing the approval.

Without slash commands, `bevel method show shape` and `bevel method show
implement` print the same text the skills load.

## Gotchas

<!-- Conventions and traps a competent newcomer to this repo would get wrong.
     Not a tutorial, and not anything the file tree already says.
     Keep this file under 50 lines. -->

`bevel review`, `board`, `doctor --context --html` and `index --html` write
HTML into `.bevel/cache/`. **Never read one back.** They exist for a human's
eyes; a page costs several times the tokens of the markdown behind it, and
every fact in one is available to you from `--json` or the source file. Point
the user at the path and move on.
