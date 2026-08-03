//! `INBOX.md` (DESIGN.md §3 and §16).
//!
//! One inbox at the repository root. Capture is the highest-leverage friction
//! in the whole system: an idea rarely knows which package it belongs to
//! before it has been shaped, so filing it correctly is not asked for here.

use anyhow::{bail, Context, Result};
use std::path::Path;

#[derive(Debug, Clone)]
pub struct Item {
    /// 1-based position among list items, stable across shaping.
    pub index: usize,
    pub line_no: usize,
    pub text: String,
    /// `Some(spec dir)` once shaped.
    pub linked: Option<String>,
}

pub fn parse(path: &Path) -> Result<Vec<Item>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let text =
        std::fs::read_to_string(path).with_context(|| format!("cannot read {}", path.display()))?;
    Ok(parse_str(&text))
}

pub fn parse_str(text: &str) -> Vec<Item> {
    let mut out = Vec::new();
    for (line_no, line) in text.lines().enumerate() {
        let Some(rest) = bullet_body(line) else {
            continue;
        };
        if rest.is_empty() {
            continue;
        }
        let (linked, text_len) = match extract_link(rest) {
            Some((target, start)) => (Some(target), start),
            None => (None, rest.len()),
        };
        out.push(Item {
            index: out.len() + 1,
            line_no,
            text: rest[..text_len].trim().to_string(),
            linked,
        });
    }
    out
}

/// The content of a `- ` or `* ` list item, if this line is one.
fn bullet_body(line: &str) -> Option<&str> {
    let t = line.trim_start();
    for marker in ["- ", "* "] {
        if let Some(rest) = t.strip_prefix(marker) {
            return Some(rest.trim());
        }
    }
    None
}

/// A shaped item carries a markdown link into the specs directory.
///
/// Returns the link target and the offset where the annotation starts, so the
/// item can be displayed without repeating the link.
fn extract_link(text: &str) -> Option<(String, usize)> {
    let anchor = text.find("](specs/")?;
    let open = text[..anchor].rfind('[')?;
    let rest = &text[anchor + 2..];
    let end = rest.find(')')?;

    let head = text[..open].trim_end();
    let head = head.strip_suffix('→').unwrap_or(head).trim_end();
    Some((rest[..end].to_string(), head.len()))
}

pub fn add(path: &Path, text: &str) -> Result<()> {
    let text = text.trim();
    if text.is_empty() {
        bail!("refusing to add an empty inbox item");
    }
    let mut content = if path.exists() {
        std::fs::read_to_string(path)?
    } else {
        String::new()
    };
    if !content.is_empty() && !content.ends_with('\n') {
        content.push('\n');
    }
    content.push_str(&format!("- {text}\n"));
    std::fs::write(path, content).with_context(|| format!("cannot write {}", path.display()))
}

/// Mark an item as shaped by appending a link to its spec. Rewrites only the
/// one line, so hand-written formatting elsewhere survives.
pub fn link(path: &Path, index: usize, spec_id: &str, spec_rel: &str) -> Result<()> {
    let content = std::fs::read_to_string(path)?;
    let items = parse_str(&content);
    let item = items
        .iter()
        .find(|i| i.index == index)
        .with_context(|| format!("no inbox item {index}"))?;

    let mut lines: Vec<String> = content.lines().map(|l| l.to_string()).collect();
    let line = &mut lines[item.line_no];
    line.push_str(&format!(" → [{spec_id}]({spec_rel})"));

    let mut out = lines.join("\n");
    out.push('\n');
    std::fs::write(path, out).with_context(|| format!("cannot write {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
# Inbox

Some prose that is not an item.

- first idea
- second idea → [0003](specs/0003-second-idea/spec.md)
* third idea
";

    #[test]
    fn parses_bullets_and_ignores_prose() {
        let items = parse_str(SAMPLE);
        assert_eq!(items.len(), 3);
        assert_eq!(items[0].text, "first idea");
        assert_eq!(items[0].index, 1);
        assert_eq!(items[2].text, "third idea");
    }

    #[test]
    fn detects_already_shaped_items_and_keeps_their_text_clean() {
        let items = parse_str(SAMPLE);
        assert!(items[0].linked.is_none());
        assert_eq!(
            items[1].linked.as_deref(),
            Some("specs/0003-second-idea/spec.md")
        );
        // The link is metadata, not part of the idea.
        assert_eq!(items[1].text, "second idea");
    }

    #[test]
    fn add_appends_and_link_annotates_one_line() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("INBOX.md");
        std::fs::write(&path, SAMPLE).unwrap();

        add(&path, "fourth idea").unwrap();
        assert_eq!(parse(&path).unwrap().len(), 4);

        link(&path, 1, "0009", "specs/0009-first-idea/spec.md").unwrap();
        let items = parse(&path).unwrap();
        assert_eq!(
            items[0].linked.as_deref(),
            Some("specs/0009-first-idea/spec.md")
        );
        // Untouched neighbours keep their original text.
        assert_eq!(items[2].text, "third idea");
    }

    #[test]
    fn add_refuses_empty_text() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("INBOX.md");
        assert!(add(&path, "   ").is_err());
    }
}
