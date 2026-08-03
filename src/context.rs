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
}

impl Item {
    pub fn over(&self) -> bool {
        self.lines > self.limit
    }
}

#[derive(Debug, Serialize)]
pub struct Audit {
    pub items: Vec<Item>,
    /// Estimated tokens of everything loaded unconditionally.
    pub always_tokens: usize,
}

impl Audit {
    pub fn over_budget(&self) -> Vec<&Item> {
        self.items.iter().filter(|i| i.over()).collect()
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
        let limit = if label.starts_with("skill/") { 120 } else { 60 };
        let load = if label.starts_with("skill/") {
            Load::OnInvocation
        } else {
            Load::OnDemand
        };
        if let Some(i) = measure_file(label, &path, limit, load) {
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
            if i.over() { "   OVER" } else { "" }
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
        let over = a.over_budget();
        assert_eq!(over.len(), 1);
        assert_eq!(over[0].name, "CLAUDE.md");
        assert!(render(&a).contains("OVER"));
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
