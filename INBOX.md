# Inbox

Raw ideas, one per line. Capture is supposed to be cheap — do not try to be
precise here, that is what shaping is for.

Add with `bevel inbox add "..."`, shape with `bevel shape <n>`.

- I want to make Bevel compatible with agents other than Claude Code. We can start with Open Code. → [0001](specs/0001-i-want-to-make-bevel-compatible/spec.md)
- bevel close counts phantom pending markers: validate::pending_markers text-searches the whole repo, so it matches the string inside spec.md prose and inside test fixtures in src/*.rs that use a real spec id. Its sibling locate() already excludes specs/ for exactly this reason and documents why. Surfaced while implementing 0001 in bevel's own repo.
