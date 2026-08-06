//! Where the method comes from.
//!
//! The method — skills, subagents, packs, artifact templates — lives in the
//! GitHub repository, not in this binary. Editing a markdown file must not
//! require a release, which is the whole point of moving it out.
//!
//! The cost, stated plainly because it reverses an earlier decision: a machine
//! that has never fetched and cannot reach the network has no method at all.
//! That is a missing install rather than a degraded mode, and three things keep
//! it from biting — the cache is permanent once warm, `postinstall` fills it
//! while the network is demonstrably reachable, and a local checkout needs no
//! network ever.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

use crate::config::Method as MethodConfig;
use crate::paths::Layers;
use crate::project::Project;

/// A method tree is only usable if this exists inside it.
const SENTINEL: &str = "method/skills/shape/SKILL.md";
const META_FILE: &str = ".bevel-fetch.json";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    /// A directory on disk: a checkout, or this repository itself. Always live,
    /// never fetched — this is the mode for working on bevel itself.
    Local,
    /// Extracted from a GitHub tarball and cached.
    Fetched { repo: String, git_ref: String },
}

#[derive(Debug, Clone)]
pub struct Source {
    pub kind: Kind,
    /// Directory containing `method/` and `packs/`.
    pub root: PathBuf,
    /// Why this root was chosen, for `doctor`.
    pub origin: String,
}

impl Source {
    pub fn method_dir(&self) -> PathBuf {
        self.root.join("method")
    }

    pub fn packs_dir(&self) -> PathBuf {
        self.root.join("packs")
    }

    pub fn is_usable(&self) -> bool {
        self.root.join(SENTINEL).is_file()
    }

    pub fn is_local(&self) -> bool {
        self.kind == Kind::Local
    }
}

/// Resolve the base method, most explicit first.
///
/// A local path always wins over a fetched copy: if you have pointed at a
/// checkout you are working on the method, and silently preferring a cached
/// download would be maddening.
pub fn resolve(project: Option<&Project>, layers: &Layers, cfg: &MethodConfig) -> Source {
    if let Some(dir) = std::env::var_os("BEVEL_METHOD_DIR") {
        return local(PathBuf::from(dir), "BEVEL_METHOD_DIR");
    }

    // Lets the harness repository host its own method with no configuration.
    if let (Some(p), Some(rel)) = (project, project.and_then(|p| p.config.method_path.as_ref())) {
        return local(p.root.join(rel), ".bevel/project.toml [method] path");
    }

    if let Some(dir) = &cfg.path {
        return local(PathBuf::from(dir), "config.toml [method] path");
    }

    let root = cache_dir(layers, &cfg.repo, &cfg.git_ref);
    Source {
        kind: Kind::Fetched {
            repo: cfg.repo.clone(),
            git_ref: cfg.git_ref.clone(),
        },
        origin: format!("{}@{} (cached)", cfg.repo, cfg.git_ref),
        root,
    }
}

fn local(root: PathBuf, origin: &str) -> Source {
    Source {
        kind: Kind::Local,
        root,
        origin: origin.to_string(),
    }
}

pub fn cache_dir(layers: &Layers, repo: &str, git_ref: &str) -> PathBuf {
    layers
        .cache
        .join("method")
        .join(repo.replace('/', "_"))
        .join(sanitise(git_ref))
}

/// Refs can contain slashes (`release/1.x`) and must not escape the cache.
///
/// Mapping separators to `_` is not sufficient on its own: a ref of exactly
/// `..` survives a character filter that permits dots, and would resolve one
/// level above the cache. Anything that is only dots is therefore replaced
/// outright rather than filtered.
fn sanitise(s: &str) -> String {
    let mapped: String = s
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if mapped.is_empty() || mapped.chars().all(|c| c == '.') {
        "ref".to_string()
    } else {
        mapped
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Meta {
    pub repo: String,
    pub git_ref: String,
    /// Hash of the method tree's contents.
    ///
    /// Recorded instead of a commit SHA on purpose: it needs no API call, and it
    /// answers the question that actually matters when two machines disagree —
    /// whether the *instructions* differ, not whether the commit does.
    pub content_hash: String,
    pub fetched_at: String,
}

pub fn meta(root: &Path) -> Option<Meta> {
    serde_json::from_str(&std::fs::read_to_string(root.join(META_FILE)).ok()?).ok()
}

#[derive(Debug)]
pub struct FetchReport {
    pub root: PathBuf,
    pub content_hash: String,
    pub git_ref: String,
    pub changed: bool,
}

/// Download and extract the method tree.
///
/// A tarball rather than a clone: one request, no history, and the same URL form
/// works for a branch, a tag or a commit SHA.
pub fn fetch(
    layers: &Layers,
    cfg: &MethodConfig,
    git_ref: Option<&str>,
    timeout_secs: u32,
) -> Result<FetchReport> {
    let git_ref = git_ref.unwrap_or(&cfg.git_ref).to_string();
    let dest = cache_dir(layers, &cfg.repo, &git_ref);
    let previous = meta(&dest).map(|m| m.content_hash);

    let url = format!(
        "https://codeload.github.com/{}/tar.gz/{}",
        cfg.repo, git_ref
    );
    // Downloaded before the staging directory exists, so that a failed request
    // leaves nothing behind at all.
    let timeout = std::time::Duration::from_secs(timeout_secs.into());
    let response = crate::http::get(&url, timeout, None).map_err(|e| {
        anyhow::anyhow!(
            "could not download {url}\n  {e}\n  \
             Check the repo and ref, or point at a local checkout with \
             `[method] path` in {}",
            layers.config_file().display()
        )
    })?;
    if response.status != 200 {
        bail!(
            "could not download {url}\n  http {}\n  \
             Check the repo and ref, or point at a local checkout with \
             `[method] path` in {}",
            response.status,
            layers.config_file().display()
        );
    }

    let staging = dest.with_extension("incoming");
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(&staging)
        .with_context(|| format!("cannot create {}", staging.display()))?;

    if let Err(e) = crate::archive::extract_tar_gz(&response.body, &staging) {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(e.context(format!("could not extract {url}")));
    }

    if !staging.join(SENTINEL).is_file() {
        let _ = std::fs::remove_dir_all(&staging);
        bail!(
            "{}@{git_ref} does not look like a bevel method tree ({SENTINEL} is missing)",
            cfg.repo
        );
    }

    let content_hash = hash_tree(&staging);
    std::fs::write(
        staging.join(META_FILE),
        serde_json::to_string_pretty(&Meta {
            repo: cfg.repo.clone(),
            git_ref: git_ref.clone(),
            content_hash: content_hash.clone(),
            fetched_at: chrono::Utc::now().to_rfc3339(),
        })?,
    )?;

    // Swap in only once the tree is known good, so a failed fetch never leaves
    // a half-extracted method behind.
    if dest.exists() {
        let old = dest.with_extension("previous");
        let _ = std::fs::remove_dir_all(&old);
        std::fs::rename(&dest, &old)?;
        std::fs::rename(&staging, &dest)?;
        let _ = std::fs::remove_dir_all(&old);
    } else {
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::rename(&staging, &dest)?;
    }

    Ok(FetchReport {
        changed: previous.as_deref() != Some(content_hash.as_str()),
        root: dest,
        content_hash,
        git_ref,
    })
}

/// Hash every method and pack file, path included, in a stable order.
pub fn hash_tree(root: &Path) -> String {
    let mut files = Vec::new();
    for sub in ["method", "packs"] {
        collect(&root.join(sub), root, &mut files);
    }
    files.sort();

    let mut h = Sha256::new();
    h.update(b"bevel-method-v1\n");
    for rel in &files {
        h.update(rel.as_bytes());
        h.update(b"\n");
        if let Ok(bytes) = std::fs::read(root.join(rel)) {
            h.update(&bytes);
        }
    }
    format!("sha256:{:x}", h.finalize())
}

fn collect(dir: &Path, root: &Path, out: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            collect(&p, root, out);
        } else if let Ok(rel) = p.strip_prefix(root) {
            out.push(rel.to_string_lossy().replace('\\', "/"));
        }
    }
}

/// The message shown when the method is not installed. Actionable, because the
/// user hit this on a machine that may have no network at all.
pub fn missing_help(source: &Source, layers: &Layers) -> String {
    let mut s = format!(
        "the bevel method is not available at {}\n",
        source.root.display()
    );
    match &source.kind {
        Kind::Fetched { repo, git_ref } => {
            s.push_str(&format!(
                "  fetch it:            bevel method fetch\n  \
                 (source: {repo}@{git_ref})\n  \
                 If this machine cannot reach GitHub, point at a local checkout:\n    \
                 [method]\n    path = \"/path/to/bevel\"\n  in {}\n",
                layers.config_file().display()
            ));
        }
        Kind::Local => s.push_str(
            "  that path came from a local override; check it exists and \
             contains method/ and packs/\n",
        ),
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layers(tmp: &Path) -> Layers {
        Layers {
            config: tmp.join("cfg"),
            cache: tmp.join("cache"),
            home: tmp.join("home"),
            opencode: tmp.join("home/.config/opencode"),
        }
    }

    fn write_tree(root: &Path) {
        std::fs::create_dir_all(root.join("method/skills/shape")).unwrap();
        std::fs::create_dir_all(root.join("packs/rust")).unwrap();
        std::fs::write(root.join(SENTINEL), "---\nname: shape\n---\nbody\n").unwrap();
        std::fs::write(root.join("packs/rust/pack.toml"), "id = \"rust\"\n").unwrap();
    }

    #[test]
    fn an_env_override_beats_everything_else() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("checkout");
        write_tree(&dir);
        std::env::set_var("BEVEL_METHOD_DIR", &dir);

        let cfg = MethodConfig {
            path: Some("/somewhere/else".into()),
            ..Default::default()
        };
        let s = resolve(None, &layers(tmp.path()), &cfg);
        std::env::remove_var("BEVEL_METHOD_DIR");

        assert_eq!(s.kind, Kind::Local);
        assert_eq!(s.root, dir);
        assert!(s.is_usable());
        assert_eq!(s.origin, "BEVEL_METHOD_DIR");
    }

    #[test]
    fn a_configured_path_wins_over_the_fetch_cache() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("checkout");
        write_tree(&dir);
        std::env::remove_var("BEVEL_METHOD_DIR");

        let cfg = MethodConfig {
            path: Some(dir.to_string_lossy().into_owned()),
            ..Default::default()
        };
        let s = resolve(None, &layers(tmp.path()), &cfg);
        assert!(s.is_local());
        assert_eq!(s.method_dir(), dir.join("method"));
    }

    #[test]
    fn with_no_override_the_source_is_the_cache_for_the_pinned_ref() {
        let tmp = tempfile::tempdir().unwrap();
        std::env::remove_var("BEVEL_METHOD_DIR");
        let cfg = MethodConfig {
            repo: "orovp/bevel".into(),
            git_ref: "v1.2.3".into(),
            path: None,
        };
        let s = resolve(None, &layers(tmp.path()), &cfg);
        assert!(matches!(s.kind, Kind::Fetched { .. }));
        assert!(s.root.ends_with("method/orovp_bevel/v1.2.3"));
        // Nothing fetched yet, so it is not usable and must say so.
        assert!(!s.is_usable());
    }

    /// The invariant is that a ref becomes exactly one path component beneath
    /// the cache directory, whatever it contains.
    #[test]
    fn a_hostile_ref_cannot_escape_the_cache_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let l = layers(tmp.path());
        let base = l.cache.join("method").join("orovp_bevel");

        for git_ref in ["../../etc/passwd", "..", ".", "...", "", "/etc", "a/../.."] {
            let d = cache_dir(&l, "orovp/bevel", git_ref);
            assert!(d.starts_with(&base), "{git_ref} escaped: {}", d.display());
            let tail = d.strip_prefix(&base).unwrap();
            assert_eq!(
                tail.components().count(),
                1,
                "{git_ref} produced {} components",
                tail.components().count()
            );
            // Normalising must not leave a component that traverses upwards.
            assert_ne!(tail.to_string_lossy(), "..");
            assert_ne!(tail.to_string_lossy(), ".");
        }

        // Ordinary refs stay readable.
        assert!(cache_dir(&l, "orovp/bevel", "release/1.x").ends_with("release_1.x"));
        assert!(cache_dir(&l, "orovp/bevel", "v1.2.3").ends_with("v1.2.3"));
    }

    #[test]
    fn the_content_hash_tracks_method_and_pack_files_only() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write_tree(root);
        let base = hash_tree(root);

        // An unrelated file must not move it.
        std::fs::write(root.join("README.md"), "hello").unwrap();
        assert_eq!(hash_tree(root), base);

        // A skill edit must.
        std::fs::write(root.join(SENTINEL), "---\nname: shape\n---\nchanged\n").unwrap();
        let after = hash_tree(root);
        assert_ne!(after, base);

        // So must a pack edit.
        std::fs::write(root.join("packs/rust/pack.toml"), "id = \"rust\"\nx = 1\n").unwrap();
        assert_ne!(hash_tree(root), after);
    }

    #[test]
    fn the_missing_message_names_both_ways_out() {
        let tmp = tempfile::tempdir().unwrap();
        let l = layers(tmp.path());
        std::env::remove_var("BEVEL_METHOD_DIR");
        let s = resolve(None, &l, &MethodConfig::default());
        let help = missing_help(&s, &l);
        assert!(help.contains("bevel method fetch"));
        // The offline machine needs the second route spelled out.
        assert!(help.contains("[method]"));
        assert!(help.contains("path ="));
    }
}
