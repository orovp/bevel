//! The gate (DESIGN.md §5).
//!
//! Most spec-driven workflows leave the gate as a sentence in a markdown file
//! and an agent, in good faith, walks straight past it. Here approval is a
//! hash written to a versioned lockfile, and the check is an exit code.
//!
//! Threat model, stated plainly: this stops an enthusiastic agent, not an
//! adversary. The agent runs as the user and could edit `gates.lock` directly.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::IsTerminal;
use std::path::Path;

use crate::project::Project;
use crate::spec::{Spec, Status};

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct GatesLock {
    #[serde(default)]
    pub spec: BTreeMap<String, Entry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    pub hash: String,
    pub approved_at: String,
    pub approved_by: String,
    pub bevel_version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub done_commit: Option<String>,
}

impl GatesLock {
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("cannot read {}", path.display()))?;
        toml::from_str(&text).with_context(|| format!("cannot parse {}", path.display()))
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let header = "# Approval record. Versioned deliberately: an approval is a historical\n\
                      # fact about the project, and it has to survive a clone and show up in\n\
                      # a diff. Written by `bevel approve`.\n\n";
        std::fs::write(path, format!("{header}{}", toml::to_string_pretty(self)?))
            .with_context(|| format!("cannot write {}", path.display()))
    }
}

/// Canonical hash over the parts of a spec that constitute the contract.
///
/// Deliberately excluded: `status` (managed by the gate itself), and
/// `decisions.md` / `open-questions.md` / `mockup.html`, which are logs and
/// references. Annotating a log must not invalidate an approval.
///
/// The layout is written by hand rather than delegated to a serializer so that
/// a serde or YAML upgrade cannot silently invalidate every approval in every
/// project on disk.
pub fn spec_hash(spec: &Spec) -> Result<String> {
    let mut h = Sha256::new();
    h.update(b"bevel-gate-v1\n");
    h.update(format!("id:{}\n", spec.front.id));
    h.update(format!("title:{}\n", spec.front.title));

    let mut packages = spec.front.packages.clone();
    packages.sort();
    h.update(format!("packages:{}\n", packages.join(",")));

    for s in &spec.front.supersedes {
        h.update(format!("supersedes:{}\n", s.id));
        for d in &s.dispositions {
            h.update(format!(
                "  disposition:{}:{:?}:{}\n",
                d.test,
                d.action,
                d.note.as_deref().unwrap_or("")
            ));
        }
    }

    for c in &spec.front.acceptance {
        h.update(format!("criterion:{}\n", c.canonical()));
    }

    h.update(b"--body--\n");
    h.update(spec.body.trim().as_bytes());
    h.update(b"\n");

    for file in spec.acceptance_files()? {
        let name = file
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        h.update(format!("--file:{name}--\n"));
        h.update(std::fs::read(&file).with_context(|| format!("cannot read {}", file.display()))?);
    }

    Ok(format!("sha256:{:x}", h.finalize()))
}

#[derive(Debug, PartialEq, Eq)]
pub enum Verdict {
    /// Approved, hash matches, and no other spec holds the active slot.
    Open,
    NotApproved,
    /// Approved once, but the contract has changed since.
    HashMismatch,
    /// Another spec is already `implementing` (DESIGN.md §5).
    OtherSpecActive(String),
    /// Terminal states cannot be implemented.
    WrongStatus(Status),
}

impl Verdict {
    pub fn is_open(&self) -> bool {
        matches!(self, Verdict::Open)
    }

    pub fn explain(&self, id: &str) -> String {
        match self {
            Verdict::Open => format!("spec {id} is approved and the active slot is free"),
            Verdict::NotApproved => {
                format!("spec {id} is not approved\n  a human must run: bevel approve {id}")
            }
            Verdict::HashMismatch => format!(
                "spec {id} was approved, then edited — the gate has reopened\n  \
                 review the change and re-approve: bevel approve {id}"
            ),
            Verdict::OtherSpecActive(other) => format!(
                "spec {other} is already being implemented\n  \
                 one active spec at a time: finish it, or run bevel pause {other}"
            ),
            Verdict::WrongStatus(s) => {
                format!("spec {id} is `{s}` and cannot be implemented")
            }
        }
    }
}

/// Evaluate the gate for `spec`. This is what `/implement` calls first.
pub fn check(project: &Project, spec: &Spec) -> Result<Verdict> {
    match spec.front.status {
        Status::Done | Status::Superseded => {
            return Ok(Verdict::WrongStatus(spec.front.status));
        }
        Status::Draft | Status::Review => return Ok(Verdict::NotApproved),
        Status::Approved | Status::Implementing => {}
    }

    let lock = GatesLock::load(&project.gates_lock())?;
    let Some(entry) = lock.spec.get(&spec.front.id) else {
        return Ok(Verdict::NotApproved);
    };
    if entry.hash != spec_hash(spec)? {
        return Ok(Verdict::HashMismatch);
    }

    // The active slot: exactly one spec may be `implementing`.
    if let Some(other) = active_spec(project)? {
        if other != spec.front.id {
            return Ok(Verdict::OtherSpecActive(other));
        }
    }

    Ok(Verdict::Open)
}

/// The id of the spec currently holding the active slot, if any.
///
/// Derived from the specs themselves rather than stored in a lockfile: a
/// second source of truth would desync, would not travel between machines
/// through git, and would need cleaning up after a crashed session.
pub fn active_spec(project: &Project) -> Result<Option<String>> {
    for s in crate::spec::all(&project.specs_dir())? {
        if s.front.status == Status::Implementing {
            return Ok(Some(s.front.id));
        }
    }
    Ok(None)
}

/// Freeze the current contract. Refuses without a terminal, because an agent's
/// bash is not one.
pub fn approve(project: &Project, spec: &mut Spec, assume_yes: bool) -> Result<()> {
    if !assume_yes && !std::io::stdin().is_terminal() {
        bail!(
            "approval requires a terminal\n  \
             This is the human gate: an agent's shell is non-interactive, which is the point.\n  \
             Run it yourself in a terminal."
        );
    }

    match spec.front.status {
        Status::Draft => bail!(
            "spec {} is still a draft — run `bevel validate {}` first",
            spec.front.id,
            spec.front.id
        ),
        Status::Done | Status::Superseded => bail!(
            "spec {} is `{}` and cannot be approved",
            spec.front.id,
            spec.front.status
        ),
        Status::Review | Status::Approved | Status::Implementing => {}
    }

    let hash = spec_hash(spec)?;
    let mut lock = GatesLock::load(&project.gates_lock())?;
    lock.spec.insert(
        spec.front.id.clone(),
        Entry {
            hash,
            approved_at: chrono::Utc::now().to_rfc3339(),
            approved_by: whoami(),
            bevel_version: crate::VERSION.to_string(),
            done_commit: lock
                .spec
                .get(&spec.front.id)
                .and_then(|e| e.done_commit.clone()),
        },
    );
    lock.save(&project.gates_lock())?;

    // Re-approval mid-implementation keeps the spec active; it does not
    // restart it.
    if spec.front.status == Status::Review {
        spec.front.status = Status::Approved;
        spec.save()?;
    }
    Ok(())
}

/// Release the active slot without losing anything: the hash is untouched, so
/// resuming needs no re-approval, and progress lives in the acceptance markers
/// rather than in the status.
pub fn pause(spec: &mut Spec) -> Result<()> {
    if spec.front.status != Status::Implementing {
        bail!(
            "spec {} is `{}`, not implementing — nothing to pause",
            spec.front.id,
            spec.front.status
        );
    }
    spec.front.status = Status::Approved;
    spec.save()
}

fn whoami() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("LOGNAME"))
        .unwrap_or_else(|_| "unknown".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::Criterion;
    use std::path::PathBuf;

    fn project_with_spec(status: Status) -> (tempfile::TempDir, Project, PathBuf) {
        let tmp = tempfile::tempdir().unwrap();
        crate::project::init(tmp.path(), false).unwrap();
        let project = Project::discover_from(tmp.path()).unwrap();
        let dir = project.specs_dir().join("0001-example");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("spec.md"),
            format!(
                "---\nid: '0001'\ntitle: Example\nstatus: {}\nschema_version: 1\ncreated: '2026-08-02'\nacceptance:\n- tier: A\n  test: does_the_thing\n---\n# Problem\n\nSomething.\n",
                status.as_str()
            ),
        )
        .unwrap();
        std::fs::write(dir.join("acceptance.rs"), "// pending\n").unwrap();
        (tmp, project, dir)
    }

    #[test]
    fn hash_ignores_status_but_tracks_body_and_acceptance_files() {
        let (_tmp, _p, dir) = project_with_spec(Status::Review);
        let mut spec = Spec::load(&dir).unwrap();
        let base = spec_hash(&spec).unwrap();

        // Status is gate-managed and must not move the hash.
        spec.front.status = Status::Approved;
        assert_eq!(spec_hash(&spec).unwrap(), base);

        // The body is the contract.
        spec.body.push_str("\nmore\n");
        assert_ne!(spec_hash(&spec).unwrap(), base);

        // So is every acceptance file.
        let mut spec = Spec::load(&dir).unwrap();
        assert_eq!(spec_hash(&spec).unwrap(), base);
        std::fs::write(dir.join("acceptance.rs"), "// changed\n").unwrap();
        assert_ne!(spec_hash(&spec).unwrap(), base);

        // Logs are not.
        std::fs::write(dir.join("acceptance.rs"), "// pending\n").unwrap();
        std::fs::write(dir.join("decisions.md"), "# Q&A\n").unwrap();
        spec = Spec::load(&dir).unwrap();
        assert_eq!(spec_hash(&spec).unwrap(), base);
    }

    #[test]
    fn editing_an_approved_spec_reopens_the_gate() {
        let (_tmp, project, dir) = project_with_spec(Status::Review);
        let mut spec = Spec::load(&dir).unwrap();
        approve(&project, &mut spec, true).unwrap();
        assert_eq!(spec.front.status, Status::Approved);
        assert_eq!(check(&project, &spec).unwrap(), Verdict::Open);

        spec.body.push_str("\nan amendment\n");
        spec.save().unwrap();
        let spec = Spec::load(&dir).unwrap();
        assert_eq!(check(&project, &spec).unwrap(), Verdict::HashMismatch);
    }

    #[test]
    fn unapproved_specs_do_not_pass_the_gate() {
        let (_tmp, project, dir) = project_with_spec(Status::Review);
        let spec = Spec::load(&dir).unwrap();
        assert_eq!(check(&project, &spec).unwrap(), Verdict::NotApproved);
    }

    #[test]
    fn a_second_spec_cannot_claim_the_active_slot() {
        let (_tmp, project, dir) = project_with_spec(Status::Review);
        let mut first = Spec::load(&dir).unwrap();
        approve(&project, &mut first, true).unwrap();
        first.front.status = Status::Implementing;
        first.save().unwrap();

        let dir2 = project.specs_dir().join("0002-other");
        std::fs::create_dir_all(&dir2).unwrap();
        std::fs::write(
            dir2.join("spec.md"),
            "---\nid: '0002'\ntitle: Other\nstatus: review\nschema_version: 1\ncreated: '2026-08-02'\nacceptance:\n- tier: A\n  test: other_thing\n---\n# Problem\n\nElse.\n",
        )
        .unwrap();
        let mut second = Spec::load(&dir2).unwrap();
        approve(&project, &mut second, true).unwrap();

        assert_eq!(
            check(&project, &second).unwrap(),
            Verdict::OtherSpecActive("0001".into())
        );

        // Pausing the first frees the slot without touching its approval.
        let mut first = Spec::load(&dir).unwrap();
        pause(&mut first).unwrap();
        assert_eq!(check(&project, &second).unwrap(), Verdict::Open);
        assert_eq!(check(&project, &first).unwrap(), Verdict::Open);
    }

    #[test]
    fn done_specs_are_not_implementable() {
        let (_tmp, project, dir) = project_with_spec(Status::Done);
        let spec = Spec::load(&dir).unwrap();
        assert_eq!(
            check(&project, &spec).unwrap(),
            Verdict::WrongStatus(Status::Done)
        );
    }

    #[test]
    fn criteria_order_is_part_of_the_contract() {
        let (_tmp, _p, dir) = project_with_spec(Status::Review);
        let mut spec = Spec::load(&dir).unwrap();
        spec.front.acceptance = vec![
            Criterion::A { test: "a".into() },
            Criterion::B { cmd: "b".into() },
        ];
        let one = spec_hash(&spec).unwrap();
        spec.front.acceptance.reverse();
        assert_ne!(spec_hash(&spec).unwrap(), one);
    }
}
