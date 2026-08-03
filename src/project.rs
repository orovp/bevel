//! Project discovery and `.bevel/project.toml` (DESIGN.md §3).

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const STATE_DIR: &str = ".bevel";
/// The name this tool used before it was called bevel. Recognised only to give
/// a precise error instead of "no project found".
pub const LEGACY_STATE_DIR: &str = ".harness";
pub const CONFIG_FILE: &str = "project.toml";
pub const GATES_LOCK: &str = "gates.lock";

#[derive(Debug, Serialize, Deserialize)]
pub struct ProjectConfig {
    /// Version requirement for bevel itself. Pinned per project so a
    /// second machine on an older build fails loudly instead of behaving
    /// subtly differently (DESIGN.md §2).
    pub bevel: String,
    #[serde(default = "default_specs_dir")]
    pub specs_dir: String,
    #[serde(default = "default_inbox")]
    pub inbox: String,
    /// A method tree inside this repository, relative to the root. Lets the
    /// bevel repository host its own method with no configuration at all.
    /// A flat key rather than a `[method]` table, because a table declared
    /// after `packages` (an array of tables) would be parsed into it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub method_path: Option<String>,
    /// Workspace map, written by `doctor`. Detected rather than hand-kept: a
    /// drifted map is what produces a false green in `verify --affected`.
    #[serde(default)]
    pub packages: Vec<PackageEntry>,
}

fn default_specs_dir() -> String {
    "specs".into()
}

fn default_inbox() -> String {
    "INBOX.md".into()
}

impl Default for ProjectConfig {
    fn default() -> Self {
        Self {
            bevel: format!("^{}", crate::VERSION),
            specs_dir: default_specs_dir(),
            inbox: default_inbox(),
            method_path: None,
            packages: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageEntry {
    pub name: String,
    /// Repo-relative directory.
    pub path: String,
    pub ecosystem: Ecosystem,
    /// Workspace-internal dependencies, by package name.
    #[serde(default)]
    pub depends_on: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Ecosystem {
    Cargo,
    Npm,
}

#[derive(Debug)]
pub struct Project {
    pub root: PathBuf,
    pub config: ProjectConfig,
}

impl Project {
    pub fn discover() -> Result<Self> {
        let cwd = std::env::current_dir().context("cannot read the current directory")?;
        Self::discover_from(&cwd)
    }

    pub fn discover_from(start: &Path) -> Result<Self> {
        let mut dir = Some(start);
        while let Some(d) = dir {
            let cfg = d.join(STATE_DIR).join(CONFIG_FILE);
            if cfg.is_file() {
                let text = std::fs::read_to_string(&cfg)
                    .with_context(|| format!("cannot read {}", cfg.display()))?;
                let config: ProjectConfig = toml::from_str(&text)
                    .with_context(|| format!("cannot parse {}", cfg.display()))?;
                return Ok(Self {
                    root: d.to_path_buf(),
                    config,
                });
            }
            // A checkout from before the rename. Saying so precisely beats
            // claiming there is no project here, which is what the user would
            // otherwise be told about a directory that plainly has one.
            if d.join(LEGACY_STATE_DIR).join(CONFIG_FILE).is_file() {
                bail!(
                    "{} was created when this tool was called `harness`\n  \
                     move the state directory:\n    \
                     git mv {LEGACY_STATE_DIR} {STATE_DIR}\n  \
                     then rename the version-pin key inside {CONFIG_FILE} from \
                     `harness` to `bevel`",
                    d.join(LEGACY_STATE_DIR).display()
                );
            }
            dir = d.parent();
        }
        bail!(
            "no bevel project found in {} or any parent directory\n\
             run `bevel project init` at the repository root",
            start.display()
        )
    }

    pub fn state_dir(&self) -> PathBuf {
        self.root.join(STATE_DIR)
    }

    pub fn gates_lock(&self) -> PathBuf {
        self.state_dir().join(GATES_LOCK)
    }

    pub fn specs_dir(&self) -> PathBuf {
        self.root.join(&self.config.specs_dir)
    }

    pub fn inbox_path(&self) -> PathBuf {
        self.root.join(&self.config.inbox)
    }

    pub fn cache_dir(&self) -> PathBuf {
        self.state_dir().join("cache")
    }

    /// Path shown to the user: relative to the project root when possible.
    pub fn display_path(&self, p: &Path) -> String {
        p.strip_prefix(&self.root)
            .unwrap_or(p)
            .display()
            .to_string()
    }
}

/// Does `version` satisfy `req`?
///
/// Supports the caret and bare forms, which is all a pin needs. An
/// unrecognised requirement returns `None` — reported as unknown rather than
/// silently treated as satisfied, since the whole point of the pin is to fail
/// loudly when two machines disagree.
pub fn version_satisfies(req: &str, version: &str) -> Option<bool> {
    let caret = req.starts_with('^');
    let want = parse_semver(req.trim_start_matches(['^', '=', 'v']).trim())?;
    let have = parse_semver(version.trim_start_matches('v'))?;

    if have < want {
        return Some(false);
    }
    if !caret {
        return Some(have == want);
    }
    // Caret: 0.x pins the minor, everything else pins the major.
    Some(if want.0 == 0 {
        have.0 == 0 && have.1 == want.1
    } else {
        have.0 == want.0
    })
}

/// Which side of a pin mismatch is stale.
///
/// "Version mismatch" is not an actionable message: the fix is to upgrade the
/// binary in one direction and to migrate the project in the other.
#[derive(Debug, PartialEq, Eq)]
pub enum Pin {
    Satisfied,
    /// The project needs a newer bevel than this one.
    BinaryTooOld,
    /// This bevel is newer than the project was pinned to.
    ProjectTooOld,
    Unrecognised,
}

pub fn classify_pin(req: &str, version: &str) -> Pin {
    match version_satisfies(req, version) {
        Some(true) => Pin::Satisfied,
        None => Pin::Unrecognised,
        Some(false) => {
            let want = parse_semver(req.trim_start_matches(['^', '=', 'v']).trim());
            let have = parse_semver(version.trim_start_matches('v'));
            match (want, have) {
                (Some(w), Some(h)) if h < w => Pin::BinaryTooOld,
                (Some(_), Some(_)) => Pin::ProjectTooOld,
                _ => Pin::Unrecognised,
            }
        }
    }
}

fn parse_semver(s: &str) -> Option<(u64, u64, u64)> {
    let core = s.split(['-', '+']).next()?;
    let mut it = core.split('.');
    let major = it.next()?.parse().ok()?;
    let minor = it.next().unwrap_or("0").parse().ok()?;
    let patch = it.next().unwrap_or("0").parse().ok()?;
    if it.next().is_some() {
        return None;
    }
    Some((major, minor, patch))
}

/// Scaffold a project. Deliberately mechanical: it creates directories and
/// manifests but invents no architecture — that is what spec 0000 is for
/// (DESIGN.md §4).
pub fn init(root: &Path, monorepo: bool) -> Result<PathBuf> {
    let state = root.join(STATE_DIR);
    let cfg_path = state.join(CONFIG_FILE);
    if cfg_path.exists() {
        bail!(
            "{} already exists — this project is already initialised",
            cfg_path.display()
        );
    }
    std::fs::create_dir_all(state.join("cache"))?;

    let config = ProjectConfig::default();
    std::fs::write(&cfg_path, toml::to_string_pretty(&config)?)?;

    // Machine state is regenerable; artifacts are not. Only the former is ignored.
    std::fs::write(
        state.join(".gitignore"),
        "# Regenerable machine state. Artifacts live in specs/ and are versioned.\ncache/\n",
    )?;

    let specs = root.join(&config.specs_dir);
    std::fs::create_dir_all(&specs)?;

    let inbox = root.join(&config.inbox);
    if !inbox.exists() {
        std::fs::write(&inbox, INBOX_TEMPLATE)?;
    }

    if monorepo {
        for d in ["apps", "crates"] {
            std::fs::create_dir_all(root.join(d))?;
        }
    }

    Ok(cfg_path)
}

const INBOX_TEMPLATE: &str = "\
# Inbox

Raw ideas, one per line. Capture is supposed to be cheap — do not try to be
precise here, that is what shaping is for.

Add with `bevel inbox add \"...\"`, shape with `bevel shape <n>`.

";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_then_discover_from_a_nested_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        init(root, true).unwrap();

        let nested = root.join("crates").join("core").join("src");
        std::fs::create_dir_all(&nested).unwrap();

        let p = Project::discover_from(&nested).unwrap();
        assert_eq!(p.root.canonicalize().unwrap(), root.canonicalize().unwrap());
        assert_eq!(p.config.specs_dir, "specs");
        assert!(root.join("INBOX.md").is_file());
        assert!(root.join("specs").is_dir());
    }

    #[test]
    fn init_refuses_to_clobber_an_existing_project() {
        let tmp = tempfile::tempdir().unwrap();
        init(tmp.path(), false).unwrap();
        assert!(init(tmp.path(), false).is_err());
    }

    #[test]
    fn discover_fails_outside_a_project() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(Project::discover_from(tmp.path()).is_err());
    }

    #[test]
    fn a_pre_rename_checkout_gets_the_exact_move_command() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join(LEGACY_STATE_DIR)).unwrap();
        std::fs::write(
            tmp.path().join(LEGACY_STATE_DIR).join(CONFIG_FILE),
            "harness = \"^0.1.0\"\n",
        )
        .unwrap();

        let err = Project::discover_from(tmp.path()).unwrap_err().to_string();
        assert!(err.contains("was called `harness`"), "{err}");
        assert!(err.contains("git mv .harness .bevel"), "{err}");
        // The pin key is renamed too, so mentioning only the directory would
        // leave the user stuck one step later.
        assert!(err.contains("version-pin key"), "{err}");
    }

    #[test]
    fn caret_pins_the_minor_below_one_and_the_major_above() {
        // 0.x: a minor bump is breaking.
        assert_eq!(version_satisfies("^0.1.0", "0.1.4"), Some(true));
        assert_eq!(version_satisfies("^0.1.0", "0.2.0"), Some(false));
        assert_eq!(version_satisfies("^0.1.5", "0.1.2"), Some(false));
        // 1.x and up: minors are compatible.
        assert_eq!(version_satisfies("^1.2.0", "1.9.9"), Some(true));
        assert_eq!(version_satisfies("^1.2.0", "2.0.0"), Some(false));
    }

    #[test]
    fn a_pin_mismatch_says_which_side_is_stale() {
        // The project wants something newer than this build.
        assert_eq!(classify_pin("^0.9.0", "0.1.0"), Pin::BinaryTooOld);
        // This build has moved past what the project was pinned to.
        assert_eq!(classify_pin("^0.1.0", "0.9.0"), Pin::ProjectTooOld);
        assert_eq!(classify_pin("^0.1.0", "0.1.3"), Pin::Satisfied);
        assert_eq!(classify_pin("not a version", "0.1.0"), Pin::Unrecognised);
    }

    #[test]
    fn a_bare_requirement_is_exact_and_junk_is_unknown() {
        assert_eq!(version_satisfies("0.1.0", "0.1.0"), Some(true));
        assert_eq!(version_satisfies("0.1.0", "0.1.1"), Some(false));
        assert_eq!(version_satisfies(">=0.1, <2", "0.1.0"), None);
    }
}
