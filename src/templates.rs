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

/// One value, encoded so it survives being pasted into a YAML frontmatter slot.
///
/// The templates are prose with holes in them, and `render` substitutes text
/// without knowing what a hole means. That is fine for every hole but one:
/// `title:` is YAML, and a title carrying `: ` — `bevel close counts: why` —
/// produced a spec.md that could not be parsed by the very next command. The
/// value has to arrive already encoded, because nothing downstream will do it.
///
/// serde_yaml decides when quoting is needed, so `plain title` stays plain and
/// `true`, `123`, `- dash` and `has: colon` are quoted. Hand-rolling that rule
/// is how the next surprise gets in.
fn yaml_scalar(value: &str) -> String {
    serde_yaml::to_string(&value.to_string())
        .unwrap_or_else(|_| format!("{value:?}"))
        .trim_end()
        .to_string()
}

fn read(source: &Source, name: &str) -> Result<String> {
    let path = source.method_dir().join("templates").join(name);
    std::fs::read_to_string(&path)
        .with_context(|| format!("cannot read the {name} template at {}", path.display()))
}

/// Substitute into a file that opens with a frontmatter fence, encoding the
/// title differently on each side of it.
///
/// `{{title}}` appears above the fence as a YAML value and below it as an H1,
/// and no single encoding serves both: a quoted scalar reads as a mistake in a
/// heading, and a bare heading with a colon in it ends the YAML early.
///
/// Splitting here rather than adding a second placeholder is the whole point.
/// A template is data — it lives in the method tree and DESIGN.md §2 promises
/// editing one needs no release — so a template that only works on a new
/// binary breaks that promise in the worst direction: an older bevel leaves
/// the unknown placeholder in place and writes a spec whose frontmatter parses
/// as a map. The knowledge belongs in the binary, where the version is.
///
/// A file with no fence is returned rendered with the prose title, since every
/// hole in it is prose.
fn render_fenced(template: &str, title: &str, vars: &[(&str, &str)]) -> String {
    let Ok((yaml, body)) = crate::spec::split_frontmatter(template) else {
        return render(template, &[vars, &[("title", title)]].concat());
    };
    let front = render(yaml, &[vars, &[("title", &yaml_scalar(title))]].concat());
    let body = render(body, &[vars, &[("title", title)]].concat());
    format!("---\n{front}---\n{body}")
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
    let vars = [("id", id), ("created", created), ("source", inbox_source)];
    for name in ["spec.md", "decisions.md", "open-questions.md"] {
        let body = render_fenced(&read(source, name)?, title, &vars);
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

    /// The bug this guards: `bevel shape 2` on an inbox item reading
    /// "bevel close counts phantom markers: pending_markers searches the repo"
    /// wrote a spec.md whose own frontmatter would not parse, and every later
    /// command on that spec failed with a YAML error pointing at a column.
    ///
    /// Scaffolding that produces an unusable artifact is worse than refusing,
    /// because the folder exists and the inbox item is already marked shaped.
    ///
    /// The template used here is the real shipped one, and the assertion that
    /// matters most is the one it makes implicitly: **it still says
    /// `title: {{title}}`**, the same text every released binary already knows
    /// how to render. The first attempt at this fix invented a second
    /// placeholder, which turned every older bevel into one that writes
    /// `title: {{title_yaml}}` — valid YAML, parsed as a map, and a worse
    /// error than the one being fixed. A template is data; it may not require
    /// a binary newer than itself.
    #[test]
    fn a_hostile_title_still_produces_a_parseable_spec() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("m");
        let tpl = root.join("method/templates");
        std::fs::create_dir_all(&tpl).unwrap();
        std::fs::copy(
            concat!(env!("CARGO_MANIFEST_DIR"), "/method/templates/spec.md"),
            tpl.join("spec.md"),
        )
        .unwrap();
        std::fs::write(tpl.join("decisions.md"), "# {{id}} {{title}}\n").unwrap();
        std::fs::write(tpl.join("open-questions.md"), "# {{id}}\n").unwrap();
        let source = Source {
            kind: crate::method::Kind::Local,
            root,
            origin: "test".into(),
        };

        // Every one of these is a title someone could reasonably write, and
        // every one is a different way to end a YAML document early.
        for (n, title) in [
            "bevel close counts phantom markers: pending_markers scans the repo",
            "true",
            "123",
            "- a leading dash",
            "#not a comment",
            "a \"quoted\" word",
            "trailing space ",
            "back\\slash",
        ]
        .iter()
        .enumerate()
        {
            let dir = tmp.path().join(format!("spec{n}"));
            std::fs::create_dir_all(&dir).unwrap();
            write_all(&source, &dir, "0007", title, "2026-08-03", "raw idea").unwrap();

            let text = std::fs::read_to_string(dir.join("spec.md")).unwrap();
            let (yaml, _) = crate::spec::split_frontmatter(&text)
                .unwrap_or_else(|e| panic!("`{title}` broke the fence: {e}"));
            let front: serde_yaml::Value = serde_yaml::from_str(yaml)
                .unwrap_or_else(|e| panic!("`{title}` broke the frontmatter: {e}"));

            // Parseable is not enough — it has to survive as the same string.
            // `true` and `123` parse fine and come back as the wrong type.
            assert_eq!(
                front["title"].as_str(),
                Some(*title),
                "`{title}` did not round-trip"
            );

            // And the H1 keeps the prose form, with no quoting bled into it.
            assert!(
                text.contains(&format!("# {title}")),
                "`{title}` was quoted in the heading"
            );
        }
    }

    /// The template must stay renderable by a bevel older than this one, since
    /// it is fetched from a tree that updates independently of the binary.
    /// Any placeholder the released binary does not bind is left verbatim, so
    /// a new name in the frontmatter is not a missing value — it is a YAML map
    /// where a string belongs, which parses and then fails somewhere else.
    #[test]
    fn the_spec_template_asks_for_no_placeholder_a_released_bevel_lacks() {
        let template = include_str!("../method/templates/spec.md");
        let (yaml, _) = crate::spec::split_frontmatter(template).unwrap();
        let known = ["{{id}}", "{{title}}", "{{created}}", "{{source}}"];
        for hole in yaml.match_indices("{{").map(|(i, _)| {
            let rest = &yaml[i..];
            &rest[..rest.find("}}").map(|j| j + 2).unwrap_or(rest.len())]
        }) {
            assert!(
                known.contains(&hole),
                "frontmatter asks for {hole}, which an older bevel leaves verbatim"
            );
        }
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
