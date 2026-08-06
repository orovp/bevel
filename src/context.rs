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
use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;

use crate::html::{self, card, escape, pill, Page};
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
    /// Repo-relative, when the file is in the project at all. The method often
    /// resolves out of `$HOME`, and a file git has never seen has no history to
    /// plot — which is a fact worth keeping rather than papering over.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
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
        path: None,
    }
}

fn measure_file(name: impl Into<String>, path: &Path, limit: usize, load: Load) -> Option<Item> {
    let text = std::fs::read_to_string(path).ok()?;
    Some(measure(name, &text, limit, load))
}

/// As `measure_file`, but remembering where the file was when it is inside the
/// project — which is what makes the trend in the HTML report possible.
fn measure_tracked(
    name: impl Into<String>,
    path: &Path,
    limit: usize,
    load: Load,
    root: &Path,
) -> Option<Item> {
    let mut item = measure_file(name, path, limit, load)?;
    item.path = path
        .strip_prefix(root)
        .ok()
        .map(|p| p.to_string_lossy().replace('\\', "/"));
    Some(item)
}

pub fn audit(project: &Project, layers: &Layers, source: &method::Source) -> Result<Audit> {
    let mut items = Vec::new();

    // Both are loaded every turn — AGENTS.md carries the pipeline notes and
    // CLAUDE.md points at it — so both are measured. Measuring only one would
    // retire this check silently the moment the body sat in the other, which is
    // the failure this pair of entries exists to prevent (DESIGN.md §13).
    //
    // Neither is written by bevel; `bevel notes` prints them and the user
    // applies what they want. That makes the budget more load-bearing here, not
    // less: nothing upstream constrains a file bevel does not author, so this
    // audit is the only thing that ever counts its lines. Both are absent until
    // someone applies them, and `measure_tracked` returns `None` for a file
    // that is not there rather than reporting an empty one.
    for name in ["AGENTS.md", "CLAUDE.md"] {
        if let Some(i) = measure_tracked(
            name,
            &project.root.join(name),
            50,
            Load::Always,
            &project.root,
        ) {
            items.push(i);
        }
    }
    for pkg in &project.config.packages {
        let path = project.root.join(&pkg.path).join("AGENTS.md");
        if let Some(i) = measure_tracked(
            format!("{}/AGENTS.md", pkg.path),
            &path,
            30,
            Load::OnDemand,
            &project.root,
        ) {
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
        if let Some(mut i) = measure_tracked(label, &path, limit, load, &project.root) {
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

/// How many commits back the trend looks. Enough to cover a few months of a
/// harness that should barely change, and bounded so one `git log` stays cheap.
const HISTORY_DEPTH: usize = 80;

/// The harness's size over time, reconstructed from git.
///
/// This section names the failure this project is most likely to have, and it
/// is a *slow* one: not a bug, but three thousand lines of markdown accumulated
/// over six months. A single measurement cannot see it — a table says the
/// harness is four hundred lines today and has no way to say it was two hundred
/// in March. So: one `git log --numstat` for the deltas, and the totals walk
/// backwards from what is on disk now.
///
/// Approximate on purpose. Renames drop out of the window and files that live
/// in `$HOME` were never in git at all; the number that matters is whether the
/// line is climbing, not whether a point is eight lines out.
pub fn history(project: &Project, audit: &Audit) -> Vec<(String, usize)> {
    let tracked: BTreeMap<&str, usize> = audit
        .items
        .iter()
        .filter_map(|i| Some((i.path.as_deref()?, i.lines)))
        .collect();
    if tracked.is_empty() {
        return Vec::new();
    }

    let mut cmd = Command::new("git");
    cmd.args(["log", "--numstat", "--date=short", "--format=%x01%ad"])
        .arg(format!("-n{HISTORY_DEPTH}"))
        .arg("--")
        .args(tracked.keys())
        .current_dir(&project.root);
    let Ok(out) = cmd.output() else {
        return Vec::new();
    };
    if !out.status.success() {
        return Vec::new();
    }

    let text = String::from_utf8_lossy(&out.stdout);
    let mut running: i64 = tracked.values().sum::<usize>() as i64;
    let mut points: Vec<(String, usize)> = Vec::new();
    let mut open: Option<(String, i64)> = None;

    for line in text.lines() {
        if let Some(date) = line.strip_prefix('\x01') {
            // `running` is the total as of the end of the commit just closed.
            if let Some((d, delta)) = open.take() {
                points.push((d, running.max(0) as usize));
                running -= delta;
            }
            open = Some((date.to_string(), 0));
            continue;
        }
        let Some((added, rest)) = line.split_once('\t') else {
            continue;
        };
        let Some((deleted, path)) = rest.split_once('\t') else {
            continue;
        };
        // A binary file, or a rename we cannot follow: `-` in both columns.
        let (Ok(added), Ok(deleted)) = (added.parse::<i64>(), deleted.parse::<i64>()) else {
            continue;
        };
        if tracked.contains_key(path) {
            if let Some((_, delta)) = open.as_mut() {
                *delta += added - deleted;
            }
        }
    }
    if let Some((d, _)) = open {
        points.push((d, running.max(0) as usize));
    }

    points.reverse();
    // One point per day, the last of that day: a busy afternoon is not a trend.
    let mut per_day: Vec<(String, usize)> = Vec::with_capacity(points.len());
    for p in points {
        match per_day.last_mut() {
            Some(last) if last.0 == p.0 => *last = p,
            _ => per_day.push(p),
        }
    }
    per_day
}

pub fn render_html(audit: &Audit, history: &[(String, usize)]) -> Page {
    let over = audit.over_budget();
    let vendored = audit.vendored_over();

    let by_load = |l: Load| -> usize {
        audit
            .items
            .iter()
            .filter(|i| i.load == l)
            .map(|i| i.tokens)
            .sum()
    };
    let composition = html::stacked(&[
        ("always", by_load(Load::Always)),
        ("on invocation", by_load(Load::OnInvocation)),
        ("on demand", by_load(Load::OnDemand)),
    ]);

    let verdict = if over.is_empty() {
        format!(
            "<p class=note>Every file we own is within its budget.{}</p>",
            if vendored.is_empty() {
                String::new()
            } else {
                format!(
                    " {} vendored file{} sits over its limit; that cost was accepted \
                     when it was vendored, and the only lever left is whether to keep it.",
                    vendored.len(),
                    if vendored.len() == 1 { "" } else { "s" }
                )
            }
        )
    } else {
        let items: String = over
            .iter()
            .map(|i| {
                format!(
                    "<li><code>{}</code> is {} lines against a budget of {}</li>",
                    escape(&i.name),
                    i.lines,
                    i.limit
                )
            })
            .collect();
        format!(
            "<p class=note>{} file{} over budget.</p><ul>{items}</ul>\
             <p class=note>The question each release is not what else to add, but \
             <b>what to stop doing</b>. Every new model makes some scaffolding obsolete.</p>",
            over.len(),
            if over.len() == 1 { "" } else { "s" }
        )
    };

    let mut body = format!(
        "<section class=\"card {}\"><h2>Every turn</h2>\
         <p class=big>~{} tokens</p>\
         <p class=note>enter the model's context unconditionally, before you have \
         typed anything. Everything else is paid for only when it is read.</p>\
         {composition}{verdict}</section>\n",
        if over.is_empty() { "" } else { "warn" },
        audit.always_tokens,
    );

    body.push_str(&card(
        "Trend",
        &format!(
            "{}<p class=note>The failure this design predicts for itself is slow: \
             in six months the harness is three thousand lines of markdown arguing \
             with the model. A single measurement cannot see that coming — only the \
             slope can. Reconstructed from git, so files resolved out of \
             <code>$HOME</code> do not appear.</p>",
            html::line_chart(history, "lines of harness, over time")
        ),
    ));

    let rows: String = audit
        .items
        .iter()
        .map(|i| {
            format!(
                "<tr><td><code>{}</code></td><td>{}</td><td>{}</td>\
                 <td style=\"text-align:right\">{}</td><td>{}</td></tr>",
                escape(&i.name),
                pill(load_class(i.load), i.load.as_str()),
                html::meter(i.lines, i.limit, &format!("{} / {}", i.lines, i.limit)),
                i.tokens,
                match (i.over(), i.vendored) {
                    (true, false) => pill("bad", "over"),
                    (true, true) => pill("mute", "over, vendored"),
                    _ => String::new(),
                }
            )
        })
        .collect();
    body.push_str(&card(
        "Every file the harness can load",
        &format!(
            "<div class=scroll><table><tr><th>file</th><th>loaded</th>\
             <th>lines against budget</th><th>~tokens</th><th></th></tr>{rows}</table></div>"
        ),
    ));

    Page::new("Context budget", "bevel doctor --context --html")
        .subtitle("A linter for the harness itself")
        .body(body)
}

fn load_class(l: Load) -> &'static str {
    match l {
        Load::Always => "b",
        Load::OnInvocation => "c",
        Load::OnDemand => "mute",
    }
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
            opencode: tmp.path().join("home/.config/opencode"),
        };
        // The repository's own method tree, which is what ships.
        let source = method::Source {
            kind: method::Kind::Local,
            root: std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")),
            origin: "repo".into(),
        };
        (tmp, project, layers, source)
    }

    /// `git init` plus two commits, so the trend has something to reconstruct.
    /// Config is passed per-invocation because the machine running the tests may
    /// have no global identity, and a test that depends on one is a test that
    /// fails in CI for a reason unrelated to what it checks.
    fn git(root: &std::path::Path, args: &[&str]) -> bool {
        Command::new("git")
            .args([
                "-c",
                "user.email=t@example.com",
                "-c",
                "user.name=t",
                "-c",
                "commit.gpgsign=false",
            ])
            .args(args)
            .current_dir(root)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    #[test]
    fn the_trend_reconstructs_earlier_sizes_from_the_deltas() {
        let (_t, p, l, m) = setup();
        if !git(&p.root, &["init", "-q"]) {
            return; // No git on this machine; the report degrades to no trend.
        }

        std::fs::write(p.root.join("CLAUDE.md"), "one\ntwo\n").unwrap();
        assert!(git(&p.root, &["add", "CLAUDE.md"]));
        assert!(git(&p.root, &["commit", "-qm", "first"]));

        std::fs::write(p.root.join("CLAUDE.md"), "one\ntwo\nthree\nfour\nfive\n").unwrap();
        assert!(git(&p.root, &["commit", "-qam", "second"]));

        let a = audit(&p, &l, &m).unwrap();
        let h = history(&p, &a);
        assert_eq!(h.len(), 1, "both commits are the same day: {h:?}");
        assert_eq!(h[0].1, 5, "the day ends at the later size");

        // And the file has to be findable in the first place.
        assert!(a
            .items
            .iter()
            .any(|i| i.name == "CLAUDE.md" && i.path.as_deref() == Some("CLAUDE.md")));
    }

    #[test]
    fn outside_a_git_repository_the_report_simply_has_no_trend() {
        let (_t, p, l, m) = setup();
        let a = audit(&p, &l, &m).unwrap();
        let html = render_html(&a, &history(&p, &a)).render();
        assert!(html.contains("Not enough history"));
        assert!(html.contains("Context budget"));
    }

    #[test]
    fn the_html_report_leads_with_what_enters_every_turn() {
        let (_t, p, l, m) = setup();
        std::fs::write(
            p.root.join("CLAUDE.md"),
            (0..80).map(|i| format!("line {i}\n")).collect::<String>(),
        )
        .unwrap();
        let a = audit(&p, &l, &m).unwrap();
        let html = render_html(&a, &[]).render();

        assert!(html.contains("tokens"));
        assert!(html.contains("what to stop doing"));
        assert!(html.contains("class=\"card warn\""), "over budget is loud");
        // Same invariant as every other report: it reports, it does not act.
        for probe in ["<form", "<button", "onclick"] {
            assert!(!html.contains(probe), "found `{probe}`");
        }
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

    // ------------------------------------------------ acceptance, spec 0001

    /// `audit` hardcoded `CLAUDE.md` as the only `Load::Always` item, and the
    /// loop below it walks packages only — so a root `AGENTS.md` went
    /// unaudited. Move the body without moving the measurement and DESIGN.md
    /// §13's budget check retires itself while every other test still passes.
    #[test]
    fn the_always_loaded_context_budget_follows_the_body_to_agents_md() {
        let (_t, p, l, m) = setup();
        let bloat: String = (0..80).map(|i| format!("line {i}\n")).collect();
        std::fs::write(p.root.join("AGENTS.md"), &bloat).unwrap();

        let a = audit(&p, &l, &m).unwrap();
        let item = a
            .items
            .iter()
            .find(|i| i.name == "AGENTS.md")
            .expect("the root AGENTS.md is not measured at all");
        // Always-loaded, or the budget is measuring something that costs
        // nothing per turn.
        assert_eq!(item.load, Load::Always);
        assert_eq!(item.limit, 50);
        assert!(item.over(), "80 lines went unflagged");
        assert!(a.items.iter().any(|i| i.name == "AGENTS.md" && i.over()));

        // The pointer is loaded every turn too, so it stays measured — the
        // point is that the *body* cannot escape the budget by moving.
        std::fs::write(p.root.join("CLAUDE.md"), "See AGENTS.md.\n").unwrap();
        let a = audit(&p, &l, &m).unwrap();
        let pointer = a.items.iter().find(|i| i.name == "CLAUDE.md").unwrap();
        assert_eq!(pointer.load, Load::Always);
        assert!(!pointer.over());
    }
}
