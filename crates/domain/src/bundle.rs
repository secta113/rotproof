//! `docs/` as one OKF 0.2 bundle: sorting out its documents once they are read, and the index file each directory
//! should contain.
//!
//! Every `.md` under `docs/` except the reserved names is a document with frontmatter. The index files are never
//! written by hand: `rotproof index` writes them, and `rotproof check` fails when one differs from what it would write.

use std::collections::{BTreeMap, BTreeSet};

use chrono::NaiveDate;

use crate::layers::DECLARATION;
use crate::schema::{
    CLOSED_SECTION, ClosedAs, Guide, Item, Knowledge, KnowledgeDoc, Milestone, Spec, Status, Time,
    WHEN_SECTION, WorkDoc, knowledge_doc, work_doc,
};
use utils::frontmatter::{Sections, split};
use yaml_rust2::Yaml;

/// File names OKF reserves. Never used for a document
pub const RESERVED: [&str; 2] = ["index.md", "log.md"];
/// The notice at the top of every generated index. An HTML comment, so OKF readers do not see it
pub const GENERATED: &str = "<!-- Generated from the frontmatter by `rotproof index`. Do not edit: `rotproof check` \
                             fails when this file differs from what `rotproof index` writes. -->";
/// The rules of `docs/work/`. Rotproof writes them like an index file, so the rules a project reads are the rules its
/// Rotproof checks
pub const WORK_RULES: &str = include_str!("../../../records/work-rules.md");
/// The knowledge rules, written like the rules of `docs/work/`
pub const KNOWLEDGE_RULES: &str = include_str!("../../../records/knowledge-rules.md");
/// The log as `rotproof create` makes it. From then on it is the project's
pub const LOG: &str = include_str!("../../../records/log.md");
/// Where `rotproof create` writes a milestone when `docs/work/` has none
pub const FIRST_MILESTONE: &str = "docs/work/next-milestone.md";

/// The milestone `rotproof create` writes when `docs/work/` has none, in the area `area`. Its `# Condition` holds only
/// a comment, which a reader does not see, so the check fails on it until a person writes what to look at.
pub fn first_milestone(area: &str) -> String {
    include_str!("../../../records/milestone.md").replace("{area}", area)
}

/// Whether any document among `docs` (those of `docs/work/`) is a milestone, open or closed, passing or not.
pub fn has_milestone(docs: &Docs) -> bool {
    docs.values().any(|text| {
        split(text).is_ok_and(|(meta, _)| {
            meta.get(&Yaml::String("type".into()))
                .and_then(Yaml::as_str)
                == Some("Milestone")
        })
    })
}
/// The bundle-root index links to these, in this order
const ROOT_ENTRIES: [(&str, &str, &str); 3] = [
    (
        "Work",
        "work/",
        "Specs and work items: what was decided, and the work waiting on it, open and closed.",
    ),
    ("Knowledge", "knowledge/", "How things are now, and why."),
    ("Log", "log.md", "What was done, newest first."),
];

/// File name -> text.
pub type Docs = BTreeMap<String, String>;
/// File name -> why it was left out.
pub type Problems = BTreeMap<String, String>;

/// The documents of `docs/work/`, sorted out and read together: a record names its parent by slug.
#[derive(Debug, Default)]
pub struct Work {
    /// File name -> the work item, for the items that pass
    pub items: BTreeMap<String, (Item, Sections)>,
    /// File name -> the spec, for the specs that pass
    pub specs: BTreeMap<String, (Spec, Sections)>,
    /// File name -> the milestone, for the milestones that pass
    pub milestones: BTreeMap<String, (Milestone, Sections)>,
    /// File name -> the guide: the rules, and any a project adds
    pub guides: BTreeMap<String, Guide>,
    /// `work/<file name>` -> why the document is left out of the index files
    pub problems: Problems,
}

/// The documents of `docs/knowledge/`, sorted out.
#[derive(Debug, Default)]
pub struct KnowledgeFolder {
    pub documents: BTreeMap<String, (Knowledge, Sections)>,
    pub guides: BTreeMap<String, Guide>,
    pub problems: Problems,
}

/// The documents of `docs/knowledge/`, sorted out. A document whose area is not among `areas` is left out with why.
pub fn knowledge(docs: &Docs, areas: &[String]) -> KnowledgeFolder {
    let mut out = KnowledgeFolder::default();
    for (name, text) in docs {
        match knowledge_doc(text) {
            Ok(KnowledgeDoc::Knowledge(doc, _)) if !areas.contains(&doc.tag) => {
                out.problems
                    .insert(name.clone(), undeclared(&doc.tag, areas));
            }
            Ok(KnowledgeDoc::Knowledge(doc, sections)) => {
                out.documents.insert(name.clone(), (doc, sections));
            }
            Ok(KnowledgeDoc::Guide(guide)) => {
                out.guides.insert(name.clone(), guide);
            }
            Err(why) => {
                out.problems.insert(name.clone(), why);
            }
        }
    }
    out
}

impl Work {
    /// The spec with this slug.
    pub fn find(&self, slug: &str) -> Option<&Spec> {
        self.specs.get(&format!("{slug}.md")).map(|(spec, _)| spec)
    }

    /// The open children of a closed spec: the parts of an epic and the work items of a spec. A parent closes after its
    /// children, so one of the two has the wrong status. They stay in the index files: the relation is clear, and
    /// leaving the open child out would hide open work.
    pub fn closed_before_its_children(&self) -> Vec<String> {
        let specs = self.specs.iter().map(|(name, (spec, _))| {
            (
                name,
                "spec",
                spec.parent.as_ref(),
                spec.status != Status::Deprecated,
            )
        });
        let items = self
            .items
            .iter()
            .map(|(name, (item, _))| (name, "work item", item.parent.as_ref(), item.is_open()));
        specs
            .chain(items)
            .filter_map(|(name, kind, parent, open)| {
                let parent_spec = self.find(parent?)?;
                (open && parent_spec.status == Status::Deprecated).then(|| {
                    format!(
                        "{}: its parent {} is closed while this {kind} is open. Close the {kind} first (as \
                         dropped, if it was), or reopen the parent",
                        in_work(name),
                        parent.expect("found above")
                    )
                })
            })
            .collect()
    }

    /// Why the project has no next moment for its work to wait for, or nothing while a milestone is open. A milestone
    /// left out of the index files does not count: the message about it says what to fix.
    pub fn without_an_open_milestone(&self) -> Option<String> {
        let open = self
            .milestones
            .values()
            .any(|(milestone, _)| milestone.status != Status::Deprecated);
        (!open).then(|| {
            "no open milestone in docs/work/: write the next moment the work waits for (a release, a deploy, something \
             outside the project) as a milestone; docs/work/rules.md says how"
                .into()
        })
    }

    /// The open milestones whose `date` is before `today`, each with its date. Passing the date fails nothing: work is
    /// late more often than not, and a check that failed on a date alone would turn CI red with no change.
    pub fn past_their_date(&self, today: NaiveDate) -> Vec<(String, NaiveDate)> {
        self.milestones
            .iter()
            .filter(|(_, (milestone, _))| milestone.status != Status::Deprecated)
            .filter_map(|(name, (milestone, _))| {
                let date = milestone.date?;
                (date < today).then(|| (name.clone(), date))
            })
            .collect()
    }

    /// Why the record `name` cannot have the spec `parent` as its parent, or `None` when it can. The parent of a spec
    /// is an epic, which has no parent of its own; the parent of a work item is any spec.
    fn wrong_parent(&self, name: &str, parent: &str, of_spec: bool) -> Option<String> {
        let file = format!("{parent}.md");
        if file == name {
            return Some("names the record itself".into());
        }
        Some(match self.find(parent) {
            Some(spec) => match (&spec.parent, of_spec) {
                (Some(above), true) => format!(
                    "{parent} is itself a part of {above}: an epic is one level deep, so name {above} or \
                     remove one of the two"
                ),
                _ => return None,
            },
            None if self.problems.contains_key(&in_work(&file)) => {
                format!("{parent} is left out of the index files itself: fix it first")
            }
            None => format!(
                "names no spec in docs/work/: {parent} (a work item, a milestone or a guide is not a spec)"
            ),
        })
    }
}

/// A document of `docs/work/` as the problems name it.
fn in_work(name: &str) -> String {
    format!("work/{name}")
}

/// Every document of `docs/work/` (file name -> text): the work items, specs, milestones and guides that pass, and
/// why the others do not, checked one by one and against each other. A record whose area is not among `areas` does not
/// pass, and a record is left out of the index files when its relation to its parent is undefined: the parent is
/// missing, is not a spec, is the record itself, or, for a spec, is a part of another.
pub fn work(docs: &Docs, areas: &[String]) -> Work {
    let mut out = Work::default();
    for (name, text) in docs {
        match work_doc(text) {
            Ok(WorkDoc::Item(item, _)) if !areas.contains(&item.tag) => {
                out.problems
                    .insert(in_work(name), undeclared(&item.tag, areas));
            }
            Ok(WorkDoc::Spec(spec, _)) if !areas.contains(&spec.tag) => {
                out.problems
                    .insert(in_work(name), undeclared(&spec.tag, areas));
            }
            Ok(WorkDoc::Milestone(milestone, _)) if !areas.contains(&milestone.tag) => {
                out.problems
                    .insert(in_work(name), undeclared(&milestone.tag, areas));
            }
            Ok(WorkDoc::Item(item, sections)) => {
                out.items.insert(name.clone(), (item, sections));
            }
            Ok(WorkDoc::Spec(spec, sections)) => {
                out.specs.insert(name.clone(), (spec, sections));
            }
            Ok(WorkDoc::Milestone(milestone, sections)) => {
                out.milestones.insert(name.clone(), (milestone, sections));
            }
            Ok(WorkDoc::Guide(guide)) => {
                out.guides.insert(name.clone(), guide);
            }
            Err(why) => {
                out.problems.insert(in_work(name), why);
            }
        }
    }
    // The specs first: a work item whose spec is left out is then told to fix the spec first
    let left: Problems = out
        .specs
        .iter()
        .filter_map(|(name, (spec, _))| {
            let why = out.wrong_parent(name, spec.parent.as_ref()?, true)?;
            Some((name.clone(), why))
        })
        .collect();
    for (name, why) in left {
        out.specs.remove(&name);
        out.problems
            .insert(in_work(&name), format!("parent: {why}"));
    }
    let left: Problems = out
        .items
        .iter()
        .filter_map(|(name, (item, _))| {
            let why = out.wrong_parent(name, item.parent.as_ref()?, false)?;
            Some((name.clone(), why))
        })
        .collect();
    for (name, why) in left {
        out.items.remove(&name);
        out.problems
            .insert(in_work(&name), format!("parent: {why}"));
    }
    // The arrows, until no record is left out: a record whose arrow names one left out here is then told to fix that
    // one
    loop {
        let left = crate::arrows::unresolved(&out);
        if left.is_empty() {
            break;
        }
        for (name, why) in left {
            out.items.remove(&name);
            out.specs.remove(&name);
            out.milestones.remove(&name);
            out.problems.insert(in_work(&name), why);
        }
    }
    out
}

/// Why a tag is not an area, with the areas it could be.
fn undeclared(tag: &str, areas: &[String]) -> String {
    let declared = if areas.is_empty() {
        "it declares none".to_string()
    } else {
        format!("it declares {}", areas.join(", "))
    };
    format!("tags: {tag:?} is not an area declared in {DECLARATION} ({declared})")
}

/// Where the bundle sits, from the root of the repository.
pub const DOCS: &str = "docs";

/// The path of `rest` in the bundle, from the root of the repository.
pub fn in_docs(rest: &str) -> String {
    format!("{DOCS}/{rest}")
}

/// Whether the entry `name` of a directory of the bundle is a document: a `.md` file with a name OKF does not reserve.
pub fn is_document(name: &str, is_dir: bool) -> bool {
    name.ends_with(".md") && !RESERVED.contains(&name) && !is_dir
}

/// Every file Rotproof generates in the bundle (the index files and the rules) -> what it should contain now, and the
/// documents left out of the index files, from the documents read in `docs/work/` and `docs/knowledge/`.
pub fn expected(
    work_docs: &Docs,
    knowledge_docs: &Docs,
    areas: &[String],
) -> (Vec<(String, String)>, Problems) {
    let work = work_with_rules(work_docs, areas);
    let mut files = vec![
        (in_docs("work/rules.md"), WORK_RULES.into()),
        (in_docs("index.md"), render_root()),
        (in_docs("work/index.md"), render_work(&work, areas)),
    ];
    let mut problems = work.problems;
    let read = knowledge_with_rules(knowledge_docs, areas);
    files.push((in_docs("knowledge/rules.md"), KNOWLEDGE_RULES.into()));
    files.push((
        in_docs("knowledge/index.md"),
        render_knowledge(&read.documents, &read.guides, areas),
    ));
    problems.extend(
        read.problems
            .into_iter()
            .map(|(name, why)| (format!("knowledge/{name}"), why)),
    );
    (files, problems)
}

/// The documents of `docs/knowledge/`, from those read there, with the knowledge rules read as Rotproof writes them, so
/// the index lists them on the run that writes them.
pub fn knowledge_with_rules(docs: &Docs, areas: &[String]) -> KnowledgeFolder {
    let mut docs = docs.clone();
    docs.insert("rules.md".into(), KNOWLEDGE_RULES.into());
    knowledge(&docs, areas)
}

/// The documents of `docs/work/`, from those read there, with the rules read as Rotproof writes them, so the index
/// lists them on the run that writes them.
pub fn work_with_rules(docs: &Docs, areas: &[String]) -> Work {
    let mut docs = docs.clone();
    docs.insert("rules.md".into(), WORK_RULES.into());
    work(&docs, areas)
}

/// A title as the text of a link in the index. `[`, `]` and `\` are escaped, so a title with brackets stays the text
/// of its own entry instead of closing the link early and opening another.
pub fn link_text(title: &str) -> String {
    let mut out = String::with_capacity(title.len());
    for c in title.chars() {
        if matches!(c, '[' | ']' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// The first sentence of the first line, for the index. Bold text at the start counts as a sentence on its own.
/// Works for any language: a sentence ends at "。" or at "." followed by a space or the end of the line.
pub fn first_sentence(text: &str) -> &str {
    let first = text.split('\n').next().unwrap_or("").trim();
    if let Some(rest) = first.strip_prefix("**")
        && let Some(end) = rest.find('*')
        && end > 0
        && rest[end..].starts_with("**")
    {
        return &first[..end + 4];
    }
    let mut chars = first.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        let ends = match c {
            '。' => true,
            '.' => chars.peek().is_none_or(|(_, next)| next.is_whitespace()),
            _ => false,
        };
        if ends {
            return &first[..i + c.len_utf8()];
        }
    }
    first
}

/// One record of `docs/work/` as the index lists it.
struct Entry<'a> {
    name: &'a str,
    tag: &'a str,
    parent: Option<&'a String>,
    closed: bool,
    /// Specs before work items under the same heading or parent; work items by filing date
    order: (bool, Option<NaiveDate>),
    /// The line without its parent, which is added when the entry is not listed under it
    line: String,
}

/// The index of `work/` (an OKF index.md): the guides, the open milestones under `# Milestones`, the open records by
/// area in the order `areas` declares them, and the closed records last under `# Closed`, milestones first, so they do
/// not bury the open ones. A milestone is listed across the areas: it is the moment the work of every area waits for.
///
/// A record in the same area as its parent, and open or closed as its parent is, is listed under it, indented: the
/// parts of an epic and the work items of a spec. Any other record with a parent is listed on its own with its parent
/// after its line, so each record appears once. Under a heading or a parent, the specs come first, then the work items
/// by filing date.
///
/// Each entry has the OKF form `* [title](target) - description`, with the frontmatter's `description`, and adds after
/// ` | ` what a reader needs: an open spec its status; an open item its status when nobody has sorted it yet, the date
/// of the last measurement and the first sentence of its state; an open record its arrows (what it waits for, and what
/// waits for it); a closed record how it closed (`Done:` or `Dropped:`) and the first sentence of its resolution. The
/// separator is a symbol so the parts stay apart in any language.
pub fn render_work(work: &Work, areas: &[String]) -> String {
    let records = crate::arrows::records(work);
    let waiting = crate::arrows::waiting(work);
    // What an open record waits for and what waits for it, after its line. A milestone collects the arrows of much of
    // the work, so it says how many records it waits for instead of naming them
    let arrows = |name: &str, milestone: bool| -> String {
        let Some(waiting) = waiting.get(name) else {
            return String::new();
        };
        let links = |names: &[String]| -> String {
            let links: Vec<String> = names
                .iter()
                .map(|name| format!("[{}]({name})", link_text(records[name.as_str()].title)))
                .collect();
            links.join(", ")
        };
        let mut out = String::new();
        if waiting.ready {
            out.push_str(" | Ready.");
        }
        match waiting.before.len() {
            0 => {}
            1 if milestone => out.push_str(" | Waits for 1 open record."),
            n if milestone => out.push_str(&format!(" | Waits for {n} open records.")),
            _ => out.push_str(&format!(" | Waits for: {}", links(&waiting.before))),
        }
        if !waiting.until.is_empty() {
            out.push_str(&format!(" | Until: {}", links(&waiting.until)));
        }
        out
    };
    let specs = work.specs.iter().map(|(name, (spec, sections))| {
        let after = if let Some(closed_as) = spec.closed_as {
            resolution(closed_as, sections)
        } else {
            format!("Status: {}.", spec.status.name())
        };
        Entry {
            name,
            tag: &spec.tag,
            parent: spec.parent.as_ref(),
            closed: spec.status == Status::Deprecated,
            order: (false, None),
            line: format!(
                "* [{}]({name}) - {} | {after}{}",
                link_text(&spec.title),
                spec.description,
                arrows(name, false)
            ),
        }
    });
    let items = work.items.iter().map(|(name, (item, sections))| Entry {
        name,
        tag: &item.tag,
        parent: item.parent.as_ref(),
        closed: !item.is_open(),
        order: (true, Some(item.filed)),
        line: format!("{}{}", item_line(name, item, sections), arrows(name, false)),
    });
    let mut entries: Vec<Entry> = specs.chain(items).collect();
    entries.sort_by_key(|entry| (entry.order, entry.name));
    // The parent an entry is listed under, in this index
    let nested_in = |entry: &Entry| -> Option<String> {
        let name = format!("{}.md", entry.parent?);
        let (spec, _) = work.specs.get(&name)?;
        (spec.tag == entry.tag && (spec.status == Status::Deprecated) == entry.closed)
            .then_some(name)
    };
    let line = |entry: &Entry| match entry.parent {
        Some(parent) if nested_in(entry).is_none() => {
            let spec = work
                .find(parent)
                .expect("work leaves out a record whose parent it cannot find");
            format!(
                "{} | Parent: [{}]({parent}.md)",
                entry.line,
                link_text(&spec.title)
            )
        }
        _ => entry.line.clone(),
    };
    // Each entry `keep` takes that is not listed under a parent, with its children under it
    let tree = |keep: &dyn Fn(&Entry) -> bool| {
        let mut lines = Vec::new();
        let mut under = vec![];
        for entry in entries.iter().rev() {
            if keep(entry) && nested_in(entry).is_none() {
                under.push((entry, 0));
            }
        }
        while let Some((entry, depth)) = under.pop() {
            lines.push(format!("{}{}", "  ".repeat(depth), line(entry)));
            for child in entries.iter().rev() {
                if nested_in(child).as_deref() == Some(entry.name) {
                    under.push((child, depth + 1));
                }
            }
        }
        lines
    };
    let section = |heading: &str, mut lines: Vec<String>| {
        if !lines.is_empty() {
            lines.splice(0..0, ["".into(), format!("# {heading}"), "".into()]);
        }
        lines
    };
    // The milestones: across the areas, as the moments the work in every area waits for. The open ones by date, those
    // with none last
    let mut milestones: Vec<_> = work.milestones.iter().collect();
    milestones.sort_by_key(|(name, (milestone, _))| {
        (milestone.date.is_none(), milestone.date, name.as_str())
    });
    let milestone_line = |name: &str, milestone: &Milestone, sections: &Sections| {
        let after = match milestone.closed_as {
            Some(closed_as) => format!(" | {}", resolution(closed_as, sections)),
            None => {
                let proposed = if milestone.status == Status::Draft {
                    " | Status: draft."
                } else {
                    ""
                };
                let date = milestone
                    .date
                    .map(|date| format!(" | Date: {date}."))
                    .unwrap_or_default();
                format!("{proposed}{date}{}", arrows(name, true))
            }
        };
        format!(
            "* [{}]({name}) - {}{after}",
            link_text(&milestone.title),
            milestone.description
        )
    };
    let (closed_milestones, open_milestones): (Vec<_>, Vec<_>) = milestones
        .into_iter()
        .map(|(name, (milestone, sections))| {
            (
                milestone.status == Status::Deprecated,
                milestone_line(name, milestone, sections),
            )
        })
        .partition(|(closed, _)| *closed);
    let lines = |pairs: Vec<(bool, String)>| -> Vec<String> {
        pairs.into_iter().map(|(_, line)| line).collect()
    };
    let mut out = vec![GENERATED.to_string()];
    out.extend(guide_section(&work.guides));
    out.extend(section("Milestones", lines(open_milestones)));
    for area in areas {
        out.extend(section(
            area,
            tree(&|entry| entry.tag == area && !entry.closed),
        ));
    }
    let mut closed = lines(closed_milestones);
    closed.extend(tree(&|entry| entry.closed));
    out.extend(section("Closed", closed));
    out.join("\n") + "\n"
}

/// The guides of a directory, first in its index so the rules are found before the records. Nothing when it has none.
fn guide_section(guides: &BTreeMap<String, Guide>) -> Vec<String> {
    if guides.is_empty() {
        return Vec::new();
    }
    let mut out = vec!["".into(), "# Guides".into(), "".into()];
    out.extend(guides.iter().map(|(name, guide)| {
        format!(
            "* [{}]({name}) - {}",
            link_text(&guide.title),
            guide.description
        )
    }));
    out
}

/// How a closed record of `docs/work/` ends its index line: how it closed, and the first sentence of its resolution.
fn resolution(closed_as: ClosedAs, sections: &Sections) -> String {
    let how = match closed_as {
        ClosedAs::Done => "Done",
        ClosedAs::Dropped => "Dropped",
    };
    format!("{how}: {}", first_sentence(&sections[CLOSED_SECTION]))
}

/// The index line of a work item, without its parent.
fn item_line(name: &str, item: &Item, sections: &Sections) -> String {
    let start = format!(
        "* [{}]({name}) - {}",
        link_text(&item.title),
        item.description
    );
    if let Some(closed_as) = item.closed_as {
        return format!("{start} | {}", resolution(closed_as, sections));
    }
    // Only the date, in the time zone of the measurement: the time of day would not change what a reader does.
    // Whether stale_after has passed is not shown: the index would then depend on today's date, and the check that
    // compares it with the generated text would pass on some days and fail on others
    let measured = item.last_verified().at.date_naive();
    let unsorted = if item.status == Status::Draft {
        " | Status: draft."
    } else {
        ""
    };
    let stale = item
        .stale_after
        .map(|at| format!(" | Re-measure after {}.", at.date_naive()))
        .unwrap_or_default();
    format!(
        "{start}{unsorted} | State ({measured}): {}{stale}",
        first_sentence(&sections["State"])
    )
}

/// The knowledge index: the guides, the alarms that hold under `# When something happens`, the other documents that
/// hold by area in the order `areas` declares them, and the deprecated ones last under `# Closed`, with what replaced
/// them.
pub fn render_knowledge(
    documents: &BTreeMap<String, (Knowledge, Sections)>,
    guides: &BTreeMap<String, Guide>,
    areas: &[String],
) -> String {
    let mut out = vec![GENERATED.to_string()];
    out.extend(guide_section(guides));
    let entry = |name: &str, doc: &Knowledge| {
        format!(
            "* [{}]({name}) - {}",
            link_text(&doc.title),
            doc.description
        )
    };
    // The alarms first, across the areas: reading the index before the work is what watches for them. Each with what
    // is seen and the strings to search for, so a message met is found in the index too
    let alarm = |sections: &Sections| sections.contains_key(WHEN_SECTION);
    let alarms: Vec<String> = documents
        .iter()
        .filter(|(_, (doc, sections))| doc.status == Status::Stable && alarm(sections))
        .map(|(name, (doc, sections))| {
            let matches = if doc.matches.is_empty() {
                String::new()
            } else {
                let spans: Vec<String> = doc.matches.iter().map(|text| code_span(text)).collect();
                format!(" | Match: {}", spans.join(", "))
            };
            format!(
                "{} | When: {}{matches}",
                entry(name, doc),
                first_sentence(&sections[WHEN_SECTION])
            )
        })
        .collect();
    if !alarms.is_empty() {
        out.extend(["".into(), "# When something happens".into(), "".into()]);
        out.extend(alarms);
    }
    for area in areas {
        let in_area: Vec<_> = documents
            .iter()
            .filter(|(_, (doc, sections))| {
                &doc.tag == area && doc.status == Status::Stable && !alarm(sections)
            })
            .collect();
        if in_area.is_empty() {
            continue;
        }
        out.extend(["".into(), format!("# {area}"), "".into()]);
        out.extend(in_area.into_iter().map(|(name, (doc, _))| entry(name, doc)));
    }
    let closed: Vec<_> = documents
        .iter()
        .filter(|(_, (doc, _))| doc.status == Status::Deprecated)
        .collect();
    if !closed.is_empty() {
        out.extend(["".into(), "# Closed".into(), "".into()]);
        for (name, (doc, sections)) in closed {
            out.push(format!(
                "{} | Resolution: {}",
                entry(name, doc),
                first_sentence(&sections[CLOSED_SECTION])
            ));
        }
    }
    out.join("\n") + "\n"
}

/// `text` as a code span, as CommonMark reads one: fenced with more backticks than any run inside it, and with a space
/// at each end when it starts or ends with a backtick, so a string to match is shown as it is, whatever it holds.
fn code_span(text: &str) -> String {
    let longest = text.split(|c| c != '`').map(str::len).max().unwrap_or(0);
    let fence = "`".repeat(longest + 1);
    let pad = if text.starts_with('`') || text.ends_with('`') {
        " "
    } else {
        ""
    };
    format!("{fence}{pad}{text}{pad}{fence}")
}

/// The bundle-root index. Only this index file may carry frontmatter (OKF 0.2, section 12).
pub fn render_root() -> String {
    let mut out: Vec<String> = [
        "---",
        "okf_version: \"0.2\"",
        "---",
        "",
        GENERATED,
        "",
        "# Records",
        "",
    ]
    .map(String::from)
    .into();
    out.extend(
        ROOT_ENTRIES
            .iter()
            .map(|(title, target, description)| format!("* [{title}]({target}) - {description}")),
    );
    out.join("\n") + "\n"
}

/// Every tag the work items, specs, milestones and knowledge documents among `docs` use. A document that cannot be
/// read is left to `rotproof check`.
pub fn record_tags(docs: &Docs) -> BTreeSet<String> {
    let mut tags = BTreeSet::new();
    for text in docs.values() {
        let Ok((meta, _)) = split(text) else {
            continue;
        };
        let kind = meta
            .get(&Yaml::String("type".into()))
            .and_then(Yaml::as_str);
        if !matches!(kind, Some("Work Item" | "Spec" | "Milestone" | "Knowledge")) {
            continue;
        }
        match meta.get(&Yaml::String("tags".into())) {
            Some(Yaml::Array(list)) => {
                tags.extend(list.iter().filter_map(Yaml::as_str).map(String::from))
            }
            Some(Yaml::String(tag)) => {
                tags.insert(tag.clone());
            }
            _ => {}
        }
    }
    tags
}

/// The open items past `stale_after`. As in OKF, an item is stale when `now >= stale_after`.
pub fn stale(items: &BTreeMap<String, (Item, Sections)>, now: Time) -> Vec<String> {
    items
        .iter()
        .filter(|(_, (item, _))| item.is_open() && item.stale_after.is_some_and(|at| now >= at))
        .map(|(name, _)| name.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use chrono::{FixedOffset, TimeZone};

    use super::*;

    const GOOD: &str = "---
type: Work Item
title: Some problem
description: Something is wrong.
tags: [operations]
status: draft
filed: 2026-09-27
verified: {by: human:someone, at: 2026-09-28T08:00:00+09:00}
---

# Trigger

The next deploy

# State

Not yet. Measured by hand.

# Details

[somewhere](/log.md)
";

    fn parsed(docs: &[(&str, String)]) -> Work {
        let docs: Docs = docs
            .iter()
            .map(|(name, text)| (name.to_string(), text.clone()))
            .collect();
        let parsed = work(&docs, &areas());
        assert!(parsed.problems.is_empty(), "{:?}", parsed.problems);
        parsed
    }

    #[test]
    fn first_sentence_ends_at_a_full_stop_in_any_language() {
        let cases = [
            ("Not yet. Measured by hand.", "Not yet."),
            (
                "発火済み・未着手。手元の Rust は古い。",
                "発火済み・未着手。",
            ),
            ("**Bold first.** Then more.", "**Bold first.**"),
            ("Version 1.75 is old. Update it.", "Version 1.75 is old."),
            ("Ends at the end.", "Ends at the end."),
            ("No full stop", "No full stop"),
            ("  Padded.  \nsecond line.", "Padded."),
            ("**unclosed bold. Then", "**unclosed bold."),
            ("", ""),
        ];
        for (text, expected) in cases {
            assert_eq!(first_sentence(text), expected, "{text:?}");
        }
    }

    #[test]
    fn an_open_item_shows_its_state() {
        // Measured at 08:00 in +09:00, which is the day before in UTC: the date is the one where it was measured
        let index = render_work(&parsed(&[("good.md", GOOD.into())]), &areas());
        // Nobody has sorted it yet, and the line says so
        let line = "* [Some problem](good.md) - Something is wrong. | Status: draft. | State (2026-09-28): Not yet.";
        assert_eq!(index, format!("{GENERATED}\n\n# operations\n\n{line}\n"));
        // Sorted, it is listed under its spec without a status
        let (spec_name, spec) = spec_doc("big", "operations", None, false);
        let sorted = GOOD.replace("status: draft", "status: stable\nparent: big");
        let index = render_work(
            &parsed(&[("good.md", sorted), (&spec_name, spec)]),
            &areas(),
        );
        let line =
            "  * [Some problem](good.md) - Something is wrong. | State (2026-09-28): Not yet.";
        assert_eq!(
            index,
            format!(
                "{GENERATED}\n\n# operations\n\n* [big](big.md) - D. | Status: stable.\n{line}\n"
            )
        );
    }

    #[test]
    fn a_title_with_brackets_stays_the_text_of_its_own_entry() {
        let title = r#"title: 'Evil ](fake.md) [hacked \ end'"#;
        let item = GOOD.replace("title: Some problem", title);
        let index = render_work(&parsed(&[("x.md", item)]), &areas());
        let line = index.lines().find(|l| l.starts_with("* ")).unwrap();
        assert!(
            line.starts_with(r"* [Evil \](fake.md) \[hacked \\ end](x.md) - "),
            "{line}"
        );
        // Read back as a reader reads it, the entry is one link to its own document, with the title as its text
        let found = utils::markdown::links(line);
        assert_eq!(
            found,
            [(
                r"Evil ](fake.md) [hacked \ end".to_string(),
                "x.md".to_string()
            )]
        );
    }

    #[test]
    fn the_stale_date_is_listed_but_not_judged() {
        // The index shows the stale_after date; whether it has passed is answered by `stale`, given the current time
        // (OKF: stale when now >= stale_after)
        let fresh = GOOD.replace("filed:", "stale_after: 2027-03-31T00:00:00+09:00\nfiled:");
        let parsed = parsed(&[("fresh.md", fresh), ("plain.md", GOOD.into())]);
        assert!(render_work(&parsed, &areas()).contains("| Re-measure after 2027-03-31."));
        let cutoff = FixedOffset::east_opt(9 * 3600)
            .unwrap()
            .with_ymd_and_hms(2027, 3, 31, 0, 0, 0)
            .unwrap();
        assert_eq!(
            stale(&parsed.items, cutoff - chrono::Duration::seconds(1)),
            Vec::<String>::new()
        );
        assert_eq!(stale(&parsed.items, cutoff), vec!["fresh.md"]);
    }

    #[test]
    fn the_alarms_come_first_with_what_is_seen_and_what_to_search_for() {
        let doc = |title: &str, tag: &str, matches: &str, body: &str| {
            format!(
                "---\ntype: Knowledge\ntitle: {title}\ndescription: D.\ntags: [{tag}]\nstatus: stable\n{matches}---\n\n{body}"
            )
        };
        let alarm_body = "# When\n\nThe publish job prints 403. More.\n\n# Do\n\nFile a draft.\n";
        let docs: Docs = [
            (
                "api.md".to_string(),
                doc("API", "billing", "", "# Shape\n\nX.\n"),
            ),
            (
                "publish.md".to_string(),
                doc(
                    "Publish",
                    "operations",
                    "match: [403, \"`x` \"]\n",
                    alarm_body,
                ),
            ),
            (
                "quiet.md".to_string(),
                doc("Quiet", "billing", "", alarm_body),
            ),
            (
                "gone.md".to_string(),
                crate::schema::closed_record(
                    &doc("Gone", "billing", "", alarm_body),
                    "Fixed upstream.",
                ),
            ),
        ]
        .into();
        let read = knowledge(&docs, &areas());
        assert!(read.problems.is_empty(), "{:?}", read.problems);
        let index = render_knowledge(&read.documents, &read.guides, &areas());
        assert_eq!(
            index
                .lines()
                .filter(|l| l.contains("](") || l.starts_with('#'))
                .collect::<Vec<_>>(),
            [
                "# When something happens",
                // A string with a backtick at its end is still shown as it is
                "* [Publish](publish.md) - D. | When: The publish job prints 403. | Match: `403`, `` `x`  ``",
                "* [Quiet](quiet.md) - D. | When: The publish job prints 403.",
                "# billing",
                "* [API](api.md) - D.",
                "# Closed",
                "* [Gone](gone.md) - D. | Resolution: Fixed upstream.",
            ]
        );
    }

    #[test]
    fn the_knowledge_index_lists_by_area_then_what_no_longer_holds() {
        let doc = |title: &str, tag: &str| {
            format!(
                "---\ntype: Knowledge\ntitle: {title}\ndescription: D.\ntags: [{tag}]\nstatus: stable\n---\n"
            )
        };
        let docs: Docs = [
            ("api.md".to_string(), doc("API", "billing")),
            ("ops.md".to_string(), doc("Ops", "operations")),
            (
                "old.md".to_string(),
                crate::schema::closed_record(&doc("Old", "billing"), "Replaced by the API. More."),
            ),
            ("rules.md".to_string(), KNOWLEDGE_RULES.to_string()),
        ]
        .into();
        let read = knowledge(&docs, &areas());
        assert!(read.problems.is_empty(), "{:?}", read.problems);
        let index = render_knowledge(&read.documents, &read.guides, &areas());
        assert_eq!(
            index
                .lines()
                .filter(|l| l.contains("](") || l.starts_with('#'))
                .collect::<Vec<_>>(),
            [
                "# Guides",
                "* [Knowledge rules](rules.md) - What goes in docs/knowledge/, how each document and alarm is written, \
                 and how the log names every edit. The format is OKF 0.2; rotproof check checks it.",
                "# operations",
                "* [Ops](ops.md) - D.",
                "# billing",
                "* [API](api.md) - D.",
                "# Closed",
                "* [Old](old.md) - D. | Resolution: Replaced by the API.",
            ]
        );
    }

    #[test]
    fn a_closed_item_leaves_the_open_list() {
        let closed = crate::schema::closed_record(GOOD, "Fixed.");
        let index = render_work(&parsed(&[("closed.md", closed)]), &areas());
        assert!(
            index.contains("# Closed") && index.contains("Fixed."),
            "{index}"
        );
        assert!(
            !index.contains("# operations"),
            "a closed item is still listed under its area"
        );
    }

    /// The areas the tests declare: not in alphabetical order, so an index sorted by name would differ
    fn areas() -> Vec<String> {
        ["operations", "billing", "unused"].map(String::from).into()
    }

    #[test]
    fn the_tags_of_the_records_are_collected_and_others_are_not() {
        let docs: Docs = [
            ("a.md", "---\ntype: Work Item\ntags: [billing, ops]\n---\n"),
            ("b.md", "---\ntype: Spec\ntags: records\n---\n"),
            ("rules.md", "---\ntype: Guide\ntags: [guides]\n---\n"),
            ("broken.md", "no frontmatter"),
        ]
        .map(|(name, text)| (name.to_string(), text.to_string()))
        .into();
        assert_eq!(
            record_tags(&docs).into_iter().collect::<Vec<_>>(),
            ["billing", "ops", "records"]
        );
    }

    #[test]
    fn every_generated_file_is_expected_with_the_rules_listed() {
        let none = Docs::new();
        let (files, problems) = expected(&none, &none, &areas());
        let paths: Vec<&str> = files.iter().map(|(path, _)| path.as_str()).collect();
        assert_eq!(
            paths,
            [
                "docs/work/rules.md",
                "docs/index.md",
                "docs/work/index.md",
                "docs/knowledge/rules.md",
                "docs/knowledge/index.md",
            ]
        );
        assert!(problems.is_empty(), "{problems:?}");
        // The rules are listed on the run that writes them
        let index = |path: &str| &files.iter().find(|(p, _)| p == path).unwrap().1;
        for (path, rules) in [
            ("docs/work/index.md", "rules.md"),
            ("docs/knowledge/index.md", "rules.md"),
        ] {
            assert!(index(path).contains(rules), "{path}");
        }
        assert!(is_document("a.md", false));
        assert!(!is_document("index.md", false) && !is_document("log.md", false));
        assert!(!is_document("a.md", true) && !is_document("a.txt", false));
    }

    /// The headings and the targets of an index, in order.
    fn outline(index: &str) -> Vec<&str> {
        index
            .lines()
            .filter(|line| line.trim_start().starts_with(['#', '*']))
            .map(|line| line.split(')').next().unwrap())
            .collect()
    }

    #[test]
    fn items_are_grouped_by_area_in_the_declared_order_and_by_filing_date() {
        let later = GOOD.replace("filed: 2026-09-27", "filed: 2026-09-28");
        let other = GOOD.replace("tags: [operations]", "tags: [billing]");
        let parsed = parsed(&[("a.md", later), ("b.md", GOOD.into()), ("c.md", other)]);
        let index = render_work(&parsed, &areas());
        // A declared area that no record uses has no heading
        assert_eq!(
            outline(&index),
            [
                "# operations",
                "* [Some problem](b.md",
                "* [Some problem](a.md",
                "# billing",
                "* [Some problem](c.md",
            ]
        );
    }

    #[test]
    fn a_record_in_an_undeclared_area_is_left_out() {
        let docs: Docs = [(
            "x.md".to_string(),
            GOOD.replace("tags: [operations]", "tags: [Operations]"),
        )]
        .into();
        let parsed = work(&docs, &areas());
        assert!(parsed.items.is_empty());
        assert!(
            parsed.problems["work/x.md"].contains("\"Operations\" is not an area declared")
                && parsed.problems["work/x.md"].contains("operations, billing, unused"),
            "{:?}",
            parsed.problems
        );
        let spec =
            "---\ntype: Spec\ntitle: S\ndescription: D.\ntags: [nowhere]\nstatus: stable\n---\n";
        let Work {
            specs: passed,
            problems,
            ..
        } = work(&[("s.md".to_string(), spec.to_string())].into(), &[]);
        assert!(passed.is_empty());
        assert!(
            problems["work/s.md"].contains("it declares none"),
            "{problems:?}"
        );
    }

    #[test]
    fn specs_are_grouped_by_area_in_the_declared_order() {
        let spec = |tag: &str| {
            format!(
                "---\ntype: Spec\ntitle: S\ndescription: D.\ntags: [{tag}]\nstatus: draft\n---\n"
            )
        };
        let docs: Docs = [
            ("a.md".to_string(), spec("billing")),
            ("b.md".to_string(), spec("operations")),
            ("c.md".to_string(), spec("billing")),
        ]
        .into();
        let all = work(&docs, &areas());
        assert!(all.problems.is_empty(), "{:?}", all.problems);
        assert_eq!(
            outline(&render_work(&all, &areas())),
            [
                "# operations",
                "* [S](b.md",
                "# billing",
                "* [S](a.md",
                "* [S](c.md"
            ]
        );
    }

    #[test]
    fn specs_and_work_items_share_one_index_specs_first() {
        let docs: Docs = [
            ("item.md".to_string(), GOOD.to_string()),
            spec_doc("spec", "operations", None, false),
            (
                "old-item.md".to_string(),
                crate::schema::closed_record(GOOD, "Fixed."),
            ),
            spec_doc("old-spec", "operations", None, true),
            ("rules.md".to_string(), WORK_RULES.to_string()),
        ]
        .into();
        let all = work(&docs, &areas());
        assert!(all.problems.is_empty(), "{:?}", all.problems);
        assert_eq!(
            outline(&render_work(&all, &areas())),
            [
                "# Guides",
                "* [Work rules](rules.md",
                "# operations",
                "* [spec](spec.md",
                "* [Some problem](item.md",
                "# Closed",
                "* [old-spec](old-spec.md",
                "* [Some problem](old-item.md",
            ]
        );
    }

    /// A spec titled after its slug, open or closed.
    fn spec_doc(slug: &str, tag: &str, parent: Option<&str>, closed: bool) -> (String, String) {
        let parent = parent.map(|p| format!("parent: {p}\n")).unwrap_or_default();
        let (status, resolution) = if closed {
            (
                "deprecated\nclosed_as: done",
                "\n# Resolution\n\nImplemented.\n",
            )
        } else {
            ("stable", "")
        };
        (
            format!("{slug}.md"),
            format!(
                "---\ntype: Spec\ntitle: {slug}\ndescription: D.\ntags: [{tag}]\nstatus: {status}\n{parent}---\n{resolution}"
            ),
        )
    }

    /// The record closed as dropped instead of done.
    fn dropped((name, text): (String, String)) -> (String, String) {
        assert!(text.contains("closed_as: done"), "{text}");
        (name, text.replace("closed_as: done", "closed_as: dropped"))
    }

    /// `docs/work/` holding the open and the closed specs together.
    fn spec_docs(open: Vec<(String, String)>, closed: Vec<(String, String)>) -> Docs {
        open.into_iter().chain(closed).collect()
    }

    #[test]
    fn a_part_is_listed_under_its_epic_or_names_it() {
        let all = work(
            &spec_docs(
                vec![
                    spec_doc("big", "operations", None, false),
                    // Same area, same index: under the epic
                    spec_doc("part-b", "operations", Some("big"), false),
                    spec_doc("part-a", "operations", Some("big"), false),
                    // Another area: under its own area, naming the epic
                    spec_doc("part-c", "billing", Some("big"), false),
                    spec_doc("alone", "operations", None, false),
                ],
                vec![
                    // Dropped part of an open epic: under Closed, naming it
                    dropped(spec_doc("part-d", "operations", Some("big"), true)),
                    // A closed epic and its closed part: under Closed, the part under its epic
                    spec_doc("old", "operations", None, true),
                    spec_doc("old-part", "operations", Some("old"), true),
                ],
            ),
            &areas(),
        );
        assert!(all.problems.is_empty(), "{:?}", all.problems);
        assert_eq!(all.closed_before_its_children(), Vec::<String>::new());
        let index = render_work(&all, &areas());
        assert_eq!(
            index
                .lines()
                .filter(|l| l.contains("](") || l.starts_with('#'))
                .collect::<Vec<_>>(),
            [
                "# operations",
                "* [alone](alone.md) - D. | Status: stable.",
                "* [big](big.md) - D. | Status: stable.",
                "  * [part-a](part-a.md) - D. | Status: stable.",
                "  * [part-b](part-b.md) - D. | Status: stable.",
                "# billing",
                "* [part-c](part-c.md) - D. | Status: stable. | Parent: [big](big.md)",
                "# Closed",
                "* [old](old.md) - D. | Done: Implemented.",
                "  * [old-part](old-part.md) - D. | Done: Implemented.",
                "* [part-d](part-d.md) - D. | Dropped: Implemented. | Parent: [big](big.md)",
            ]
        );
    }

    #[test]
    fn an_epic_closes_after_its_parts() {
        let all = work(
            &spec_docs(
                vec![spec_doc("part", "operations", Some("big"), false)],
                vec![spec_doc("big", "operations", None, true)],
            ),
            &areas(),
        );
        // The open part stays listed, naming its closed epic, and the check names the order
        assert!(all.problems.is_empty(), "{:?}", all.problems);
        assert!(render_work(&all, &areas()).contains("| Parent: [big](big.md)"));
        let found = all.closed_before_its_children();
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(found[0].starts_with("work/part.md: its parent big is closed"));
    }

    #[test]
    fn an_undefined_epic_leaves_the_part_out() {
        let broken = (
            "broken.md".to_string(),
            "---\ntype: Spec\ntitle: B\n---\n".to_string(),
        );
        let cases = [
            (
                "no such spec",
                spec_doc("p", "operations", Some("nowhere"), false),
                "names no spec",
            ),
            // A work item has a slug too, but it is not a spec
            (
                "a work item",
                spec_doc("p", "operations", Some("some-item"), false),
                "names no spec",
            ),
            (
                "itself",
                spec_doc("p", "operations", Some("p"), false),
                "names the record itself",
            ),
            (
                "a part",
                spec_doc("p", "operations", Some("mid"), false),
                "one level deep",
            ),
            (
                "a spec that fails",
                spec_doc("p", "operations", Some("broken"), false),
                "left out of the index",
            ),
            // The rules sit among the specs, but are a guide
            (
                "a guide",
                spec_doc("p", "operations", Some("rules"), false),
                "names no spec",
            ),
        ];
        for (name, part, said) in cases {
            let all = work(
                &spec_docs(
                    vec![
                        part,
                        spec_doc("top", "operations", None, false),
                        spec_doc("mid", "operations", Some("top"), false),
                        broken.clone(),
                        ("some-item.md".to_string(), GOOD.to_string()),
                        ("rules.md".to_string(), WORK_RULES.to_string()),
                    ],
                    vec![],
                ),
                &areas(),
            );
            assert!(!all.specs.contains_key("p.md"), "{name}: still listed");
            let why = all
                .problems
                .get("work/p.md")
                .map(String::as_str)
                .unwrap_or("");
            assert!(
                why.starts_with("parent: ") && why.contains(said),
                "{name}: {why:?}"
            );
        }
    }

    /// A work item titled after its slug: sorted into `parent`, a draft without one, or closed.
    fn item_doc(slug: &str, tag: &str, parent: Option<&str>, closed: bool) -> (String, String) {
        let text = GOOD
            .replace("title: Some problem", &format!("title: {slug}"))
            .replace("tags: [operations]", &format!("tags: [{tag}]"));
        let text = match parent {
            Some(parent) => text.replace(
                "status: draft",
                &format!("status: stable\nparent: {parent}"),
            ),
            None => text,
        };
        let text = if closed {
            crate::schema::closed_record(&text, "Fixed.")
        } else {
            text
        };
        (format!("{slug}.md"), text)
    }

    #[test]
    fn a_work_item_is_listed_under_its_spec_or_names_it() {
        let all = work(
            &spec_docs(
                vec![
                    spec_doc("big", "operations", None, false),
                    spec_doc("part", "operations", Some("big"), false),
                    // Under the epic, after its parts
                    item_doc("step-of-big", "operations", Some("big"), false),
                    // Under the part, one level deeper
                    item_doc("step-of-part", "operations", Some("part"), false),
                    // Another area: under its own area, naming the spec
                    item_doc("billing-step", "billing", Some("part"), false),
                    // Nobody has sorted it yet: in its area, after the specs
                    item_doc("unsorted", "operations", None, false),
                ],
                vec![
                    // Done while the spec is open: under Closed, naming it
                    item_doc("done-step", "operations", Some("big"), true),
                    spec_doc("old", "operations", None, true),
                    item_doc("old-step", "operations", Some("old"), true),
                ],
            ),
            &areas(),
        );
        assert!(all.problems.is_empty(), "{:?}", all.problems);
        assert_eq!(all.closed_before_its_children(), Vec::<String>::new());
        assert_eq!(
            outline(&render_work(&all, &areas())),
            [
                "# operations",
                "* [big](big.md",
                "  * [part](part.md",
                "    * [step-of-part](step-of-part.md",
                "  * [step-of-big](step-of-big.md",
                "* [unsorted](unsorted.md",
                "# billing",
                "* [billing-step](billing-step.md",
                "# Closed",
                "* [old](old.md",
                "  * [old-step](old-step.md",
                "* [done-step](done-step.md",
            ]
        );
        let index = render_work(&all, &areas());
        for (slug, parent) in [("billing-step", "part"), ("done-step", "big")] {
            assert!(
                index.contains(&format!("({slug}.md) - Something is wrong.")),
                "{index}"
            );
            let line = index
                .lines()
                .find(|l| l.contains(&format!("({slug}.md)")))
                .unwrap();
            assert!(
                line.ends_with(&format!("| Parent: [{parent}]({parent}.md)")),
                "{line}"
            );
        }
    }

    #[test]
    fn a_spec_closes_after_its_work_items() {
        let all = work(
            &spec_docs(
                vec![item_doc("step", "operations", Some("big"), false)],
                vec![spec_doc("big", "operations", None, true)],
            ),
            &areas(),
        );
        assert!(all.problems.is_empty(), "{:?}", all.problems);
        let found = all.closed_before_its_children();
        assert_eq!(
            found,
            [
                "work/step.md: its parent big is closed while this work item is open. Close the work item first (as \
              dropped, if it was), or reopen the parent"
            ]
        );
        // A draft names no parent, and closes whenever it is dealt with
        let all = work(
            &spec_docs(
                vec![item_doc("unsorted", "operations", None, false)],
                vec![spec_doc("big", "operations", None, true)],
            ),
            &areas(),
        );
        assert_eq!(all.closed_before_its_children(), Vec::<String>::new());
    }

    #[test]
    fn an_undefined_parent_leaves_the_work_item_out() {
        let cases = [
            ("no such spec", "nowhere", "names no spec"),
            ("another work item", "other-item", "names no spec"),
            ("itself", "p", "names the record itself"),
            ("a spec that fails", "broken", "left out of the index"),
            (
                "a spec whose own parent is undefined",
                "lost",
                "left out of the index",
            ),
        ];
        for (name, parent, said) in cases {
            let all = work(
                &spec_docs(
                    vec![
                        item_doc("p", "operations", Some(parent), false),
                        item_doc("other-item", "operations", None, false),
                        (
                            "broken.md".to_string(),
                            "---\ntype: Spec\ntitle: B\n---\n".to_string(),
                        ),
                        spec_doc("lost", "operations", Some("nowhere"), false),
                    ],
                    vec![],
                ),
                &areas(),
            );
            assert!(!all.items.contains_key("p.md"), "{name}: still listed");
            let why = all
                .problems
                .get("work/p.md")
                .map(String::as_str)
                .unwrap_or("");
            assert!(
                why.starts_with("parent: ") && why.contains(said),
                "{name}: {why:?}"
            );
        }
        // A part of an epic is a spec, and a work item may be a part of it
        let all = work(
            &spec_docs(
                vec![
                    spec_doc("top", "operations", None, false),
                    spec_doc("mid", "operations", Some("top"), false),
                    item_doc("p", "operations", Some("mid"), false),
                ],
                vec![],
            ),
            &areas(),
        );
        assert!(all.problems.is_empty(), "{:?}", all.problems);
    }

    /// A milestone titled after its slug: open with `status` and `date`, or closed when `status` is `deprecated`.
    fn milestone_doc(slug: &str, status: &str, date: Option<&str>) -> (String, String) {
        let date = date.map(|d| format!("date: {d}\n")).unwrap_or_default();
        let text = format!(
            "---\ntype: Milestone\ntitle: {slug}\ndescription: D.\ntags: [billing]\nstatus: {}\n{date}---\n\n\
             # Condition\n\nThe tag is pushed.\n",
            if status == "deprecated" {
                "stable"
            } else {
                status
            }
        );
        let text = if status == "deprecated" {
            crate::schema::closed_record(&text, "Pushed.")
        } else {
            text
        };
        (format!("{slug}.md"), text)
    }

    #[test]
    fn the_milestones_come_first_across_the_areas_by_date() {
        let all = work(
            &[
                milestone_doc("undated", "stable", None),
                milestone_doc("later", "draft", Some("2026-12-01")),
                milestone_doc("sooner", "stable", Some("2026-11-01")),
                milestone_doc("past", "deprecated", Some("2026-01-01")),
                spec_doc("old", "operations", None, true),
                item_doc("unsorted", "operations", None, false),
            ]
            .into_iter()
            .collect(),
            &areas(),
        );
        assert!(all.problems.is_empty(), "{:?}", all.problems);
        let index = render_work(&all, &areas());
        assert_eq!(
            index
                .lines()
                .filter(|l| l.contains("](") || l.starts_with('#'))
                .collect::<Vec<_>>(),
            [
                "# Milestones",
                "* [sooner](sooner.md) - D. | Date: 2026-11-01.",
                "* [later](later.md) - D. | Status: draft. | Date: 2026-12-01.",
                "* [undated](undated.md) - D.",
                "# operations",
                "* [unsorted](unsorted.md) - Something is wrong. | Status: draft. | State (2026-09-28): Not yet.",
                "# Closed",
                "* [past](past.md) - D. | Done: Pushed.",
                "* [old](old.md) - D. | Done: Implemented.",
            ]
        );
        let today = NaiveDate::from_ymd_opt(2026, 11, 15).unwrap();
        // The closed one is past its date too, but it is closed
        assert_eq!(
            all.past_their_date(today),
            [(
                "sooner.md".to_string(),
                NaiveDate::from_ymd_opt(2026, 11, 1).unwrap()
            )]
        );
        assert_eq!(all.without_an_open_milestone(), None);
    }

    #[test]
    fn a_project_always_has_an_open_milestone() {
        let cases = [
            ("none at all", vec![]),
            (
                "only closed ones",
                vec![milestone_doc("past", "deprecated", None)],
            ),
            // One that breaks the format does not count: the message about it says what to fix
            (
                "only a broken one",
                vec![(
                    "broken.md".to_string(),
                    "---\ntype: Milestone\ntitle: B\n---\n".to_string(),
                )],
            ),
        ];
        for (name, docs) in cases {
            let docs: Docs = docs.into_iter().collect();
            assert!(
                work(&docs, &areas())
                    .without_an_open_milestone()
                    .is_some_and(|why| why.starts_with("no open milestone in docs/work/")),
                "{name}"
            );
            // create writes one only when there is none at all, broken or closed
            assert_eq!(has_milestone(&docs), name != "none at all", "{name}");
        }
        // A proposed one is open
        let docs: Docs = [milestone_doc("next", "draft", None)].into_iter().collect();
        assert_eq!(work(&docs, &areas()).without_an_open_milestone(), None);
        // What create writes has no condition yet, so it does not count until a person writes one
        let docs: Docs = [("next-milestone.md".to_string(), first_milestone("billing"))]
            .into_iter()
            .collect();
        let read = work(&docs, &areas());
        assert!(
            read.problems["work/next-milestone.md"].contains("missing or empty: [\"Condition\"]"),
            "{:?}",
            read.problems
        );
        assert!(read.without_an_open_milestone().is_some());
    }

    #[test]
    fn a_milestone_is_no_parent() {
        let all = work(
            &[
                milestone_doc("release", "stable", None),
                item_doc("p", "operations", Some("release"), false),
            ]
            .into_iter()
            .collect(),
            &areas(),
        );
        assert!(
            all.problems["work/p.md"]
                .contains("names no spec in docs/work/: release (a work item, a milestone"),
            "{:?}",
            all.problems
        );
    }
}
