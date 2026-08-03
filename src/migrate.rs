//! Moving a project across bevel versions (DESIGN.md §2).
//!
//! Version drift between two machines is the failure this exists to prevent.
//! The rule is that drift must be loud: a project pinned to a version this
//! binary cannot serve stops, rather than behaving subtly differently on the
//! other laptop.

use anyhow::{bail, Result};
use std::path::Path;

use crate::project::{self, Project};
use crate::spec::{self, Spec, SCHEMA_VERSION};

/// A single artifact upgrade. Returns whether it changed anything.
type Step = fn(&mut Spec) -> Result<bool>;

/// Registered as `(from, to, step)`. Empty while schema 1 is the only schema —
/// the plumbing exists so the first real migration is a one-line addition
/// rather than a redesign under time pressure.
const STEPS: &[(u32, u32, Step)] = &[];

#[derive(Debug)]
pub struct Plan {
    pub current_pin: String,
    pub new_pin: String,
    pub pin_changes: bool,
    /// Specs written by a newer bevel than this one. Blocking.
    pub ahead: Vec<(String, u32)>,
    /// Specs that a registered step would upgrade.
    pub upgradable: Vec<(String, u32)>,
}

impl Plan {
    pub fn blocked(&self) -> bool {
        !self.ahead.is_empty()
    }

    pub fn is_noop(&self) -> bool {
        !self.pin_changes && self.upgradable.is_empty()
    }
}

pub fn plan(project: &Project) -> Result<Plan> {
    let mut ahead = Vec::new();
    let mut upgradable = Vec::new();

    for s in spec::all(&project.specs_dir())? {
        let v = s.front.schema_version;
        if v > SCHEMA_VERSION {
            ahead.push((s.front.id.clone(), v));
        } else if v < SCHEMA_VERSION && STEPS.iter().any(|(from, _, _)| *from == v) {
            upgradable.push((s.front.id.clone(), v));
        }
    }

    let new_pin = format!("^{}", crate::VERSION);
    Ok(Plan {
        pin_changes: project.config.bevel != new_pin,
        current_pin: project.config.bevel.clone(),
        new_pin,
        ahead,
        upgradable,
    })
}

pub fn apply(project: &mut Project) -> Result<Vec<String>> {
    let plan = plan(project)?;
    if plan.blocked() {
        bail!(
            "specs {} were written by a newer bevel (schema {} vs {} here)\n  \
             This binary is behind, not the project. Upgrade it:\n    \
             npm i -g @orovp/bevel   (or: cargo install bevel)",
            plan.ahead
                .iter()
                .map(|(id, _)| id.as_str())
                .collect::<Vec<_>>()
                .join(", "),
            plan.ahead.iter().map(|(_, v)| *v).max().unwrap_or(0),
            SCHEMA_VERSION
        );
    }

    let mut done = Vec::new();

    for (id, from) in &plan.upgradable {
        let mut s = spec::find(&project.specs_dir(), id)?;
        let mut version = *from;
        while let Some((_, to, step)) = STEPS.iter().find(|(f, _, _)| *f == version) {
            if step(&mut s)? {
                done.push(format!("spec {id}: schema {version} -> {to}"));
            }
            version = *to;
        }
        s.front.schema_version = version;
        s.save()?;
    }

    if plan.pin_changes {
        project.config.bevel = plan.new_pin.clone();
        write_config(project)?;
        done.push(format!(
            "pin: {} -> {}",
            plan.current_pin, project.config.bevel
        ));
    }

    Ok(done)
}

fn write_config(project: &Project) -> Result<()> {
    let path = project.state_dir().join(project::CONFIG_FILE);
    std::fs::write(&path, toml::to_string_pretty(&project.config)?)?;
    Ok(())
}

/// A spec directory that exists but holds no `spec.md` is not a spec; report it
/// rather than letting it look like a missing artifact later.
pub fn orphan_dirs(specs_dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    if let Ok(entries) = std::fs::read_dir(specs_dir) {
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() && !p.join(spec::SPEC_FILE).is_file() {
                out.push(e.file_name().to_string_lossy().into_owned());
            }
        }
    }
    out.sort();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project_with(pin: &str) -> (tempfile::TempDir, Project) {
        let tmp = tempfile::tempdir().unwrap();
        crate::project::init(tmp.path(), false).unwrap();
        let mut p = Project::discover_from(tmp.path()).unwrap();
        p.config.bevel = pin.to_string();
        write_config(&p).unwrap();
        let reloaded = Project::discover_from(tmp.path()).unwrap();
        (tmp, reloaded)
    }

    fn write_spec(p: &Project, id: &str, schema: u32) {
        let dir = p.specs_dir().join(format!("{id}-x"));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("spec.md"),
            format!(
                "---\nid: '{id}'\ntitle: X\nstatus: draft\nschema_version: {schema}\n\
                 created: '2026-08-03'\nacceptance: []\n---\n# X\n\ny\n"
            ),
        )
        .unwrap();
    }

    #[test]
    fn an_outdated_pin_is_updated_and_reported() {
        let (_t, mut p) = project_with("^0.0.1");
        let first = plan(&p).unwrap();
        assert!(first.pin_changes);
        assert!(!first.blocked());

        let done = apply(&mut p).unwrap();
        assert_eq!(done.len(), 1);
        assert!(done[0].starts_with("pin: ^0.0.1 -> ^"));

        // Re-running changes nothing.
        let p2 = Project::discover_from(&p.root).unwrap();
        assert!(plan(&p2).unwrap().is_noop());
    }

    #[test]
    fn a_spec_from_a_newer_bevel_blocks_and_says_to_upgrade_the_binary() {
        let (_t, mut p) = project_with(&format!("^{}", crate::VERSION));
        write_spec(&p, "0001", SCHEMA_VERSION + 3);

        let plan = plan(&p).unwrap();
        assert!(plan.blocked());
        assert_eq!(plan.ahead, vec![("0001".to_string(), SCHEMA_VERSION + 3)]);

        let err = apply(&mut p).unwrap_err().to_string();
        // The direction of the problem has to be unambiguous.
        assert!(err.contains("This binary is behind"), "{err}");
        assert!(err.contains("npm i -g"), "{err}");
    }

    #[test]
    fn specs_at_the_current_schema_need_nothing() {
        let (_t, p) = project_with(&format!("^{}", crate::VERSION));
        write_spec(&p, "0001", SCHEMA_VERSION);
        let plan = plan(&p).unwrap();
        assert!(plan.upgradable.is_empty());
        assert!(plan.is_noop());
    }

    #[test]
    fn a_directory_without_a_spec_file_is_reported_as_an_orphan() {
        let (_t, p) = project_with("^0.1.0");
        std::fs::create_dir_all(p.specs_dir().join("0009-half-made")).unwrap();
        write_spec(&p, "0001", SCHEMA_VERSION);
        assert_eq!(orphan_dirs(&p.specs_dir()), vec!["0009-half-made"]);
    }
}
