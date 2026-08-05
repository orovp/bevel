//! Extracting the method tarball, in this process.
//!
//! This used to be `tar -xzf --strip-components=1`, which meant a second system
//! tool to be missing on Windows, and — worse — meant trusting whatever `tar`
//! the machine had with paths that arrived over the network. Doing it here
//! makes the safety rule explicit and testable rather than delegated.

use anyhow::{Context, Result};
use std::path::{Component, Path, PathBuf};

/// Extract a gzipped tar into `dest`, dropping the first path component.
///
/// The drop is not a nicety: a GitHub codeload tarball wraps everything in a
/// `<repo>-<ref>` directory whose exact name depends on how the ref was
/// written, so a caller that wanted to know the path would have to guess it.
pub fn extract_tar_gz(bytes: &[u8], dest: &Path) -> Result<()> {
    let gz = flate2::read::GzDecoder::new(bytes);
    let mut archive = tar::Archive::new(gz);

    for entry in archive.entries().context("archive is not readable")? {
        let mut entry = entry.context("archive entry is not readable")?;

        // Only the two kinds a method tree is made of. A symlink or a device
        // node in a downloaded archive has no business being written to disk,
        // and on Windows the symlink would need a privilege the user does not
        // normally have — so skipping is both the safe answer and the portable
        // one. A tree that genuinely needed them would fail its sentinel check.
        let kind = entry.header().entry_type();
        if !kind.is_file() && !kind.is_dir() {
            continue;
        }

        let path = entry.path().context("archive entry has no path")?;
        let Some(rel) = strip_first_component(&path) else {
            continue;
        };
        let target = dest.join(&rel);

        if kind.is_dir() {
            std::fs::create_dir_all(&target)
                .with_context(|| format!("cannot create {}", target.display()))?;
            continue;
        }
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("cannot create {}", parent.display()))?;
        }
        entry
            .unpack(&target)
            .with_context(|| format!("cannot write {}", target.display()))?;
    }
    Ok(())
}

/// The entry's path with its first component removed, or `None` if there is
/// nothing left to write.
///
/// This is also the only thing standing between a hostile archive and the rest
/// of the disk: an entry named `x/../../../.bashrc` or `/etc/passwd` would
/// otherwise be written exactly where it asked. Every component after the strip
/// must be a plain name.
fn strip_first_component(path: &Path) -> Option<PathBuf> {
    let mut components = path.components();
    // The component being dropped must itself be an ordinary name. An entry
    // that starts at the root or above it is refused rather than reinterpreted:
    // `/etc/passwd` would otherwise arrive here as `etc/passwd` and be written,
    // which is contained but not something a method tarball ever means.
    if !matches!(components.next(), Some(Component::Normal(_))) {
        return None;
    }

    let mut out = PathBuf::new();
    for component in components {
        match component {
            Component::Normal(part) => out.push(part),
            // `.` is noise; anything else is an escape attempt or an absolute
            // path, and there is no safe interpretation of either.
            Component::CurDir => {}
            _ => return None,
        }
    }
    (!out.as_os_str().is_empty()).then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// The name is written into the raw header rather than through
    /// `append_data`, which rejects a traversing path — the archives worth
    /// testing against are exactly the ones a well-behaved writer will not
    /// produce.
    fn tar_gz(entries: &[(&str, &str)]) -> Vec<u8> {
        let mut builder = tar::Builder::new(Vec::new());
        for (path, body) in entries {
            let mut header = tar::Header::new_gnu();
            header.set_size(body.len() as u64);
            header.set_mode(0o644);
            let name = &mut header.as_gnu_mut().unwrap().name;
            name[..path.len()].copy_from_slice(path.as_bytes());
            header.set_cksum();
            builder.append(&header, body.as_bytes()).unwrap();
        }
        let tar = builder.into_inner().unwrap();
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        gz.write_all(&tar).unwrap();
        gz.finish().unwrap()
    }

    #[test]
    fn the_wrapper_directory_is_dropped() {
        let tmp = tempfile::tempdir().unwrap();
        let bytes = tar_gz(&[
            ("bevel-main/method/skills/shape/SKILL.md", "# shape"),
            ("bevel-main/packs/rust/pack.toml", "id = \"rust\""),
        ]);
        extract_tar_gz(&bytes, tmp.path()).unwrap();

        let skill = tmp.path().join("method/skills/shape/SKILL.md");
        assert_eq!(std::fs::read_to_string(skill).unwrap(), "# shape");
        assert!(tmp.path().join("packs/rust/pack.toml").is_file());
        // The wrapper itself must not survive as a directory.
        assert!(!tmp.path().join("bevel-main").exists());
    }

    /// The archive arrives over the network, so this is the one rule that is
    /// not about convenience.
    #[test]
    fn an_entry_that_climbs_out_is_refused_not_written() {
        let tmp = tempfile::tempdir().unwrap();
        let outside = tmp.path().join("outside");
        std::fs::create_dir_all(&outside).unwrap();
        let dest = tmp.path().join("dest");

        let bytes = tar_gz(&[
            ("bevel-main/../outside/stolen.txt", "no"),
            ("bevel-main/method/kept.md", "yes"),
        ]);
        extract_tar_gz(&bytes, &dest).unwrap();

        assert!(!outside.join("stolen.txt").exists());
        assert!(!dest.join("outside/stolen.txt").exists());
        // The rest of the archive is still extracted: one bad entry is not a
        // reason to leave the user with nothing.
        assert!(dest.join("method/kept.md").is_file());
    }

    /// The rule itself, stated where a tar writer cannot quietly change it:
    /// `append_data` refuses to write most of these, so the test above proves
    /// the outcome and this one proves the reason.
    #[test]
    fn only_ordinary_names_survive_the_strip() {
        let strip = |p: &str| strip_first_component(Path::new(p));
        assert_eq!(
            strip("wrap/method/a.md"),
            Some(PathBuf::from("method/a.md"))
        );
        assert_eq!(
            strip("wrap/./method/a.md"),
            Some(PathBuf::from("method/a.md"))
        );
        // Nothing left after the wrapper is dropped.
        assert_eq!(strip("wrap"), None);
        // Every way out of the destination directory.
        assert_eq!(strip("wrap/../../etc/passwd"), None);
        assert_eq!(strip("wrap/a/../../../x"), None);
        assert_eq!(strip("/etc/passwd"), None);
        assert_eq!(strip("../wrap/a.md"), None);
    }

    #[test]
    fn a_body_that_is_not_a_gzipped_tar_is_an_error_not_a_panic() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(extract_tar_gz(b"<!doctype html><title>404</title>", tmp.path()).is_err());
    }
}
