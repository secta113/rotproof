//! `docs/` as one OKF 0.2 bundle: reading its documents, and the index file each directory should contain.
//!
//! Every `.md` under `docs/` except the reserved names is a document with frontmatter. The index files are never
//! written by hand: `rotproof index` writes them, and `rotproof check` fails when one differs from what it would write.

use std::collections::BTreeMap;
use std::io;

use crate::layers::DECLARATION;
use crate::schema::{
    BacklogDoc, CLOSED_SECTION, DeadlineKind, Guide, Item, Knowledge, KnowledgeDoc, SPEC_FOLDERS,
    Spec, Status, Time, backlog_doc, guide_doc, knowledge_doc, spec,
};
use crate::tree::{Tree, read_text};
use utils::frontmatter::Sections;

/// File names OKF reserves. Never used for a document
pub const RESERVED: [&str; 2] = ["index.md", "log.md"];
/// The notice at the top of every generated index. An HTML comment, so OKF readers do not see it
pub const GENERATED: &str = "<!-- Generated from the frontmatter by `rotproof index`. Do not edit: `rotproof check` \
                             fails when this file differs from what `rotproof index` writes. -->";
/// The backlog rules. Rotproof writes them like an index file, so the rules a project reads are the rules its Rotproof
/// checks
pub const RULES: &str = include_str!("../records/rules.md");
/// The spec rules, written like the backlog rules
pub const SPEC_RULES: &str = include_str!("../records/spec-rules.md");
/// The knowledge rules, written like the backlog rules
pub const KNOWLEDGE_RULES: &str = include_str!("../records/knowledge-rules.md");
/// The one directory of specs that holds guides: the spec rules, and any a project adds
const GUIDES_AMONG_SPECS: &str = "specs";
/// The log as `rotproof create` makes it. From then on it is the project's
pub const LOG: &str = include_str!("../records/log.md");
/// The bundle-root index links to these, in this order
const ROOT_ENTRIES: [(&str, &str, &str); 4] = [
    ("Backlog", "backlog/", "Open problems and postponed work."),
    (
        "Specs",
        "specs/",
        "Proposed changes: being written, in progress, or closed.",
    ),
    ("Knowledge", "knowledge/", "How things are now, and why."),
    ("Log", "log.md", "What was done, newest first."),
];

/// File name -> text.
pub type Docs = BTreeMap<String, String>;
/// File name -> why it was left out.
pub type Problems = BTreeMap<String, String>;

/// The documents of `docs/backlog/`, sorted out.
#[derive(Debug, Default)]
pub struct Backlog {
    pub items: BTreeMap<String, (Item, Sections)>,
    pub guides: BTreeMap<String, Guide>,
    pub problems: Problems,
}

/// The documents of `docs/backlog/`, sorted out. An item whose area is not among `areas` is left out with why.
pub fn backlog(docs: &Docs, areas: &[String]) -> Backlog {
    let mut out = Backlog::default();
    for (name, text) in docs {
        match backlog_doc(text) {
            Ok(BacklogDoc::Item(item, _)) if !areas.contains(&item.tag) => {
                out.problems
                    .insert(name.clone(), undeclared(&item.tag, areas));
            }
            Ok(BacklogDoc::Item(item, sections)) => {
                out.items.insert(name.clone(), (item, sections));
            }
            Ok(BacklogDoc::Guide(guide)) => {
                out.guides.insert(name.clone(), guide);
            }
            Err(why) => {
                out.problems.insert(name.clone(), why);
            }
        }
    }
    out
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

/// The documents of one spec directory, sorted out.
#[derive(Debug, Default)]
pub struct SpecFolder {
    pub specs: BTreeMap<String, (Spec, Sections)>,
    /// Only `docs/specs/` holds guides, as the spec rules
    pub guides: BTreeMap<String, Guide>,
    pub problems: Problems,
}

/// The documents of `docs/<folder>/`: the specs and guides that pass, and why the others do not. A spec whose area is
/// not among `areas` does not pass.
pub fn specs(folder: &str, docs: &Docs, areas: &[String]) -> SpecFolder {
    let mut out = SpecFolder::default();
    for (name, text) in docs {
        let guide = (folder == GUIDES_AMONG_SPECS)
            .then(|| guide_doc(text))
            .flatten();
        match (guide, spec(folder, text)) {
            (Some(Ok(guide)), _) => {
                out.guides.insert(name.clone(), guide);
            }
            (Some(Err(why)), _) | (None, Err(why)) => {
                out.problems.insert(name.clone(), why);
            }
            (None, Ok((spec, _))) if !areas.contains(&spec.tag) => {
                out.problems
                    .insert(name.clone(), undeclared(&spec.tag, areas));
            }
            (None, Ok(parsed)) => {
                out.specs.insert(name.clone(), parsed);
            }
        }
    }
    out
}

/// The specs of every spec directory (`docs/specs/`), read together: a part names its epic by slug.
#[derive(Debug, Default)]
pub struct Specs {
    /// Directory -> file name -> the spec, for the specs that pass
    pub folders: BTreeMap<&'static str, BTreeMap<String, (Spec, Sections)>>,
    /// Directory -> file name -> the guide
    pub guides: BTreeMap<&'static str, BTreeMap<String, Guide>>,
    /// `<directory>/<file name>` -> why the document is left out of the index files
    pub problems: Problems,
}

impl Specs {
    /// The spec with this slug, and its directory.
    pub fn find(&self, slug: &str) -> Option<(&'static str, &Spec)> {
        let name = format!("{slug}.md");
        self.folders
            .iter()
            .find_map(|(folder, specs)| specs.get(&name).map(|(spec, _)| (*folder, spec)))
    }

    /// The open parts of a closed epic. An epic closes after its parts, so one of the two has the wrong status. They
    /// stay in the index files: the relation is clear, and leaving the open part out would hide open work.
    pub fn closed_before_its_parts(&self) -> Vec<String> {
        self.folders
            .iter()
            .flat_map(|(folder, specs)| specs.iter().map(move |spec| (folder, spec)))
            .filter_map(|(folder, (name, (spec, _)))| {
                let epic = spec.epic.as_ref()?;
                let (_, epic_spec) = self.find(epic)?;
                (spec.status != Status::Deprecated && epic_spec.status == Status::Deprecated).then(|| {
                    format!(
                        "{folder}/{name}: its epic {epic} is closed while this part is open. Close the part first \
                         (as dropped, if it was), or reopen the epic"
                    )
                })
            })
            .collect()
    }

    /// Take the specs at `<directory>/<file name>` out of the index files, with why.
    fn leave_out(&mut self, left: Problems) {
        for (path, why) in left {
            if let Some((folder, name)) = path.split_once('/')
                && let Some(specs) = self.folders.get_mut(folder)
            {
                specs.remove(name);
            }
            self.problems.insert(path, why);
        }
    }
}

/// Every spec of the spec directories (directory -> its documents), checked one by one and against each other. A spec
/// is left out of the index files when its relation to its epic is undefined: the epic is missing, is itself, or is a
/// part of another.
pub fn all_specs(folders: &[(&'static str, Docs)], areas: &[String]) -> Specs {
    let mut out = Specs::default();
    for (folder, docs) in folders {
        let read = specs(folder, docs, areas);
        out.folders.insert(folder, read.specs);
        out.guides.insert(folder, read.guides);
        out.problems.extend(
            read.problems
                .into_iter()
                .map(|(name, why)| (format!("{folder}/{name}"), why)),
        );
    }
    let mut left = Problems::new();
    for (folder, specs) in &out.folders {
        for (name, (spec, _)) in specs {
            let Some(epic) = &spec.epic else {
                continue;
            };
            let file = format!("{epic}.md");
            let why = if file == *name {
                "names the spec itself".to_string()
            } else {
                match out.find(epic) {
                    Some((_, epic_spec)) => match &epic_spec.epic {
                        Some(above) => format!(
                            "{epic} is itself a part of {above}: an epic is one level deep, so name {above} or \
                             remove one of the two"
                        ),
                        None => continue,
                    },
                    None if folders.iter().any(|(folder, _)| {
                        out.problems.contains_key(&format!("{folder}/{file}"))
                    }) =>
                    {
                        format!("{epic} is left out of the index files itself: fix it first")
                    }
                    None => format!(
                        "names no spec in docs/specs/: {epic} (a backlog item or a guide is not a \
                         spec)"
                    ),
                }
            };
            left.insert(format!("{folder}/{name}"), format!("epic: {why}"));
        }
    }
    out.leave_out(left);
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

/// The bundle of one repository, `docs/` in its tree, and the areas its records are grouped by.
pub struct Bundle<'a> {
    tree: &'a dyn Tree,
    /// In the order the index files list them
    pub areas: Vec<String>,
}

/// The path of `rest` in the bundle, from the root of the repository.
pub fn in_docs(rest: &str) -> String {
    format!("{DOCS}/{rest}")
}

impl<'a> Bundle<'a> {
    pub fn new(tree: &'a dyn Tree, areas: Vec<String>) -> Self {
        Bundle { tree, areas }
    }

    /// The documents directly in `docs/<folder>/`, except reserved names: file name -> text.
    pub fn read_folder(&self, folder: &str) -> io::Result<Docs> {
        let dir = in_docs(folder);
        let mut docs = Docs::new();
        for (name, is_dir) in self.tree.entries(&dir).map_err(|e| with_path(e, &dir))? {
            if name.ends_with(".md") && !RESERVED.contains(&name.as_str()) && !is_dir {
                let path = format!("{dir}/{name}");
                let text = read_text(self.tree, &path).map_err(|e| with_path(e, &path))?;
                docs.insert(name, text);
            }
        }
        Ok(docs)
    }

    /// Every file Rotproof generates in the bundle (the index files and the backlog rules) -> what it should contain
    /// now, and the documents left out of the index files.
    pub fn expected(&self) -> io::Result<(Vec<(String, String)>, Problems)> {
        // The rules as they are about to be written, so the backlog index lists them on the run that writes them
        let mut docs = self.read_folder("backlog")?;
        docs.insert("rules.md".into(), RULES.into());
        let backlog = backlog(&docs, &self.areas);
        let mut files = vec![
            (in_docs("backlog/rules.md"), RULES.into()),
            (in_docs("index.md"), render_root()),
            (
                in_docs("backlog/index.md"),
                render_backlog(&backlog.items, &backlog.guides, &self.areas),
            ),
        ];
        files.push((
            in_docs(&format!("{GUIDES_AMONG_SPECS}/rules.md")),
            SPEC_RULES.into(),
        ));
        let mut problems = backlog.problems;
        let specs = self.read_specs()?;
        for (folder, _) in SPEC_FOLDERS {
            files.push((
                in_docs(&format!("{folder}/index.md")),
                render_specs(folder, &specs, &self.areas),
            ));
        }
        problems.extend(specs.problems);
        let read = self.read_knowledge()?;
        files.push((in_docs("knowledge/rules.md"), KNOWLEDGE_RULES.into()));
        files.push((
            in_docs("knowledge/index.md"),
            render_knowledge(&read.documents, &read.guides, &self.areas),
        ));
        problems.extend(
            read.problems
                .into_iter()
                .map(|(name, why)| (format!("knowledge/{name}"), why)),
        );
        Ok((files, problems))
    }

    /// The documents of `docs/knowledge/`, with the knowledge rules read as Rotproof writes them, so the index lists
    /// them on the run that writes them.
    pub fn read_knowledge(&self) -> io::Result<KnowledgeFolder> {
        let mut docs = self.read_folder("knowledge")?;
        docs.insert("rules.md".into(), KNOWLEDGE_RULES.into());
        Ok(knowledge(&docs, &self.areas))
    }

    /// Every spec of `docs/specs/`, checked one by one and against each other. The spec rules are read
    /// as Rotproof writes them, so the index lists them on the run that writes them.
    pub fn read_specs(&self) -> io::Result<Specs> {
        let mut folders = SPEC_FOLDERS
            .iter()
            .map(|(folder, _)| Ok((*folder, self.read_folder(folder)?)))
            .collect::<io::Result<Vec<_>>>()?;
        for (folder, docs) in &mut folders {
            if *folder == GUIDES_AMONG_SPECS {
                docs.insert("rules.md".into(), SPEC_RULES.into());
            }
        }
        Ok(all_specs(&folders, &self.areas))
    }
}

fn with_path(e: io::Error, path: &str) -> io::Error {
    io::Error::new(e.kind(), format!("{path}: {e}"))
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

/// The backlog index (an OKF index.md): the guides, the open items by area in the order `areas` declares them, and the
/// closed items last so they do not bury the open ones.
///
/// Each entry has the OKF form `* [title](target) - description`, with the frontmatter's `description`. An open item
/// adds, after ` | `, the date of the last measurement, the first sentence of its state and its deadline: what a
/// reader of the backlog needs. The separator is a symbol so the parts stay apart in any language.
pub fn render_backlog(
    items: &BTreeMap<String, (Item, Sections)>,
    guides: &BTreeMap<String, Guide>,
    areas: &[String],
) -> String {
    let mut out = vec![GENERATED.to_string()];
    out.extend(guide_section(guides));
    let open: Vec<(&String, &(Item, Sections))> = items
        .iter()
        .filter(|(_, (item, _))| item.status == Status::Stable)
        .collect();
    for area in areas {
        let mut in_area: Vec<_> = open
            .iter()
            .filter(|(_, (item, _))| &item.tag == area)
            .collect();
        if in_area.is_empty() {
            continue;
        }
        in_area.sort_by_key(|(name, (item, _))| (item.filed, *name));
        out.extend(["".into(), format!("# {area}"), "".into()]);
        out.extend(
            in_area
                .into_iter()
                .map(|(name, (item, sections))| open_line(name, item, sections)),
        );
    }
    let closed: Vec<_> = items
        .iter()
        .filter(|(_, (item, _))| item.status == Status::Deprecated)
        .collect();
    if !closed.is_empty() {
        out.extend(["".into(), "# Closed".into(), "".into()]);
        for (name, (item, sections)) in closed {
            out.push(format!(
                "* [{}]({name}) - {} | Resolution: {}",
                link_text(&item.title),
                item.description,
                first_sentence(&sections[CLOSED_SECTION])
            ));
        }
    }
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

fn open_line(name: &str, item: &Item, sections: &Sections) -> String {
    // Only the date, in the time zone of the measurement: the time of day would not change what a reader does.
    // Whether stale_after has passed is not shown: the index would then depend on today's date, and the check that
    // compares it with the generated text would pass on some days and fail on others
    let measured = item.last_verified().at.date_naive();
    let deadline = match item.deadline_kind {
        DeadlineKind::Until => format!("Deadline: {}", item.deadline),
        DeadlineKind::NoDeadline => "No deadline.".to_string(),
    };
    let stale = item
        .stale_after
        .map(|at| format!(" | Re-measure after {}.", at.date_naive()))
        .unwrap_or_default();
    format!(
        "* [{}]({name}) - {} | State ({measured}): {} | {deadline}{stale}",
        link_text(&item.title),
        item.description,
        first_sentence(&sections["State"])
    )
}

/// The index of `specs/`: the open specs by area, in the order `areas` declares them, and the closed specs last under
/// `# Closed`, as the backlog index does, so they do not bury the open ones. An open spec shows its status; a closed
/// one its resolution.
///
/// A part in the same area as its epic, and open or closed as its epic is, is listed under it, indented. Any other part
/// is listed on its own with its epic after its line, so each spec appears once.
pub fn render_specs(folder: &str, all: &Specs, areas: &[String]) -> String {
    let empty = BTreeMap::new();
    let here = all.folders.get(folder).unwrap_or(&empty);
    let closed = |spec: &Spec| spec.status == Status::Deprecated;
    // The epic a part is listed under, in this index
    let nested_in = |spec: &Spec| -> Option<String> {
        let epic = spec.epic.as_ref()?;
        let name = format!("{epic}.md");
        let (epic_spec, _) = here.get(&name)?;
        (epic_spec.tag == spec.tag && closed(epic_spec) == closed(spec)).then_some(name)
    };
    let line = |name: &str, spec: &Spec, sections: &Sections| {
        let after = if spec.status == Status::Deprecated {
            format!("Resolution: {}", first_sentence(&sections[CLOSED_SECTION]))
        } else {
            format!("Status: {}.", spec.status.name())
        };
        let epic = match &spec.epic {
            Some(epic) if nested_in(spec).is_none() => {
                let (epic_folder, epic_spec) = all
                    .find(epic)
                    .expect("all_specs leaves out a part whose epic it cannot find");
                let target = if epic_folder == folder {
                    format!("{epic}.md")
                } else {
                    format!("../{epic_folder}/{epic}.md")
                };
                format!(" | Epic: [{}]({target})", link_text(&epic_spec.title))
            }
            _ => String::new(),
        };
        format!(
            "* [{}]({name}) - {} | {after}{epic}",
            link_text(&spec.title),
            spec.description
        )
    };
    let mut out = vec![GENERATED.to_string()];
    if let Some(guides) = all.guides.get(folder) {
        out.extend(guide_section(guides));
    }
    // Each top-level spec that `keep` takes, with its parts under it
    let section = |heading: &str, keep: &dyn Fn(&Spec) -> bool| {
        let top: Vec<_> = here
            .iter()
            .filter(|(_, (spec, _))| keep(spec) && nested_in(spec).is_none())
            .collect();
        if top.is_empty() {
            return Vec::new();
        }
        let mut lines = vec!["".into(), format!("# {heading}"), "".into()];
        for (name, (spec, sections)) in top {
            lines.push(line(name, spec, sections));
            for (part_name, (part, part_sections)) in here {
                if nested_in(part).as_ref() == Some(name) {
                    lines.push(format!("  {}", line(part_name, part, part_sections)));
                }
            }
        }
        lines
    };
    for area in areas {
        out.extend(section(area, &|spec| &spec.tag == area && !closed(spec)));
    }
    out.extend(section("Closed", &|spec| closed(spec)));
    out.join("\n") + "\n"
}

/// The knowledge index: the guides, the documents that hold by area in the order `areas` declares them, and the
/// deprecated ones last under `# Closed`, with what replaced them.
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
    for area in areas {
        let in_area: Vec<_> = documents
            .iter()
            .filter(|(_, (doc, _))| &doc.tag == area && doc.status == Status::Stable)
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

/// The open items past `stale_after`. As in OKF, an item is stale when `now >= stale_after`.
pub fn stale(items: &BTreeMap<String, (Item, Sections)>, now: Time) -> Vec<String> {
    items
        .iter()
        .filter(|(_, (item, _))| {
            item.status == Status::Stable && item.stale_after.is_some_and(|at| now >= at)
        })
        .map(|(name, _)| name.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use chrono::{FixedOffset, TimeZone};

    use super::*;

    const GOOD: &str = "---
type: Backlog Item
title: Some problem
description: Something is wrong.
tags: [operations]
status: stable
filed: 2026-09-27
verified: {by: human:someone, at: 2026-09-28T08:00:00+09:00}
deadline_kind: until
deadline: until the next deploy
---

# Trigger

The next deploy

# State

Not yet. Measured by hand.

# Details

[somewhere](/log.md)
";

    fn parsed(docs: &[(&str, String)]) -> Backlog {
        let docs: Docs = docs
            .iter()
            .map(|(name, text)| (name.to_string(), text.clone()))
            .collect();
        let parsed = backlog(&docs, &areas());
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
    fn an_open_item_shows_its_state_and_deadline() {
        // Measured at 08:00 in +09:00, which is the day before in UTC: the date is the one where it was measured
        let index = render_backlog(
            &parsed(&[("good.md", GOOD.into())]).items,
            &BTreeMap::new(),
            &areas(),
        );
        let line = "* [Some problem](good.md) - Something is wrong. | State (2026-09-28): Not yet. | Deadline: until \
                    the next deploy";
        assert_eq!(index, format!("{GENERATED}\n\n# operations\n\n{line}\n"));
    }

    #[test]
    fn a_title_with_brackets_stays_the_text_of_its_own_entry() {
        let title = r#"title: 'Evil ](fake.md) [hacked \ end'"#;
        let item = GOOD.replace("title: Some problem", title);
        let index = render_backlog(&parsed(&[("x.md", item)]).items, &BTreeMap::new(), &areas());
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
        let fresh = GOOD.replace(
            "deadline_kind:",
            "stale_after: 2027-03-31T00:00:00+09:00\ndeadline_kind:",
        );
        let parsed = parsed(&[("fresh.md", fresh), ("plain.md", GOOD.into())]);
        assert!(
            render_backlog(&parsed.items, &BTreeMap::new(), &areas())
                .contains("| Re-measure after 2027-03-31.")
        );
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
                "* [Knowledge rules](rules.md) - What goes in docs/knowledge/, how each document is written, and how \
                 the log names every edit. The format is OKF 0.2; rotproof check checks it.",
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
        let index = render_backlog(
            &parsed(&[("closed.md", closed)]).items,
            &BTreeMap::new(),
            &areas(),
        );
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
        let index = render_backlog(&parsed.items, &BTreeMap::new(), &areas());
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
        let parsed = backlog(&docs, &areas());
        assert!(parsed.items.is_empty());
        assert!(
            parsed.problems["x.md"].contains("\"Operations\" is not an area declared")
                && parsed.problems["x.md"].contains("operations, billing, unused"),
            "{:?}",
            parsed.problems
        );
        let spec =
            "---\ntype: Spec\ntitle: S\ndescription: D.\ntags: [nowhere]\nstatus: stable\n---\n";
        let SpecFolder {
            specs: passed,
            problems,
            ..
        } = specs(
            "specs",
            &[("s.md".to_string(), spec.to_string())].into(),
            &[],
        );
        assert!(passed.is_empty());
        assert!(
            problems["s.md"].contains("it declares none"),
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
        let all = all_specs(&[("specs", docs)], &areas());
        assert!(all.problems.is_empty(), "{:?}", all.problems);
        assert_eq!(
            outline(&render_specs("specs", &all, &areas())),
            [
                "# operations",
                "* [S](b.md",
                "# billing",
                "* [S](a.md",
                "* [S](c.md"
            ]
        );
    }

    /// A spec titled after its slug, open or closed.
    fn spec_doc(slug: &str, tag: &str, epic: Option<&str>, closed: bool) -> (String, String) {
        let epic = epic.map(|e| format!("epic: {e}\n")).unwrap_or_default();
        let (status, resolution) = if closed {
            ("deprecated", "\n# Resolution\n\nDone.\n")
        } else {
            ("stable", "")
        };
        (
            format!("{slug}.md"),
            format!(
                "---\ntype: Spec\ntitle: {slug}\ndescription: D.\ntags: [{tag}]\nstatus: {status}\n{epic}---\n{resolution}"
            ),
        )
    }

    /// `docs/specs/` holding the open and the closed specs together.
    fn folders(
        open: Vec<(String, String)>,
        closed: Vec<(String, String)>,
    ) -> Vec<(&'static str, Docs)> {
        vec![("specs", open.into_iter().chain(closed).collect())]
    }

    #[test]
    fn a_part_is_listed_under_its_epic_or_names_it() {
        let all = all_specs(
            &folders(
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
                    // Finished part of an open epic: under Closed, naming it
                    spec_doc("part-d", "operations", Some("big"), true),
                    // A closed epic and its closed part: under Closed, the part under its epic
                    spec_doc("old", "operations", None, true),
                    spec_doc("old-part", "operations", Some("old"), true),
                ],
            ),
            &areas(),
        );
        assert!(all.problems.is_empty(), "{:?}", all.problems);
        assert_eq!(all.closed_before_its_parts(), Vec::<String>::new());
        let index = render_specs("specs", &all, &areas());
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
                "* [part-c](part-c.md) - D. | Status: stable. | Epic: [big](big.md)",
                "# Closed",
                "* [old](old.md) - D. | Resolution: Done.",
                "  * [old-part](old-part.md) - D. | Resolution: Done.",
                "* [part-d](part-d.md) - D. | Resolution: Done. | Epic: [big](big.md)",
            ]
        );
    }

    #[test]
    fn an_epic_closes_after_its_parts() {
        let all = all_specs(
            &folders(
                vec![spec_doc("part", "operations", Some("big"), false)],
                vec![spec_doc("big", "operations", None, true)],
            ),
            &areas(),
        );
        // The open part stays listed, naming its closed epic, and the check names the order
        assert!(all.problems.is_empty(), "{:?}", all.problems);
        assert!(render_specs("specs", &all, &areas()).contains("| Epic: [big](big.md)"));
        let found = all.closed_before_its_parts();
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(found[0].starts_with("specs/part.md: its epic big is closed"));
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
            // A backlog item has a slug too, but it is not a spec
            (
                "a backlog item",
                spec_doc("p", "operations", Some("some-item"), false),
                "names no spec",
            ),
            (
                "itself",
                spec_doc("p", "operations", Some("p"), false),
                "names the spec itself",
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
            // The spec rules sit among the specs, but are a guide
            (
                "a guide",
                spec_doc("p", "operations", Some("rules"), false),
                "names no spec",
            ),
        ];
        for (name, part, said) in cases {
            let all = all_specs(
                &folders(
                    vec![
                        part,
                        spec_doc("top", "operations", None, false),
                        spec_doc("mid", "operations", Some("top"), false),
                        broken.clone(),
                        ("rules.md".to_string(), SPEC_RULES.to_string()),
                    ],
                    vec![],
                ),
                &areas(),
            );
            assert!(
                !all.folders["specs"].contains_key("p.md"),
                "{name}: still listed"
            );
            let why = all
                .problems
                .get("specs/p.md")
                .map(String::as_str)
                .unwrap_or("");
            assert!(
                why.starts_with("epic: ") && why.contains(said),
                "{name}: {why:?}"
            );
        }
    }
}
