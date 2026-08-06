//! `bevel status` (DESIGN.md §11).
//!
//! Output size must be independent of the number of specs. This is not
//! polish: the summary is injected at `SessionStart` against a fifteen-line
//! budget, so a listing that grows with the repo would quietly eat the context
//! budget the rest of the design depends on. `bevel list` is the enumeration.

use anyhow::Result;
use serde::Serialize;
use std::collections::BTreeMap;

use crate::project::Project;
use crate::spec::{self, Status};
use crate::validate;

/// Ids shown inline before collapsing to a count.
const INLINE_IDS: usize = 4;

#[derive(Debug, Serialize)]
pub struct Summary {
    pub inbox: usize,
    pub active: Option<Active>,
    pub counts: BTreeMap<String, usize>,
    pub review: Vec<String>,
    pub approved: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct Active {
    pub id: String,
    pub slug: String,
    /// Tier A criteria whose pending marker is gone.
    pub live: usize,
    pub total: usize,
}

pub fn build(project: &Project) -> Result<Summary> {
    let specs = spec::all(&project.specs_dir())?;
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for s in Status::ALL {
        counts.insert(s.as_str().to_string(), 0);
    }

    let mut active = None;
    let mut review = Vec::new();
    let mut approved = Vec::new();

    for s in &specs {
        *counts
            .entry(s.front.status.as_str().to_string())
            .or_insert(0) += 1;
        match s.front.status {
            Status::Review => review.push(s.front.id.clone()),
            Status::Approved => approved.push(s.front.id.clone()),
            Status::Implementing => {
                let p = validate::progress(project, s);
                active = Some(Active {
                    id: s.front.id.clone(),
                    slug: s.slug(),
                    live: p.live,
                    total: p.total,
                });
            }
            _ => {}
        }
    }

    Ok(Summary {
        inbox: crate::inbox::parse(&project.inbox_path())?
            .iter()
            .filter(|i| i.linked.is_none())
            .count(),
        active,
        counts,
        review,
        approved,
    })
}

impl Summary {
    /// Fixed-size rendering: five lines at a hundred specs, five at a thousand.
    pub fn render(&self) -> String {
        let mut lines = vec![
            format!("  inbox      {} unshaped", self.inbox),
            match &self.active {
                Some(a) => format!(
                    "  active     {:<24} {}/{} criteria live",
                    a.slug, a.live, a.total
                ),
                None => "  active     none".to_string(),
            },
        ];

        let waiting = if self.review.is_empty() {
            String::new()
        } else {
            format!("{:<20} <- waiting on you", ids(&self.review))
        };
        lines.push(format!("  review     {:<3} {waiting}", self.review.len()));
        lines.push(format!(
            "  approved   {:<3} {}",
            self.approved.len(),
            ids(&self.approved)
        ));
        lines.push(format!(
            "  done       {}",
            self.counts.get("done").copied().unwrap_or(0)
        ));

        lines
            .iter()
            .map(|l| format!("{}\n", l.trim_end()))
            .collect()
    }
}

impl Summary {
    /// Two lines at most, for injection at session start. The budget is fifteen
    /// lines and this shares it with nothing else, so it stays well under.
    pub fn render_brief(&self) -> String {
        let mut out = String::new();
        if let Some(a) = &self.active {
            out.push_str(&format!(
                "bevel: implementing {} ({}/{} criteria live)\n",
                a.slug, a.live, a.total
            ));
        }
        if !self.review.is_empty() {
            out.push_str(&format!(
                "bevel: {} spec(s) awaiting your approval: {}\n",
                self.review.len(),
                ids(&self.review)
            ));
        }
        // Silence is correct when there is nothing to say: an unconditional
        // banner every session is exactly the kind of noise §13 budgets against.
        out
    }
}

fn ids(list: &[String]) -> String {
    if list.is_empty() {
        return String::new();
    }
    if list.len() <= INLINE_IDS {
        return list.join(", ");
    }
    format!(
        "{}, +{} more",
        list[..INLINE_IDS].join(", "),
        list.len() - INLINE_IDS
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec_at(project: &Project, id: &str, status: Status) {
        let dir = project.specs_dir().join(format!("{id}-s{id}"));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("spec.md"),
            format!(
                "---\nid: '{id}'\ntitle: S{id}\nstatus: {}\nschema_version: 1\n\
                 created: '2026-08-02'\nacceptance:\n- tier: B\n  cmd: 'true'\n---\n# P\n\nx\n",
                status.as_str()
            ),
        )
        .unwrap();
    }

    #[test]
    fn output_stays_fixed_size_as_specs_accumulate() {
        let tmp = tempfile::tempdir().unwrap();
        crate::project::init(tmp.path(), false).unwrap();
        let p = Project::discover_from(tmp.path()).unwrap();

        for i in 1..=3 {
            spec_at(&p, &format!("{i:04}"), Status::Done);
        }
        let small = build(&p).unwrap().render();

        for i in 4..=120 {
            spec_at(&p, &format!("{i:04}"), Status::Done);
        }
        let large = build(&p).unwrap().render();

        assert_eq!(small.lines().count(), large.lines().count());
        assert_eq!(large.lines().count(), 5);
        assert!(large.contains("done       120"));
    }

    #[test]
    fn long_review_queues_collapse_to_a_count() {
        let tmp = tempfile::tempdir().unwrap();
        crate::project::init(tmp.path(), false).unwrap();
        let p = Project::discover_from(tmp.path()).unwrap();
        for i in 1..=9 {
            spec_at(&p, &format!("{i:04}"), Status::Review);
        }
        let s = build(&p).unwrap();
        assert_eq!(s.review.len(), 9);
        assert!(s.render().contains("+5 more"));
    }

    #[test]
    fn the_active_spec_reports_live_criteria_from_markers() {
        let tmp = tempfile::tempdir().unwrap();
        crate::project::init(tmp.path(), false).unwrap();
        let p = Project::discover_from(tmp.path()).unwrap();

        let dir = p.specs_dir().join("0001-active");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("spec.md"),
            "---\nid: '0001'\ntitle: Active\nstatus: implementing\nschema_version: 1\n\
             created: '2026-08-02'\nacceptance:\n- tier: A\n  test: sync_keeps_the_local_edit\n\
             - tier: A\n  test: sync_reports_a_conflict\n\
             - tier: A\n  test: sync_retries_once_on_timeout\n---\n# P\n\nx\n",
        )
        .unwrap();
        // Names long enough to mean something: a criterion called `one` is
        // found in INBOX.md's own "one per line", which is a substring match
        // working as documented and a fixture that was never realistic.
        std::fs::write(
            dir.join("acceptance.rs"),
            "fn sync_keeps_the_local_edit() {}\n\
             fn sync_reports_a_conflict() {}\n\
             #[ignore = \"acceptance: 0001 pending\"]\n\
             fn sync_retries_once_on_timeout() {}\n",
        )
        .unwrap();

        let a = build(&p).unwrap().active.unwrap();
        assert_eq!((a.live, a.total), (2, 3));
    }
}
