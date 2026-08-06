//! The global layers (DESIGN.md §2).
//!
//! Three lifetimes, three locations. The split exists so that syncing between
//! machines needs no ignore rules: `~/.config` travels, `~/.cache` does not.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

pub struct Layers {
    /// User layer: config and hand-written packs. Small, syncable.
    pub config: PathBuf,
    /// Machine layer: regenerable, never synced, safe to delete.
    pub cache: PathBuf,
    /// The user's home. `~/.claude` hangs off this one: the method is the same
    /// text in every project, so it installs once per machine.
    pub home: PathBuf,
    /// Another tool's directory, resolved by that tool's rules rather than
    /// ours. Stored rather than computed so it stays deterministic in tests:
    /// environment variables are process-global, and reading them from a getter
    /// would make every test that touches this race the others.
    pub opencode: PathBuf,
}

impl Layers {
    pub fn resolve() -> Result<Self> {
        let home = home_dir()
            .with_context(|| format!("cannot find your home directory: {HOME_VAR} is not set"))?;
        // BEVEL_HOME collapses config and cache into one directory for anyone
        // who prefers a single location over the XDG split. It deliberately
        // leaves `~/.claude` alone: that one is Claude Code's, not bevel's, and
        // moving it would hide the skills from the only thing that reads them.
        let opencode = opencode_dir(&home);
        if let Some(root) = env_path("BEVEL_HOME") {
            return Ok(Self {
                config: root.join("config"),
                cache: root.join("cache"),
                home,
                opencode,
            });
        }
        // `~/.config` and `~/.cache` on every platform, Windows included, rather
        // than %APPDATA% and %LOCALAPPDATA% there. The roaming/local split does
        // encode the same lifetimes, but the sync boundary this design is built
        // on is a dotfile manager pointed at the home directory, not a domain
        // roaming profile — and `~/.claude` is already in the home directory on
        // Windows, so all three layers stay in one place a person can find.
        let config = env_path("XDG_CONFIG_HOME").unwrap_or_else(|| home.join(".config"));
        let cache = env_path("XDG_CACHE_HOME").unwrap_or_else(|| home.join(".cache"));
        Ok(Self {
            config: config.join("bevel"),
            cache: cache.join("bevel"),
            home,
            opencode,
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

    /// Where opencode keeps its global configuration.
    ///
    /// Resolved once in `resolve` rather than derived from `home` here, because
    /// opencode honours `XDG_CONFIG_HOME` and `OPENCODE_CONFIG_DIR`. Hardcoding
    /// `~/.config/opencode` would, for anyone who sets either, write to a
    /// directory opencode never reads — and that failure looks exactly like
    /// success, which is the trap `claude_home` already names.
    pub fn opencode_home(&self) -> PathBuf {
        self.opencode.clone()
    }

    /// Plural. opencode accepts the singular for backwards compatibility and
    /// documents the plural, and a file written where nothing reads it is
    /// indistinguishable from one written correctly — see `claude_home`.
    pub fn opencode_agents(&self) -> PathBuf {
        self.opencode_home().join("agents")
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

/// What the platform calls the variable holding the user's home.
///
/// Windows is never asked for `HOME`. MSYS shells (Git Bash, MSYS2) set it to a
/// POSIX path such as `/home/you`, which a native binary would resolve against
/// the current drive and create as a literal directory of that name — so a
/// machine would end up with two homes depending on which shell started bevel.
/// `USERPROFILE` names the same place in every shell, and is where Claude Code
/// keeps `~/.claude`.
#[cfg(windows)]
pub const HOME_VAR: &str = "USERPROFILE";
#[cfg(not(windows))]
pub const HOME_VAR: &str = "HOME";

#[cfg(windows)]
fn home_dir() -> Option<PathBuf> {
    env_path(HOME_VAR).or_else(|| {
        // The pre-USERPROFILE pair, still what a locked-down domain profile
        // sets. Neither half means anything alone.
        let mut p = env_os("HOMEDRIVE")?;
        p.push(env_os("HOMEPATH")?);
        Some(PathBuf::from(p))
    })
}

#[cfg(not(windows))]
fn home_dir() -> Option<PathBuf> {
    env_path(HOME_VAR)
}

/// opencode's own resolution order, followed rather than approximated:
/// `OPENCODE_CONFIG_DIR`, then `XDG_CONFIG_HOME/opencode`, then
/// `~/.config/opencode`. `BEVEL_HOME` is deliberately not consulted — it moves
/// bevel's layers, and moving another tool's configuration with them would
/// hide opencode's files from opencode.
fn opencode_dir(home: &Path) -> PathBuf {
    if let Some(dir) = env_path("OPENCODE_CONFIG_DIR") {
        return dir;
    }
    env_path("XDG_CONFIG_HOME")
        .unwrap_or_else(|| home.join(".config"))
        .join("opencode")
}

fn env_path(key: &str) -> Option<PathBuf> {
    env_os(key).map(PathBuf::from)
}

fn env_os(key: &str) -> Option<std::ffi::OsString> {
    std::env::var_os(key).filter(|v| !v.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Env vars are process-global, so the layout cases share one test to
    /// avoid racing under the default parallel test runner.
    #[test]
    fn layout_honours_bevel_home_then_falls_back_to_xdg() {
        let _guard = EnvGuard::set(&[
            ("BEVEL_HOME", Some("/tmp/hh")),
            (HOME_VAR, Some("/home/someone")),
            ("XDG_CONFIG_HOME", None),
            ("XDG_CACHE_HOME", None),
            ("OPENCODE_CONFIG_DIR", None),
        ]);
        let l = Layers::resolve().unwrap();
        assert_eq!(l.config, PathBuf::from("/tmp/hh/config"));
        assert_eq!(l.cache, PathBuf::from("/tmp/hh/cache"));
        // Collapsing the bevel layers must not drag either agent's own along.
        assert_eq!(l.claude_home(), PathBuf::from("/home/someone/.claude"));
        assert_eq!(
            l.opencode_home(),
            PathBuf::from("/home/someone/.config/opencode")
        );

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
        // Plural: opencode documents the plural and keeps the singular only for
        // backwards compatibility.
        assert_eq!(
            l.opencode_agents(),
            PathBuf::from("/home/someone/.config/opencode/agents")
        );

        // opencode honours both of these, so bevel has to as well — writing to
        // `~/.config/opencode` when the user redirected it is the failure that
        // looks identical to success.
        let _x = EnvGuard::set(&[("XDG_CONFIG_HOME", Some("/custom/cfg"))]);
        let l = Layers::resolve().unwrap();
        assert_eq!(l.opencode_home(), PathBuf::from("/custom/cfg/opencode"));
        // bevel's own layer still tracks XDG too; the two are independent.
        assert_eq!(l.config, PathBuf::from("/custom/cfg/bevel"));

        let _o = EnvGuard::set(&[("OPENCODE_CONFIG_DIR", Some("/elsewhere/oc"))]);
        let l = Layers::resolve().unwrap();
        assert_eq!(l.opencode_home(), PathBuf::from("/elsewhere/oc"));

        // The reported bug was `HOME is not set` from a shell with no reason to
        // set it. Windows must resolve without it, and must ignore it when an
        // MSYS shell has left a POSIX path there — a third case in this test
        // rather than its own, for the same reason the other two share it.
        #[cfg(windows)]
        {
            let _guard3 = EnvGuard::set(&[("HOME", Some("/c/Users/someone"))]);
            let l = Layers::resolve().unwrap();
            assert_eq!(l.home, PathBuf::from("/home/someone"));
        }
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
