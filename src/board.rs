//! `bevel board` — the whole pipeline on one page (DESIGN.md §11).
//!
//! `bevel status` is fixed-size on purpose: it is injected at session start
//! against a fifteen-line budget, so it must not grow with the repo. That is a
//! constraint of the *channel*, not of the question. Sometimes you do want the
//! enumeration — with age, with progress, and above all with which gates have
//! quietly reopened while nobody was looking.
//!
//! This is that view, and it is deliberately not injected anywhere. If it ever
//! ends up in a prompt, the budget in §13 has been broken.

use anyhow::Result;
use std::path::PathBuf;

use crate::gate::{self, Verdict};
use crate::html::{self, card, escape, pill, Page};
use crate::project::Project;
use crate::spec::{self, Criterion, Spec, Status};
use crate::validate;

/// Columns, in pipeline order. `superseded` is not here: it is an outcome
/// rather than a stage, and a column of them would be dead weight on every
/// board forever.
const COLUMNS: [Status; 5] = [
    Status::Draft,
    Status::Review,
    Status::Approved,
    Status::Implementing,
    Status::Done,
];

pub struct Card {
    pub id: String,
    pub title: String,
    pub status: Status,
    /// Days since the spec was created. `None` when `created` will not parse.
    pub age: Option<i64>,
    pub live: usize,
    pub tier_a: usize,
    pub tier_b: usize,
    pub tier_c: usize,
    /// Approved, then edited. The one state on this board that is an alarm.
    pub reopened: bool,
    pub packages: Vec<String>,
}

pub struct Board {
    pub inbox: usize,
    pub cards: Vec<Card>,
    pub superseded: usize,
}

pub fn build(project: &Project) -> Result<Board> {
    let today = chrono::Utc::now().date_naive();
    let mut cards = Vec::new();
    let mut superseded = 0;

    for s in spec::all(&project.specs_dir())? {
        if s.front.status == Status::Superseded {
            superseded += 1;
            continue;
        }
        let tier_a = s.tier_a_tests().len();
        let pending = validate::pending_markers(&project.root, &s.front.id);
        cards.push(Card {
            age: chrono::NaiveDate::parse_from_str(&s.front.created, "%Y-%m-%d")
                .ok()
                .map(|d| (today - d).num_days()),
            live: tier_a.saturating_sub(pending),
            tier_a,
            tier_b: count(&s, 'B'),
            tier_c: count(&s, 'C'),
            reopened: matches!(s.front.status, Status::Approved | Status::Implementing)
                && gate::check(project, &s)? == Verdict::HashMismatch,
            id: s.front.id.clone(),
            title: s.front.title.clone(),
            status: s.front.status,
            packages: s.front.packages.clone(),
        });
    }

    Ok(Board {
        inbox: crate::inbox::parse(&project.inbox_path())?
            .iter()
            .filter(|i| i.linked.is_none())
            .count(),
        cards,
        superseded,
    })
}

fn count(s: &Spec, tier: char) -> usize {
    s.front
        .acceptance
        .iter()
        .filter(|c| match c {
            Criterion::A { .. } => tier == 'A',
            Criterion::B { .. } => tier == 'B',
            Criterion::C { .. } => tier == 'C',
        })
        .count()
}

pub fn render(b: &Board) -> Page {
    let reopened: Vec<&Card> = b.cards.iter().filter(|c| c.reopened).collect();
    let waiting = b
        .cards
        .iter()
        .filter(|c| c.status == Status::Review)
        .count();

    let mut body = String::new();

    // An alarm belongs above the thing it is about. A gate that reopened means
    // work is being done against a contract nobody agreed to.
    if !reopened.is_empty() {
        let items: String = reopened
            .iter()
            .map(|c| {
                format!(
                    "<li><b>{}</b> {} — approved, then edited</li>",
                    escape(&c.id),
                    escape(&c.title)
                )
            })
            .collect();
        body.push_str(&format!(
            "<section class=\"card warn\"><h2>Gates reopened</h2><ul>{items}</ul>\
             <p class=note>Re-approve before any more work goes in: \
             <code>bevel approve &lt;id&gt;</code>.</p></section>\n"
        ));
    }

    body.push_str(&card(
        "Where things stand",
        &format!(
            "{}<p class=note>{}</p>",
            html::stacked(&[
                ("unshaped ideas", b.inbox),
                (
                    "in flight",
                    b.cards.iter().filter(|c| in_flight(c.status)).count()
                ),
                (
                    "done",
                    b.cards.iter().filter(|c| c.status == Status::Done).count()
                ),
            ]),
            if waiting > 0 {
                format!(
                    "{waiting} spec{} waiting on you. Nothing moves past review \
                     without a human, which is the point.",
                    if waiting == 1 { "" } else { "s" }
                )
            } else {
                "Nothing is waiting on your approval.".to_string()
            }
        ),
    ));

    let columns: String = COLUMNS
        .iter()
        .map(|status| {
            let cards: Vec<&Card> = b.cards.iter().filter(|c| c.status == *status).collect();
            format!(
                "<div class=col><h3>{} <span class=count>{}</span></h3>{}</div>",
                escape(status.as_str()),
                cards.len(),
                if cards.is_empty() {
                    "<p class=empty>—</p>".to_string()
                } else {
                    cards.iter().map(|c| spec_card(c)).collect()
                }
            )
        })
        .collect();
    body.push_str(&format!(
        "<section class=card><h2>Pipeline</h2><div class=board>{columns}</div>{}</section>\n",
        if b.superseded > 0 {
            format!(
                "<p class=note>{} superseded spec{} not shown; they are an outcome, \
                 not a stage.</p>",
                b.superseded,
                if b.superseded == 1 { "" } else { "s" }
            )
        } else {
            String::new()
        }
    ));

    Page::new("Pipeline", "bevel board")
        .subtitle("The enumeration `status` deliberately refuses to be")
        .body(body)
}

fn in_flight(s: Status) -> bool {
    matches!(
        s,
        Status::Draft | Status::Review | Status::Approved | Status::Implementing
    )
}

fn spec_card(c: &Card) -> String {
    let mut meta = Vec::new();
    if let Some(days) = c.age {
        meta.push(match days {
            0 => "today".to_string(),
            1 => "1 day old".to_string(),
            d => format!("{d} days old"),
        });
    }
    if !c.packages.is_empty() {
        meta.push(escape(&c.packages.join(", ")));
    }

    // Progress is only meaningful once the work has started; before that the
    // tier counts say more, and a 0/5 bar on an approved spec reads as failure.
    let progress = if c.status == Status::Implementing && c.tier_a > 0 {
        format!(
            "<div class=progress><i style=\"width:{:.0}%\"></i></div>\
             <span class=meterlabel>{}/{} criteria live</span>",
            c.live as f64 / c.tier_a as f64 * 100.0,
            c.live,
            c.tier_a
        )
    } else {
        [('A', c.tier_a), ('B', c.tier_b), ('C', c.tier_c)]
            .iter()
            .filter(|(_, n)| *n > 0)
            .map(|(t, n)| pill(&t.to_ascii_lowercase().to_string(), &format!("{n} × {t}")))
            .collect::<Vec<_>>()
            .join(" ")
    };

    format!(
        "<article class=\"spec{}\"><b>{}</b> {}<div class=meta>{}</div>{progress}</article>",
        if c.reopened { " reopened" } else { "" },
        escape(&c.id),
        escape(&c.title),
        meta.join(" · ")
    )
}

pub fn write(project: &Project, open: bool) -> Result<PathBuf> {
    let page = render(&build(project)?);
    html::write(project, "board.html", &page, open)
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

    fn add(p: &Project, id: &str, status: Status, created: &str, acceptance: &str) -> Spec {
        let dir = p.specs_dir().join(format!("{id}-example"));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("spec.md"),
            format!(
                "---\nid: '{id}'\ntitle: Spec {id}\nstatus: {}\nschema_version: 1\n\
                 created: '{created}'\nacceptance:\n{acceptance}---\n# T\n\n## Problem\n\nx.\n",
                status.as_str()
            ),
        )
        .unwrap();
        Spec::load(&dir).unwrap()
    }

    #[test]
    fn every_stage_gets_a_column_even_when_it_is_empty() {
        let (_t, p) = project();
        add(
            &p,
            "0001",
            Status::Review,
            "2026-08-01",
            "- tier: B\n  cmd: 'true'\n",
        );
        let html = render(&build(&p).unwrap()).render();
        for stage in ["draft", "review", "approved", "implementing", "done"] {
            assert!(
                html.contains(&format!(">{stage} ")),
                "no column for {stage}"
            );
        }
        assert!(html.contains("1 spec waiting on you"));
    }

    #[test]
    fn progress_is_shown_only_once_the_work_has_started() {
        let (_t, p) = project();
        let s = add(
            &p,
            "0001",
            Status::Implementing,
            "2026-08-01",
            "- tier: A\n  test: one\n- tier: A\n  test: two\n",
        );
        std::fs::write(
            s.dir.join("acceptance.rs"),
            "#[ignore = \"acceptance: 0001 pending\"]\nfn two() {}\nfn one() {}\n",
        )
        .unwrap();
        assert!(render(&build(&p).unwrap())
            .render()
            .contains("1/2 criteria live"));

        // An approved spec shows its shape instead, not an alarming 0/2.
        let (_t2, p2) = project();
        add(
            &p2,
            "0002",
            Status::Approved,
            "2026-08-01",
            "- tier: A\n  test: one\n- tier: C\n  text: reads well\n",
        );
        let html = render(&build(&p2).unwrap()).render();
        assert!(!html.contains("criteria live"));
        assert!(html.contains("1 × A"));
        assert!(html.contains("1 × C"));
    }

    #[test]
    fn a_gate_that_reopened_is_raised_above_the_board_not_buried_in_it() {
        let (_t, p) = project();
        let mut s = add(
            &p,
            "0001",
            Status::Review,
            "2026-08-01",
            "- tier: B\n  cmd: 'true'\n",
        );
        gate::approve(&p, &mut s, true).unwrap();
        s.body.push_str("\nchanged after approval\n");
        s.save().unwrap();

        let b = build(&p).unwrap();
        assert!(b.cards[0].reopened);
        let html = render(&b).render();
        assert!(html.contains("Gates reopened"));
        assert!(html.find("<h2>Gates reopened</h2>") < html.find("<h2>Pipeline</h2>"));
    }

    #[test]
    fn an_unparseable_creation_date_costs_the_age_and_nothing_else() {
        let (_t, p) = project();
        add(
            &p,
            "0001",
            Status::Draft,
            "whenever",
            "- tier: B\n  cmd: 'true'\n",
        );
        let b = build(&p).unwrap();
        assert_eq!(b.cards[0].age, None);
        assert!(render(&b).render().contains("Spec 0001"));
    }
}
