# Inbox

Raw ideas, one per row. Capture is supposed to be cheap — do not try to be
precise here, that is what shaping is for.

Add with `bevel inbox add "..."`, shape with `bevel shape <n>`. The ID is
the number `shape` takes; it is rewritten from position, so do not curate it.

| ID | Raw idea |
| --- | --- |
| 1 | I want to make Bevel compatible with agents other than Claude Code. We can start with Open Code. → [0001](specs/0001-i-want-to-make-bevel-compatible/spec.md) |
| 2 | bevel close counts phantom pending markers: validate::pending_markers text-searches the whole repo, so it matches the string inside spec.md prose and inside test fixtures in src/*.rs that use a real spec id. Its sibling locate() already excludes specs/ for exactly this reason and documents why. Surfaced while implementing 0001 in bevel's own repo. → [0002](specs/0002-bevel-close-counts-phantom-pending-markers/spec.md) |
| 3 | criteria_state matches a tier A test name as a substring anywhere outside specs/, so a short name like 'one' resolves to INBOX.md's own 'one per line' and the criterion reports live with no test behind it. False green, and it now feeds the bevel close gate rather than just a report label. Shares the property with locate() and check_tier_a_tests_exist(). Surfaced implementing 0002. |
| 4 | marker_near's proximity window is positional — at-3..=at regardless of content — so a pending marker bleeds onto any test declared within three lines of the marked one, and a blank line between them costs one of the three rather than ending the block. Fails toward blocking so it is safe, but it will refuse a close nobody can explain. Surfaced implementing 0002. |
| 5 | every_command_reports_the_same_state_for_the_same_criterion pins progress, blockers, summary, review and board, but not cmd_pending or cmd_pause: those live in main.rs, outside the library, so a lib test cannot reach them. Both call validate::progress directly so they cannot disagree by construction, but the criterion claims 'every command' and proves four of six. Surfaced implementing 0002. |
| 6 | bevel close records done_commit = git rev-parse HEAD without checking the work is committed. Closing 0002 with its implementation still unstaged wrote done_commit = 4850f82, the previous commit, which does not contain a line of 0002. gates.lock is versioned and that record is now permanently wrong. close should refuse on a dirty tree, or record nothing rather than something false. |
| 7 | bevel validate fails on a paused spec that was already implemented: check_tier_a_tests_exist derives 'relocated' from status, and approved is not in the set, so after bevel pause the relocated acceptance.* looks like a missing one and validate reports acceptance/file. Mirror of the bug 7f2dedf fixed. Does not block close (close never runs validate) and goes away on bevel start. |
