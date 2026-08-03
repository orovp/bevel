//! Artifact skeletons, read from the method tree (DESIGN.md §2).
//!
//! These are files rather than string literals so that editing the shape of a
//! spec needs no release. The headings follow Shape Up because the vocabulary
//! already fits: appetite is fixed before designing, rabbit holes are patched up
//! front, no-gos are written down rather than assumed.

use anyhow::{Context, Result};
use std::path::Path;

use crate::method::Source;

/// Substitute `{{key}}` placeholders. Deliberately not a template engine: the
/// templates are prose with four holes in them, and a dependency would be
/// scaffolding for a need that does not exist.
pub fn render(body: &str, vars: &[(&str, &str)]) -> String {
    let mut out = body.to_string();
    for (k, v) in vars {
        out = out.replace(&format!("{{{{{k}}}}}"), v);
    }
    out
}

fn read(source: &Source, name: &str) -> Result<String> {
    let path = source.method_dir().join("templates").join(name);
    std::fs::read_to_string(&path)
        .with_context(|| format!("cannot read the {name} template at {}", path.display()))
}

/// Write the three artifacts a new spec starts with.
pub fn write_all(
    source: &Source,
    dir: &Path,
    id: &str,
    title: &str,
    created: &str,
    inbox_source: &str,
) -> Result<()> {
    let vars = [
        ("id", id),
        ("title", title),
        ("created", created),
        ("source", inbox_source),
    ];
    for name in ["spec.md", "decisions.md", "open-questions.md"] {
        let body = render(&read(source, name)?, &vars);
        std::fs::write(dir.join(name), body)
            .with_context(|| format!("cannot write {}", dir.join(name).display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholders_are_substituted_everywhere_they_appear() {
        let body = "id {{id}}, again {{id}}, title {{title}}, missing {{nope}}";
        let out = render(body, &[("id", "0007"), ("title", "Sync")]);
        assert_eq!(out, "id 0007, again 0007, title Sync, missing {{nope}}");
    }

    #[test]
    fn the_three_artifacts_are_written_and_rendered() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("m");
        let tpl = root.join("method/templates");
        std::fs::create_dir_all(&tpl).unwrap();
        std::fs::write(
            tpl.join("spec.md"),
            "---\nid: '{{id}}'\ntitle: {{title}}\ncreated: '{{created}}'\n---\n{{source}}\n",
        )
        .unwrap();
        std::fs::write(tpl.join("decisions.md"), "# {{id}} {{title}}\n").unwrap();
        std::fs::write(tpl.join("open-questions.md"), "# {{id}}\n").unwrap();

        let source = Source {
            kind: crate::method::Kind::Local,
            root,
            origin: "test".into(),
        };
        let dir = tmp.path().join("spec");
        std::fs::create_dir_all(&dir).unwrap();
        write_all(&source, &dir, "0007", "Sync it", "2026-08-03", "raw idea").unwrap();

        let spec = std::fs::read_to_string(dir.join("spec.md")).unwrap();
        assert!(spec.contains("id: '0007'"));
        assert!(spec.contains("title: Sync it"));
        assert!(spec.contains("raw idea"));
        assert!(dir.join("decisions.md").is_file());
        assert!(dir.join("open-questions.md").is_file());
    }

    #[test]
    fn a_missing_template_names_the_path_it_looked_for() {
        let tmp = tempfile::tempdir().unwrap();
        let source = Source {
            kind: crate::method::Kind::Local,
            root: tmp.path().to_path_buf(),
            origin: "test".into(),
        };
        let err = write_all(&source, tmp.path(), "0001", "x", "2026-08-03", "y")
            .unwrap_err()
            .to_string();
        assert!(err.contains("spec.md template"), "{err}");
        assert!(err.contains("templates/spec.md"), "{err}");
    }
}
