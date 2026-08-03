//! Workspace detection (DESIGN.md §3).
//!
//! No task runner: cargo and npm side by side. Cargo already ships the
//! dependency graph via `cargo metadata`, so the only real work is npm, where
//! the edges are the workspace-internal entries of each `package.json`.

use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;

use crate::project::{Ecosystem, PackageEntry};

pub struct Workspace {
    pub packages: Vec<PackageEntry>,
    /// Whether dependency edges are known for every package.
    ///
    /// When false, `affected` must widen to a full run. A file-to-package map
    /// without the edges produces false greens, and a verification tool that
    /// reports false greens is worse than none: it turns a caught bug into a
    /// trusted one.
    pub graph_complete: bool,
    pub notes: Vec<String>,
}

pub fn detect(root: &Path) -> Result<Workspace> {
    let mut packages = Vec::new();
    let mut notes = Vec::new();
    let mut graph_complete = true;

    if root.join("Cargo.toml").is_file() {
        match cargo_packages(root) {
            Ok(mut p) => packages.append(&mut p),
            Err(e) => {
                graph_complete = false;
                notes.push(format!(
                    "cargo metadata unavailable ({e}); verify will not scope"
                ));
            }
        }
    }

    if root.join("package.json").is_file() {
        let (mut p, complete) = npm_packages(root)?;
        packages.append(&mut p);
        if !complete {
            graph_complete = false;
            notes.push("some npm workspace globs could not be expanded".into());
        }
    }

    packages.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(Workspace {
        packages,
        graph_complete,
        notes,
    })
}

// ---------------------------------------------------------------- cargo

#[derive(Deserialize)]
struct CargoMetadata {
    packages: Vec<CargoPackage>,
    workspace_members: Vec<String>,
    workspace_root: String,
}

#[derive(Deserialize)]
struct CargoPackage {
    id: String,
    name: String,
    manifest_path: String,
    dependencies: Vec<CargoDep>,
}

#[derive(Deserialize)]
struct CargoDep {
    name: String,
}

fn cargo_packages(root: &Path) -> Result<Vec<PackageEntry>> {
    let out = Command::new("cargo")
        .args(["metadata", "--no-deps", "--format-version", "1"])
        .current_dir(root)
        .output()
        .context("could not run `cargo metadata`")?;
    if !out.status.success() {
        anyhow::bail!(
            "cargo metadata failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    let meta: CargoMetadata =
        serde_json::from_slice(&out.stdout).context("could not parse cargo metadata output")?;

    let members: Vec<&CargoPackage> = meta
        .packages
        .iter()
        .filter(|p| meta.workspace_members.contains(&p.id))
        .collect();
    let member_names: Vec<&str> = members.iter().map(|p| p.name.as_str()).collect();
    let ws_root = Path::new(&meta.workspace_root);

    Ok(members
        .iter()
        .map(|p| {
            let dir = Path::new(&p.manifest_path)
                .parent()
                .unwrap_or(ws_root)
                .strip_prefix(ws_root)
                .unwrap_or(Path::new(""));
            PackageEntry {
                name: p.name.clone(),
                path: normalise(dir),
                ecosystem: Ecosystem::Cargo,
                // Only workspace-internal edges matter for scoping.
                depends_on: p
                    .dependencies
                    .iter()
                    .filter(|d| member_names.contains(&d.name.as_str()))
                    .map(|d| d.name.clone())
                    .collect(),
            }
        })
        .collect())
}

// ------------------------------------------------------------------ npm

#[derive(Deserialize)]
struct PackageJson {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    workspaces: Option<Workspaces>,
    #[serde(default)]
    dependencies: BTreeMap<String, String>,
    #[serde(default)]
    #[serde(rename = "devDependencies")]
    dev_dependencies: BTreeMap<String, String>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Workspaces {
    List(Vec<String>),
    Object { packages: Vec<String> },
}

impl Workspaces {
    fn globs(&self) -> &[String] {
        match self {
            Workspaces::List(v) => v,
            Workspaces::Object { packages } => packages,
        }
    }
}

fn npm_packages(root: &Path) -> Result<(Vec<PackageEntry>, bool)> {
    let text = std::fs::read_to_string(root.join("package.json"))?;
    let manifest: PackageJson = serde_json::from_str(&text)
        .with_context(|| format!("cannot parse {}", root.join("package.json").display()))?;
    let Some(ws) = manifest.workspaces else {
        return Ok((Vec::new(), true));
    };

    let mut dirs = Vec::new();
    let mut complete = true;
    for glob in ws.globs() {
        match expand(root, glob) {
            Some(mut found) => dirs.append(&mut found),
            None => complete = false,
        }
    }

    let mut raw = Vec::new();
    for dir in dirs {
        let pj = dir.join("package.json");
        if !pj.is_file() {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&pj) else {
            continue;
        };
        let Ok(m) = serde_json::from_str::<PackageJson>(&text) else {
            complete = false;
            continue;
        };
        let Some(name) = m.name.clone() else { continue };
        let rel = dir.strip_prefix(root).unwrap_or(&dir).to_path_buf();
        raw.push((name, normalise(&rel), m));
    }

    let names: Vec<String> = raw.iter().map(|(n, _, _)| n.clone()).collect();
    let packages = raw
        .iter()
        .map(|(name, path, m)| PackageEntry {
            name: name.clone(),
            path: path.clone(),
            ecosystem: Ecosystem::Npm,
            depends_on: m
                .dependencies
                .keys()
                .chain(m.dev_dependencies.keys())
                .filter(|d| names.contains(d))
                .cloned()
                .collect(),
        })
        .collect();
    Ok((packages, complete))
}

/// Expand the glob shapes npm workspaces actually use: a literal path, a
/// single-level `dir/*`, or a bounded recursive `dir/**`. Returns `None` when
/// the pattern is something else, which widens verification rather than
/// guessing.
fn expand(root: &Path, glob: &str) -> Option<Vec<std::path::PathBuf>> {
    let glob = glob.trim_end_matches('/');
    if let Some(base) = glob.strip_suffix("/**") {
        let mut out = Vec::new();
        collect_dirs(&root.join(base), 4, &mut out);
        return Some(out);
    }
    if let Some(base) = glob.strip_suffix("/*") {
        let dir = root.join(base);
        let entries = std::fs::read_dir(dir).ok()?;
        return Some(
            entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.is_dir())
                .collect(),
        );
    }
    if glob.contains('*') {
        return None;
    }
    let p = root.join(glob);
    p.is_dir().then(|| vec![p])
}

fn collect_dirs(dir: &Path, depth: usize, out: &mut Vec<std::path::PathBuf>) {
    if depth == 0 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() && e.file_name() != "node_modules" {
            out.push(p.clone());
            collect_dirs(&p, depth - 1, out);
        }
    }
}

fn normalise(p: &Path) -> String {
    let s = p.to_string_lossy().replace('\\', "/");
    if s.is_empty() {
        ".".into()
    } else {
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn npm_workspaces_are_expanded_with_internal_edges_only() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        std::fs::write(
            root.join("package.json"),
            r#"{"name":"root","workspaces":["apps/*","tools/cli"]}"#,
        )
        .unwrap();

        for (dir, json) in [
            (
                "apps/web",
                r#"{"name":"web","dependencies":{"shared":"*","react":"^19"}}"#,
            ),
            ("apps/shared", r#"{"name":"shared","dependencies":{}}"#),
            (
                "tools/cli",
                r#"{"name":"cli","devDependencies":{"shared":"*"}}"#,
            ),
        ] {
            std::fs::create_dir_all(root.join(dir)).unwrap();
            std::fs::write(root.join(dir).join("package.json"), json).unwrap();
        }

        let ws = detect(root).unwrap();
        let names: Vec<&str> = ws.packages.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, vec!["shared", "web", "cli"]);
        assert!(ws.graph_complete);

        let web = ws.packages.iter().find(|p| p.name == "web").unwrap();
        // `react` is external and must not become an edge.
        assert_eq!(web.depends_on, vec!["shared"]);
        assert_eq!(web.path, "apps/web");

        let cli = ws.packages.iter().find(|p| p.name == "cli").unwrap();
        assert_eq!(cli.depends_on, vec!["shared"]);
    }

    #[test]
    fn an_unexpandable_glob_marks_the_graph_incomplete() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("package.json"),
            r#"{"name":"root","workspaces":["pkg-*-thing"]}"#,
        )
        .unwrap();
        let ws = detect(tmp.path()).unwrap();
        assert!(!ws.graph_complete);
    }

    #[test]
    fn a_repo_with_no_manifests_yields_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        let ws = detect(tmp.path()).unwrap();
        assert!(ws.packages.is_empty());
        assert!(ws.graph_complete);
    }
}
