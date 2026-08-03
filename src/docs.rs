//! Version-pinned documentation retrieval (DESIGN.md §10).
//!
//! Four rules, all of them consequences of decisions in the design:
//!
//! 1. Pin the version from the lockfile. Asking for "latest" reintroduces the
//!    staleness problem backwards — documentation for a version you do not run.
//! 2. Go through this binary rather than only through MCP, so the pipeline
//!    behaves identically in agents that have no MCP at all.
//! 3. Cache by (library, version, topic). Determinism within a task, and cost.
//! 4. **Offline is a supported mode, not an error path.** Nothing here is ever
//!    on the critical path of a gate or a verification.
//!
//! HTTP goes through `curl` rather than a Rust client. That keeps a binary
//! which is otherwise pure `std` free of a TLS stack — which also removes the
//! usual musl friction — and this is an occasional, entirely optional call. A
//! missing `curl` is simply another way to be offline, which is already a
//! handled state.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::config::{self, Context7};
use crate::lockfile;

/// Below this, treat the response as "no content" rather than documentation.
const MIN_USEFUL_BYTES: usize = 60;

#[derive(Debug, Clone)]
pub struct Request {
    /// A Context7 library id, e.g. `/websites/rs_tokio`.
    pub library: String,
    pub version: Option<String>,
    pub topic: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Doc {
    /// The id actually served, which may carry a version suffix.
    pub library_used: String,
    pub version_pinned: bool,
    pub version: Option<String>,
    pub topic: Option<String>,
    pub text: String,
    pub fetched_at: i64,
}

#[derive(Debug)]
pub enum Outcome {
    Fetched(Doc),
    Cached(Doc),
    /// Network failed but something usable was on disk. Stale beats absent.
    Stale(Doc, String),
    /// Nothing to serve. Carries the marker line for `notes.md`.
    Unavailable {
        marker: String,
        reason: String,
    },
}

impl Outcome {
    pub fn doc(&self) -> Option<&Doc> {
        match self {
            Outcome::Fetched(d) | Outcome::Cached(d) | Outcome::Stale(d, _) => Some(d),
            Outcome::Unavailable { .. } => None,
        }
    }
}

/// The line that goes into `notes.md` when documentation could not be pinned.
///
/// This is the deliverable when the network is closed. Code written without
/// pinned documentation is the code most likely to use an API that moved, and
/// without the marker it is indistinguishable from code written with full
/// context.
pub fn marker(library: &str, version: Option<&str>, reason: &str) -> String {
    let v = version.unwrap_or("unknown version");
    format!(
        "[offline] No version-pinned docs for {library}@{v} ({reason}) — \
         implemented from model knowledge. Review these APIs in the diff before merging."
    )
}

pub fn fetch(cfg: &Context7, cache_dir: &Path, req: &Request, offline: bool) -> Result<Outcome> {
    let path = cache_path(cache_dir, cfg, req);
    let cached = read_cache(&path);

    if let Some(doc) = &cached {
        if !offline && is_fresh(doc, cfg.ttl_days) {
            return Ok(Outcome::Cached(doc.clone()));
        }
    }

    if offline {
        return Ok(match cached {
            Some(doc) => Outcome::Stale(doc, "offline requested".into()),
            None => Outcome::Unavailable {
                marker: marker(
                    &req.library,
                    req.version.as_deref(),
                    "offline, nothing cached",
                ),
                reason: "offline requested and nothing cached".into(),
            },
        });
    }

    // Try the version-specific id first, then the bare one. A 404 on the
    // former is expected and cheap.
    let mut attempts: Vec<(String, bool)> = Vec::new();
    if let Some(suffix) = req.version.as_deref().and_then(lockfile::version_suffix) {
        attempts.push((format!("{}{}", req.library, suffix), true));
    }
    attempts.push((req.library.clone(), false));

    let (key, _) = config::api_key(cfg);
    let mut last_error = String::from("no attempt made");

    for (library_id, pinned) in attempts {
        match http_get(cfg, &library_id, req.topic.as_deref(), key.as_deref()) {
            Ok(text) => {
                let doc = Doc {
                    library_used: library_id,
                    version_pinned: pinned,
                    version: req.version.clone(),
                    topic: req.topic.clone(),
                    text,
                    fetched_at: chrono::Utc::now().timestamp(),
                };
                write_cache(&path, &doc)?;
                return Ok(Outcome::Fetched(doc));
            }
            Err(e) => last_error = e,
        }
    }

    Ok(match cached {
        Some(doc) => Outcome::Stale(doc, last_error),
        None => Outcome::Unavailable {
            marker: marker(&req.library, req.version.as_deref(), &last_error),
            reason: last_error,
        },
    })
}

fn is_fresh(doc: &Doc, ttl_days: u64) -> bool {
    let age = chrono::Utc::now().timestamp() - doc.fetched_at;
    age >= 0 && (age as u64) < ttl_days.saturating_mul(86_400)
}

fn cache_path(cache_dir: &Path, cfg: &Context7, req: &Request) -> PathBuf {
    let mut h = Sha256::new();
    h.update(req.library.as_bytes());
    h.update(b"|");
    h.update(req.version.as_deref().unwrap_or("-").as_bytes());
    h.update(b"|");
    h.update(req.topic.as_deref().unwrap_or("-").as_bytes());
    h.update(b"|");
    h.update(cfg.tokens.to_string().as_bytes());
    cache_dir.join(format!("{:x}.json", h.finalize()))
}

fn read_cache(path: &Path) -> Option<Doc> {
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

fn write_cache(path: &Path, doc: &Doc) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, serde_json::to_string_pretty(doc)?)?;
    Ok(())
}

/// Returns the body, or a short human-readable reason it could not be had.
fn http_get(
    cfg: &Context7,
    library_id: &str,
    topic: Option<&str>,
    key: Option<&str>,
) -> std::result::Result<String, String> {
    let mut url = format!(
        "{}{}?type=txt&tokens={}",
        cfg.base_url.trim_end_matches('/'),
        library_id,
        cfg.tokens
    );
    if let Some(t) = topic {
        url.push_str(&format!("&topic={}", percent_encode(t)));
    }

    let body_path = std::env::temp_dir().join(format!("bevel-docs-{}.txt", std::process::id()));
    let mut cmd = Command::new("curl");
    cmd.arg("-sS")
        .arg("--max-time")
        .arg(cfg.timeout_secs.to_string())
        .arg("--retry")
        .arg("1")
        .arg("-o")
        .arg(&body_path)
        .arg("-w")
        .arg("%{http_code}");
    if let Some(k) = key {
        cmd.arg("-H").arg(format!("Authorization: Bearer {k}"));
    }
    cmd.arg(&url);

    let out = match cmd.output() {
        Ok(o) => o,
        Err(e) => return Err(format!("curl unavailable: {e}")),
    };
    let status = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let body = std::fs::read_to_string(&body_path).unwrap_or_default();
    let _ = std::fs::remove_file(&body_path);

    if !out.status.success() {
        return Err(format!(
            "network error: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    if status != "200" {
        return Err(format!("http {status}"));
    }
    if body.trim().len() < MIN_USEFUL_BYTES {
        return Err("no content for this library".into());
    }
    Ok(body)
}

/// Percent-encode a query value. Small enough not to justify a dependency.
fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> Context7 {
        Context7 {
            // Unroutable, so no test in this module ever reaches the network.
            base_url: "http://127.0.0.1:1/api".into(),
            timeout_secs: 1,
            ..Default::default()
        }
    }

    fn req() -> Request {
        Request {
            library: "/websites/rs_tokio".into(),
            version: Some("1.49.0".into()),
            topic: Some("graceful shutdown".into()),
        }
    }

    fn seed(cache: &Path, cfg: &Context7, req: &Request, age_secs: i64) -> Doc {
        let doc = Doc {
            library_used: "/websites/rs_tokio_1_49_0".into(),
            version_pinned: true,
            version: req.version.clone(),
            topic: req.topic.clone(),
            text: "x".repeat(200),
            fetched_at: chrono::Utc::now().timestamp() - age_secs,
        };
        write_cache(&cache_path(cache, cfg, req), &doc).unwrap();
        doc
    }

    #[test]
    fn a_fresh_cache_entry_is_served_without_touching_the_network() {
        let tmp = tempfile::tempdir().unwrap();
        seed(tmp.path(), &cfg(), &req(), 60);
        let out = fetch(&cfg(), tmp.path(), &req(), false).unwrap();
        assert!(matches!(out, Outcome::Cached(_)));
    }

    #[test]
    fn an_expired_entry_is_still_served_when_the_network_fails() {
        let tmp = tempfile::tempdir().unwrap();
        // Older than the 14-day default.
        seed(tmp.path(), &cfg(), &req(), 40 * 86_400);
        let out = fetch(&cfg(), tmp.path(), &req(), false).unwrap();
        match out {
            Outcome::Stale(doc, reason) => {
                assert_eq!(doc.library_used, "/websites/rs_tokio_1_49_0");
                assert!(!reason.is_empty());
            }
            other => panic!("expected stale, got {other:?}"),
        }
    }

    #[test]
    fn nothing_cached_and_no_network_yields_a_marker_not_an_error() {
        let tmp = tempfile::tempdir().unwrap();
        let out = fetch(&cfg(), tmp.path(), &req(), false).unwrap();
        match out {
            Outcome::Unavailable { marker, .. } => {
                assert!(marker.starts_with("[offline]"));
                assert!(marker.contains("/websites/rs_tokio@1.49.0"));
                assert!(marker.contains("before merging"));
            }
            other => panic!("expected unavailable, got {other:?}"),
        }
    }

    #[test]
    fn offline_mode_never_attempts_the_network_but_still_serves_a_stale_entry() {
        let tmp = tempfile::tempdir().unwrap();
        seed(tmp.path(), &cfg(), &req(), 99 * 86_400);
        match fetch(&cfg(), tmp.path(), &req(), true).unwrap() {
            Outcome::Stale(_, reason) => assert_eq!(reason, "offline requested"),
            other => panic!("expected stale, got {other:?}"),
        }
    }

    #[test]
    fn the_cache_key_separates_topics_and_versions() {
        let tmp = tempfile::tempdir().unwrap();
        let c = cfg();
        let a = cache_path(tmp.path(), &c, &req());

        let mut other_topic = req();
        other_topic.topic = Some("timers".into());
        assert_ne!(a, cache_path(tmp.path(), &c, &other_topic));

        let mut other_version = req();
        other_version.version = Some("1.40.0".into());
        assert_ne!(a, cache_path(tmp.path(), &c, &other_version));
    }

    #[test]
    fn topics_with_spaces_and_symbols_are_encoded() {
        assert_eq!(percent_encode("graceful shutdown"), "graceful%20shutdown");
        assert_eq!(percent_encode("a/b?c&d"), "a%2Fb%3Fc%26d");
        assert_eq!(percent_encode("plain-1.0_x~y"), "plain-1.0_x~y");
    }
}
