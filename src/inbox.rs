//! `INBOX.md` (DESIGN.md §3 and §16).
//!
//! One inbox at the repository root. Capture is the highest-leverage friction
//! in the whole system: an idea rarely knows which package it belongs to
//! before it has been shaped, so filing it correctly is not asked for here.
//!
//! Items live in a two-column table, `ID` and `Raw idea`. The number is
//! written into the file so a human can read off what `bevel shape <n>`
//! wants, but it stays a *derivation*: `parse_str` numbers rows by position
//! and `add` rewrites the column to agree. A hand-edited number is therefore
//! cosmetic and can never aim `shape` at the wrong row. Bullet lists are
//! still read, and an inbox already written in them keeps them — inboxes
//! predating the table exist in other repositories.

use anyhow::{bail, Context, Result};
use std::path::Path;

const HEADER: &str = "| ID | Raw idea |\n| --- | --- |\n";

#[derive(Debug, Clone)]
pub struct Item {
    /// 1-based position among items, stable across shaping.
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
    let lines: Vec<&str> = text.lines().collect();
    let delimiter: Vec<bool> = lines.iter().map(|l| is_delimiter(l)).collect();

    let mut out = Vec::new();
    for (line_no, line) in lines.iter().enumerate() {
        let raw = if is_row(line) {
            // Neither the delimiter nor the header above it carries an idea.
            if delimiter[line_no] || delimiter.get(line_no + 1).copied().unwrap_or(false) {
                continue;
            }
            match cells(line).get(1) {
                Some(cell) => unescape(cell),
                None => continue,
            }
        } else {
            match bullet_body(line) {
                Some(body) => body.to_string(),
                None => continue,
            }
        };

        let body = raw.trim();
        if body.is_empty() {
            continue;
        }
        let (linked, text_len) = match extract_link(body) {
            Some((target, start)) => (Some(target), start),
            None => (None, body.len()),
        };
        out.push(Item {
            index: out.len() + 1,
            line_no,
            text: body[..text_len].trim().to_string(),
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

fn is_row(line: &str) -> bool {
    line.trim_start().starts_with('|')
}

/// `| --- | --- |`, and the alignment variants of it.
///
/// A data row can never be mistaken for one: its first cell is a number.
fn is_delimiter(line: &str) -> bool {
    if !is_row(line) {
        return false;
    }
    let cells = cells(line);
    !cells.is_empty()
        && cells.iter().all(|cell| {
            let dashes = cell.trim_matches(':');
            !dashes.is_empty() && dashes.chars().all(|c| c == '-')
        })
}

/// Split a row into trimmed cells, treating `\|` as a literal pipe rather
/// than a cell boundary. The escape is left in place for the caller to undo,
/// so a cell can be written back out without being re-escaped.
fn cells(line: &str) -> Vec<String> {
    let trimmed = line.trim();
    let inner = trimmed.strip_prefix('|').unwrap_or(trimmed);
    let inner = inner.strip_suffix('|').unwrap_or(inner);

    let mut out = Vec::new();
    let mut cur = String::new();
    let mut chars = inner.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' if chars.peek() == Some(&'|') => {
                chars.next();
                cur.push_str("\\|");
            }
            '|' => {
                out.push(cur.trim().to_string());
                cur.clear();
            }
            _ => cur.push(c),
        }
    }
    out.push(cur.trim().to_string());
    out
}

fn escape(text: &str) -> String {
    text.replace('|', "\\|")
}

fn unescape(cell: &str) -> String {
    cell.replace("\\|", "|")
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

    // An inbox written before the table keeps its bullets, rather than
    // growing a second format halfway down.
    if uses_bullets(&content) {
        content.push_str(&format!("- {text}\n"));
        return write(path, &content);
    }

    if !content.lines().any(is_delimiter) {
        if !content.is_empty() && !content.ends_with("\n\n") {
            content.push('\n');
        }
        content.push_str(HEADER);
    }
    let n = parse_str(&content).len() + 1;
    content.push_str(&format!("| {n} | {} |\n", escape(text)));
    write(path, &renumber(&content))
}

/// Items, but no table to put them in.
fn uses_bullets(content: &str) -> bool {
    !content.lines().any(is_delimiter)
        && content
            .lines()
            .any(|l| matches!(bullet_body(l), Some(b) if !b.is_empty()))
}

/// Rewrite the ID column so it agrees with position.
///
/// The number in the file is what a human reads off to call `bevel shape
/// <n>`. Position is what `shape` actually resolves, so a number left stale
/// by a hand-deleted row would name a different idea than the one it sits on.
fn renumber(content: &str) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let delimiter: Vec<bool> = lines.iter().map(|l| is_delimiter(l)).collect();

    let mut n = 0;
    let mut out: Vec<String> = Vec::with_capacity(lines.len());
    for (i, line) in lines.iter().enumerate() {
        let is_data =
            is_row(line) && !delimiter[i] && !delimiter.get(i + 1).copied().unwrap_or(false);
        if !is_data {
            out.push((*line).to_string());
            continue;
        }
        n += 1;
        let cells = cells(line);
        let body = cells.get(1).cloned().unwrap_or_default();
        out.push(format!("| {n} | {body} |"));
    }
    let mut joined = out.join("\n");
    joined.push('\n');
    joined
}

/// Mark an item as shaped by appending a link to its idea. Rewrites only the
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
    let annotation = format!(" → [{spec_id}]({spec_rel})");
    if is_row(line) {
        // Inside the idea cell. Appended to the line it would land beyond the
        // closing pipe, where it stops being part of the table at all.
        let cells = cells(line);
        let id = cells.first().cloned().unwrap_or_default();
        let body = cells.get(1).cloned().unwrap_or_default();
        *line = format!("| {id} | {body}{annotation} |");
    } else {
        line.push_str(&annotation);
    }

    let mut out = lines.join("\n");
    out.push('\n');
    write(path, &out)
}

fn write(path: &Path, content: &str) -> Result<()> {
    std::fs::write(path, content).with_context(|| format!("cannot write {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
# Inbox

Some prose that is not an item.

| ID | Raw idea |
| --- | --- |
| 1 | first idea |
| 2 | second idea → [0003](specs/0003-second-idea/spec.md) |
| 3 | third idea |
";

    const BULLETS: &str = "\
# Inbox

- first idea
- second idea → [0003](specs/0003-second-idea/spec.md)
* third idea
";

    #[test]
    fn parses_rows_and_ignores_prose_and_the_header() {
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
    fn add_appends_a_row_and_link_annotates_one_row() {
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
        // And the annotated row is still a row.
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text
            .lines()
            .any(|l| l.starts_with("| 1 |") && l.ends_with('|')));
    }

    #[test]
    fn add_refuses_empty_text() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("INBOX.md");
        assert!(add(&path, "   ").is_err());
    }

    #[test]
    fn a_pipe_in_an_idea_cannot_break_the_table() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("INBOX.md");
        std::fs::write(&path, SAMPLE).unwrap();

        add(&path, "pipe a | b through it").unwrap();

        let items = parse(&path).unwrap();
        assert_eq!(items.len(), 4);
        // The idea survives the round trip whole, pipe and all.
        assert_eq!(items[3].text, "pipe a | b through it");
        // And the row it lives in still has exactly two cells.
        let text = std::fs::read_to_string(&path).unwrap();
        let row = text.lines().find(|l| l.starts_with("| 4 |")).unwrap();
        assert_eq!(cells(row).len(), 2);
    }

    #[test]
    fn an_inbox_written_in_bullets_is_still_read_and_still_grows_bullets() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("INBOX.md");
        std::fs::write(&path, BULLETS).unwrap();

        let items = parse(&path).unwrap();
        assert_eq!(items.len(), 3);
        assert_eq!(items[1].text, "second idea");

        add(&path, "fourth idea").unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.ends_with("- fourth idea\n"));
        // No half-table grew underneath the bullets.
        assert!(!text.contains('|'));
        assert_eq!(parse(&path).unwrap().len(), 4);
    }

    #[test]
    fn the_id_column_is_renumbered_to_match_position() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("INBOX.md");
        // A row was deleted by hand and the numbering left behind.
        std::fs::write(
            &path,
            "| ID | Raw idea |\n| --- | --- |\n| 1 | first |\n| 7 | second |\n",
        )
        .unwrap();

        add(&path, "third").unwrap();

        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("| 2 | second |"), "{text}");
        assert!(text.contains("| 3 | third |"), "{text}");
        assert!(!text.contains("| 7 |"), "{text}");

        // What shape resolves and what the human reads now agree.
        let items = parse(&path).unwrap();
        assert_eq!(items[1].index, 2);
        assert_eq!(items[1].text, "second");
    }
}
