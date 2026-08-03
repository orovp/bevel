//! Deterministic spec checks (DESIGN.md §7).
//!
//! Every rule here is a lookup or a string match. None of them is a judgment,
//! which is the whole point: "is anything missing from this spec?" becomes an
//! exit code instead of an opinion.

use anyhow::Result;
use std::path::Path;

use crate::project::Project;
use crate::spec::{self, Criterion, DispositionAction, Spec, Status};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub rule: &'static str,
    pub message: String,
}

impl std::fmt::Display for Finding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.rule, self.message)
    }
}

pub fn validate(project: &Project, spec: &Spec) -> Result<Vec<Finding>> {
    let mut findings = Vec::new();

    if spec.front.title.trim().is_empty() {
        findings.push(Finding {
            rule: "title",
            message: "the spec has no title".into(),
        });
    }

    let dir_id: String = spec.slug().chars().take(4).collect();
    if dir_id != spec.front.id {
        findings.push(Finding {
            rule: "id",
            message: format!(
                "frontmatter id `{}` does not match directory `{}`",
                spec.front.id,
                spec.slug()
            ),
        });
    }

    // Subjective criteria are allowed; only subjective criteria are not.
    if !spec.front.acceptance.iter().any(|c| c.machine_verifiable()) {
        findings.push(Finding {
            rule: "acceptance/tier",
            message: "no tier A or B criterion — a spec needs at least one stop condition \
                      a machine can check"
                .into(),
        });
    }

    findings.extend(check_tier_a_tests_exist(spec)?);
    findings.extend(check_supersessions(project, spec)?);
    Ok(findings)
}

/// Every tier A criterion must name a test that actually exists in an
/// `acceptance.*` file.
///
/// A substring match rather than a parse, deliberately: the same rule then
/// works for `fn name()`, `test.todo('name')` and `xit('name')` without the
/// bevel needing a parser per language.
fn check_tier_a_tests_exist(spec: &Spec) -> Result<Vec<Finding>> {
    let names = spec.tier_a_tests();
    if names.is_empty() {
        return Ok(Vec::new());
    }

    let files = spec.acceptance_files()?;
    if files.is_empty() {
        return Ok(vec![Finding {
            rule: "acceptance/file",
            message: format!(
                "{} tier A criteria but no acceptance.* file in {}",
                names.len(),
                spec.slug()
            ),
        }]);
    }

    let mut haystack = String::new();
    for f in &files {
        haystack.push_str(&std::fs::read_to_string(f)?);
        haystack.push('\n');
    }

    Ok(names
        .iter()
        .filter(|n| !haystack.contains(**n))
        .map(|n| Finding {
            rule: "acceptance/missing-test",
            message: format!("tier A criterion `{n}` has no matching test in acceptance.*"),
        })
        .collect())
}

/// Supersession is a line-by-line reckoning with the old contract, not a
/// status flip: every tier A criterion of the superseded spec needs a
/// disposition, and dropping one needs a written reason.
fn check_supersessions(project: &Project, spec: &Spec) -> Result<Vec<Finding>> {
    let mut findings = Vec::new();
    for s in &spec.front.supersedes {
        let old = match spec::find(&project.specs_dir(), &s.id) {
            Ok(o) => o,
            Err(_) => {
                findings.push(Finding {
                    rule: "supersedes/unknown",
                    message: format!("supersedes `{}`, which does not exist", s.id),
                });
                continue;
            }
        };

        for test in old.tier_a_tests() {
            match s.dispositions.iter().find(|d| d.test == test) {
                None => findings.push(Finding {
                    rule: "supersedes/undisposed",
                    message: format!(
                        "no disposition for `{test}` from spec {} \
                         (inherited | replaced | dropped)",
                        s.id
                    ),
                }),
                Some(d) if d.action == DispositionAction::Dropped => {
                    if d.note.as_deref().map(str::trim).unwrap_or("").is_empty() {
                        findings.push(Finding {
                            rule: "supersedes/dropped-without-reason",
                            message: format!(
                                "`{test}` is dropped but has no note — deleting a passing \
                                 test is exactly the change that needs a written reason"
                            ),
                        });
                    }
                }
                Some(_) => {}
            }
        }

        for d in &s.dispositions {
            if d.action == DispositionAction::Replaced
                && !spec
                    .front
                    .acceptance
                    .iter()
                    .any(|c| matches!(c, Criterion::A { .. }))
            {
                findings.push(Finding {
                    rule: "supersedes/replaced-without-replacement",
                    message: format!(
                        "`{}` is marked replaced but this spec has no tier A criterion",
                        d.test
                    ),
                });
            }
        }
    }
    Ok(findings)
}

/// A clean validation promotes a draft to `review`; anything further along is
/// left alone.
pub fn promote_if_clean(spec: &mut Spec, findings: &[Finding]) -> Result<bool> {
    if findings.is_empty() && spec.front.status == Status::Draft {
        spec.front.status = Status::Review;
        spec.save()?;
        return Ok(true);
    }
    Ok(false)
}

/// Count of pending acceptance markers for a spec, searched across the repo so
/// the number stays right after `/implement` relocates the file into a package.
pub fn pending_markers(root: &Path, spec_id: &str) -> usize {
    let needle = format!("acceptance: {spec_id} pending");
    let mut count = 0;
    walk(root, 0, &mut |path| {
        if let Ok(text) = std::fs::read_to_string(path) {
            count += text.matches(&needle).count();
        }
    });
    count
}

const SKIP_DIRS: [&str; 6] = [".git", "target", "node_modules", "dist", ".venv", ".bevel"];

fn walk(dir: &Path, depth: usize, f: &mut impl FnMut(&Path)) {
    if depth > 12 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if path.is_dir() {
            if !SKIP_DIRS.contains(&name.as_str()) {
                walk(&path, depth + 1, f);
            }
        } else if is_texty(&name) {
            f(&path);
        }
    }
}

fn is_texty(name: &str) -> bool {
    const EXT: [&str; 9] = ["rs", "ts", "tsx", "js", "mjs", "md", "toml", "json", "html"];
    name.rsplit_once('.')
        .map(|(_, e)| EXT.contains(&e))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_spec(
        project: &Project,
        id: &str,
        slug: &str,
        front_extra: &str,
    ) -> std::path::PathBuf {
        let dir = project.specs_dir().join(format!("{id}-{slug}"));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("spec.md"),
            format!(
                "---\nid: '{id}'\ntitle: Example\nstatus: draft\nschema_version: 1\n\
                 created: '2026-08-02'\n{front_extra}---\n# Problem\n\nSomething.\n"
            ),
        )
        .unwrap();
        dir
    }

    fn project() -> (tempfile::TempDir, Project) {
        let tmp = tempfile::tempdir().unwrap();
        crate::project::init(tmp.path(), false).unwrap();
        let p = Project::discover_from(tmp.path()).unwrap();
        (tmp, p)
    }

    #[test]
    fn a_spec_with_only_subjective_criteria_is_rejected() {
        let (_t, p) = project();
        let dir = write_spec(
            &p,
            "0001",
            "example",
            "acceptance:\n- tier: C\n  text: it feels nice\n",
        );
        let spec = Spec::load(&dir).unwrap();
        let f = validate(&p, &spec).unwrap();
        assert!(f.iter().any(|f| f.rule == "acceptance/tier"), "{f:?}");
    }

    #[test]
    fn a_tier_a_criterion_needs_a_matching_test() {
        let (_t, p) = project();
        let dir = write_spec(
            &p,
            "0001",
            "example",
            "acceptance:\n- tier: A\n  test: does_the_thing\n",
        );
        let spec = Spec::load(&dir).unwrap();
        assert!(validate(&p, &spec)
            .unwrap()
            .iter()
            .any(|f| f.rule == "acceptance/file"));

        std::fs::write(dir.join("acceptance.rs"), "fn does_the_thing() {}\n").unwrap();
        assert!(validate(&p, &spec).unwrap().is_empty());

        std::fs::write(dir.join("acceptance.rs"), "fn something_else() {}\n").unwrap();
        assert!(validate(&p, &spec)
            .unwrap()
            .iter()
            .any(|f| f.rule == "acceptance/missing-test"));
    }

    #[test]
    fn the_same_rule_accepts_typescript_test_forms() {
        let (_t, p) = project();
        let dir = write_spec(
            &p,
            "0001",
            "example",
            "acceptance:\n- tier: A\n  test: syncs an empty document\n",
        );
        std::fs::write(
            dir.join("acceptance.spec.ts"),
            "test.todo('syncs an empty document');\n",
        )
        .unwrap();
        let spec = Spec::load(&dir).unwrap();
        assert!(validate(&p, &spec).unwrap().is_empty());
    }

    #[test]
    fn superseding_requires_a_disposition_for_every_old_criterion() {
        let (_t, p) = project();
        let old = write_spec(
            &p,
            "0001",
            "old",
            "acceptance:\n- tier: A\n  test: old_behaviour\n",
        );
        std::fs::write(old.join("acceptance.rs"), "fn old_behaviour() {}\n").unwrap();

        let new = write_spec(
            &p,
            "0002",
            "new",
            "supersedes:\n- id: '0001'\nacceptance:\n- tier: A\n  test: new_behaviour\n",
        );
        std::fs::write(new.join("acceptance.rs"), "fn new_behaviour() {}\n").unwrap();

        let spec = Spec::load(&new).unwrap();
        let f = validate(&p, &spec).unwrap();
        assert!(f.iter().any(|f| f.rule == "supersedes/undisposed"), "{f:?}");
    }

    #[test]
    fn dropping_a_criterion_requires_a_written_reason() {
        let (_t, p) = project();
        let old = write_spec(
            &p,
            "0001",
            "old",
            "acceptance:\n- tier: A\n  test: old_behaviour\n",
        );
        std::fs::write(old.join("acceptance.rs"), "fn old_behaviour() {}\n").unwrap();

        let new = write_spec(
            &p,
            "0002",
            "new",
            "supersedes:\n- id: '0001'\n  dispositions:\n  - test: old_behaviour\n    action: dropped\n\
             acceptance:\n- tier: B\n  cmd: 'true'\n",
        );
        let spec = Spec::load(&new).unwrap();
        assert!(validate(&p, &spec)
            .unwrap()
            .iter()
            .any(|f| f.rule == "supersedes/dropped-without-reason"));

        let with_reason = concat!(
            "supersedes:\n",
            "- id: '0001'\n",
            "  dispositions:\n",
            "  - test: old_behaviour\n",
            "    action: dropped\n",
            "    note: the feature was removed in 0002\n",
            "acceptance:\n",
            "- tier: B\n",
            "  cmd: 'true'\n",
        );
        let new2 = write_spec(&p, "0003", "new-with-reason", with_reason);
        let spec2 = Spec::load(&new2).unwrap();
        assert!(validate(&p, &spec2).unwrap().is_empty());
    }

    #[test]
    fn clean_validation_promotes_a_draft_to_review() {
        let (_t, p) = project();
        let dir = write_spec(
            &p,
            "0001",
            "example",
            "acceptance:\n- tier: B\n  cmd: 'true'\n",
        );
        let mut spec = Spec::load(&dir).unwrap();
        let f = validate(&p, &spec).unwrap();
        assert!(promote_if_clean(&mut spec, &f).unwrap());
        assert_eq!(Spec::load(&dir).unwrap().front.status, Status::Review);
    }

    #[test]
    fn pending_markers_are_counted_across_the_repo() {
        let (_t, p) = project();
        let dir = write_spec(
            &p,
            "0007",
            "example",
            "acceptance:\n- tier: B\n  cmd: 'true'\n",
        );
        std::fs::write(
            dir.join("acceptance.rs"),
            "#[ignore = \"acceptance: 0007 pending\"]\nfn a() {}\n\
             #[ignore = \"acceptance: 0007 pending\"]\nfn b() {}\n",
        )
        .unwrap();
        assert_eq!(pending_markers(&p.root, "0007"), 2);
        assert_eq!(pending_markers(&p.root, "0008"), 0);
    }
}
