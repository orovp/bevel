//! HTTP, in this process.
//!
//! This used to be `curl`, and the reasons it is not any more are worth stating
//! because shelling out was the cheaper thing to write:
//!
//! - `curl` is not on Windows before 10 1803, and `curl` in PowerShell is an
//!   alias for a cmdlet that takes none of the same flags. A tool whose first
//!   run downloads its own method cannot start by failing there.
//! - Both facts a caller needs — the status and the body — do not fit in one
//!   subprocess, so the body had to go through a temp file named after the pid.
//! - A subprocess inherits whatever the machine's `curl` was configured to do,
//!   including proxies and `~/.curlrc`, which is a class of bug this side of
//!   the process cannot see or report.
//!
//! Certificates come from `webpki-roots`, compiled in, rather than from the
//! platform trust store. That is the same reasoning one layer down: the two
//! hosts bevel talks to are public and well-known, and a binary that verifies
//! them identically on every machine has one behaviour to debug instead of one
//! per machine.

use std::time::Duration;

/// A response that arrived. A status this side considers a failure is still a
/// response — `docs` reports `http 404` to the user and carries on, so the
/// status has to survive as a number rather than becoming an error.
#[derive(Debug)]
pub struct Response {
    pub status: u16,
    pub body: Vec<u8>,
}

/// Refuse a body larger than this. Nothing bevel fetches is close: the method
/// tarball is measured in hundreds of kilobytes and a documentation answer in
/// tens. Without a ceiling, `read_to_vec` is unbounded and a bad answer from a
/// proxy is an out-of-memory kill.
const MAX_BODY_BYTES: u64 = 64 * 1024 * 1024;

/// GET `url`, following redirects, with a total time budget.
///
/// The error is a short sentence for a human, not a type to match on: every
/// caller either prints it or wraps it, and none of them can retry differently
/// depending on which layer failed.
pub fn get(url: &str, timeout: Duration, bearer: Option<&str>) -> Result<Response, String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        .user_agent(concat!("bevel/", env!("CARGO_PKG_VERSION")))
        // A status is data here, not an error — see `Response`.
        .http_status_as_error(false)
        .build()
        .into();

    let mut request = agent.get(url);
    if let Some(key) = bearer {
        request = request.header("Authorization", format!("Bearer {key}"));
    }

    let mut response = request.call().map_err(describe)?;
    let status = response.status().as_u16();
    let body = response
        .body_mut()
        .with_config()
        .limit(MAX_BODY_BYTES)
        .read_to_vec()
        .map_err(describe)?;

    Ok(Response { status, body })
}

/// What went wrong, in the words the user needs: the host they cannot reach or
/// the budget they hit, rather than a type name from three crates down.
fn describe(e: ureq::Error) -> String {
    match e {
        ureq::Error::Timeout(_) => "timed out".into(),
        ureq::Error::HostNotFound => "host not found".into(),
        ureq::Error::Io(e) => format!("network error: {e}"),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Offline is the normal state of a laptop on a train, and every caller
    /// treats it as a degraded answer rather than a crash. What must never
    /// happen is a panic or a wait with no end.
    #[test]
    fn an_unreachable_host_fails_quickly_with_a_sentence() {
        let e = get("https://localhost:1/never", Duration::from_secs(2), None).unwrap_err();
        assert!(!e.is_empty());
        assert!(!e.contains("ureq"), "{e}");
    }
}
