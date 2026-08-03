//! Resolved dependency versions, read from lockfiles (DESIGN.md §10).
//!
//! Two jobs, one source. Documentation must be pinned to the version the
//! project actually runs — asking for "latest" reintroduces the staleness
//! problem backwards — and a framework pack is active when the project
//! genuinely depends on that framework, which is a fact the lockfile already
//! records. Manifests would need glob and feature resolution to answer either
//! question; the lockfile has already done it.

use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::Path;

use crate::project::Ecosystem;

#[derive(Debug, Default)]
pub struct Deps {
    pub cargo: BTreeMap<String, String>,
    pub npm: BTreeMap<String, String>,
}

impl Deps {
    pub fn version(&self, ecosystem: Ecosystem, name: &str) -> Option<&str> {
        match ecosystem {
            Ecosystem::Cargo => self.cargo.get(name),
            Ecosystem::Npm => self.npm.get(name),
        }
        .map(String::as_str)
    }

    /// Any ecosystem, for when the caller does not know which one owns a name.
    pub fn any_version(&self, name: &str) -> Option<(Ecosystem, &str)> {
        self.cargo
            .get(name)
            .map(|v| (Ecosystem::Cargo, v.as_str()))
            .or_else(|| self.npm.get(name).map(|v| (Ecosystem::Npm, v.as_str())))
    }

    pub fn is_empty(&self) -> bool {
        self.cargo.is_empty() && self.npm.is_empty()
    }
}

/// Scan every lockfile at the repository root. A missing or unparseable
/// lockfile yields nothing rather than an error: the callers all degrade to
/// "unknown version", which is a supported state.
pub fn scan(root: &Path) -> Deps {
    Deps {
        cargo: std::fs::read_to_string(root.join("Cargo.lock"))
            .ok()
            .and_then(|t| parse_cargo(&t))
            .unwrap_or_default(),
        npm: std::fs::read_to_string(root.join("package-lock.json"))
            .ok()
            .and_then(|t| parse_npm(&t))
            .unwrap_or_default(),
    }
}

#[derive(Deserialize)]
struct CargoLock {
    #[serde(default)]
    package: Vec<CargoLockPackage>,
}

#[derive(Deserialize)]
struct CargoLockPackage {
    name: String,
    version: String,
}

pub fn parse_cargo(text: &str) -> Option<BTreeMap<String, String>> {
    let lock: CargoLock = toml::from_str(text).ok()?;
    Some(
        lock.package
            .into_iter()
            .map(|p| (p.name, p.version))
            .collect(),
    )
}

#[derive(Deserialize)]
struct NpmLock {
    /// Lockfile v2 and v3.
    #[serde(default)]
    packages: BTreeMap<String, NpmEntry>,
    /// Lockfile v1.
    #[serde(default)]
    dependencies: BTreeMap<String, NpmEntry>,
}

#[derive(Deserialize)]
struct NpmEntry {
    #[serde(default)]
    version: Option<String>,
}

pub fn parse_npm(text: &str) -> Option<BTreeMap<String, String>> {
    let lock: NpmLock = serde_json::from_str(text).ok()?;
    let mut out = BTreeMap::new();

    // v1: keys are bare package names.
    for (name, entry) in lock.dependencies {
        if let Some(v) = entry.version {
            out.insert(name, v);
        }
    }

    // v2/v3: keys are paths. The last `node_modules/` segment is the name, so
    // nested and scoped packages both resolve correctly.
    for (path, entry) in lock.packages {
        let Some(v) = entry.version else { continue };
        let Some(idx) = path.rfind("node_modules/") else {
            // The root project itself, keyed by "".
            continue;
        };
        let name = &path[idx + "node_modules/".len()..];
        if !name.is_empty() {
            out.insert(name.to_string(), v);
        }
    }

    Some(out)
}

/// Context7 encodes versions into some library ids as `_1_49_0`.
pub fn version_suffix(version: &str) -> Option<String> {
    let core = version.split(['-', '+']).next()?;
    let parts: Vec<&str> = core.split('.').collect();
    if parts.is_empty() || !parts.iter().all(|p| p.chars().all(|c| c.is_ascii_digit())) {
        return None;
    }
    Some(format!("_{}", parts.join("_")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cargo_lock_yields_every_resolved_package() {
        let text = r#"
version = 3

[[package]]
name = "tokio"
version = "1.49.0"

[[package]]
name = "serde"
version = "1.0.229"
"#;
        let deps = parse_cargo(text).unwrap();
        assert_eq!(deps.get("tokio").unwrap(), "1.49.0");
        assert_eq!(deps.get("serde").unwrap(), "1.0.229");
    }

    #[test]
    fn npm_lock_v3_paths_resolve_to_names_including_scoped_and_nested() {
        let text = r#"{
          "lockfileVersion": 3,
          "packages": {
            "": { "name": "root", "version": "1.0.0" },
            "node_modules/@angular/core": { "version": "19.2.1" },
            "node_modules/vite": { "version": "6.0.0" },
            "node_modules/a/node_modules/nested": { "version": "0.1.0" }
          }
        }"#;
        let deps = parse_npm(text).unwrap();
        assert_eq!(deps.get("@angular/core").unwrap(), "19.2.1");
        assert_eq!(deps.get("vite").unwrap(), "6.0.0");
        assert_eq!(deps.get("nested").unwrap(), "0.1.0");
        // The root entry has no name to key on and must not appear.
        assert!(!deps.contains_key(""));
    }

    #[test]
    fn npm_lock_v1_is_still_understood() {
        let text = r#"{
          "lockfileVersion": 1,
          "dependencies": { "milkdown": { "version": "7.5.0" } }
        }"#;
        assert_eq!(parse_npm(text).unwrap().get("milkdown").unwrap(), "7.5.0");
    }

    #[test]
    fn a_missing_or_broken_lockfile_is_not_an_error() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(scan(tmp.path()).is_empty());

        std::fs::write(tmp.path().join("Cargo.lock"), "this is not toml {{{").unwrap();
        std::fs::write(tmp.path().join("package-lock.json"), "not json").unwrap();
        assert!(scan(tmp.path()).is_empty());
    }

    #[test]
    fn both_ecosystems_coexist_and_are_queried_separately() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("Cargo.lock"),
            "[[package]]\nname = \"tokio\"\nversion = \"1.49.0\"\n",
        )
        .unwrap();
        std::fs::write(
            tmp.path().join("package-lock.json"),
            r#"{"lockfileVersion":3,"packages":{"node_modules/milkdown":{"version":"7.5.0"}}}"#,
        )
        .unwrap();

        let deps = scan(tmp.path());
        assert_eq!(deps.version(Ecosystem::Cargo, "tokio"), Some("1.49.0"));
        assert_eq!(deps.version(Ecosystem::Npm, "tokio"), None);
        assert_eq!(
            deps.any_version("milkdown"),
            Some((Ecosystem::Npm, "7.5.0"))
        );
    }

    #[test]
    fn version_suffixes_match_the_context7_id_convention() {
        assert_eq!(version_suffix("1.49.0").unwrap(), "_1_49_0");
        assert_eq!(version_suffix("19.2.1-rc.1").unwrap(), "_19_2_1");
        assert_eq!(version_suffix("7").unwrap(), "_7");
        assert_eq!(version_suffix("not.a.version"), None);
    }
}
