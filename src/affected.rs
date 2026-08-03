//! Changed files → packages to verify (DESIGN.md §3).
//!
//! Load-bearing rather than an optimisation: the implement loop verifies after
//! every task, and a full-workspace run at that cadence is what makes people
//! switch verification off.
//!
//! Every ambiguity here widens the scope. A slow run costs minutes; a false
//! green costs a bug you have stopped looking for.

use anyhow::{Context, Result};
use std::collections::BTreeSet;
use std::path::Path;
use std::process::Command;

use crate::project::PackageEntry;
use crate::workspace::Workspace;

/// Fraction of the workspace above which scoping stops paying for itself.
const FULL_RUN_THRESHOLD: f64 = 0.6;

/// Changing any of these invalidates assumptions the package map is built on.
const SENTINELS: [&str; 8] = [
    "Cargo.lock",
    "Cargo.toml",
    "package-lock.json",
    "package.json",
    "pnpm-lock.yaml",
    "rust-toolchain.toml",
    "clippy.toml",
    "rustfmt.toml",
];

#[derive(Debug, PartialEq, Eq)]
pub enum Scope {
    /// Verify everything, with the reason worth showing the user.
    Full(String),
    /// Verify exactly these package names.
    Packages(Vec<String>),
    /// Nothing changed.
    Nothing,
}

pub fn compute(root: &Path, ws: &Workspace, since: Option<&str>) -> Result<Scope> {
    let changed = changed_files(root, since)?;
    Ok(from_changed_files(ws, &changed))
}

/// Split out from `compute` so the mapping rules are testable without a repo.
pub fn from_changed_files(ws: &Workspace, changed: &[String]) -> Scope {
    if changed.is_empty() {
        return Scope::Nothing;
    }
    if !ws.graph_complete {
        return Scope::Full("the workspace dependency graph is incomplete".into());
    }
    if ws.packages.is_empty() {
        return Scope::Full("no workspace packages detected".into());
    }

    let mut direct: BTreeSet<String> = BTreeSet::new();
    for file in changed {
        if let Some(name) = sentinel(file) {
            return Scope::Full(format!("`{name}` changed"));
        }
        match owner(&ws.packages, file) {
            Some(pkg) => {
                direct.insert(pkg.name.clone());
            }
            // Unknown means unknown. A path outside every package could be a
            // shared script, a build input, or anything else.
            None => return Scope::Full(format!("`{file}` belongs to no package")),
        }
    }

    let affected = expand_dependents(&ws.packages, direct);
    let ratio = affected.len() as f64 / ws.packages.len() as f64;
    if ratio > FULL_RUN_THRESHOLD {
        return Scope::Full(format!(
            "{} of {} packages affected",
            affected.len(),
            ws.packages.len()
        ));
    }
    Scope::Packages(affected.into_iter().collect())
}

/// A sentinel matches at the repository root or inside any package.
fn sentinel(file: &str) -> Option<&'static str> {
    if file.starts_with(".bevel/") {
        return Some(".bevel/");
    }
    if file.starts_with(".github/") {
        return Some(".github/");
    }
    let base = file.rsplit('/').next().unwrap_or(file);
    SENTINELS.iter().copied().find(|s| *s == base)
}

/// Longest-prefix match, so a nested package wins over its parent.
fn owner<'a>(packages: &'a [PackageEntry], file: &str) -> Option<&'a PackageEntry> {
    packages
        .iter()
        .filter(|p| p.path == "." || file.starts_with(&format!("{}/", p.path)))
        .max_by_key(|p| p.path.len())
}

/// Add every package that transitively depends on something already in the set.
fn expand_dependents(packages: &[PackageEntry], seed: BTreeSet<String>) -> BTreeSet<String> {
    let mut out = seed;
    loop {
        let mut added = false;
        for p in packages {
            if out.contains(&p.name) {
                continue;
            }
            if p.depends_on.iter().any(|d| out.contains(d)) {
                out.insert(p.name.clone());
                added = true;
            }
        }
        if !added {
            return out;
        }
    }
}

/// Uncommitted work plus, when a base is given, everything since it.
fn changed_files(root: &Path, since: Option<&str>) -> Result<Vec<String>> {
    let mut files = BTreeSet::new();

    // Tracked modifications and untracked files in one call.
    let porcelain = git(root, &["status", "--porcelain", "--untracked-files=all"])?;
    for line in porcelain.lines() {
        if line.len() > 3 {
            // Renames appear as `old -> new`; the destination is what matters.
            let path = line[3..].rsplit(" -> ").next().unwrap_or(&line[3..]);
            files.insert(path.trim_matches('"').to_string());
        }
    }

    if let Some(base) = since {
        let range = format!("{base}...HEAD");
        for line in git(root, &["diff", "--name-only", &range])?.lines() {
            if !line.trim().is_empty() {
                files.insert(line.to_string());
            }
        }
    }

    Ok(files.into_iter().collect())
}

fn git(root: &Path, args: &[&str]) -> Result<String> {
    let out = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .with_context(|| format!("could not run `git {}`", args.join(" ")))?;
    if !out.status.success() {
        anyhow::bail!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::Ecosystem;

    fn pkg(name: &str, path: &str, deps: &[&str]) -> PackageEntry {
        PackageEntry {
            name: name.into(),
            path: path.into(),
            ecosystem: Ecosystem::Cargo,
            depends_on: deps.iter().map(|s| s.to_string()).collect(),
        }
    }

    /// Six packages so the 60% threshold does not fire on small sets.
    fn ws() -> Workspace {
        Workspace {
            packages: vec![
                pkg("core", "crates/core", &[]),
                pkg("api", "crates/api", &["core"]),
                pkg("cli", "crates/cli", &["api"]),
                pkg("web", "apps/web", &[]),
                pkg("docs", "apps/docs", &[]),
                pkg("tools", "apps/tools", &[]),
            ],
            graph_complete: true,
            notes: vec![],
        }
    }

    #[test]
    fn a_leaf_change_scopes_to_one_package() {
        let s = from_changed_files(&ws(), &["apps/web/src/main.ts".into()]);
        assert_eq!(s, Scope::Packages(vec!["web".into()]));
    }

    #[test]
    fn changing_a_dependency_pulls_in_its_dependents_transitively() {
        // core → api → cli must all be verified, not just core.
        let s = from_changed_files(&ws(), &["crates/core/src/lib.rs".into()]);
        assert_eq!(
            s,
            Scope::Packages(vec!["api".into(), "cli".into(), "core".into()])
        );
    }

    #[test]
    fn a_lockfile_change_forces_a_full_run() {
        match from_changed_files(&ws(), &["Cargo.lock".into()]) {
            Scope::Full(reason) => assert!(reason.contains("Cargo.lock")),
            other => panic!("expected full, got {other:?}"),
        }
    }

    #[test]
    fn a_path_outside_every_package_forces_a_full_run() {
        match from_changed_files(&ws(), &["scripts/release.sh".into()]) {
            Scope::Full(reason) => assert!(reason.contains("belongs to no package")),
            other => panic!("expected full, got {other:?}"),
        }
    }

    #[test]
    fn an_incomplete_graph_forces_a_full_run() {
        let mut w = ws();
        w.graph_complete = false;
        assert!(matches!(
            from_changed_files(&w, &["apps/web/x.ts".into()]),
            Scope::Full(_)
        ));
    }

    #[test]
    fn scoping_gives_up_once_most_of_the_workspace_is_affected() {
        let s = from_changed_files(
            &ws(),
            &[
                "crates/core/a.rs".into(),
                "apps/web/b.ts".into(),
                "apps/docs/c.ts".into(),
            ],
        );
        // core+api+cli+web+docs = 5 of 6, over the threshold.
        match s {
            Scope::Full(reason) => assert!(reason.contains("5 of 6")),
            other => panic!("expected full, got {other:?}"),
        }
    }

    #[test]
    fn nested_packages_resolve_to_the_innermost() {
        let w = Workspace {
            packages: vec![
                pkg("outer", "apps", &[]),
                pkg("inner", "apps/web", &[]),
                pkg("a", "x/a", &[]),
                pkg("b", "x/b", &[]),
                pkg("c", "x/c", &[]),
                pkg("d", "x/d", &[]),
            ],
            graph_complete: true,
            notes: vec![],
        };
        assert_eq!(
            from_changed_files(&w, &["apps/web/main.ts".into()]),
            Scope::Packages(vec!["inner".into()])
        );
    }

    #[test]
    fn no_changes_means_nothing_to_verify() {
        assert_eq!(from_changed_files(&ws(), &[]), Scope::Nothing);
    }
}
