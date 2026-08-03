//! Language and framework packs (DESIGN.md §10).
//!
//! A pack carries conventions, verification commands and version-specific
//! traps — the things the model cannot know. It is not a tutorial: if a
//! sentence could have been written from memory, it does not belong.
//!
//! Resolution runs method tree → user → project, last one wins. The base packs
//! come from the method tree on disk rather than the binary, so editing one
//! never requires a release.

use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::{Path, PathBuf};

use crate::lockfile::Deps;
use crate::paths::Layers;
use crate::project::{Ecosystem, Project};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Method,
    User,
    Project,
}

impl Source {
    pub fn as_str(self) -> &'static str {
        match self {
            Source::Method => "method",
            Source::User => "user",
            Source::Project => "project",
        }
    }
}

/// Why a pack is switched on, worth reporting because a pack that is silently
/// inactive looks exactly like a pack that is passing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Trigger {
    /// A detection file exists.
    File(String),
    /// The lockfile resolves a dependency, with the version it resolved to.
    Dependency(String, String),
}

impl std::fmt::Display for Trigger {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Trigger::File(p) => write!(f, "{p}"),
            Trigger::Dependency(name, version) => write!(f, "{name}@{version}"),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Context7Ref {
    /// A Context7 library id, verified against their search API.
    pub library: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Pack {
    pub id: String,
    pub ecosystem: Ecosystem,
    /// The language pack this sits on top of. Supplies `package_arg` and
    /// documents lineage; it does not duplicate verification, because both
    /// packs are active at once and the language pack's steps already run.
    #[serde(default)]
    pub extends: Option<String>,
    /// Detection by file existence, for language packs.
    #[serde(default)]
    pub detect: Vec<String>,
    /// Detection by resolved dependency, for framework packs. The lockfile has
    /// already done feature and glob resolution, so asking it is both cheaper
    /// and more accurate than reading manifests.
    #[serde(default)]
    pub detect_dependency: Option<String>,
    #[serde(default)]
    pub context7: Option<Context7Ref>,
    /// File extensions this pack owns, used to route a written file to the
    /// right formatter. Without it the format hook would have to guess.
    #[serde(default)]
    pub extensions: Vec<String>,
    #[serde(default)]
    pub package_arg: Option<String>,
    #[serde(default)]
    pub verify: Vec<Step>,
    #[serde(skip, default = "default_source")]
    pub source: Source,
    #[serde(skip)]
    pub trigger: Option<Trigger>,
}

fn default_source() -> Source {
    Source::Method
}

impl Pack {
    pub fn package_arg(&self) -> &str {
        self.package_arg.as_deref().unwrap_or("{}")
    }

    pub fn owns_extension(&self, ext: &str) -> bool {
        self.extensions.iter().any(|e| e == ext)
    }

    /// The step that can rewrite a file in place, if this pack has one.
    pub fn fix_step(&self) -> Option<&Step> {
        self.verify.iter().find(|s| s.fix.is_some())
    }

    pub fn library(&self) -> Option<&str> {
        self.context7.as_ref().map(|c| c.library.as_str())
    }

    /// Short name, for `bevel docs tokio` rather than `bevel docs rust/tokio`.
    pub fn short_id(&self) -> &str {
        self.id.rsplit('/').next().unwrap_or(&self.id)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Step {
    pub name: String,
    /// Whole-workspace form.
    pub cmd: String,
    /// Scoped form; `{packages}` expands to the batched package arguments.
    #[serde(default)]
    pub scoped: Option<String>,
    /// Auto-fix, for a format-on-write hook.
    #[serde(default)]
    pub fix: Option<String>,
}

impl Step {
    /// Resolve to a shell command. Falls back to the whole-workspace form when
    /// no scoped variant exists, which is the safe direction.
    pub fn command(&self, package_arg: &str, packages: &[String]) -> String {
        match (&self.scoped, packages.is_empty()) {
            (Some(scoped), false) => {
                let args = packages
                    .iter()
                    .map(|p| package_arg.replace("{}", p))
                    .collect::<Vec<_>>()
                    .join(" ");
                scoped.replace("{packages}", &args)
            }
            _ => self.cmd.clone(),
        }
    }
}

/// Every known pack, with outer layers overriding inner ones by id.
pub fn load_all(
    project: &Project,
    layers: &Layers,
    method: &crate::method::Source,
) -> Result<Vec<Pack>> {
    let mut packs: Vec<Pack> = Vec::new();

    for mut p in load_dir(&method.packs_dir())? {
        p.source = Source::Method;
        packs.push(p);
    }

    for (dir, source) in [
        (layers.user_packs(), Source::User),
        (project.state_dir().join("packs"), Source::Project),
    ] {
        for mut p in load_dir(&dir)? {
            p.source = source;
            match packs.iter().position(|e| e.id == p.id) {
                Some(i) => packs[i] = p,
                None => packs.push(p),
            }
        }
    }

    // Resolve `extends` after everything is loaded, so a user override of a
    // language pack is what its framework packs inherit from.
    let inherited: Vec<(usize, String)> = packs
        .iter()
        .enumerate()
        .filter_map(|(i, p)| match (&p.extends, &p.package_arg) {
            (Some(parent), None) => Some((i, parent.clone())),
            _ => None,
        })
        .collect();
    for (i, parent) in inherited {
        if let Some(arg) = packs
            .iter()
            .find(|p| p.id == parent)
            .and_then(|p| p.package_arg.clone())
        {
            packs[i].package_arg = Some(arg);
        }
    }

    packs.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(packs)
}

/// Packs live at `<dir>/<id>/pack.toml` or `<dir>/<lang>/<framework>/pack.toml`.
fn load_dir(dir: &Path) -> Result<Vec<Pack>> {
    let mut out = Vec::new();
    collect(dir, 2, &mut out)?;
    Ok(out)
}

fn collect(dir: &Path, depth: usize, out: &mut Vec<Pack>) -> Result<()> {
    if depth == 0 || !dir.is_dir() {
        return Ok(());
    }
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if !path.is_dir() {
            continue;
        }
        let manifest = path.join("pack.toml");
        if manifest.is_file() {
            let text = std::fs::read_to_string(&manifest)?;
            out.push(
                toml::from_str(&text)
                    .with_context(|| format!("cannot parse {}", manifest.display()))?,
            );
        }
        collect(&path, depth - 1, out)?;
    }
    Ok(())
}

/// Packs switched on by this project, each carrying why.
///
/// Detected rather than configured: a hand-kept list drifts, and a drifted
/// list is how verification quietly stops covering something.
pub fn active(packs: &[Pack], root: &Path, package_paths: &[String], deps: &Deps) -> Vec<Pack> {
    let mut out = Vec::new();
    for pack in packs {
        let mut p = pack.clone();
        p.trigger = trigger_for(pack, root, package_paths, deps);
        if p.trigger.is_some() {
            out.push(p);
        }
    }
    out
}

fn trigger_for(pack: &Pack, root: &Path, package_paths: &[String], deps: &Deps) -> Option<Trigger> {
    if let Some(name) = &pack.detect_dependency {
        return deps
            .version(pack.ecosystem, name)
            .map(|v| Trigger::Dependency(name.clone(), v.to_string()));
    }
    pack.detect.iter().find_map(|f| {
        if root.join(f).is_file() {
            return Some(Trigger::File(f.clone()));
        }
        package_paths.iter().find_map(|dir| {
            root.join(dir)
                .join(f)
                .is_file()
                .then(|| Trigger::File(format!("{dir}/{f}")))
        })
    })
}

/// Where a pack's `gotchas.md` would live, per layer.
///
/// The packs in the method tree deliberately ship without one. A shared pack
/// cannot know your lint configuration, your test runner or your error-handling
/// convention, and inventing framework lore that an agent would then treat as
/// authoritative is worse than leaving the file absent.
pub fn gotchas_candidates(pack: &Pack, project: &Project, layers: &Layers) -> Vec<PathBuf> {
    vec![
        project
            .state_dir()
            .join("packs")
            .join(&pack.id)
            .join("gotchas.md"),
        layers.user_packs().join(&pack.id).join("gotchas.md"),
    ]
}

pub fn gotchas(pack: &Pack, project: &Project, layers: &Layers) -> Option<PathBuf> {
    gotchas_candidates(pack, project, layers)
        .into_iter()
        .find(|p| p.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (tempfile::TempDir, Project, Layers, crate::method::Source) {
        let tmp = tempfile::tempdir().unwrap();
        crate::project::init(tmp.path(), false).unwrap();
        let project = Project::discover_from(tmp.path()).unwrap();
        let layers = Layers {
            config: tmp.path().join("cfg"),
            cache: tmp.path().join("cache"),
            home: tmp.path().join("home"),
        };
        let method = crate::method::Source {
            kind: crate::method::Kind::Local,
            root: std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")),
            origin: "repo".into(),
        };
        (tmp, project, layers, method)
    }

    #[test]
    fn every_builtin_parses_and_framework_packs_declare_a_library() {
        let (_t, p, l, m) = setup();
        let packs = load_all(&p, &l, &m).unwrap();
        assert_eq!(packs.len(), 8);

        for pack in &packs {
            if pack.extends.is_some() {
                assert!(
                    pack.library().is_some(),
                    "{} has no context7 library",
                    pack.id
                );
                assert!(
                    pack.detect_dependency.is_some(),
                    "{} has no dependency trigger",
                    pack.id
                );
            }
        }
    }

    #[test]
    fn a_framework_pack_inherits_package_arg_from_its_language_pack() {
        let (_t, p, l, m) = setup();
        let packs = load_all(&p, &l, &m).unwrap();
        let tokio = packs.iter().find(|p| p.id == "rust/tokio").unwrap();
        assert_eq!(tokio.package_arg(), "-p {}");
        let angular = packs.iter().find(|p| p.id == "ts/angular").unwrap();
        assert_eq!(angular.package_arg(), "-w {}");
    }

    #[test]
    fn framework_packs_activate_from_the_lockfile_not_from_files() {
        let (_t, p, l, m) = setup();
        let packs = load_all(&p, &l, &m).unwrap();
        std::fs::write(p.root.join("Cargo.toml"), "[workspace]\n").unwrap();

        // No lockfile: the language pack is on, the framework pack is not.
        let empty = Deps::default();
        let on = active(&packs, &p.root, &[], &empty);
        let ids: Vec<&str> = on.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(ids, vec!["rust"]);

        std::fs::write(
            p.root.join("Cargo.lock"),
            "[[package]]\nname = \"tokio\"\nversion = \"1.49.0\"\n",
        )
        .unwrap();
        let deps = crate::lockfile::scan(&p.root);
        let on = active(&packs, &p.root, &[], &deps);
        let tokio = on.iter().find(|p| p.id == "rust/tokio").unwrap();
        assert_eq!(
            tokio.trigger,
            Some(Trigger::Dependency("tokio".into(), "1.49.0".into()))
        );
    }

    #[test]
    fn a_dependency_of_the_wrong_ecosystem_does_not_activate_a_pack() {
        let (_t, p, l, m) = setup();
        let packs = load_all(&p, &l, &m).unwrap();
        // An npm package called `tokio` must not switch on the Rust pack.
        let mut deps = Deps::default();
        deps.npm.insert("tokio".into(), "9.9.9".into());
        let on = active(&packs, &p.root, &[], &deps);
        assert!(!on.iter().any(|p| p.id == "rust/tokio"));
    }

    #[test]
    fn a_nested_user_pack_overrides_a_nested_builtin() {
        let (_t, p, l, m) = setup();
        let dir = l.user_packs().join("rust/tokio");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("pack.toml"),
            "id = \"rust/tokio\"\nextends = \"rust\"\necosystem = \"cargo\"\n\
             detect_dependency = \"tokio\"\n\n[context7]\nlibrary = \"/mine/tokio\"\n",
        )
        .unwrap();

        let packs = load_all(&p, &l, &m).unwrap();
        let tokio = packs.iter().find(|p| p.id == "rust/tokio").unwrap();
        assert_eq!(tokio.source, Source::User);
        assert_eq!(tokio.library(), Some("/mine/tokio"));
        // Inheritance still resolves through the override.
        assert_eq!(tokio.package_arg(), "-p {}");
    }

    #[test]
    fn scoped_commands_batch_package_arguments() {
        let (_t, p, l, m) = setup();
        let packs = load_all(&p, &l, &m).unwrap();
        let rust = packs.iter().find(|p| p.id == "rust").unwrap();
        let lint = rust.verify.iter().find(|s| s.name == "lint").unwrap();
        assert_eq!(
            lint.command(rust.package_arg(), &["core".into(), "api".into()]),
            "cargo clippy -p core -p api --all-targets -- -D warnings"
        );
        assert_eq!(
            lint.command(rust.package_arg(), &[]),
            "cargo clippy --all-targets -- -D warnings"
        );
    }

    #[test]
    fn builtins_ship_no_gotchas_and_the_layers_are_reported() {
        let (_t, p, l, m) = setup();
        let packs = load_all(&p, &l, &m).unwrap();
        let tokio = packs.iter().find(|p| p.id == "rust/tokio").unwrap();
        assert!(gotchas(tokio, &p, &l).is_none());

        let dir = l.user_packs().join("rust/tokio");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("gotchas.md"), "we use nextest\n").unwrap();
        assert!(gotchas(tokio, &p, &l).is_some());
    }
}
