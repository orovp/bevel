//! `bevel review` — the dossier behind the two decisions only a human makes
//! (DESIGN.md §11).
//!
//! Approval and closure are the points where the pipeline stops and a person
//! decides. Both mean reading four or five files scattered across a terminal
//! and holding them in your head at once: the spec, the criteria, what was
//! decided and rejected, what the critic found, what deviated. This assembles
//! them into one page.
//!
//! **The page never acts.** It ends in the command to run, and the terminal
//! check in `gate.rs` stays exactly where it is. A page with an approve button
//! would make that check decorative — and the agent is what generated the page.
//!
//! Which view you get follows the status, because the question differs. Before
//! approval it is *"is this contract worth freezing?"*; after implementation it
//! is *"did the work meet the contract that was frozen?"*.

use anyhow::Result;
use std::path::Path;

use crate::gate::{self, Verdict};
use crate::html::{self, card, escape, markdown, pill, Page};
use crate::project::Project;
use crate::spec::{Criterion, Spec, Status};
use crate::validate::{self, Finding};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Before the gate: is this contract worth freezing?
    Approval,
    /// After the work: did it meet the contract that was frozen?
    Close,
}

impl Kind {
    fn of(status: Status) -> Self {
        match status {
            Status::Implementing | Status::Done | Status::Superseded => Kind::Close,
            _ => Kind::Approval,
        }
    }
}

/// What is known about one acceptance criterion at the moment of the report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Evidence {
    /// Tier A before the gate: the named test exists, inert, in `acceptance.*`.
    Named,
    /// Tier A: no test of that name anywhere. Also a `validate` finding.
    Missing,
    /// Tier A after relocation: found, and still carrying its pending marker.
    Pending(String),
    /// Tier A after relocation: found, marker gone, body filled in.
    Live(String),
    /// Tier B: a command. `verify` runs it; this report deliberately does not.
    Command,
    /// Tier C: judged by a human, optionally anchored to a mockup section.
    Judged(Option<u32>),
    /// Tier C pointing at a mockup section that does not exist.
    Dangling(u32),
}

impl Evidence {
    fn pill(&self) -> String {
        match self {
            Evidence::Named => pill("mute", "named, inert"),
            Evidence::Missing => pill("bad", "no such test"),
            Evidence::Pending(_) => pill("c", "pending"),
            Evidence::Live(_) => pill("a", "live"),
            Evidence::Command => pill("b", "run by verify"),
            Evidence::Judged(_) => pill("c", "yours to judge"),
            Evidence::Dangling(_) => pill("bad", "dangling reference"),
        }
    }

    fn detail(&self, mockup: Option<&Mockup>) -> String {
        match self {
            Evidence::Pending(at) | Evidence::Live(at) => format!("<code>{}</code>", escape(at)),
            Evidence::Missing => "nothing in <code>acceptance.*</code> matches this name".into(),
            Evidence::Judged(Some(n)) => match mockup {
                Some(m) => format!(
                    "<a href=\"{}#s{n}\">open mockup §{n}</a>",
                    escape(&m.href),
                    n = n
                ),
                None => format!("mockup §{n}"),
            },
            Evidence::Dangling(n) => format!("the mockup has no §{n}"),
            _ => String::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct CriterionView {
    pub tier: char,
    pub label: String,
    pub evidence: Evidence,
}

#[derive(Debug, Clone)]
pub struct Mockup {
    /// Relative to the cache directory the report is written into.
    pub href: String,
    pub sections: Vec<(u32, String)>,
}

pub struct Dossier {
    pub kind: Kind,
    pub id: String,
    pub title: String,
    pub slug: String,
    pub status: Status,
    pub created: String,
    pub packages: Vec<String>,
    pub verdict: Verdict,
    pub hash: String,
    pub approval: Option<gate::Entry>,
    pub findings: Vec<Finding>,
    pub criteria: Vec<CriterionView>,
    pub body: String,
    pub decisions: Option<String>,
    pub open_questions: Option<String>,
    pub notes: Option<String>,
    pub mockup: Option<Mockup>,
    /// Repo-wide count, the same number `status` reports.
    pub pending_markers: usize,
}

pub fn build(project: &Project, spec: &Spec) -> Result<Dossier> {
    let kind = Kind::of(spec.front.status);
    let mockup = read_mockup(project, spec);
    let lock = gate::GatesLock::load(&project.gates_lock())?;

    let criteria = spec
        .front
        .acceptance
        .iter()
        .map(|c| CriterionView {
            tier: c.tier(),
            label: match c {
                Criterion::A { test } => test.clone(),
                Criterion::B { cmd } => cmd.clone(),
                Criterion::C { text } => text.clone(),
            },
            evidence: evidence(project, spec, c, kind, mockup.as_ref()),
        })
        .collect();

    Ok(Dossier {
        kind,
        id: spec.front.id.clone(),
        title: spec.front.title.clone(),
        slug: spec.slug(),
        status: spec.front.status,
        created: spec.front.created.clone(),
        packages: spec.front.packages.clone(),
        verdict: gate::check(project, spec)?,
        hash: gate::spec_hash(spec)?,
        approval: lock.spec.get(&spec.front.id).cloned(),
        findings: validate::validate(project, spec)?,
        criteria,
        body: spec.body.clone(),
        decisions: read(&spec.dir.join("decisions.md")),
        open_questions: read(&spec.dir.join("open-questions.md")),
        notes: read(&spec.dir.join("notes.md")),
        mockup,
        pending_markers: validate::pending_markers(&project.root, &spec.front.id),
    })
}

fn evidence(
    project: &Project,
    spec: &Spec,
    criterion: &Criterion,
    kind: Kind,
    mockup: Option<&Mockup>,
) -> Evidence {
    match criterion {
        Criterion::B { .. } => Evidence::Command,
        Criterion::C { text } => match section_of(text) {
            Some(n) if mockup.is_some_and(|m| m.sections.iter().all(|(s, _)| *s != n)) => {
                Evidence::Dangling(n)
            }
            other => Evidence::Judged(other),
        },
        Criterion::A { test } => {
            // Before the gate the test is supposed to be inert and to live in
            // the spec folder; after it, the plan has moved it into a package
            // and the question becomes where it went and whether it is live.
            if kind == Kind::Approval {
                return match in_spec_folder(spec, test) {
                    true => Evidence::Named,
                    false => Evidence::Missing,
                };
            }
            match validate::locate(project, test) {
                None => Evidence::Missing,
                Some((path, line)) => {
                    // Forward slashes even on Windows: this is a label a person
                    // reads, and it is the same label on both machines.
                    let at = format!("{}:{line}", project.display_path(&path).replace('\\', "/"));
                    if marker_near(&path, test, &spec.front.id) {
                        Evidence::Pending(at)
                    } else {
                        Evidence::Live(at)
                    }
                }
            }
        }
    }
}

fn in_spec_folder(spec: &Spec, test: &str) -> bool {
    spec.acceptance_files()
        .unwrap_or_default()
        .iter()
        .filter_map(|f| std::fs::read_to_string(f).ok())
        .any(|text| text.contains(test))
}

/// Is this particular test still pending?
///
/// The marker sits on the attribute line directly above the test in Rust and on
/// the test's own line in the TypeScript forms, so a small window above and
/// including the name covers every pack template. The repo-wide count from
/// `pending_markers` remains the authority for *how many*; this only decides
/// which row to colour, and the report says so when the two disagree.
fn marker_near(path: &Path, test: &str, id: &str) -> bool {
    const WINDOW: usize = 3;
    let Ok(text) = std::fs::read_to_string(path) else {
        return false;
    };
    let needle = format!("acceptance: {id} pending");
    let lines: Vec<&str> = text.lines().collect();
    lines
        .iter()
        .position(|l| l.contains(test))
        .is_some_and(|at| {
            lines[at.saturating_sub(WINDOW)..=at]
                .iter()
                .any(|l| l.contains(&needle))
        })
}

/// The `§N` a tier C criterion points at, if it points at the mockup at all.
fn section_of(text: &str) -> Option<u32> {
    if !text.to_ascii_lowercase().contains("mockup") {
        return None;
    }
    let after = text.split_once('§')?.1;
    after
        .trim_start()
        .chars()
        .take_while(char::is_ascii_digit)
        .collect::<String>()
        .parse()
        .ok()
}

fn read(path: &Path) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    (!text.trim().is_empty()).then_some(text)
}

/// The mockup's numbered states, read back out of the file the agent wrote.
fn read_mockup(project: &Project, spec: &Spec) -> Option<Mockup> {
    let path = spec.dir.join("mockup.html");
    let text = std::fs::read_to_string(&path).ok()?;
    let mut sections = Vec::new();
    for (n, label) in scan_anchors(&text) {
        sections.push((n, label));
    }
    sections.sort_by_key(|(n, _)| *n);
    Some(Mockup {
        // Relative, because the report lives two directories down in
        // `.bevel/cache/` and has to survive being copied with the repo.
        href: format!("../../{}", project.display_path(&path).replace('\\', "/")),
        sections,
    })
}

/// Anchors and a human label for each, taken from the text that follows.
fn scan_anchors(html: &str) -> Vec<(u32, String)> {
    let mut out = Vec::new();
    let mut rest = html;
    while let Some(at) = rest.find("id=") {
        rest = &rest[at + 3..];
        let value: String = rest
            .trim_start_matches(['"', '\''])
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric())
            .collect();
        let Some(digits) = value.strip_prefix('s') else {
            continue;
        };
        let Ok(n) = digits.parse::<u32>() else {
            continue;
        };
        // Past the end of the opening tag: the label is the section's own
        // content, not the rest of its attributes.
        let content = match rest.find('>') {
            Some(i) => &rest[i + 1..],
            None => continue,
        };
        out.push((n, first_text(content)));
    }
    out
}

/// The first run of readable text after a tag, used as the section's name.
fn first_text(html: &str) -> String {
    let mut label = String::new();
    let mut in_tag = false;
    for c in html.chars().take(400) {
        match c {
            '<' => {
                if !label.trim().is_empty() {
                    break;
                }
                in_tag = true;
                label.clear();
            }
            '>' => in_tag = false,
            _ if in_tag => {}
            _ => label.push(c),
        }
    }
    let label = label.split_whitespace().collect::<Vec<_>>().join(" ");
    if label.chars().count() > 60 {
        label.chars().take(59).collect::<String>() + "…"
    } else {
        label
    }
}

pub fn render(d: &Dossier) -> Page {
    let mut body = String::new();
    body.push_str(&decision_card(d));
    body.push_str(&criteria_card(d));

    if let Some(m) = &d.mockup {
        if !m.sections.is_empty() {
            let rows: String = m
                .sections
                .iter()
                .map(|(n, label)| {
                    format!(
                        "<tr><td><a href=\"{}#s{n}\">§{n}</a></td><td>{}</td></tr>",
                        escape(&m.href),
                        escape(label)
                    )
                })
                .collect();
            body.push_str(&card(
                "Mockup",
                &format!(
                    "<div class=scroll><table><tr><th>state</th><th>what it shows</th></tr>\
                     {rows}</table></div>\
                     <p class=note>Reference only, frozen when the spec is done and never \
                     maintained against the implementation.</p>"
                ),
            ));
        }
    }

    if d.kind == Kind::Close {
        body.push_str(&prose_card(
            "Deviations and review notes",
            d.notes.as_deref(),
            "Nothing recorded. Deviations found while building, and anything \
             <code>diff-reviewer</code> reported, belong in <code>notes.md</code>.",
        ));
    }

    body.push_str(&card(
        "Spec",
        &format!("<div class=prose>{}</div>", markdown(&d.body)),
    ));
    body.push_str(&prose_card(
        "Decisions",
        d.decisions.as_deref(),
        "No decision log. The rejected alternatives are the part git will not \
         remember for you.",
    ));
    if d.kind == Kind::Approval {
        body.push_str(&prose_card(
            "Open questions",
            d.open_questions.as_deref(),
            "None recorded.",
        ));
    }

    Page::new(
        format!("{} {}", d.id, d.title),
        format!("bevel review {}", d.id),
    )
    .subtitle(match d.kind {
        Kind::Approval => "Approval dossier — read this, then decide in a terminal",
        Kind::Close => "Close report — the contract, and what was built against it",
    })
    .body(body)
}

/// The card that says what is being asked and what to type. Always first: a
/// reviewer who reads nothing else should still know what the state is.
fn decision_card(d: &Dossier) -> String {
    let mut inner = format!(
        "<dl class=kv><dt>status</dt><dd>{}</dd>\
         <dt>created</dt><dd>{}</dd>\
         <dt>packages</dt><dd>{}</dd>\
         <dt>criteria</dt><dd>{}</dd>",
        pill(status_class(d.status), d.status.as_str()),
        escape(&d.created),
        if d.packages.is_empty() {
            "not yet decided — the plan chooses them".to_string()
        } else {
            escape(&d.packages.join(", "))
        },
        tier_counts(d),
    );
    match &d.approval {
        Some(a) => inner.push_str(&format!(
            "<dt>approved</dt><dd>{} by {}</dd><dt>hash</dt><dd><code>{}</code>{}</dd>",
            escape(&a.approved_at),
            escape(&a.approved_by),
            escape(short(&d.hash)),
            if d.verdict == Verdict::HashMismatch {
                format!(" {}", pill("bad", "changed since approval"))
            } else {
                String::new()
            }
        )),
        None => inner.push_str(&format!(
            "<dt>hash</dt><dd><code>{}</code> — this is what approval freezes</dd>",
            escape(short(&d.hash))
        )),
    }
    inner.push_str("</dl>");

    let (headline, command, class) = next_step(d);
    inner.push_str(&format!(
        "<p class=note>{headline}</p>\
         <div class=cmd><b>{}</b><span>you, in a terminal — this page cannot do it</span></div>",
        escape(&command)
    ));

    if !d.findings.is_empty() {
        let items: String = d
            .findings
            .iter()
            .map(|f| {
                format!(
                    "<li><code>{}</code> — {}</li>",
                    escape(f.rule),
                    escape(&f.message)
                )
            })
            .collect();
        inner.push_str(&format!(
            "<h3>{} finding{} from <code>bevel validate</code></h3><ul>{items}</ul>",
            d.findings.len(),
            if d.findings.len() == 1 { "" } else { "s" }
        ));
    }

    format!("<section class=\"card {class}\"><h2>Decision</h2>{inner}</section>\n")
}

/// What the reviewer should do next, in the order the pipeline cares about.
fn next_step(d: &Dossier) -> (String, String, &'static str) {
    if !d.findings.is_empty() {
        return (
            "This spec does not validate, so it cannot be approved yet.".into(),
            format!("bevel validate {}", d.id),
            "warn",
        );
    }
    match (d.kind, d.status, &d.verdict) {
        (_, _, Verdict::HashMismatch) => (
            "Approved once, then edited. The contract changed under the work — \
             read the diff before re-approving."
                .into(),
            format!("bevel approve {}", d.id),
            "warn",
        ),
        (Kind::Approval, Status::Draft, _) => (
            "Still a draft. Validation promotes it to review.".into(),
            format!("bevel validate {}", d.id),
            "",
        ),
        (Kind::Approval, Status::Review, _) => (
            "Waiting on you. Approving freezes the hash above; editing the spec \
             afterwards reopens the gate."
                .into(),
            format!("bevel approve {}", d.id),
            "",
        ),
        (Kind::Approval, _, _) => (
            "Approved and waiting for the active slot.".into(),
            format!("bevel start {}", d.id),
            "",
        ),
        (Kind::Close, Status::Done, _) => (
            "Closed. Kept as the record of what was agreed and what shipped.".into(),
            "bevel list --status done".to_string(),
            "",
        ),
        (Kind::Close, _, _) => close_step(d),
    }
}

fn close_step(d: &Dossier) -> (String, String, &'static str) {
    let judged = d.criteria.iter().filter(|c| c.tier == 'C').count();
    let mut parts = Vec::new();
    if d.pending_markers > 0 {
        parts.push(format!(
            "{} pending marker{} still to clear",
            d.pending_markers,
            if d.pending_markers == 1 { "" } else { "s" }
        ));
    }
    if judged > 0 {
        parts.push(format!(
            "{judged} tier C criteri{} only you can judge",
            if judged == 1 { "on" } else { "a" }
        ));
    }
    let headline = if parts.is_empty() {
        "Nothing outstanding here. <code>close</code> runs verification itself — \
         this report does not, so that opening it stays free."
            .to_string()
    } else {
        format!(
            "{}. <code>close</code> runs verification itself; this report does not.",
            parts.join(", ")
        )
    };
    (
        headline,
        format!("bevel close {}", d.id),
        if parts.is_empty() { "" } else { "warn" },
    )
}

fn criteria_card(d: &Dossier) -> String {
    if d.criteria.is_empty() {
        return card(
            "Acceptance criteria",
            "<p class=empty>None declared. A spec with no tier A or B criterion has \
             no stop condition, and <code>validate</code> rejects it.</p>",
        );
    }

    let rows: String = d
        .criteria
        .iter()
        .map(|c| {
            format!(
                "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
                pill(
                    &c.tier.to_ascii_lowercase().to_string(),
                    &format!("tier {}", c.tier)
                ),
                if c.tier == 'B' {
                    format!("<code>{}</code>", escape(&c.label))
                } else {
                    escape(&c.label)
                },
                c.evidence.pill(),
                c.evidence.detail(d.mockup.as_ref()),
            )
        })
        .collect();

    let mut note = String::new();
    let attached = d
        .criteria
        .iter()
        .filter(|c| matches!(c.evidence, Evidence::Pending(_)))
        .count();
    if d.kind == Kind::Close && attached != d.pending_markers {
        note = format!(
            "<p class=note>{} pending marker{} found in the repo, {attached} of them sitting on \
             a named criterion. The rest are on helpers, or on another spec's work.</p>",
            d.pending_markers,
            if d.pending_markers == 1 { "" } else { "s" }
        );
    }

    card(
        "Acceptance criteria",
        &format!(
            "<div class=scroll><table><tr><th>tier</th><th>criterion</th>\
             <th>state</th><th>where</th></tr>{rows}</table></div>{note}"
        ),
    )
}

fn prose_card(title: &str, text: Option<&str>, empty: &str) -> String {
    match text {
        Some(t) => card(title, &format!("<div class=prose>{}</div>", markdown(t))),
        None => card(title, &format!("<p class=empty>{empty}</p>")),
    }
}

fn tier_counts(d: &Dossier) -> String {
    let mut parts = Vec::new();
    for tier in ['A', 'B', 'C'] {
        let n = d.criteria.iter().filter(|c| c.tier == tier).count();
        if n > 0 {
            parts.push(pill(
                &tier.to_ascii_lowercase().to_string(),
                &format!("{n} × tier {tier}"),
            ));
        }
    }
    if parts.is_empty() {
        return "none".into();
    }
    parts.join(" ")
}

fn status_class(s: Status) -> &'static str {
    match s {
        Status::Draft | Status::Superseded => "mute",
        Status::Review => "c",
        Status::Approved | Status::Implementing => "b",
        Status::Done => "a",
    }
}

fn short(hash: &str) -> &str {
    let end = hash.len().min("sha256:".len() + 12);
    &hash[..end]
}

/// Build and write in one step; the caller only needs the path.
pub fn write(project: &Project, spec: &Spec, open: bool) -> Result<std::path::PathBuf> {
    let dossier = build(project, spec)?;
    let page = render(&dossier);
    html::write(project, &format!("review-{}.html", dossier.id), &page, open)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project() -> (tempfile::TempDir, Project) {
        let tmp = tempfile::tempdir().unwrap();
        crate::project::init(tmp.path(), false).unwrap();
        let p = Project::discover_from(tmp.path()).unwrap();
        (tmp, p)
    }

    fn spec_at(p: &Project, id: &str, status: Status, acceptance: &str) -> Spec {
        let dir = p.specs_dir().join(format!("{id}-example"));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("spec.md"),
            format!(
                "---\nid: '{id}'\ntitle: Example\nstatus: {}\nschema_version: 1\n\
                 created: '2026-08-05'\nacceptance:\n{acceptance}---\n\
                 # Example\n\n## Problem\n\nUploads restart from zero.\n",
                status.as_str()
            ),
        )
        .unwrap();
        Spec::load(&dir).unwrap()
    }

    #[test]
    fn the_view_follows_the_status_because_the_question_does() {
        assert_eq!(Kind::of(Status::Draft), Kind::Approval);
        assert_eq!(Kind::of(Status::Review), Kind::Approval);
        assert_eq!(Kind::of(Status::Approved), Kind::Approval);
        assert_eq!(Kind::of(Status::Implementing), Kind::Close);
        assert_eq!(Kind::of(Status::Done), Kind::Close);
    }

    #[test]
    fn a_dossier_states_the_command_and_never_offers_to_run_it() {
        let (_t, p) = project();
        let s = spec_at(&p, "0007", Status::Review, "- tier: B\n  cmd: 'true'\n");
        let html = render(&build(&p, &s).unwrap()).render();

        assert!(html.contains("bevel approve 0007"));
        assert!(html.contains("this page cannot do it"));
        for probe in ["<form", "<button", "onclick", "fetch("] {
            assert!(!html.contains(probe), "found `{probe}`");
        }
    }

    #[test]
    fn before_the_gate_a_named_test_reads_as_inert_not_as_a_failure() {
        let (_t, p) = project();
        let s = spec_at(
            &p,
            "0007",
            Status::Review,
            "- tier: A\n  test: conflict_prefers_local\n",
        );
        std::fs::write(
            s.dir.join("acceptance.rs"),
            "#[ignore = \"acceptance: 0007 pending\"]\nfn conflict_prefers_local() { todo!() }\n",
        )
        .unwrap();

        let d = build(&p, &s).unwrap();
        assert_eq!(d.criteria[0].evidence, Evidence::Named);
        assert!(render(&d).render().contains("named, inert"));
    }

    #[test]
    fn after_relocation_the_report_says_where_the_test_went_and_whether_it_is_live() {
        let (_t, p) = project();
        let s = spec_at(
            &p,
            "0007",
            Status::Implementing,
            "- tier: A\n  test: conflict_prefers_local\n- tier: A\n  test: empty_document_syncs\n",
        );
        let tests = p.root.join("crates/core/tests");
        std::fs::create_dir_all(&tests).unwrap();
        std::fs::write(
            tests.join("acceptance_0007.rs"),
            "#[test]\nfn conflict_prefers_local() { assert!(true) }\n\n\
             #[test]\n#[ignore = \"acceptance: 0007 pending\"]\nfn empty_document_syncs() { todo!() }\n",
        )
        .unwrap();

        let d = build(&p, &s).unwrap();
        assert_eq!(
            d.criteria[0].evidence,
            Evidence::Live("crates/core/tests/acceptance_0007.rs:2".into())
        );
        assert!(matches!(d.criteria[1].evidence, Evidence::Pending(_)));

        let html = render(&d).render();
        assert!(html.contains("crates/core/tests/acceptance_0007.rs:2"));
        assert!(html.contains("bevel close 0007"));
    }

    #[test]
    fn a_tier_c_criterion_links_to_the_mockup_section_it_names() {
        let (_t, p) = project();
        let s = spec_at(
            &p,
            "0007",
            Status::Review,
            "- tier: B\n  cmd: 'true'\n- tier: C\n  text: 'the banner matches mockup.html §2'\n",
        );
        std::fs::write(
            s.dir.join("mockup.html"),
            "<section id=\"s1\"><h2>§1 Empty</h2></section>\
             <section id=\"s2\"><h2>§2 Conflict banner</h2></section>",
        )
        .unwrap();

        let d = build(&p, &s).unwrap();
        assert_eq!(d.criteria[1].evidence, Evidence::Judged(Some(2)));
        assert_eq!(d.mockup.as_ref().unwrap().sections.len(), 2);

        let html = render(&d).render();
        assert!(html.contains("mockup.html#s2"), "{html}");
        assert!(html.contains("§2 Conflict banner"));
    }

    #[test]
    fn a_criterion_pointing_at_a_section_that_does_not_exist_is_flagged_not_linked() {
        let (_t, p) = project();
        let s = spec_at(
            &p,
            "0007",
            Status::Review,
            "- tier: B\n  cmd: 'true'\n- tier: C\n  text: 'matches mockup.html §9'\n",
        );
        std::fs::write(
            s.dir.join("mockup.html"),
            "<section id=\"s1\">Empty</section>",
        )
        .unwrap();

        let d = build(&p, &s).unwrap();
        assert_eq!(d.criteria[1].evidence, Evidence::Dangling(9));
        assert!(render(&d).render().contains("dangling reference"));
    }

    /// The spec that was edited after approval is the dangerous one: the work
    /// was done against a contract nobody agreed to.
    #[test]
    fn a_reopened_gate_is_the_headline_not_a_footnote() {
        let (_t, p) = project();
        let mut s = spec_at(&p, "0007", Status::Review, "- tier: B\n  cmd: 'true'\n");
        gate::approve(&p, &mut s, true).unwrap();

        s.body.push_str("\n## No-gos\n\nSomething new.\n");
        s.save().unwrap();
        let s = Spec::load(&s.dir).unwrap();

        let d = build(&p, &s).unwrap();
        assert_eq!(d.verdict, Verdict::HashMismatch);
        let html = render(&d).render();
        assert!(html.contains("changed since approval"));
        assert!(html.contains("class=\"card warn\""));
    }
}
