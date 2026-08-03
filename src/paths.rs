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
    /// The user's home. `~/.claude` hangs off this one: the method is the same
    /// text in every project, so it installs once per machine.
    pub home: PathBuf,
}

impl Layers {
    pub fn resolve() -> Result<Self> {
        let home = env_path("HOME").context("HOME is not set")?;
        // BEVEL_HOME collapses config and cache into one directory for anyone
        // who prefers a single location over the XDG split. It deliberately
        // leaves `~/.claude` alone: that one is Claude Code's, not bevel's, and
        // moving it would hide the skills from the only thing that reads them.
        if let Some(root) = env_path("BEVEL_HOME") {
            return Ok(Self {
                config: root.join("config"),
                cache: root.join("cache"),
                home,
            });
        }
        let config = env_path("XDG_CONFIG_HOME").unwrap_or_else(|| home.join(".config"));
        let cache = env_path("XDG_CACHE_HOME").unwrap_or_else(|| home.join(".cache"));
        Ok(Self {
            config: config.join("bevel"),
            cache: cache.join("bevel"),
            home,
        })
    }

    /// `~/.claude`, where Claude Code reads personal skills and subagents from.
    ///
    /// Not `~/.agents`: that is a proposal, and Claude Code does not read it.
    /// Installing to a path nothing loads is worse than not installing at all,
    /// because it looks like it worked.
    pub fn claude_home(&self) -> PathBuf {
        self.home.join(".claude")
    }

    /// Where skills install: once per machine, because the method does not vary
    /// by project. One copy per project would be N copies of one file to drift.
    pub fn claude_skills(&self) -> PathBuf {
        self.claude_home().join("skills")
    }

    pub fn claude_agents(&self) -> PathBuf {
        self.claude_home().join("agents")
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
        // Collapsing the bevel layers must not drag the agent's own along.
        assert_eq!(l.claude_home(), PathBuf::from("/home/someone/.claude"));

        let _guard2 = EnvGuard::set(&[("BEVEL_HOME", None)]);
        let l = Layers::resolve().unwrap();
        assert_eq!(l.config, PathBuf::from("/home/someone/.config/bevel"));
        assert_eq!(l.cache, PathBuf::from("/home/someone/.cache/bevel"));
        assert_eq!(
            l.claude_skills(),
            PathBuf::from("/home/someone/.claude/skills")
        );
        assert_eq!(
            l.claude_agents(),
            PathBuf::from("/home/someone/.claude/agents")
        );
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
