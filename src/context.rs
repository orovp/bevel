//! The context budget (DESIGN.md §13).
//!
//! The likeliest failure of this project is not technical: it is that in six
//! months the harness is three thousand lines of markdown arguing with the
//! model. Anthropic deleted 80% of their own system prompt; we need to be able
//! to do the same, and that means measuring it.
//!
//! A linter for the harness itself.

use anyhow::Result;
use serde::Serialize;
use std::path::Path;

use crate::method;
use crate::paths::Layers;
use crate::project::Project;
use crate::sync;

/// When a file reaches the model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Load {
    /// In context for every single turn. The budget that actually bites.
    Always,
    /// Read when a skill is invoked.
    OnInvocation,
    /// Read only when the task touches it.
    OnDemand,
}

impl Load {
    fn as_str(self) -> &'static str {
        match self {
            Load::Always => "always",
            Load::OnInvocation => "on invocation",
            Load::OnDemand => "on demand",
        }
    }
}

#[derive(Debug, Serialize)]
pub struct Item {
    pub name: String,
    pub lines: usize,
    pub limit: usize,
    pub tokens: usize,
    pub load: Load,
    /// Vendored from elsewhere, so its length is measured but not enforced.
    pub vendored: bool,
}

impl Item {
    /// The measurement, which is the same fact whoever wrote the file.
    pub fn over(&self) -> bool {
        self.lines > self.limit
    }

    /// Whether being over is a failure. It is not, for a file we did not write:
    /// see `sync::VENDORED_SKILLS`.
    pub fn enforced(&self) -> bool {
        self.over() && !self.vendored
    }
}

#[derive(Debug, Serialize)]
pub struct Audit {
    pub items: Vec<Item>,
    /// Estimated tokens of everything loaded unconditionally.
    pub always_tokens: usize,
}

impl Audit {
    /// What fails the budget. Vendored files are excluded on purpose; use
    /// `vendored_over` to see them.
    pub fn over_budget(&self) -> Vec<&Item> {
        self.items.iter().filter(|i| i.enforced()).collect()
    }

    /// Measured, over, and not our text to cut. Reported so that vendoring a
    /// large skill stays a visible cost rather than a silent one.
    pub fn vendored_over(&self) -> Vec<&Item> {
        self.items
            .iter()
            .filter(|i| i.over() && i.vendored)
            .collect()
    }
}

/// Rough token estimate. Four characters per token is the usual heuristic and
/// is close enough for a budget: the number that matters is whether a file
/// doubled, not whether it is 812 tokens or 847.
fn estimate_tokens(text: &str) -> usize {
    text.len().div_ceil(4)
}

fn measure(name: impl Into<String>, text: &str, limit: usize, load: Load) -> Item {
    Item {
        name: name.into(),
        lines: text.lines().count(),
        limit,
        tokens: estimate_tokens(text),
        load,
        // Set by the caller that knows; almost nothing is vendored.
        vendored: false,
    }
}

fn measure_file(name: impl Into<String>, path: &Path, limit: usize, load: Load) -> Option<Item> {
    let text = std::fs::read_to_string(path).ok()?;
    Some(measure(name, &text, limit, load))
}

pub fn audit(project: &Project, layers: &Layers, source: &method::Source) -> Result<Audit> {
    let mut items = Vec::new();

    // Loaded every turn, and now the only file that is: the pipeline notes
    // live in CLAUDE.md itself rather than behind a pointer.
    if let Some(i) = measure_file(
        "CLAUDE.md",
        &project.root.join("CLAUDE.md"),
        50,
        Load::Always,
    ) {
        items.push(i);
    }
    for pkg in &project.config.packages {
        let path = project.root.join(&pkg.path).join("AGENTS.md");
        if let Some(i) = measure_file(format!("{}/AGENTS.md", pkg.path), &path, 30, Load::OnDemand)
        {
            items.push(i);
        }
    }

    // The method, from whichever layer actually resolves.
    for (label, path, _which) in sync::method_sources(layers, source) {
        // A lifecycle skill is read on every pass of the loop, so it pays for
        // its length constantly. A conventions skill is read only when the work
        // matches it, and its length is set by the surface it covers rather
        // than by what the loop needs: twice the lifecycle budget, and still a
        // number that bites — same file kind, different limit.
        let limit = match label.as_str() {
            "skill/shape" | "skill/implement" => 120,
            l if l.starts_with("skill/") => 240,
            _ => 60,
        };
        let load = if label.starts_with("skill/") {
            Load::OnInvocation
        } else {
            Load::OnDemand
        };
        let vendored = label.strip_prefix("skill/").is_some_and(sync::is_vendored);
        if let Some(mut i) = measure_file(label, &path, limit, load) {
            i.vendored = vendored;
            items.push(i);
        }
    }

    // Pack gotchas carry conventions and traps, not tutorials.
    for dir in [layers.user_packs(), project.state_dir().join("packs")] {
        if !dir.is_dir() {
            continue;
        }
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path().join("gotchas.md");
            let id = entry.file_name().to_string_lossy().into_owned();
            if let Some(i) =
                measure_file(format!("pack/{id}/gotchas.md"), &path, 80, Load::OnDemand)
            {
                items.push(i);
            }
        }
    }

    let always_tokens = items
        .iter()
        .filter(|i| i.load == Load::Always)
        .map(|i| i.tokens)
        .sum();

    Ok(Audit {
        items,
        always_tokens,
    })
}

pub fn render(audit: &Audit) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "{:<26} {:>5} {:>7} {:>7}  {}\n",
        "file", "lines", "limit", "~tokens", "loaded"
    ));
    for i in &audit.items {
        out.push_str(&format!(
            "{:<26} {:>5} {:>7} {:>7}  {}{}\n",
            i.name,
            i.lines,
            i.limit,
            i.tokens,
            i.load.as_str(),
            match (i.over(), i.vendored) {
                (true, false) => "   OVER",
                (true, true) => "   OVER (vendored)",
                _ => "",
            }
        ));
    }
    out.push_str(&format!(
        "\n~{} tokens enter every turn unconditionally.\n",
        audit.always_tokens
    ));

    let over = audit.over_budget();
    if over.is_empty() {
        out.push_str("all within budget.\n");
    } else {
        out.push_str(&format!("\n{} over budget:\n", over.len()));
        for i in over {
            out.push_str(&format!(
                "  {} is {} lines, budget {}\n",
                i.name, i.lines, i.limit
            ));
        }
        out.push_str("\nThe question each release is not what to add, but what to stop doing.\n");
    }

    // Listed after the verdict rather than inside it: this is a cost that was
    // accepted, not a regression to fix, and the only lever left is whether to
    // keep vendoring the skill.
    let vendored = audit.vendored_over();
    if !vendored.is_empty() {
        out.push_str(&format!(
            "\n{} vendored, over its limit and not enforced:\n",
            vendored.len()
        ));
        for i in vendored {
            out.push_str(&format!(
                "  {} is {} lines, limit {}\n",
                i.name, i.lines, i.limit
            ));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (tempfile::TempDir, Project, Layers, method::Source) {
        let tmp = tempfile::tempdir().unwrap();
        crate::project::init(tmp.path(), false).unwrap();
        let project = Project::discover_from(tmp.path()).unwrap();
        let layers = Layers {
            config: tmp.path().join("cfg"),
            cache: tmp.path().join("cache"),
            home: tmp.path().join("home"),
        };
        // The repository's own method tree, which is what ships.
        let source = method::Source {
            kind: method::Kind::Local,
            root: std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")),
            origin: "repo".into(),
        };
        (tmp, project, layers, source)
    }

    #[test]
    fn the_shipped_method_is_within_its_own_budget() {
        // If this fails, the harness has started doing the thing it warns
        // against, and the fix is deletion rather than a bigger limit.
        let (_t, p, l, m) = setup();
        let a = audit(&p, &l, &m).unwrap();
        let over = a.over_budget();
        assert!(
            over.is_empty(),
            "over budget: {:?}",
            over.iter()
                .map(|i| (&i.name, i.lines, i.limit))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn an_oversized_agents_file_is_caught() {
        let (_t, p, l, m) = setup();
        let bloat: String = (0..80).map(|i| format!("line {i}\n")).collect();
        std::fs::write(p.root.join("CLAUDE.md"), bloat).unwrap();

        let a = audit(&p, &l, &m).unwrap();
        // Named rather than counted: what else the method tree happens to
        // contain is the business of the budget test, not of this one.
        assert!(a.over_budget().iter().any(|i| i.name == "CLAUDE.md"));
        assert!(render(&a).contains("OVER"));
    }

    /// A vendored skill is measured like any other and shown when it is over,
    /// but it does not fail the budget: its length was never ours to cut, and
    /// the only lever is whether to vendor it at all.
    #[test]
    fn a_vendored_skill_is_measured_but_not_enforced() {
        let (_t, p, l, m) = setup();
        let a = audit(&p, &l, &m).unwrap();

        let sc = a
            .items
            .iter()
            .find(|i| i.name == "skill/skill-creator")
            .expect("skill-creator is part of the shipped method");
        assert!(sc.vendored);
        // The whole case rests on it being over the limit.
        assert!(sc.over(), "{} lines, limit {}", sc.lines, sc.limit);
        assert!(!sc.enforced());
        assert!(!a
            .over_budget()
            .iter()
            .any(|i| i.name == "skill/skill-creator"));

        // Excluded from the verdict, but never hidden.
        let text = render(&a);
        assert!(text.contains("OVER (vendored)"), "{text}");
        assert!(text.contains("not enforced"), "{text}");
    }

    /// The exemption is per-skill, not a general loosening: text we wrote is
    /// still held to the limit, which is the point of the budget.
    #[test]
    fn a_skill_we_wrote_ourselves_is_still_enforced() {
        let (_t, p, l, m) = setup();
        let src = l.user_method().join("skills/rust-architecture/SKILL.md");
        std::fs::create_dir_all(src.parent().unwrap()).unwrap();
        let huge: String = (0..400).map(|i| format!("l{i}\n")).collect();
        std::fs::write(&src, huge).unwrap();

        let a = audit(&p, &l, &m).unwrap();
        let item = a
            .items
            .iter()
            .find(|i| i.name == "skill/rust-architecture")
            .unwrap();
        assert!(!item.vendored);
        assert!(item.enforced());
        assert!(a
            .over_budget()
            .iter()
            .any(|i| i.name == "skill/rust-architecture"));
    }

    #[test]
    fn only_always_loaded_files_count_towards_the_per_turn_total() {
        let (_t, p, l, m) = setup();
        std::fs::write(p.root.join("CLAUDE.md"), "one line\n").unwrap();

        let a = audit(&p, &l, &m).unwrap();
        // Skills and agents are far larger, so a naive sum would dwarf this.
        assert!(a.always_tokens < 20, "{}", a.always_tokens);
        assert!(a.items.iter().any(|i| i.load == Load::OnInvocation));
    }

    #[test]
    fn a_user_override_is_measured_instead_of_the_builtin() {
        let (_t, p, l, m) = setup();
        let src = l.user_method().join("skills/shape/SKILL.md");
        std::fs::create_dir_all(src.parent().unwrap()).unwrap();
        let huge: String = (0..200).map(|i| format!("l{i}\n")).collect();
        std::fs::write(&src, huge).unwrap();

        let a = audit(&p, &l, &m).unwrap();
        let shape = a.items.iter().find(|i| i.name == "skill/shape").unwrap();
        assert_eq!(shape.lines, 200);
        assert!(shape.over());
    }
}
