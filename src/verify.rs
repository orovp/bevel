//! Running pack verification (DESIGN.md §8, phase 4).
//!
//! This is where "is the code good?" stops being a judgment and becomes an
//! exit code.

use anyhow::{Context, Result};
use std::path::Path;
use std::process::Command;

use crate::affected::Scope;
use crate::packs::Pack;
use crate::project::PackageEntry;

#[derive(Debug)]
pub struct StepResult {
    pub pack: String,
    pub step: String,
    pub command: String,
    pub code: Option<i32>,
}

impl StepResult {
    pub fn ok(&self) -> bool {
        self.code == Some(0)
    }
}

/// Run every active pack's steps against `scope`.
///
/// Output is inherited rather than captured: the point of this command is that
/// a human or an agent reads the compiler's own diagnostics, and swallowing
/// them to reprint a summary would lose the part that matters.
pub fn run(
    root: &Path,
    packs: &[Pack],
    packages: &[PackageEntry],
    scope: &Scope,
    only: Option<&str>,
) -> Result<Vec<StepResult>> {
    let mut results = Vec::new();

    for pack in packs {
        if only.is_some_and(|id| id != pack.id) {
            continue;
        }

        // Names are per-ecosystem: a cargo step must not be handed npm packages.
        let scoped_names: Vec<String> = match scope {
            Scope::Packages(names) => packages
                .iter()
                .filter(|p| p.ecosystem == pack.ecosystem && names.contains(&p.name))
                .map(|p| p.name.clone())
                .collect(),
            _ => Vec::new(),
        };

        // A scoped run that touches none of this pack's packages has nothing
        // to do — skipping is correct, not a silent pass.
        if matches!(scope, Scope::Packages(_)) && scoped_names.is_empty() {
            continue;
        }

        for step in &pack.verify {
            let command = step.command(pack.package_arg(), &scoped_names);
            println!("  → [{}/{}] {command}", pack.id, step.name);

            let status = Command::new("sh")
                .arg("-c")
                .arg(&command)
                .current_dir(root)
                .status()
                .with_context(|| format!("could not run `{command}`"))?;

            results.push(StepResult {
                pack: pack.id.clone(),
                step: step.name.clone(),
                command,
                code: status.code(),
            });
        }
    }

    Ok(results)
}

pub fn summarise(results: &[StepResult]) -> String {
    let failed: Vec<&StepResult> = results.iter().filter(|r| !r.ok()).collect();
    if results.is_empty() {
        return "nothing to verify".into();
    }
    if failed.is_empty() {
        return format!("{} checks passed", results.len());
    }
    let mut out = format!("{} of {} checks failed:\n", failed.len(), results.len());
    for r in failed {
        out.push_str(&format!(
            "  {}/{} exited {}\n",
            r.pack,
            r.step,
            r.code.map(|c| c.to_string()).unwrap_or("by signal".into())
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::packs::{Source, Step};
    use crate::project::Ecosystem;

    fn pack(id: &str, ecosystem: Ecosystem, cmd: &str) -> Pack {
        Pack {
            id: id.into(),
            ecosystem,
            detect: vec![],
            extends: None,
            detect_dependency: None,
            context7: None,
            extensions: vec![],
            package_arg: Some("-p {}".into()),
            verify: vec![Step {
                name: "check".into(),
                cmd: cmd.into(),
                scoped: Some(format!("{cmd} {{packages}}")),
                fix: None,
            }],
            source: Source::Method,
            trigger: None,
        }
    }

    fn entry(name: &str, ecosystem: Ecosystem) -> PackageEntry {
        PackageEntry {
            name: name.into(),
            path: name.into(),
            ecosystem,
            depends_on: vec![],
        }
    }

    #[test]
    fn a_failing_step_is_reported_with_its_exit_code() {
        let tmp = tempfile::tempdir().unwrap();
        let packs = vec![pack("x", Ecosystem::Cargo, "exit 3")];
        let r = run(tmp.path(), &packs, &[], &Scope::Full("all".into()), None).unwrap();
        assert_eq!(r.len(), 1);
        assert!(!r[0].ok());
        assert_eq!(r[0].code, Some(3));
        assert!(summarise(&r).contains("1 of 1 checks failed"));
    }

    #[test]
    fn a_passing_step_summarises_cleanly() {
        let tmp = tempfile::tempdir().unwrap();
        let packs = vec![pack("x", Ecosystem::Cargo, "true")];
        let r = run(tmp.path(), &packs, &[], &Scope::Full("all".into()), None).unwrap();
        assert!(r[0].ok());
        assert_eq!(summarise(&r), "1 checks passed");
    }

    #[test]
    fn scoping_only_hands_a_pack_packages_from_its_own_ecosystem() {
        let tmp = tempfile::tempdir().unwrap();
        let packs = vec![
            pack("rust", Ecosystem::Cargo, "true"),
            pack("ts", Ecosystem::Npm, "true"),
        ];
        let packages = vec![
            entry("core", Ecosystem::Cargo),
            entry("web", Ecosystem::Npm),
        ];

        // Only the cargo package changed, so the ts pack has nothing to run.
        let scope = Scope::Packages(vec!["core".into()]);
        let r = run(tmp.path(), &packs, &packages, &scope, None).unwrap();
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].pack, "rust");
        assert!(r[0].command.ends_with("-p core"));
    }

    #[test]
    fn only_filters_to_a_single_pack() {
        let tmp = tempfile::tempdir().unwrap();
        let packs = vec![
            pack("rust", Ecosystem::Cargo, "true"),
            pack("ts", Ecosystem::Npm, "true"),
        ];
        let r = run(
            tmp.path(),
            &packs,
            &[],
            &Scope::Full("all".into()),
            Some("ts"),
        )
        .unwrap();
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].pack, "ts");
    }
}
