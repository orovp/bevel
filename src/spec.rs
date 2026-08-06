//! Spec artifacts: frontmatter, status machine, acceptance criteria
//! (DESIGN.md §5 and §7).

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const SCHEMA_VERSION: u32 = 1;
pub const SPEC_FILE: &str = "spec.md";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Draft,
    Review,
    Approved,
    Implementing,
    Done,
    Superseded,
}

impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Status::Draft => "draft",
            Status::Review => "review",
            Status::Approved => "approved",
            Status::Implementing => "implementing",
            Status::Done => "done",
            Status::Superseded => "superseded",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "draft" => Status::Draft,
            "review" => Status::Review,
            "approved" => Status::Approved,
            "implementing" => Status::Implementing,
            "done" => Status::Done,
            "superseded" => Status::Superseded,
            _ => return None,
        })
    }

    pub const ALL: [Status; 6] = [
        Status::Draft,
        Status::Review,
        Status::Approved,
        Status::Implementing,
        Status::Done,
        Status::Superseded,
    ];
}

impl std::fmt::Display for Status {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// An acceptance criterion. Tier A and B are the stop condition; tier C is
/// never marked satisfied by an agent (DESIGN.md §7).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "tier")]
pub enum Criterion {
    /// Executable: a named test that must exist and pass.
    A { test: String },
    /// Commanded: anything with an exit code.
    B { cmd: String },
    /// Judged: prose, checked by a human at review time.
    C { text: String },
}

impl Criterion {
    pub fn tier(&self) -> char {
        match self {
            Criterion::A { .. } => 'A',
            Criterion::B { .. } => 'B',
            Criterion::C { .. } => 'C',
        }
    }

    pub fn machine_verifiable(&self) -> bool {
        matches!(self, Criterion::A { .. } | Criterion::B { .. })
    }

    /// Stable text used for hashing. Deliberately explicit rather than derived
    /// from a serializer, so a serde or YAML upgrade cannot silently
    /// invalidate every approval in every project.
    pub fn canonical(&self) -> String {
        match self {
            Criterion::A { test } => format!("A:{test}"),
            Criterion::B { cmd } => format!("B:{cmd}"),
            Criterion::C { text } => format!("C:{text}"),
        }
    }
}

/// What the superseding spec does with each of the old spec's tier A criteria.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DispositionAction {
    /// The behaviour still holds; the test carries over unchanged.
    Inherited,
    /// Covered differently here; the old test goes, a new named one replaces it.
    Replaced,
    /// Intentionally gone. Requires a written reason.
    Dropped,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Disposition {
    pub test: String,
    pub action: DispositionAction,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Supersession {
    pub id: String,
    #[serde(default)]
    pub dispositions: Vec<Disposition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Frontmatter {
    pub id: String,
    pub title: String,
    pub status: Status,
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    pub created: String,
    /// Repo-relative package paths this spec touches. Set during planning.
    #[serde(default)]
    pub packages: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub supersedes: Vec<Supersession>,
    #[serde(default)]
    pub acceptance: Vec<Criterion>,
}

fn default_schema_version() -> u32 {
    SCHEMA_VERSION
}

pub struct Spec {
    pub dir: PathBuf,
    pub front: Frontmatter,
    pub body: String,
}

impl Spec {
    pub fn path(&self) -> PathBuf {
        self.dir.join(SPEC_FILE)
    }

    pub fn load(dir: &Path) -> Result<Self> {
        let path = dir.join(SPEC_FILE);
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("cannot read {}", path.display()))?;
        let (yaml, body) = split_frontmatter(&text)
            .with_context(|| format!("cannot parse frontmatter in {}", path.display()))?;
        let front: Frontmatter = serde_yaml::from_str(yaml)
            .with_context(|| format!("invalid frontmatter in {}", path.display()))?;
        Ok(Self {
            dir: dir.to_path_buf(),
            front,
            body: body.to_string(),
        })
    }

    pub fn save(&self) -> Result<()> {
        let yaml = serde_yaml::to_string(&self.front)?;
        let text = format!("---\n{}---\n{}", yaml, self.body);
        std::fs::write(self.path(), text)
            .with_context(|| format!("cannot write {}", self.path().display()))
    }

    /// Acceptance files, sorted by name so hashing is order-independent.
    /// `acceptance.*` only — `decisions.md` and friends are logs and are
    /// deliberately outside the hashed set (DESIGN.md §5).
    pub fn acceptance_files(&self) -> Result<Vec<PathBuf>> {
        let mut out = Vec::new();
        if !self.dir.is_dir() {
            return Ok(out);
        }
        for entry in std::fs::read_dir(&self.dir)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with("acceptance.") && entry.path().is_file() {
                out.push(entry.path());
            }
        }
        out.sort();
        Ok(out)
    }

    pub fn tier_a_tests(&self) -> Vec<&str> {
        self.front
            .acceptance
            .iter()
            .filter_map(|c| match c {
                Criterion::A { test } => Some(test.as_str()),
                _ => None,
            })
            .collect()
    }

    pub fn slug(&self) -> String {
        self.dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    }
}

/// Split `---\n<yaml>\n---\n<body>`.
pub fn split_frontmatter(text: &str) -> Result<(&str, &str)> {
    let rest = text
        .strip_prefix("---\n")
        .ok_or_else(|| anyhow!("expected the file to start with a `---` frontmatter fence"))?;
    let end = rest
        .find("\n---\n")
        .ok_or_else(|| anyhow!("unterminated frontmatter: no closing `---` fence"))?;
    let yaml = &rest[..end + 1];
    let body = &rest[end + 5..];
    Ok((yaml, body))
}

/// Every spec directory under `specs/`, sorted by id.
pub fn all(specs_dir: &Path) -> Result<Vec<Spec>> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    if !specs_dir.is_dir() {
        return Ok(Vec::new());
    }
    for entry in std::fs::read_dir(specs_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() && path.join(SPEC_FILE).is_file() {
            dirs.push(path);
        }
    }
    dirs.sort();
    dirs.iter().map(|d| Spec::load(d)).collect()
}

/// Resolve a spec by id, accepting `7`, `0007` or the full `0007-slug`.
pub fn find(specs_dir: &Path, id: &str) -> Result<Spec> {
    let normalised = normalise_id(id);
    let mut matches: Vec<PathBuf> = Vec::new();
    if specs_dir.is_dir() {
        for entry in std::fs::read_dir(specs_dir)? {
            let entry = entry?;
            let path = entry.path();
            if !path.join(SPEC_FILE).is_file() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            if name == id || name.starts_with(&format!("{normalised}-")) || name == normalised {
                matches.push(path);
            }
        }
    }
    matches.sort();
    match matches.len() {
        0 => bail!("no spec {id} in {}", specs_dir.display()),
        1 => Spec::load(&matches[0]),
        _ => bail!(
            "ambiguous spec id {id}: {} directories match",
            matches.len()
        ),
    }
}

/// Zero-pad to the four-digit form used in directory names.
pub fn normalise_id(id: &str) -> String {
    match id.parse::<u32>() {
        Ok(n) => format!("{n:04}"),
        Err(_) => id.to_string(),
    }
}

/// Next free id: highest existing plus one. Reserved by this binary rather
/// than chosen by a model, because clobbering a directory is exactly the kind
/// of 2%-failure a model contributes and `read_dir` does not.
pub fn next_id(specs_dir: &Path) -> Result<String> {
    let mut max = 0u32;
    if specs_dir.is_dir() {
        for entry in std::fs::read_dir(specs_dir)? {
            let name = entry?.file_name().to_string_lossy().into_owned();
            let digits: String = name.chars().take_while(|c| c.is_ascii_digit()).collect();
            if digits.len() == 4 {
                if let Ok(n) = digits.parse::<u32>() {
                    max = max.max(n);
                }
            }
        }
    }
    Ok(format!("{:04}", max + 1))
}

/// Lowercase, hyphenated, ASCII-only, capped at six words so directory names
/// stay readable in a listing.
pub fn slugify(text: &str) -> String {
    let words: Vec<String> = text
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .take(6)
        .map(|w| w.to_string())
        .collect();
    let slug = words.join("-");
    if slug.is_empty() {
        "untitled".into()
    } else {
        slug
    }
}

/// How long a derived title may be before it stops being a title.
///
/// It lands in an H1, a `bevel list` row and a `specs/README.md` table cell.
/// Seventy-two keeps the row under eighty once the id is prefixed.
const TITLE_MAX: usize = 72;

/// A spec title from inbox prose: the first sentence, capped at a word.
///
/// An inbox item is a paragraph, deliberately — capture is meant to be cheap,
/// so it holds the whole thought including the reasoning. A title is a line.
/// Using the paragraph whole made the H1, the listing and the index table
/// unreadable at once, and `slugify` already takes six words for the directory
/// name, so the title was the one place the paragraph survived intact.
///
/// Nothing is lost by shortening it: `{{source}}` puts the full item in the
/// body under "From the inbox", which is where the reasoning belongs.
///
/// A sentence ends at a full stop *followed by a space*, so `spec.md prose`
/// and `src/*.rs that` do not end one. An explicit `--title` never comes here.
pub fn title_from(text: &str) -> String {
    let text = text.trim();
    let sentence = match text.find(". ") {
        Some(i) => &text[..=i],
        None => text,
    };
    let sentence = sentence.trim_end_matches('.').trim();
    if sentence.chars().count() <= TITLE_MAX {
        return sentence.to_string();
    }
    // Cut at a word boundary, never mid-word and never mid-character.
    let cut = sentence
        .char_indices()
        .take_while(|(i, _)| *i <= TITLE_MAX)
        .filter(|(_, c)| c.is_whitespace())
        .map(|(i, _)| i)
        .last();
    match cut {
        Some(i) => sentence[..i].trim_end().to_string(),
        // One word longer than the cap. Truncating it would invent a word, so
        // the whole word survives and the cap gives way.
        None => sentence.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frontmatter_round_trips() {
        let text = "---\nid: '0007'\ntitle: Example\nstatus: draft\nschema_version: 1\ncreated: '2026-08-02'\nacceptance:\n- tier: A\n  test: does_the_thing\n---\n# Body\n\ntext\n";
        let (yaml, body) = split_frontmatter(text).unwrap();
        let front: Frontmatter = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(front.id, "0007");
        assert_eq!(front.status, Status::Draft);
        assert_eq!(body, "# Body\n\ntext\n");
        assert_eq!(
            front.acceptance,
            vec![Criterion::A {
                test: "does_the_thing".into()
            }]
        );
    }

    #[test]
    fn frontmatter_rejects_a_missing_fence() {
        assert!(split_frontmatter("no fence here\n").is_err());
        assert!(split_frontmatter("---\nid: x\nnever closed\n").is_err());
    }

    #[test]
    fn ids_are_monotonic_and_zero_padded() {
        let tmp = tempfile::tempdir().unwrap();
        let specs = tmp.path();
        assert_eq!(next_id(specs).unwrap(), "0001");

        for name in ["0001-first", "0002-second", "not-a-spec"] {
            std::fs::create_dir(specs.join(name)).unwrap();
        }
        assert_eq!(next_id(specs).unwrap(), "0003");
    }

    #[test]
    fn slugify_is_bounded_and_ascii() {
        assert_eq!(slugify("Add a Sync Button!"), "add-a-sync-button");
        assert_eq!(
            slugify("one two three four five six seven"),
            "one-two-three-four-five-six"
        );
        assert_eq!(slugify("   "), "untitled");
    }

    /// An inbox item is a paragraph on purpose. A title is a line, and it was
    /// the one place the paragraph survived whole — `slugify` already bounded
    /// the directory name, so an unreadable H1 and a wrapped index table were
    /// the only visible symptoms until one of them broke the YAML outright.
    #[test]
    fn a_title_derived_from_inbox_prose_is_one_readable_line() {
        // The item that surfaced this. Note the abbreviations: a full stop only
        // ends a sentence when a space follows, or this would stop at `spec.md`.
        let item = "bevel close counts phantom pending markers: pending_markers \
                    searches spec.md prose and fixtures in src/*.rs too. Its \
                    sibling locate() already excludes specs/ for this reason.";
        let title = title_from(item);
        assert!(title.chars().count() <= TITLE_MAX, "{title}");
        assert!(title.starts_with("bevel close counts phantom"));
        assert!(!title.contains("Its sibling"), "kept a second sentence");
        // Cut at a space, so the last word is a whole word.
        assert!(!item[title.len()..].starts_with(|c: char| c.is_alphanumeric()));

        // Short prose is left exactly as it is, minus the full stop it does not
        // need in a heading.
        assert_eq!(title_from("Add a sync button."), "Add a sync button");
        assert_eq!(title_from("  Add a sync button  "), "Add a sync button");

        // One word longer than the cap: truncating would invent a word that is
        // not in the item, so the cap gives way instead.
        let long = "s".repeat(TITLE_MAX + 10);
        assert_eq!(title_from(&long), long);

        // Multi-byte input must not be split mid-character.
        let accented = "cañón ".repeat(30);
        let t = title_from(&accented);
        assert!(t.chars().count() <= TITLE_MAX, "{t}");
        assert!(accented.starts_with(&t));
    }

    #[test]
    fn id_lookup_accepts_short_and_padded_forms() {
        assert_eq!(normalise_id("7"), "0007");
        assert_eq!(normalise_id("0007"), "0007");
        assert_eq!(normalise_id("0007-slug"), "0007-slug");
    }
}
