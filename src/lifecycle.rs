//! Claiming and releasing the active spec (DESIGN.md §5 and §8).
//!
//! These two commands exist because the design claimed enforcement it did not
//! have. §8 says closing "requires zero remaining pending markers, enforced by
//! `bevel verify`", and §5 says `/implement` sets `implementing` after passing
//! the gate — but nothing set either field, so an agent reached both by editing
//! frontmatter. That is R4 violated at the exact two moments the whole design is
//! about: claiming work, and declaring it finished.

use anyhow::{bail, Result};
use std::path::Path;
use std::process::Command;

use crate::gate;
use crate::project::Project;
use crate::spec::{Criterion, Spec, Status};
use crate::validate;

/// Claim the active slot. Refuses unless the gate is open, which also enforces
/// one-active-spec-at-a-time without a lock file.
pub fn start(project: &Project, spec: &mut Spec) -> Result<()> {
    if spec.front.status == Status::Implementing {
        return Ok(()); // Already ours; re-running is not an error.
    }
    let verdict = gate::check(project, spec)?;
    if !verdict.is_open() {
        bail!("{}", verdict.explain(&spec.front.id));
    }
    spec.front.status = Status::Implementing;
    spec.save()
}

#[derive(Debug, PartialEq, Eq)]
pub enum Blocker {
    NotImplementing(Status),
    /// The spec was amended after approval; the contract changed under the work.
    GateReopened,
    Pending {
        remaining: usize,
        total: usize,
    },
    /// Declared tier A criteria whose test is nowhere to be found.
    ///
    /// Separate from `Pending` because the repair is different — write the
    /// test, rather than take the marker off it — and because this is the
    /// failure the old marker count could not see at all: no test means no
    /// marker, which read as nothing left to do.
    MissingTest(Vec<String>),
    VerifyFailed,
    /// Tier C criteria exist and no human is present to judge them.
    NeedsHumanJudgement(Vec<String>),
}

impl Blocker {
    pub fn explain(&self, id: &str) -> String {
        match self {
            Blocker::NotImplementing(s) => {
                format!("spec {id} is `{s}`, not implementing\n  claim it first: bevel start {id}")
            }
            Blocker::GateReopened => format!(
                "spec {id} was edited after approval, so the gate reopened\n  \
                 the contract changed while the work was being done against it — \
                 have it re-approved: bevel approve {id}"
            ),
            Blocker::Pending { remaining, total } => format!(
                "{remaining} of {total} acceptance criteria are still pending\n  \
                 fill in the test bodies and remove their `acceptance: {id} pending` markers"
            ),
            Blocker::MissingTest(names) => {
                let mut s = format!(
                    "{} acceptance criteria name a test that does not exist:\n",
                    names.len()
                );
                for n in names {
                    s.push_str(&format!("  {n}\n"));
                }
                s.push_str(
                    "  write them, or amend the spec if they are no longer the contract — \
                     a criterion with no test passes by never running",
                );
                s
            }
            Blocker::VerifyFailed => {
                "verification failed — every tier A and B criterion must be green".to_string()
            }
            Blocker::NeedsHumanJudgement(items) => {
                let mut s = format!(
                    "spec {id} has {} tier C criterion/criteria, which only a human may judge:\n",
                    items.len()
                );
                for i in items {
                    s.push_str(&format!("  [ ] {i}\n"));
                }
                s.push_str(
                    "  Run `bevel close` yourself in a terminal to confirm them. A model \
                     grading its own subjective quality always awards itself a pass.",
                );
                s
            }
        }
    }
}

/// Everything that must hold before a spec may be closed.
///
/// `verify_ok` is passed in rather than computed here so the caller can show the
/// compiler's own output; `human_present` is a terminal check, because tier C is
/// the one thing an agent may never certify.
pub fn blockers(
    project: &Project,
    spec: &Spec,
    verify_ok: bool,
    human_present: bool,
) -> Result<Vec<Blocker>> {
    let mut out = Vec::new();

    if spec.front.status != Status::Implementing {
        out.push(Blocker::NotImplementing(spec.front.status));
        // Every later check assumes the spec is the active one.
        return Ok(out);
    }

    if gate::check(project, spec)? == gate::Verdict::HashMismatch {
        out.push(Blocker::GateReopened);
    }

    let p = validate::progress(project, spec);
    if p.inert > 0 {
        out.push(Blocker::Pending {
            remaining: p.inert,
            total: p.total,
        });
    }
    if !p.missing.is_empty() {
        out.push(Blocker::MissingTest(p.missing));
    }

    if !verify_ok {
        out.push(Blocker::VerifyFailed);
    }

    let judged: Vec<String> = spec
        .front
        .acceptance
        .iter()
        .filter_map(|c| match c {
            Criterion::C { text } => Some(text.clone()),
            _ => None,
        })
        .collect();
    if !judged.is_empty() && !human_present {
        out.push(Blocker::NeedsHumanJudgement(judged));
    }

    Ok(out)
}

/// Mark the spec done and record what shipped it.
pub fn finish(project: &Project, spec: &mut Spec) -> Result<Option<String>> {
    let commit = head_commit(&project.root);

    let mut lock = gate::GatesLock::load(&project.gates_lock())?;
    if let Some(entry) = lock.spec.get_mut(&spec.front.id) {
        entry.done_commit = commit.clone();
    }
    lock.save(&project.gates_lock())?;

    spec.front.status = Status::Done;
    spec.save()?;
    Ok(commit)
}

fn head_commit(root: &Path) -> Option<String> {
    let out = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(root)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let sha = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!sha.is_empty()).then_some(sha)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup(status: Status, acceptance: &str) -> (tempfile::TempDir, Project, std::path::PathBuf) {
        let tmp = tempfile::tempdir().unwrap();
        crate::project::init(tmp.path(), false).unwrap();
        let project = Project::discover_from(tmp.path()).unwrap();
        let dir = project.specs_dir().join("0001-example");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("spec.md"),
            format!(
                "---\nid: '0001'\ntitle: Example\nstatus: {}\nschema_version: 1\n\
                 created: '2026-08-03'\n{acceptance}---\n# Problem\n\nSomething.\n",
                status.as_str()
            ),
        )
        .unwrap();
        std::fs::write(dir.join("acceptance.rs"), "fn does_the_thing() {}\n").unwrap();
        (tmp, project, dir)
    }

    const TIER_A: &str = "acceptance:\n- tier: A\n  test: does_the_thing\n";

    #[test]
    fn start_refuses_until_the_spec_is_approved_then_claims_the_slot() {
        let (_t, p, dir) = setup(Status::Review, TIER_A);
        let mut spec = Spec::load(&dir).unwrap();
        assert!(start(&p, &mut spec).is_err());

        gate::approve(&p, &mut spec, true).unwrap();
        start(&p, &mut spec).unwrap();
        assert_eq!(Spec::load(&dir).unwrap().front.status, Status::Implementing);

        // Idempotent: re-running on an already-active spec is not a failure.
        start(&p, &mut spec).unwrap();
    }

    #[test]
    fn start_cannot_claim_a_slot_another_spec_holds() {
        let (_t, p, dir) = setup(Status::Review, TIER_A);
        let mut first = Spec::load(&dir).unwrap();
        gate::approve(&p, &mut first, true).unwrap();
        start(&p, &mut first).unwrap();

        let dir2 = p.specs_dir().join("0002-other");
        std::fs::create_dir_all(&dir2).unwrap();
        std::fs::write(
            dir2.join("spec.md"),
            "---\nid: '0002'\ntitle: Other\nstatus: review\nschema_version: 1\n\
             created: '2026-08-03'\nacceptance:\n- tier: B\n  cmd: 'true'\n---\n# P\n\nx\n",
        )
        .unwrap();
        let mut second = Spec::load(&dir2).unwrap();
        gate::approve(&p, &mut second, true).unwrap();

        let err = start(&p, &mut second).unwrap_err().to_string();
        assert!(err.contains("already being implemented"), "{err}");
    }

    #[test]
    fn closing_a_spec_that_was_never_started_is_refused() {
        let (_t, p, dir) = setup(Status::Approved, TIER_A);
        let spec = Spec::load(&dir).unwrap();
        let b = blockers(&p, &spec, true, true).unwrap();
        assert_eq!(b, vec![Blocker::NotImplementing(Status::Approved)]);
        assert!(b[0].explain("0001").contains("bevel start 0001"));
    }

    /// The marker has to be *on the criterion*. It used to be enough for the
    /// string to exist anywhere under the project root — this same test proved
    /// that by putting it in `notes.md`, where no test has ever lived.
    #[test]
    fn a_marker_on_the_criterion_blocks_the_close() {
        let (_t, p, dir) = setup(Status::Review, TIER_A);
        let mut spec = Spec::load(&dir).unwrap();
        gate::approve(&p, &mut spec, true).unwrap();
        start(&p, &mut spec).unwrap();

        std::fs::write(
            dir.join("acceptance.rs"),
            "#[ignore = \"acceptance: 0001 pending\"]\nfn does_the_thing() {}\n",
        )
        .unwrap();
        // Loose in a document, it is prose about the convention and blocks
        // nothing.
        std::fs::write(
            dir.join("notes.md"),
            "we still owe `acceptance: 0001 pending` on the retry path\n",
        )
        .unwrap();
        let spec = Spec::load(&dir).unwrap();
        let b = blockers(&p, &spec, true, true).unwrap();
        assert_eq!(
            b,
            vec![Blocker::Pending {
                remaining: 1,
                total: 1
            }]
        );
    }

    #[test]
    fn a_mid_flight_amendment_blocks_the_close() {
        let (_t, p, dir) = setup(Status::Review, TIER_A);
        let mut spec = Spec::load(&dir).unwrap();
        gate::approve(&p, &mut spec, true).unwrap();
        start(&p, &mut spec).unwrap();

        // Editing the contract after approval must not be closeable.
        let mut spec = Spec::load(&dir).unwrap();
        spec.body.push_str("\nan amendment\n");
        spec.save().unwrap();

        let spec = Spec::load(&dir).unwrap();
        let b = blockers(&p, &spec, true, true).unwrap();
        assert!(b.contains(&Blocker::GateReopened), "{b:?}");
    }

    #[test]
    fn tier_c_criteria_cannot_be_certified_without_a_human() {
        let acceptance = "acceptance:\n- tier: A\n  test: does_the_thing\n\
                          - tier: C\n  text: the banner reads well\n";
        let (_t, p, dir) = setup(Status::Review, acceptance);
        let mut spec = Spec::load(&dir).unwrap();
        gate::approve(&p, &mut spec, true).unwrap();
        start(&p, &mut spec).unwrap();
        let spec = Spec::load(&dir).unwrap();

        let b = blockers(&p, &spec, true, false).unwrap();
        match b.first() {
            Some(Blocker::NeedsHumanJudgement(items)) => {
                assert_eq!(items, &vec!["the banner reads well".to_string()]);
                assert!(b[0].explain("0001").contains("[ ] the banner reads well"));
            }
            other => panic!("expected a judgement blocker, got {other:?}"),
        }

        // With a human present, the same spec is closeable.
        assert!(blockers(&p, &spec, true, true).unwrap().is_empty());
    }

    #[test]
    fn a_spec_with_only_machine_criteria_closes_without_a_human() {
        let (_t, p, dir) = setup(Status::Review, TIER_A);
        let mut spec = Spec::load(&dir).unwrap();
        gate::approve(&p, &mut spec, true).unwrap();
        start(&p, &mut spec).unwrap();
        let mut spec = Spec::load(&dir).unwrap();

        assert!(blockers(&p, &spec, true, false).unwrap().is_empty());
        finish(&p, &mut spec).unwrap();
        assert_eq!(Spec::load(&dir).unwrap().front.status, Status::Done);
    }

    #[test]
    fn failed_verification_blocks_the_close() {
        let (_t, p, dir) = setup(Status::Review, TIER_A);
        let mut spec = Spec::load(&dir).unwrap();
        gate::approve(&p, &mut spec, true).unwrap();
        start(&p, &mut spec).unwrap();
        let spec = Spec::load(&dir).unwrap();
        assert_eq!(
            blockers(&p, &spec, false, true).unwrap(),
            vec![Blocker::VerifyFailed]
        );
    }

    // ---------------------------------------------------- acceptance: 0002

    /// The false green this closes, and why it is a separate blocker: a spec
    /// whose tier A tests were never written must not close, and the report
    /// must say "no test" rather than folding it in with the ones merely
    /// switched off.
    #[test]
    fn a_tier_a_criterion_with_no_test_anywhere_blocks_the_close_on_its_own() {
        let (_t, p, dir) = setup(
            Status::Review,
            "acceptance:\n- tier: A\n  test: does_the_thing\n\
             - tier: A\n  test: was_never_written_at_all\n",
        );
        let mut spec = Spec::load(&dir).unwrap();
        gate::approve(&p, &mut spec, true).unwrap();
        start(&p, &mut spec).unwrap();
        let spec = Spec::load(&dir).unwrap();

        // `setup` writes `does_the_thing` and nothing else, so one criterion is
        // live and the other has no test at all. Under the marker count this
        // was zero pending and a clean close.
        let b = blockers(&p, &spec, true, true).unwrap();
        assert_eq!(
            b,
            vec![Blocker::MissingTest(vec![
                "was_never_written_at_all".to_string()
            ])],
            "a criterion with no test did not block the close"
        );

        // Distinguishable, not merely both present: the two failures have two
        // repairs and the message has to name the right one.
        let missing = b[0].explain("0001");
        assert!(missing.contains("was_never_written_at_all"));
        assert!(missing.contains("write them"), "{missing}");
        let pending = Blocker::Pending {
            remaining: 1,
            total: 2,
        }
        .explain("0001");
        assert!(pending.contains("remove their"), "{pending}");
        assert!(
            !pending.contains("write them"),
            "the two blockers read the same"
        );
    }

    /// The unification, and the only criterion that pins the call sites. Every
    /// command that answers "is this criterion done?" must answer the same for
    /// the same spec — `close`, `status`, `pending`, `pause`, the board and
    /// `review`. Without this, an implementation can satisfy every other
    /// criterion against one new function and leave five callers on the old
    /// count.
    #[test]
    fn every_command_reports_the_same_state_for_the_same_criterion() {
        let (_t, p, dir) = setup(
            Status::Review,
            "acceptance:\n- tier: A\n  test: does_the_thing\n\
             - tier: A\n  test: also_does_the_other_thing\n",
        );
        let mut spec = Spec::load(&dir).unwrap();
        gate::approve(&p, &mut spec, true).unwrap();
        start(&p, &mut spec).unwrap();

        // One criterion live, one still carrying its marker — and a repository
        // shouting the marker string from places that are not a criterion:
        // the spec's own prose, and a source fixture using a live id.
        std::fs::write(
            dir.join("acceptance.rs"),
            "fn does_the_thing() {\n    assert!(true)\n}\n\
             \n\
             #[ignore = \"acceptance: 0001 pending\"]\n\
             fn also_does_the_other_thing() {\n    todo!()\n}\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("notes.md"),
            "we owe `acceptance: 0001 pending` on two more paths\n",
        )
        .unwrap();
        std::fs::write(
            p.root.join("fixtures.rs"),
            "let sample = \"acceptance: 0001 pending\";\n",
        )
        .unwrap();
        let spec = Spec::load(&dir).unwrap();

        // The one answer: two criteria, one live, one inert, none missing.
        let prog = validate::progress(&p, &spec);
        assert_eq!((prog.total, prog.live, prog.inert), (2, 1, 1));
        assert!(prog.missing.is_empty());

        // `close`, from the blocker it raises.
        assert_eq!(
            blockers(&p, &spec, true, true).unwrap(),
            vec![Blocker::Pending {
                remaining: 1,
                total: 2
            }]
        );

        // `status`, which is also what the `SessionStart` hook injects.
        let active = crate::summary::build(&p).unwrap().active.unwrap();
        assert_eq!((active.live, active.total), (1, 2));

        // `review`, the dossier a human closes from. It reports per criterion
        // rather than as a count, so it is checked per criterion.
        let d = crate::review::build(&p, &spec).unwrap();
        let state = |name: &str| {
            d.criteria
                .iter()
                .find(|c| c.label.contains(name))
                .map(|c| format!("{:?}", c.evidence))
                .unwrap_or_default()
        };
        assert!(
            state("does_the_thing").starts_with("Live"),
            "{}",
            state("does_the_thing")
        );
        assert!(
            state("also_does_the_other_thing").starts_with("Pending"),
            "{}",
            state("also_does_the_other_thing")
        );

        // `board`, which renders every spec at once.
        let html = crate::board::render(&crate::board::build(&p).unwrap()).render();
        assert!(html.contains("1/2 criteria live"), "board disagrees");
    }
}
