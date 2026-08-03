//! The global layers (DESIGN.md §2).
//!
//! Three lifetimes, three locations. The split exists so that syncing between
//! machines needs no ignore rules: `~/.config` travels, `~/.cache` does not.

use anyhow::{Context, Result};
use std::path::PathBuf;

pub struct Layers {
    /// User layer: config and hand-written packs. Small, syncable.
    pub config: PathBuf,
    /// Machine layer: regenerable, never synced, safe to delete.
    pub cache: PathBuf,
}

impl Layers {
    pub fn resolve() -> Result<Self> {
        // BEVEL_HOME collapses both into one directory for anyone who
        // prefers a single location over the XDG split.
        if let Some(home) = env_path("BEVEL_HOME") {
            return Ok(Self {
                config: home.join("config"),
                cache: home.join("cache"),
            });
        }
        let home = env_path("HOME").context("HOME is not set")?;
        let config = env_path("XDG_CONFIG_HOME").unwrap_or_else(|| home.join(".config"));
        let cache = env_path("XDG_CACHE_HOME").unwrap_or_else(|| home.join(".cache"));
        Ok(Self {
            config: config.join("bevel"),
            cache: cache.join("bevel"),
        })
    }

    pub fn user_packs(&self) -> PathBuf {
        self.config.join("packs")
    }

    pub fn user_method(&self) -> PathBuf {
        self.config.join("method")
    }

    pub fn config_file(&self) -> PathBuf {
        self.config.join("config.toml")
    }

    pub fn context7_cache(&self) -> PathBuf {
        self.cache.join("context7")
    }
}

fn env_path(key: &str) -> Option<PathBuf> {
    std::env::var_os(key)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Env vars are process-global, so the two layout cases share one test to
    /// avoid racing under the default parallel test runner.
    #[test]
    fn layout_honours_bevel_home_then_falls_back_to_xdg() {
        let _guard = EnvGuard::set(&[
            ("BEVEL_HOME", Some("/tmp/hh")),
            ("HOME", Some("/home/someone")),
            ("XDG_CONFIG_HOME", None),
            ("XDG_CACHE_HOME", None),
        ]);
        let l = Layers::resolve().unwrap();
        assert_eq!(l.config, PathBuf::from("/tmp/hh/config"));
        assert_eq!(l.cache, PathBuf::from("/tmp/hh/cache"));

        let _guard2 = EnvGuard::set(&[("BEVEL_HOME", None)]);
        let l = Layers::resolve().unwrap();
        assert_eq!(l.config, PathBuf::from("/home/someone/.config/bevel"));
        assert_eq!(l.cache, PathBuf::from("/home/someone/.cache/bevel"));
    }

    struct EnvGuard;
    impl EnvGuard {
        fn set(pairs: &[(&str, Option<&str>)]) -> Self {
            for (k, v) in pairs {
                match v {
                    Some(v) => std::env::set_var(k, v),
                    None => std::env::remove_var(k),
                }
            }
            EnvGuard
        }
    }
}
