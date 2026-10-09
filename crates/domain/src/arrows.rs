//! The arrows between the records of `docs/work/`: each one "this record before that one", written as `after` on the
//! later record or as `until` on the earlier one.
//!
//! An arrow is checked against how each end closed. The later record closes after the earlier one; a record that waits
//! for a dropped one, or is bounded by one, has to be pointed elsewhere, as it would otherwise wait for what never
//! comes. The arrows and the parents together make one graph, which has no cycle: a parent closes after its children,
//! which is an arrow from each child to its parent.

use std::collections::{BTreeMap, BTreeSet};

use crate::bundle::{Problems, Work};
use crate::schema::{Arrows, ClosedAs};

/// A record of `docs/work/` as the arrows see it.
#[derive(Debug, Clone, Copy)]
pub struct Record<'a> {
    pub title: &'a str,
    pub milestone: bool,
    /// `None` while the record is open
    pub closed_as: Option<ClosedAs>,
    pub arrows: &'a Arrows,
    pub parent: Option<&'a String>,
}

impl Record<'_> {
    pub fn is_open(&self) -> bool {
        self.closed_as.is_none()
    }
}

/// Every record of `docs/work/` that passes, by file name: the specs, work items and milestones.
pub fn records(work: &Work) -> BTreeMap<&str, Record<'_>> {
    let specs = work.specs.iter().map(|(name, (spec, _))| {
        (
            name.as_str(),
            Record {
                title: &spec.title,
                milestone: false,
                closed_as: spec.progress.closed_as(),
                arrows: &spec.arrows,
                parent: spec.parent.as_ref(),
            },
        )
    });
    let items = work.items.iter().map(|(name, (item, _))| {
        (
            name.as_str(),
            Record {
                title: &item.title,
                milestone: false,
                closed_as: item.progress.closed_as(),
                arrows: &item.arrows,
                parent: item.parent.as_ref(),
            },
        )
    });
    let milestones = work.milestones.iter().map(|(name, (milestone, _))| {
        (
            name.as_str(),
            Record {
                title: &milestone.title,
                milestone: true,
                closed_as: milestone.progress.closed_as(),
                arrows: &milestone.arrows,
                parent: None,
            },
        )
    });
    specs.chain(items).chain(milestones).collect()
}

/// One arrow, from the file name of the earlier record to that of the later one, with where it is written: each as the
/// record's file name and the field (`after` or `until`). Written in more than one place, it is written twice.
pub type Written = BTreeMap<(String, String), Vec<(String, &'static str)>>;

/// Every arrow whose two ends are records that pass.
pub fn arrows(work: &Work) -> Written {
    let records = records(work);
    let mut out = Written::new();
    for (name, record) in &records {
        for slug in &record.arrows.after {
            let before = format!("{slug}.md");
            if records.contains_key(before.as_str()) {
                out.entry((before, name.to_string()))
                    .or_default()
                    .push((name.to_string(), "after"));
            }
        }
        for slug in &record.arrows.until {
            let after = format!("{slug}.md");
            if records.contains_key(after.as_str()) {
                out.entry((name.to_string(), after))
                    .or_default()
                    .push((name.to_string(), "until"));
            }
        }
    }
    out
}

/// File name -> why, for the records whose arrows name what is not a record that passes: nothing, a guide, the record
/// itself, or a record left out of the index files.
pub fn unresolved(work: &Work) -> Problems {
    let records = records(work);
    let mut out = Problems::new();
    for (name, record) in &records {
        let named = [
            ("after", &record.arrows.after),
            ("until", &record.arrows.until),
        ];
        let why = named.into_iter().find_map(|(field, slugs)| {
            slugs.iter().find_map(|slug| {
                let file = format!("{slug}.md");
                let why = if file == *name {
                    "names the record itself".to_string()
                } else if records.contains_key(file.as_str()) {
                    return None;
                } else if work.problems.contains_key(&format!("work/{file}")) {
                    format!("{slug} is left out of the index files itself: fix it first")
                } else {
                    format!("names no record in docs/work/: {slug} (a guide is not a record)")
                };
                Some(format!("{field}: {why}"))
            })
        });
        if let Some(why) = why {
            out.insert(name.to_string(), why);
        }
    }
    out
}

/// What breaks the arrows, in the order of the records: an arrow written twice, an arrow whose ends closed out of
/// order, and a cycle of arrows and parents.
pub fn problems(work: &Work) -> Vec<String> {
    let records = records(work);
    let written = arrows(work);
    let slug = |name: &str| name.trim_end_matches(".md").to_string();
    let mut found = Vec::new();
    for ((before, after), places) in &written {
        if places.len() > 1 {
            let places: Vec<String> = places
                .iter()
                .map(|(name, field)| format!("the {field} of work/{name}"))
                .collect();
            found.push(format!(
                "work/{after}: the arrow from {} to {} is written twice, in {}: keep one",
                slug(before),
                slug(after),
                places.join(" and ")
            ));
        }
        let (first, then) = (records[before.as_str()], records[after.as_str()]);
        match (first.closed_as, then.closed_as) {
            (None, Some(ClosedAs::Done)) => found.push(format!(
                "work/{after}: closed as done while {}, which comes before it, is open. Close {} first (as \
                 dropped, if it was), or reopen this one",
                slug(before),
                slug(before)
            )),
            (None, Some(ClosedAs::Dropped)) => found.push(format!(
                "work/{before}: comes before {}, which was dropped. Point the arrow at another record, or remove it",
                slug(after)
            )),
            (Some(ClosedAs::Dropped), None) => found.push(format!(
                "work/{after}: comes after {}, which was dropped, and would wait for what never comes. Remove the \
                 arrow, or point it at another record",
                slug(before)
            )),
            _ => {}
        }
    }
    found.extend(cycles(&records, &written).into_iter().map(|cycle| {
        let names: Vec<String> = cycle.iter().map(|name| format!("work/{name}")).collect();
        format!(
            "{}: wait on each other, through their arrows and parents, so none can close first. Remove an arrow",
            names.join(", ")
        )
    }));
    found
}

/// The open specs and work items that have no parent and nothing after them, each with what to write. Every other open
/// record reaches a milestone by its parent or its arrows: an open parent is itself held by its own parent or by what
/// comes after it, the arrows make no cycle, and a later record is open while an earlier one is (or the arrow fails).
/// So the work never waits on nothing, and a milestone cannot close while work bounded by it is open.
pub fn unbounded(work: &Work) -> Vec<String> {
    let records = records(work);
    let written = arrows(work);
    records
        .iter()
        .filter(|(name, record)| {
            record.is_open()
                && !record.milestone
                && record.parent.is_none()
                && !written.keys().any(|(before, _)| before == *name)
        })
        .map(|(name, _)| {
            format!(
                "work/{name}: has no parent and nothing after it, so nothing says by when it is done: name the \
                 moment it waits for in until (a milestone: until: [<slug>]), or give it a parent"
            )
        })
        .collect()
}

/// The sets of records that wait on each other, each sorted, through the arrows and the parents: a child comes before
/// its parent. Found by which records each one reaches, which is enough for the size of a project's records.
fn cycles(records: &BTreeMap<&str, Record<'_>>, written: &Written) -> Vec<Vec<String>> {
    let mut next: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for (before, after) in written.keys() {
        next.entry(before.as_str())
            .or_default()
            .insert(after.as_str());
    }
    for (name, record) in records {
        if let Some(parent) = record.parent {
            let parent = format!("{parent}.md");
            if let Some((parent, _)) = records.get_key_value(parent.as_str()) {
                next.entry(name).or_default().insert(parent);
            }
        }
    }
    let reached = |from: &str| -> BTreeSet<&str> {
        let mut seen = BTreeSet::new();
        let mut todo: Vec<&str> = next.get(from).into_iter().flatten().copied().collect();
        while let Some(name) = todo.pop() {
            if seen.insert(name) {
                todo.extend(next.get(name).into_iter().flatten().copied());
            }
        }
        seen
    };
    let reach: BTreeMap<&str, BTreeSet<&str>> =
        records.keys().map(|name| (*name, reached(name))).collect();
    let mut out: Vec<Vec<String>> = Vec::new();
    for (name, from) in &reach {
        if !from.contains(name)
            || out
                .iter()
                .any(|cycle| cycle.iter().any(|known| known == name))
        {
            continue;
        }
        let cycle: Vec<String> = from
            .iter()
            .filter(|other| reach[*other].contains(name))
            .map(|other| other.to_string())
            .collect();
        out.push(cycle);
    }
    out
}

/// What an open record waits for, and what waits for it, as the index shows it.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Waiting {
    /// The earlier records still open, by file name
    pub before: Vec<String>,
    /// Whether it has earlier records, and every one of them is done
    pub ready: bool,
    /// The later records still open, by file name
    pub until: Vec<String>,
}

/// File name -> what each open record waits for. A record with no arrow is not in it.
pub fn waiting(work: &Work) -> BTreeMap<String, Waiting> {
    let records = records(work);
    let mut out: BTreeMap<String, Waiting> = BTreeMap::new();
    let written = arrows(work);
    let mut before_all: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for (before, after) in written.keys() {
        before_all.entry(after).or_default().push(before);
        if records[after.as_str()].is_open() && records[before.as_str()].is_open() {
            out.entry(before.clone())
                .or_default()
                .until
                .push(after.clone());
            out.entry(after.clone())
                .or_default()
                .before
                .push(before.clone());
        }
    }
    for (name, befores) in before_all {
        if !records[name].is_open() {
            continue;
        }
        let ready = befores
            .iter()
            .all(|before| records[before].closed_as == Some(ClosedAs::Done));
        if ready {
            out.entry(name.to_string()).or_default().ready = true;
        }
    }
    out.retain(|name, _| records[name.as_str()].is_open());
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bundle::{Docs, WORK_RULES, render_work, work};

    /// A record of `kind` titled after its slug: open (`stable`), `done` or `dropped`, with `extra` frontmatter.
    fn record(kind: &str, slug: &str, how: &str, extra: &str) -> (String, String) {
        let (kind, fields, body) = match kind {
            "spec" => ("Spec", "", "# Goals\n\nX.\n"),
            "milestone" => ("Milestone", "", "# Condition\n\nThe tag is pushed.\n"),
            _ => (
                "Work Item",
                "filed: 2026-10-01\nverified: {by: human:a, at: 2026-10-01T10:00:00+09:00}\n",
                "# Trigger\n\nX.\n\n# State\n\nNot yet.\n\n# Details\n\n[log](/log.md)\n",
            ),
        };
        let status = match how {
            // An open work item without a parent is a draft: nobody sorted it
            "open" if kind == "Work Item" && !extra.contains("parent:") => {
                "status: draft".to_string()
            }
            "open" => "status: stable".to_string(),
            closed => format!("status: deprecated\nprogress: {closed}"),
        };
        let resolution = if how == "open" {
            ""
        } else {
            "# Resolution\n\nClosed.\n\n"
        };
        (
            format!("{slug}.md"),
            format!(
                "---\ntype: {kind}\ntitle: {slug}\ndescription: D.\ntags: [a]\n{status}\n{extra}{fields}---\n\n\
                 {resolution}{body}"
            ),
        )
    }

    fn read(records: Vec<(String, String)>) -> Work {
        let docs: Docs = records
            .into_iter()
            .chain([("rules.md".to_string(), WORK_RULES.to_string())])
            .collect();
        work(&docs, &["a".to_string()])
    }

    #[test]
    fn an_arrow_names_a_record_that_passes() {
        let cases = [
            (
                "nothing",
                "after: [nowhere]\n",
                "after: names no record in docs/work/: nowhere",
            ),
            (
                "a guide",
                "until: [rules]\n",
                "until: names no record in docs/work/: rules",
            ),
            ("itself", "until: [p]\n", "until: names the record itself"),
            (
                "a broken record",
                "after: [broken]\n",
                "after: broken is left out of the index files itself",
            ),
            // Left out for its own arrow in the same read: named in turn
            (
                "a record left out for its arrow",
                "after: [lost]\n",
                "after: lost is left out of the index files itself",
            ),
        ];
        for (name, extra, said) in cases {
            let all = read(vec![
                record("item", "p", "open", extra),
                (
                    "broken.md".to_string(),
                    "---\ntype: Spec\ntitle: B\n---\n".to_string(),
                ),
                record("item", "lost", "open", "after: [nowhere]\n"),
            ]);
            assert!(!all.items.contains_key("p.md"), "{name}: still listed");
            let why = all
                .problems
                .get("work/p.md")
                .map(String::as_str)
                .unwrap_or("");
            assert!(why.starts_with(said), "{name}: {why:?}");
        }
        // Any record that passes may be named: a milestone, a spec or a work item
        let all = read(vec![
            record("milestone", "release", "open", ""),
            record("spec", "design", "open", "until: [release]\n"),
            record(
                "item",
                "step",
                "open",
                "after: [design]\nuntil: [release]\n",
            ),
        ]);
        assert!(all.problems.is_empty(), "{:?}", all.problems);
        assert_eq!(problems(&all), Vec::<String>::new());
    }

    #[test]
    fn an_arrow_is_written_once() {
        let all = read(vec![
            record("item", "first", "open", "until: [second]\n"),
            record("item", "second", "open", "after: [first]\n"),
        ]);
        assert_eq!(
            problems(&all),
            [
                "work/second.md: the arrow from first to second is written twice, in the until of work/first.md and \
              the after of work/second.md: keep one"
            ]
        );
        // Written on either side, it is the same arrow
        for (first, second) in [("until: [second]\n", ""), ("", "after: [first]\n")] {
            let all = read(vec![
                record("item", "first", "open", first),
                record("item", "second", "open", second),
            ]);
            assert_eq!(
                arrows(&all).keys().collect::<Vec<_>>(),
                [&("first.md".to_string(), "second.md".to_string())]
            );
        }
    }

    #[test]
    fn the_later_record_closes_after_the_earlier_one() {
        // (how the earlier one closed, how the later one did, what is said, or nothing)
        let cases = [
            (
                "open",
                "done",
                Some("work/later.md: closed as done while earlier, which comes before it, is open"),
            ),
            (
                "open",
                "dropped",
                Some("work/earlier.md: comes before later, which was dropped"),
            ),
            (
                "dropped",
                "open",
                Some("work/later.md: comes after earlier, which was dropped"),
            ),
            ("done", "open", None),
            ("done", "done", None),
            ("done", "dropped", None),
            ("dropped", "dropped", None),
            ("dropped", "done", None),
            ("open", "open", None),
        ];
        for (earlier, later, said) in cases {
            let all = read(vec![
                record("item", "earlier", earlier, ""),
                record("milestone", "later", later, "after: [earlier]\n"),
            ]);
            assert!(
                all.problems.is_empty(),
                "{earlier}, {later}: {:?}",
                all.problems
            );
            let found = problems(&all);
            match said {
                Some(said) => assert!(
                    found.len() == 1 && found[0].starts_with(said),
                    "{earlier}, {later}: {found:?}"
                ),
                None => assert_eq!(found, Vec::<String>::new(), "{earlier}, {later}"),
            }
        }
    }

    #[test]
    fn the_arrows_and_the_parents_make_no_cycle() {
        let cases = [
            (
                "two arrows",
                vec![
                    record("item", "a", "open", "after: [b]\n"),
                    record("item", "b", "open", "after: [a]\n"),
                ],
                "work/a.md, work/b.md: wait on each other",
            ),
            (
                "three arrows",
                vec![
                    record("item", "a", "open", "until: [b]\n"),
                    record("item", "b", "open", "until: [c]\n"),
                    record("item", "c", "open", "until: [a]\n"),
                ],
                "work/a.md, work/b.md, work/c.md: wait on each other",
            ),
            // No cycle in the arrows alone: a parent closes after its children, which is an arrow from the child
            (
                "a parent until its own part",
                vec![
                    record("spec", "epic", "open", "until: [part]\n"),
                    record("spec", "part", "open", "parent: epic\n"),
                ],
                "work/epic.md, work/part.md: wait on each other",
            ),
            (
                "a work item after the spec it is a part of",
                vec![
                    record("spec", "design", "open", ""),
                    record("item", "step", "open", "parent: design\nafter: [design]\n"),
                ],
                "work/design.md, work/step.md: wait on each other",
            ),
        ];
        for (name, records, said) in cases {
            let all = read(records);
            assert!(all.problems.is_empty(), "{name}: {:?}", all.problems);
            let found = problems(&all);
            assert!(
                found.len() == 1 && found[0].starts_with(said),
                "{name}: {found:?}"
            );
        }
        // A work item before its spec is what a parent means already: no cycle
        let all = read(vec![
            record("spec", "design", "open", ""),
            record("item", "step", "open", "parent: design\nuntil: [design]\n"),
        ]);
        assert_eq!(problems(&all), Vec::<String>::new());
    }

    #[test]
    fn an_open_record_without_a_parent_has_something_after_it() {
        let all = read(vec![
            record("milestone", "release", "open", ""),
            // Nothing holds these two
            record("item", "loose", "open", ""),
            record("spec", "loose-spec", "open", ""),
            // Its own until, or another's after, or a parent
            record("item", "bounded", "open", "until: [release]\n"),
            record("item", "awaited", "open", ""),
            record(
                "item",
                "waiting",
                "open",
                "after: [awaited]\nuntil: [release]\n",
            ),
            record("spec", "epic", "open", "until: [release]\n"),
            record("spec", "part", "open", "parent: epic\n"),
            record("item", "step", "open", "parent: part\n"),
            // A closed record waits for nothing, and a milestone is what the work waits for
            record("item", "done-step", "done", ""),
            record("milestone", "later", "open", ""),
        ]);
        assert!(all.problems.is_empty(), "{:?}", all.problems);
        let found = unbounded(&all);
        assert_eq!(
            found
                .iter()
                .map(|line| line.split(':').next().unwrap())
                .collect::<Vec<_>>(),
            ["work/loose-spec.md", "work/loose.md"]
        );
        assert!(
            found[0].ends_with("name the moment it waits for in until (a milestone: until: [<slug>]), or give it a parent"),
            "{found:?}"
        );
    }

    #[test]
    fn the_index_says_what_an_open_record_waits_for() {
        let all = read(vec![
            record("milestone", "release", "open", ""),
            record("spec", "design", "done", "until: [release]\n"),
            record(
                "item",
                "build",
                "open",
                "after: [design]\nuntil: [release]\n",
            ),
            record("item", "docs", "open", "after: [build]\nuntil: [release]\n"),
            record("item", "done-step", "done", "until: [release]\n"),
        ]);
        assert!(all.problems.is_empty(), "{:?}", all.problems);
        assert_eq!(problems(&all), Vec::<String>::new());
        let waiting = waiting(&all);
        assert_eq!(
            waiting["build.md"],
            Waiting {
                before: vec![],
                ready: true,
                until: vec!["docs.md".into(), "release.md".into()]
            }
        );
        assert_eq!(
            waiting["docs.md"],
            Waiting {
                before: vec!["build.md".into()],
                ready: false,
                until: vec!["release.md".into()]
            }
        );
        // A closed record waits for nothing
        assert!(!waiting.contains_key("design.md") && !waiting.contains_key("done-step.md"));
        let index = render_work(&all, &["a".to_string()]);
        let line = |slug: &str| {
            index
                .lines()
                .find(|line| line.starts_with(&format!("* [{slug}]({slug}.md)")))
                .unwrap()
                .to_string()
        };
        assert!(
            line("build").ends_with(" | Ready. | Until: [docs](docs.md), [release](release.md)"),
            "{index}"
        );
        assert!(
            line("docs")
                .ends_with(" | Waits for: [build](build.md) | Until: [release](release.md)"),
            "{index}"
        );
        // A milestone counts what it waits for
        assert!(
            line("release").ends_with(" | Waits for 2 open records."),
            "{index}"
        );
    }
}
