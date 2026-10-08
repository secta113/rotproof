//! Moving the records of a project made before 0.3.0 into `docs/work/`, as `rotproof init` does once when it upgrades
//! the project to 0.3.0 or later.
//!
//! Before 0.3.0 the specs were in `docs/specs/` and the backlog items in `docs/backlog/`. The move is what a machine
//! can do without reading the records: each document to `docs/work/` under its own name, every link that reached one
//! pointed at its new place, `type: Backlog Item` as `Work Item`, `epic` as `parent`, an open item with no parent as a
//! draft (an open item was any item not closed; a stable one is now one sorted into a spec), and the deadline of a
//! closed item, which binds nothing any more, dropped. What needs a person stays and is said: how each closed record
//! closed (`closed_as`), and the deadline of each open item, which becomes a milestone named in `until`.
//!
//! The move is planned whole before anything is written: a name in both directories, or a file the move would not
//! know where to put, stops it with nothing changed. It cannot be declined: Rotproof from 0.3.0 reads no other place.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use regex::{Captures, Regex};

use utils::paths::{file_name, normalize, parent};

/// The version that brings the move.
pub const VERSION: &str = "0.3.0";
/// The directories the records were in, from the root.
pub const OLD: [&str; 2] = ["docs/backlog", "docs/specs"];
/// Where they go, from the root.
pub const WORK: &str = "docs/work";
/// The files Rotproof wrote in the old directories, which it writes again in `docs/work/`: removed, not moved.
const GENERATED: [&str; 2] = ["index.md", "rules.md"];

// The target of an inline link or an image, `](target "title")`, and of a reference definition, `[label]: target`
static INLINE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(\]\(\s*)(<[^>\n]*>|[^)\s]+)").unwrap());
static DEFINITION: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)^( {0,3}\[[^\]\n]+\]:[ \t]*)(<[^>\n]*>|\S+)").unwrap());

/// Whether an upgrade from `files` to `running` moves the records: it crosses the version that brings the move.
pub fn due(files: &str, running: &str) -> bool {
    let at = crate::upgrade::version(VERSION);
    crate::upgrade::version(files) < at && at <= crate::upgrade::version(running)
}

/// What the move writes, removes and leaves to a person.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Plan {
    /// Path from the root -> its new text: each moved record at its new place, and each other document whose links
    /// changed
    pub writes: BTreeMap<String, String>,
    /// Files to remove, from the root: the old place of each record, and the files Rotproof wrote there
    pub removes: Vec<String>,
    /// Directories to remove once empty, from the root
    pub dirs: Vec<String>,
    /// How many records moved
    pub moved: usize,
    /// What a person does next, each as one line
    pub by_hand: Vec<String>,
}

/// The move, from every file under `docs/` (path from the root -> text; a file that is not markdown with any text),
/// or why it cannot be made. Nothing to move gives an empty plan.
pub fn plan(files: &BTreeMap<String, String>) -> Result<Plan, String> {
    let mut plan = Plan::default();
    let mut moves: BTreeMap<String, String> = BTreeMap::new();
    let mut stopped = Vec::new();
    for path in files.keys() {
        let Some(dir) = OLD.iter().find(|dir| path.starts_with(&format!("{dir}/"))) else {
            continue;
        };
        let name = file_name(path);
        if parent(path) != *dir || !name.ends_with(".md") {
            stopped.push(format!(
                "{path}: not a record the move knows where to put; move or remove it by hand"
            ));
        } else if GENERATED.contains(&name) {
            plan.removes.push(path.clone());
        } else {
            let to = format!("{WORK}/{name}");
            if let Some((from, _)) = moves.iter().find(|(_, other)| **other == to) {
                stopped.push(format!(
                    "{from} and {path} have the same name: rename one by hand (and the links to it), as the two \
                     directories become one"
                ));
            } else if files.contains_key(&to) {
                stopped.push(format!("{path}: {to} exists already: rename one by hand"));
            } else {
                moves.insert(path.clone(), to);
            }
        }
    }
    if !stopped.is_empty() {
        return Err(format!(
            "the records cannot be moved to {WORK}/, and nothing was changed:\n  {}",
            stopped.join("\n  ")
        ));
    }
    if moves.is_empty() && plan.removes.is_empty() {
        return Ok(Plan::default());
    }
    let (mut closed_unsaid, mut deadlines, mut drafts) = (Vec::new(), Vec::new(), Vec::new());
    for (path, text) in files {
        if !path.ends_with(".md") || plan.removes.contains(path) {
            continue;
        }
        let at = moves.get(path).map_or(parent(path), |to| parent(to));
        let linked = repoint_links(text, parent(path), at);
        match moves.get(path) {
            Some(to) => {
                let moved = Moved::new(&linked);
                if moved.closed_unsaid {
                    closed_unsaid.push(file_name(to).to_string());
                }
                if moved.deadline_kept {
                    deadlines.push(file_name(to).to_string());
                }
                if moved.made_draft {
                    drafts.push(file_name(to).to_string());
                }
                plan.writes.insert(to.clone(), moved.text);
                plan.removes.push(path.clone());
                plan.moved += 1;
            }
            None if linked != *text => {
                plan.writes.insert(path.clone(), linked);
            }
            None => {}
        }
    }
    plan.removes.sort();
    plan.dirs = OLD
        .iter()
        .filter(|dir| {
            files
                .keys()
                .any(|path| path.starts_with(&format!("{dir}/")))
        })
        .map(|dir| dir.to_string())
        .collect();
    let listed = |names: &[String]| names.join(", ");
    if !closed_unsaid.is_empty() {
        plan.by_hand.push(format!(
            "{} closed records in {WORK}/ do not say how they closed: write closed_as: done or closed_as: dropped in \
             each, as its # Resolution says ({})",
            closed_unsaid.len(),
            listed(&closed_unsaid)
        ));
    }
    if !deadlines.is_empty() {
        plan.by_hand.push(format!(
            "{} open work items keep their deadline in words: write each moment as a milestone, name it in until, \
             and remove deadline_kind and deadline ({})",
            deadlines.len(),
            listed(&deadlines)
        ));
    }
    if !drafts.is_empty() {
        plan.by_hand.push(format!(
            "{} open work items with no parent are drafts now, as stable means sorted into a spec: sort each, giving \
             it a parent and status: stable, or keep it a draft with until naming a milestone ({})",
            drafts.len(),
            listed(&drafts)
        ));
    }
    Ok(plan)
}

/// `text`, a document that was in the directory `from` and is in `at` (both from the root), with every link target
/// that reached a record in an old directory pointed at its place in `docs/work/`, in the form it was written: from the
/// bundle root, or relative to `at`. A moved document stays at the same depth (`docs/backlog/` and `docs/work/` are
/// siblings), so its other relative links still reach what they reached.
pub fn repoint_links(text: &str, from: &str, at: &str) -> String {
    let repoint = |caps: &Captures| -> String {
        let written = &caps[2];
        let (open, target, close) =
            match written.strip_prefix('<').and_then(|t| t.strip_suffix('>')) {
                Some(inner) => ("<", inner, ">"),
                None => ("", written, ""),
            };
        let new = repoint_target(target, from, at).unwrap_or_else(|| target.to_string());
        format!("{}{open}{new}{close}", &caps[1])
    };
    let text = INLINE.replace_all(text, repoint);
    DEFINITION.replace_all(&text, repoint).into_owned()
}

/// The target `target`, written in a document that was in `from` and is in `at`, pointed at `docs/work/` when it
/// reaches a record in an old directory; `None` when it does not.
fn repoint_target(target: &str, from: &str, at: &str) -> Option<String> {
    if target.contains("://") || target.starts_with('#') || target.starts_with("mailto:") {
        return None;
    }
    let (path, fragment) = match target.split_once('#') {
        Some((path, fragment)) => (path, format!("#{fragment}")),
        None => (target, String::new()),
    };
    if path.is_empty() {
        return None;
    }
    let reached = match path.strip_prefix('/') {
        Some(in_bundle) => normalize(&format!("docs/{in_bundle}"))?,
        None => normalize(&format!("{from}/{path}"))?,
    };
    let rest = OLD
        .iter()
        .find_map(|old| reached.strip_prefix(&format!("{old}/")))?;
    let moved_to = format!("{WORK}/{rest}");
    let written = if path.starts_with('/') {
        format!(
            "/{}",
            moved_to
                .strip_prefix("docs/")
                .expect("docs/work/ is in docs/")
        )
    } else {
        relative(at, &moved_to)
    };
    Some(format!("{written}{fragment}"))
}

/// The path from the directory `from` to `to`, both from the root.
fn relative(from: &str, to: &str) -> String {
    let from: Vec<&str> = from.split('/').filter(|p| !p.is_empty()).collect();
    let to: Vec<&str> = to.split('/').collect();
    let common = from.iter().zip(&to).take_while(|(a, b)| a == b).count();
    let mut parts: Vec<&str> = vec![".."; from.len() - common];
    parts.extend(&to[common..]);
    parts.join("/")
}

/// One record, its frontmatter rewritten for `docs/work/`, and what that left to a person.
struct Moved {
    text: String,
    /// Closed, with no `closed_as`
    closed_unsaid: bool,
    /// An open work item that keeps its deadline in words
    deadline_kept: bool,
    /// An open work item with no parent, which was stable and is a draft now
    made_draft: bool,
}

impl Moved {
    fn new(text: &str) -> Moved {
        let mut moved = Moved {
            text: text.to_string(),
            closed_unsaid: false,
            deadline_kept: false,
            made_draft: false,
        };
        let Some(rest) = text.strip_prefix("---\n") else {
            return moved;
        };
        let Some(end) = rest.find("\n---\n") else {
            return moved;
        };
        let (front, body) = (&rest[..end], &rest[end..]);
        let lines: Vec<&str> = front.lines().collect();
        let key = |line: &str| {
            line.split_once(':')
                .map(|(key, _)| key.trim_end().to_string())
        };
        let has = |name: &str| lines.iter().any(|line| key(line).as_deref() == Some(name));
        let value = |name: &str| {
            lines
                .iter()
                .find(|line| key(line).as_deref() == Some(name))
                .and_then(|line| line.split_once(':'))
                .map(|(_, value)| value.trim().to_string())
        };
        let item = value("type").as_deref() == Some("Backlog Item");
        let status = value("status");
        let closed = status.as_deref() == Some("deprecated");
        let mut out: Vec<String> = Vec::new();
        let mut dropping = false;
        for line in &lines {
            // A value folded over the lines under its key belongs to that key
            if dropping && line.starts_with([' ', '\t']) {
                continue;
            }
            dropping = false;
            match key(line).as_deref() {
                Some("type") if item => out.push("type: Work Item".into()),
                Some("epic") => out.push(format!("parent:{}", &line["epic:".len()..])),
                Some("status") if item && status.as_deref() == Some("stable") && !has("parent") => {
                    out.push("status: draft".into());
                    moved.made_draft = true;
                }
                Some("deadline_kind" | "deadline") if item && closed => dropping = true,
                _ => out.push(line.to_string()),
            }
        }
        moved.closed_unsaid = closed && !has("closed_as");
        moved.deadline_kept = item && !closed && (has("deadline") || has("deadline_kind"));
        moved.text = format!("---\n{}{body}", out.join("\n"));
        moved
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(path, text)| (path.to_string(), text.to_string()))
            .collect()
    }

    const ITEM: &str = "---\ntype: Backlog Item\ntitle: X\ndescription: D.\ntags: [a]\nstatus: stable\n\
                        filed: 2026-10-01\nverified: {by: human:a, at: 2026-10-01T10:00:00+09:00}\n\
                        deadline_kind: until\ndeadline: until the next deploy\n---\n\n# Trigger\n\nT.\n\n# State\n\n\
                        S.\n\n# Details\n\n[spec](/specs/design.md#goals), [other](other.md), [up](../specs/design.md), \
                        [log](/log.md), [readme](../../README.md)\n";

    #[test]
    fn a_link_to_a_record_points_at_its_new_place_in_the_form_it_was_written() {
        let cases = [
            // From the bundle root
            ("docs/knowledge", "/backlog/a.md", "/work/a.md"),
            ("docs/knowledge", "/specs/a.md#goals", "/work/a.md#goals"),
            // Relative, from where the document is
            ("docs/knowledge", "../backlog/a.md", "../work/a.md"),
            ("docs/work", "../specs/a.md#x", "a.md#x"),
            ("docs", "backlog/a.md", "work/a.md"),
            ("docs/specs", "a.md", "../work/a.md"),
        ];
        for (dir, target, expected) in cases {
            assert_eq!(
                repoint_target(target, dir, dir).as_deref(),
                Some(expected),
                "{target} from {dir}"
            );
        }
        for (dir, target) in [
            ("docs/knowledge", "/log.md"),
            ("docs/knowledge", "../../README.md"),
            ("docs/knowledge", "https://example.com/backlog/a.md"),
            ("docs/knowledge", "#backlog"),
            ("docs/knowledge", "/knowledge/backlog.md"),
            ("docs", "../../backlog/a.md"),
        ] {
            assert_eq!(
                repoint_target(target, dir, dir),
                None,
                "{target} from {dir}"
            );
        }
        // Every form of link, and nothing else in the text
        let text = "See [a](/backlog/a.md \"title\"), ![b](<../specs/b c.md>), docs/backlog/a.md in words,\n\n\
                    [ref]: /specs/c.md\n";
        assert_eq!(
            repoint_links(text, "docs/knowledge", "docs/knowledge"),
            "See [a](/work/a.md \"title\"), ![b](<../work/b c.md>), docs/backlog/a.md in words,\n\n\
             [ref]: /work/c.md\n"
        );
    }

    #[test]
    fn the_records_move_with_their_frontmatter_read_anew() {
        let closed = ITEM
            .replace("status: stable", "status: deprecated")
            .replace(
                "---\n\n# Trigger",
                "---\n\n# Resolution\n\nFixed.\n\n# Trigger",
            );
        let sorted = ITEM.replace("status: stable", "status: stable\nparent: design");
        let docs = files(&[
            ("docs/backlog/open.md", ITEM),
            ("docs/backlog/closed.md", &closed),
            ("docs/backlog/sorted.md", &sorted),
            ("docs/backlog/index.md", "generated"),
            ("docs/backlog/rules.md", "generated"),
            (
                "docs/specs/design.md",
                "---\ntype: Spec\ntitle: D\ndescription: D.\ntags: [a]\nstatus: stable\nepic: big\n---\n\n# Goals\n",
            ),
            ("docs/specs/index.md", "generated"),
            (
                "docs/knowledge/api.md",
                "---\ntype: Knowledge\n---\n\n[open](../backlog/open.md) [log](/log.md)\n",
            ),
            (
                "docs/knowledge/untouched.md",
                "---\ntype: Knowledge\n---\n\n[log](/log.md)\n",
            ),
            (
                "docs/log.md",
                "## 2026-10-01\n\n* [x](/backlog/open.md): docs/backlog/open.md\n",
            ),
        ]);
        let plan = plan(&docs).unwrap();
        assert_eq!(plan.moved, 4);
        assert_eq!(
            plan.writes.keys().collect::<Vec<_>>(),
            [
                "docs/knowledge/api.md",
                "docs/log.md",
                "docs/work/closed.md",
                "docs/work/design.md",
                "docs/work/open.md",
                "docs/work/sorted.md",
            ]
        );
        assert_eq!(
            plan.removes,
            [
                "docs/backlog/closed.md",
                "docs/backlog/index.md",
                "docs/backlog/open.md",
                "docs/backlog/rules.md",
                "docs/backlog/sorted.md",
                "docs/specs/design.md",
                "docs/specs/index.md",
            ]
        );
        assert_eq!(plan.dirs, ["docs/backlog", "docs/specs"]);
        let open = &plan.writes["docs/work/open.md"];
        // An open item keeps its deadline for a person, and with no parent it is a draft
        assert!(open.starts_with("---\ntype: Work Item\n"), "{open}");
        assert!(
            open.contains("\nstatus: draft\n")
                && open.contains("\ndeadline: until the next deploy\n")
        );
        // Its links point at the new places, in the form they were written
        assert!(
            open.contains("[spec](/work/design.md#goals), [other](other.md), [up](design.md), [log](/log.md), \
                           [readme](../../README.md)"),
            "{open}"
        );
        // A closed item drops its deadline, which binds nothing, and keeps its status
        let closed = &plan.writes["docs/work/closed.md"];
        assert!(
            closed.contains("\nstatus: deprecated\n") && !closed.contains("deadline"),
            "{closed}"
        );
        // Sorted already: stays stable
        assert!(plan.writes["docs/work/sorted.md"].contains("\nstatus: stable\nparent: design\n"));
        assert!(plan.writes["docs/work/design.md"].contains("\nparent: big\n"));
        // The log's links move, and its words stay: they are history
        assert_eq!(
            plan.writes["docs/log.md"],
            "## 2026-10-01\n\n* [x](/work/open.md): docs/backlog/open.md\n"
        );
        assert_eq!(
            plan.writes["docs/knowledge/api.md"],
            "---\ntype: Knowledge\n---\n\n[open](../work/open.md) [log](/log.md)\n"
        );
        // What a person does next
        assert_eq!(plan.by_hand.len(), 3, "{:?}", plan.by_hand);
        assert!(
            plan.by_hand[0].starts_with("1 closed records")
                && plan.by_hand[0].ends_with("(closed.md)")
        );
        assert!(
            plan.by_hand[1].starts_with("2 open work items keep their deadline")
                && plan.by_hand[1].ends_with("(open.md, sorted.md)")
        );
        assert!(
            plan.by_hand[2].starts_with("1 open work items with no parent are drafts")
                && plan.by_hand[2].ends_with("(open.md)")
        );
    }

    #[test]
    fn what_the_move_cannot_place_stops_it_before_anything_changes() {
        let cases = [
            (
                "the same name in both",
                files(&[("docs/backlog/x.md", ITEM), ("docs/specs/x.md", ITEM)]),
                "docs/backlog/x.md and docs/specs/x.md have the same name",
            ),
            (
                "a record in docs/work/ already",
                files(&[("docs/backlog/x.md", ITEM), ("docs/work/x.md", ITEM)]),
                "docs/work/x.md exists already",
            ),
            (
                "a file that is no record",
                files(&[("docs/backlog/x.md", ITEM), ("docs/backlog/chart.png", "")]),
                "docs/backlog/chart.png: not a record",
            ),
            (
                "a deeper directory",
                files(&[("docs/specs/old/x.md", ITEM)]),
                "docs/specs/old/x.md: not a record",
            ),
        ];
        for (name, docs, said) in cases {
            let why = plan(&docs).err().unwrap_or_default();
            assert!(
                why.starts_with(
                    "the records cannot be moved to docs/work/, and nothing was changed"
                ) && why.contains(said),
                "{name}: {why}"
            );
        }
        // Nothing to move
        assert_eq!(
            plan(&files(&[("docs/work/x.md", ITEM), ("docs/log.md", "")])),
            Ok(Plan::default())
        );
    }
}
