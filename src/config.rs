//! User configuration, `~/.config/bevel/config.toml` (DESIGN.md §2).
//!
//! Small and syncable on purpose: this is the one layer expected to travel
//! between machines, so it holds preferences and never secrets.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::paths::Layers;

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub context7: Context7,
    #[serde(default)]
    pub method: Method,
}

/// Where the method tree comes from (DESIGN.md §2).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Method {
    /// GitHub `owner/repo` holding `method/` and `packs/`.
    #[serde(default = "default_repo")]
    pub repo: String,
    /// Branch, tag or commit SHA. Pin it to a tag for reproducibility across
    /// machines; leave it on a branch while iterating on the method.
    #[serde(default = "default_ref", rename = "ref")]
    pub git_ref: String,
    /// A local checkout, which wins over fetching. Set this when you are
    /// editing the method, or on a machine that cannot reach GitHub.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

impl Default for Method {
    fn default() -> Self {
        Self {
            repo: default_repo(),
            git_ref: default_ref(),
            path: None,
        }
    }
}

fn default_repo() -> String {
    "orovp/bevel".into()
}

fn default_ref() -> String {
    "main".into()
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Context7 {
    /// Overridable because a third-party API can move and a rebuild should not
    /// be the fix.
    #[serde(default = "default_base_url")]
    pub base_url: String,
    /// Roughly how much documentation to ask for per topic.
    #[serde(default = "default_tokens")]
    pub tokens: u32,
    /// Short by design: this call is never on the critical path of a gate or a
    /// verification, so waiting is pure cost.
    #[serde(default = "default_timeout")]
    pub timeout_secs: u32,
    /// Days a cached answer is considered fresh. Expiry is ignored entirely
    /// when the network is unreachable — stale beats absent.
    #[serde(default = "default_ttl_days")]
    pub ttl_days: u64,
    /// A command that prints the API key on stdout, e.g. `pass show context7`.
    ///
    /// This is what makes `config.toml` safe to commit to a private dotfiles
    /// repository: the secret is never written to disk here. Without it you
    /// end up with a gitignore rule you eventually forget.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_command: Option<String>,
}

impl Default for Context7 {
    fn default() -> Self {
        Self {
            base_url: default_base_url(),
            tokens: default_tokens(),
            timeout_secs: default_timeout(),
            ttl_days: default_ttl_days(),
            key_command: None,
        }
    }
}

fn default_base_url() -> String {
    "https://context7.com/api/v1".into()
}

fn default_tokens() -> u32 {
    4000
}

fn default_timeout() -> u32 {
    5
}

fn default_ttl_days() -> u64 {
    14
}

impl Config {
    pub fn load(layers: &Layers) -> Result<Self> {
        let path = layers.config_file();
        match std::fs::read_to_string(&path) {
            Ok(text) => {
                toml::from_str(&text).with_context(|| format!("cannot parse {}", path.display()))
            }
            Err(_) => Ok(Self::default()),
        }
    }
}

/// Where the API key comes from, in order. Context7 answers unauthenticated
/// too, so having no key at all is a supported outcome rather than an error.
#[derive(Debug, PartialEq, Eq)]
pub enum KeySource {
    Env,
    Command,
    None,
}

pub fn api_key(cfg: &Context7) -> (Option<String>, KeySource) {
    if let Ok(k) = std::env::var("CONTEXT7_API_KEY") {
        if !k.trim().is_empty() {
            return (Some(k.trim().to_string()), KeySource::Env);
        }
    }
    if let Some(cmd) = &cfg.key_command {
        if let Some(k) = run_key_command(cmd) {
            return (Some(k), KeySource::Command);
        }
    }
    (None, KeySource::None)
}

fn run_key_command(cmd: &str) -> Option<String> {
    let out = crate::shell::command(cmd).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let key = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!key.is_empty()).then_some(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_apply_when_there_is_no_config_file() {
        let tmp = tempfile::tempdir().unwrap();
        let layers = Layers {
            config: tmp.path().join("nope"),
            cache: tmp.path().join("cache"),
            home: tmp.path().join("home"),
        };
        let cfg = Config::load(&layers).unwrap();
        assert_eq!(cfg.context7.base_url, "https://context7.com/api/v1");
        assert_eq!(cfg.context7.timeout_secs, 5);
        assert!(cfg.context7.key_command.is_none());
    }

    #[test]
    fn a_partial_config_keeps_the_defaults_for_everything_else() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path()).unwrap();
        std::fs::write(tmp.path().join("config.toml"), "[context7]\ntokens = 999\n").unwrap();
        let layers = Layers {
            config: tmp.path().to_path_buf(),
            cache: tmp.path().join("cache"),
            home: tmp.path().join("home"),
        };
        let cfg = Config::load(&layers).unwrap();
        assert_eq!(cfg.context7.tokens, 999);
        assert_eq!(cfg.context7.timeout_secs, 5);
    }

    /// `echo` rather than `printf`, because this line is handed to whatever
    /// shell the platform has: cmd.exe knows neither `printf` nor single
    /// quotes. What it must still prove is that the trailing newline never
    /// reaches an HTTP header — a `\r\n` one on Windows.
    #[test]
    fn a_key_command_is_run_and_trimmed() {
        let cfg = Context7 {
            key_command: Some("echo   secret-value  ".into()),
            ..Default::default()
        };
        std::env::remove_var("CONTEXT7_API_KEY");
        let (key, source) = api_key(&cfg);
        assert_eq!(key.as_deref(), Some("secret-value"));
        assert_eq!(source, KeySource::Command);
    }

    #[test]
    fn a_failing_key_command_is_not_fatal() {
        let cfg = Context7 {
            key_command: Some("exit 1".into()),
            ..Default::default()
        };
        std::env::remove_var("CONTEXT7_API_KEY");
        let (key, source) = api_key(&cfg);
        assert!(key.is_none());
        assert_eq!(source, KeySource::None);
    }
}
