//! The frontmatter of each record type: backlog items, guides, specs and knowledge documents.
//!
//! OKF lets a producer add any key, and tells readers not to reject one they do not know. So an unknown field passes
//! as an extension, unless it looks like a misspelling of a field the type reads (OKF's or Rotproof's): that one fails,
//! as a misspelled optional field would otherwise be silently dropped. Every field OKF defines for a concept (sections 4 and 5)
//! passes as OKF writes it, so a document another OKF tool wrote correctly does not fail. The fields of an Attested
//! Computation (section 10) do not, as no such document belongs in the records.

use std::fmt::Display;
use std::sync::LazyLock;

use chrono::{DateTime, FixedOffset, NaiveDate};
use regex::Regex;
use yaml_rust2::Yaml;
use yaml_rust2::yaml::Hash;

use utils::frontmatter::{Sections, first_heading, split};

/// A datetime with a time zone, as OKF writes every timestamp.
pub type Time = DateTime<FixedOffset>;

/// Body headings every backlog item needs. A closed item also needs `CLOSED_SECTION`
pub const SECTIONS: [&str; 3] = ["Trigger", "State", "Details"];
pub const CLOSED_SECTION: &str = "Resolution";
/// The fields only one type reads, with that type: a backlog item's `filed`, `deadline_kind` and `deadline`, and a
/// spec's `epic`. Found from the readers, so a field added to a type is in it with no list to update. On another type,
/// one is not an extension: it is a sign of the wrong `type`, under which the item's trigger and deadline, or the
/// spec's place in its epic, would go unchecked
static OWN_FIELDS: LazyLock<Vec<(&'static str, &'static str)>> = LazyLock::new(|| {
    let readers = [
        (
            "backlog item",
            keys_read(|fields| {
                item(fields);
            }),
        ),
        (
            "spec",
            keys_read(|fields| {
                spec_fields(fields);
            }),
        ),
        (
            "guide",
            keys_read(|fields| {
                guide(fields);
            }),
        ),
        (
            "knowledge document",
            keys_read(|fields| {
                knowledge_fields(fields);
            }),
        ),
    ];
    let mut own = Vec::new();
    for (kind, keys) in &readers {
        for key in keys {
            let elsewhere = readers
                .iter()
                .any(|(other, keys)| other != kind && keys.contains(key));
            if !elsewhere {
                own.push((*key, *kind));
            }
        }
    }
    own
});

/// Directory -> the statuses a spec in it may have. Every spec stays in `docs/specs/` when it closes, so its path, and
/// every link to it, never changes: the status alone says it is closed
pub const SPEC_FOLDERS: [(&str, &[Status]); 1] = [(
    "specs",
    &[Status::Draft, Status::Stable, Status::Deprecated],
)];

// OKF actors (section 7): `<producer>/<version>` for an agent, `human:<id>` for a person, `process:<id>`. OKF does not
// limit the characters of `<id>` (its own samples use `human:jsmith@acme`), so only whitespace is excluded
static ACTOR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(?:human:\S+|process:\S+|[^\s:/]+/\S+)$").unwrap());
// A deadline that is only a date or a datetime. Deadlines are events, and the reason for having none is not a date
// either. Only the notation of the date is read, not the words around it, so an event in any language passes: the
// language of the content is left to the project
static DATE_ONLY: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?x)^\s*(?:
            \d{4}-\d{1,2}(?:-\d{1,2})?            # 2026-10-31, 2026-10
          | \d{4}/\d{1,2}(?:/\d{1,2})?            # 2026/10/31, 2026/10
          | \d{4}\.\d{1,2}\.\d{1,2}                # 2026.10.31 (2026.10 alone reads as a version)
          | \d{1,2}[-/.]\d{1,2}[-/.]\d{4}          # 31.10.2026, 10/31/2026
          | \d{4}\s*年\s*\d{1,2}\s*月(?:\s*\d{1,2}\s*日)?  # 2026年10月31日, 2026年10月
        )(?:[T\ ][\d:.]+(?:Z|[+-]\d{2}:?\d{2})?)?\s*$",
    )
    .unwrap()
});

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Draft,
    Stable,
    Deprecated,
}

impl Status {
    pub fn name(self) -> &'static str {
        match self {
            Status::Draft => "draft",
            Status::Stable => "stable",
            Status::Deprecated => "deprecated",
        }
    }
}

/// Who and when: one entry of OKF's `verified`, or `generated`.
#[derive(Debug, Clone, PartialEq)]
pub struct Stamp {
    pub by: String,
    pub at: Time,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeadlineKind {
    /// Dropped if the trigger has not happened by the event in `deadline`
    Until,
    /// No deadline, with the reason in `deadline`
    NoDeadline,
}

/// The frontmatter of one backlog item.
#[derive(Debug, Clone)]
pub struct Item {
    pub title: String,
    /// What the problem is, in one sentence
    pub description: String,
    /// The area. The index groups items by it
    pub tag: String,
    /// `Stable` = open / `Deprecated` = closed. A closed item stays, so references to it keep working
    pub status: Status,
    pub filed: NaiveDate,
    /// Never empty. As in OKF, written as one mapping or a list of them
    pub verified: Vec<Stamp>,
    pub deadline_kind: DeadlineKind,
    pub deadline: String,
    /// When the recorded state goes out of date with time alone, the time after which it should be measured again.
    /// Unlike the deadline, it does not end the item, and passing it does not fail the check
    pub stale_after: Option<Time>,
}

impl Item {
    /// The last measurement: the newest `at`. On a tie, the one written first.
    pub fn last_verified(&self) -> &Stamp {
        // `max_by_key` keeps the last of equal keys, so search from the end to keep the first
        self.verified
            .iter()
            .rev()
            .max_by_key(|stamp| stamp.at)
            .unwrap()
    }
}

/// A document that is not an item, such as the backlog rules.
#[derive(Debug, Clone)]
pub struct Guide {
    pub title: String,
    pub description: String,
    pub status: Status,
}

#[derive(Debug, Clone)]
pub struct Spec {
    pub title: String,
    pub description: String,
    /// The area. The index files group specs by it
    pub tag: String,
    pub status: Status,
    /// The slug of the spec this one is a part of. Whether it names one is checked against the other specs, which the
    /// document alone does not know (`bundle.rs`)
    pub epic: Option<String>,
}

/// A document of how things are now, or why: it never closes while it holds, and is edited in place.
#[derive(Debug, Clone)]
pub struct Knowledge {
    pub title: String,
    pub description: String,
    /// The area. The index groups documents by it
    pub tag: String,
    /// `Stable` = holds / `Deprecated` = no longer holds. A deprecated document stays, so links to it keep working
    pub status: Status,
}

/// A document in `docs/backlog/`.
#[derive(Debug, Clone)]
pub enum BacklogDoc {
    Item(Item, Sections),
    Guide(Guide),
}

/// A document in `docs/backlog/`, or why it breaks the backlog format.
pub fn backlog_doc(text: &str) -> Result<BacklogDoc, String> {
    let (meta, sections) = split(text)?;
    let mut fields = Fields::new(&meta);
    if fields.peek("type") == Some(&Yaml::String("Guide".into())) {
        let guide = guide(&mut fields);
        return fields.finish(guide).map(BacklogDoc::Guide);
    }
    let item = item(&mut fields);
    let item = fields.finish(item)?;
    let required = SECTIONS
        .iter()
        .copied()
        .chain((item.status == Status::Deprecated).then_some(CLOSED_SECTION));
    let empty: Vec<&str> = required
        .filter(|s| sections.get(*s).is_none_or(|text| text.is_empty()))
        .collect();
    if !empty.is_empty() {
        return Err(format!("body headings missing or empty: {empty:?}"));
    }
    if item.status == Status::Deprecated {
        resolution_first(text, "item")?;
    }
    Ok(BacklogDoc::Item(item, sections))
}

/// For tests: a record closed as it should be, `status: deprecated` and `# Resolution` as the first heading of its body.
#[cfg(test)]
pub(crate) fn closed_record(text: &str, resolution: &str) -> String {
    let text = text.replace("status: stable", "status: deprecated");
    let end = text[4..]
        .find("\n---\n")
        .expect("a record with frontmatter")
        + 4
        + "\n---\n".len();
    format!(
        "{}\n# {CLOSED_SECTION}\n\n{resolution}\n{}",
        &text[..end],
        &text[end..]
    )
}

/// A closed record opens with `# Resolution`, so a reader who opens it from a link, or an agent reading from the top,
/// meets the closing before anything that reads as current.
fn resolution_first(text: &str, kind: &str) -> Result<(), String> {
    match first_heading(text) {
        Some(first) if first == CLOSED_SECTION => Ok(()),
        first => Err(format!(
            "a closed {kind} opens with # {CLOSED_SECTION}, before every other heading (its first heading is {})",
            first.map_or("none".into(), |first| format!("# {first}"))
        )),
    }
}

/// A guide, or why it breaks the format. `None` when the document's type is not `Guide`.
pub fn guide_doc(text: &str) -> Option<Result<Guide, String>> {
    // No frontmatter: not a guide, and the reader of the document's own type says why
    let Ok((meta, _)) = split(text) else {
        return None;
    };
    let mut fields = Fields::new(&meta);
    if fields.peek("type") != Some(&Yaml::String("Guide".into())) {
        return None;
    }
    let guide = guide(&mut fields);
    Some(fields.finish(guide))
}

/// A spec in `docs/<folder>/`, or why it breaks the format or does not belong in that folder.
pub fn spec(folder: &str, text: &str) -> Result<(Spec, Sections), String> {
    let (meta, sections) = split(text)?;
    let mut fields = Fields::new(&meta);
    let spec = spec_fields(&mut fields);
    let spec = fields.finish(spec)?;
    let allowed = SPEC_FOLDERS
        .iter()
        .find(|(name, _)| *name == folder)
        .map_or(&[][..], |(_, statuses)| *statuses);
    if !allowed.contains(&spec.status) {
        let names: Vec<&str> = allowed.iter().map(|s| s.name()).collect();
        return Err(format!(
            "status {} does not belong in {folder}/ ({})",
            spec.status.name(),
            names.join(", ")
        ));
    }
    if spec.status == Status::Deprecated
        && sections
            .get(CLOSED_SECTION)
            .is_none_or(|text| text.is_empty())
    {
        return Err(format!(
            "a closed spec needs a non-empty # {CLOSED_SECTION}"
        ));
    }
    if spec.status == Status::Deprecated {
        resolution_first(text, "spec")?;
    }
    Ok((spec, sections))
}

/// A document in `docs/knowledge/`.
#[derive(Debug, Clone)]
pub enum KnowledgeDoc {
    Knowledge(Knowledge, Sections),
    Guide(Guide),
}

/// A document in `docs/knowledge/`, or why it breaks the knowledge format.
pub fn knowledge_doc(text: &str) -> Result<KnowledgeDoc, String> {
    if let Some(guide) = guide_doc(text) {
        return guide.map(KnowledgeDoc::Guide);
    }
    let (meta, sections) = split(text)?;
    let mut fields = Fields::new(&meta);
    let knowledge = knowledge_fields(&mut fields);
    let knowledge = fields.finish(knowledge)?;
    if knowledge.status == Status::Deprecated {
        if sections
            .get(CLOSED_SECTION)
            .is_none_or(|text| text.is_empty())
        {
            return Err(format!(
                "a deprecated knowledge document needs a non-empty # {CLOSED_SECTION}: what replaced it"
            ));
        }
        resolution_first(text, "knowledge document")?;
    }
    Ok(KnowledgeDoc::Knowledge(knowledge, sections))
}

// Each reader below reads every field before it can return, so running it on an empty mapping lists its fields
// (`keys_read`)

fn spec_fields(fields: &mut Fields) -> Option<Spec> {
    fields.required("type", one_of(&["Spec"]));
    let title = fields.required("title", one_line);
    let description = fields.required("description", one_line);
    let status = fields.required(
        "status",
        status(&[Status::Draft, Status::Stable, Status::Deprecated]),
    );
    let tag = fields.required("tags", one_tag);
    let epic = fields.optional("epic", slug);
    fields.optional("verified", stamps);
    fields.optional("stale_after", time);
    okf_optional(fields);
    Some(Spec {
        title: title?,
        description: description?,
        tag: tag?,
        status: status?,
        epic: epic?,
    })
}

fn knowledge_fields(fields: &mut Fields) -> Option<Knowledge> {
    fields.required("type", one_of(&["Knowledge"]));
    let title = fields.required("title", one_line);
    let description = fields.required("description", one_line);
    let tag = fields.required("tags", one_tag);
    let status = fields.required("status", status(&[Status::Stable, Status::Deprecated]));
    fields.optional("verified", stamps);
    fields.optional("stale_after", time);
    okf_optional(fields);
    Some(Knowledge {
        title: title?,
        description: description?,
        tag: tag?,
        status: status?,
    })
}

fn item(fields: &mut Fields) -> Option<Item> {
    fields.required("type", one_of(&["Backlog Item"]));
    let title = fields.required("title", one_line);
    let description = fields.required("description", one_line);
    let tag = fields.required("tags", one_tag);
    let status = fields.required("status", status(&[Status::Stable, Status::Deprecated]));
    let filed = fields.required("filed", date);
    let verified = fields.required("verified", stamps);
    let deadline_kind = fields.required("deadline_kind", deadline_kind);
    let deadline = fields.required("deadline", one_line);
    let stale_after = fields.optional("stale_after", time);
    okf_optional(fields);
    let item = Item {
        title: title?,
        description: description?,
        tag: tag?,
        status: status?,
        filed: filed?,
        verified: verified?,
        deadline_kind: deadline_kind?,
        deadline: deadline?,
        stale_after: stale_after?,
    };
    if DATE_ONLY.is_match(&item.deadline) {
        fields.wrong(
            "deadline",
            format!("is an event or a reason, not a date: {}", item.deadline),
        );
    }
    if item.verified.is_empty() {
        fields.wrong("verified", "is an empty list");
        return None;
    }
    if let Some(stale_after) = item.stale_after {
        let last = item.last_verified().at;
        if stale_after <= last {
            fields.wrong(
                "stale_after",
                format!("({stale_after}) is not later than the last verified time ({last})"),
            );
        }
    }
    Some(item)
}

fn guide(fields: &mut Fields) -> Option<Guide> {
    fields.required("type", one_of(&["Guide"]));
    let title = fields.required("title", one_line);
    let description = fields.required("description", one_line);
    let status = fields
        .optional(
            "status",
            status(&[Status::Draft, Status::Stable, Status::Deprecated]),
        )
        .map(|status| status.unwrap_or(Status::Stable));
    fields.optional("tags", text_list);
    fields.optional("verified", stamps);
    fields.optional("stale_after", time);
    okf_optional(fields);
    Some(Guide {
        title: title?,
        description: description?,
        status: status?,
    })
}

/// Fields OKF defines that have no use here yet. They pass, so a document another OKF tool wrote does not fail.
fn okf_optional(fields: &mut Fields) {
    fields.optional("generated", generated);
    fields.optional("sources", sources);
    fields.optional("usage_window", usage_window);
    fields.optional("resource", text);
}

/// The fields a reader reads, found by running it on an empty mapping.
fn keys_read(reader: impl Fn(&mut Fields<'_>)) -> Vec<&'static str> {
    let empty = Hash::new();
    let mut fields = Fields::new(&empty);
    reader(&mut fields);
    fields.read
}

/// Reads the fields of one mapping, and remembers what was wrong and which keys were read. A key that no schema reads
/// is an unknown field: an extension, or a misspelling when it is close to a key that was read.
struct Fields<'a> {
    map: &'a Hash,
    read: Vec<&'static str>,
    errors: Vec<String>,
}

/// The result of reading one field: `None` once the error is recorded.
type Read<T> = Option<T>;

impl<'a> Fields<'a> {
    fn new(map: &'a Hash) -> Self {
        Fields {
            map,
            read: Vec::new(),
            errors: Vec::new(),
        }
    }

    /// The value without reading it. A null counts as absent, as `X | None = None` does.
    fn peek(&self, key: &str) -> Option<&'a Yaml> {
        match self.map.get(&Yaml::String(key.into())) {
            None | Some(Yaml::Null) => None,
            Some(value) => Some(value),
        }
    }

    fn wrong(&mut self, key: &str, why: impl Display) {
        self.errors.push(format!("{key}: {why}"));
    }

    fn convert<T>(
        &mut self,
        key: &str,
        value: &Yaml,
        convert: impl Fn(&Yaml) -> Result<T, String>,
    ) -> Read<T> {
        convert(value).map_err(|why| self.wrong(key, why)).ok()
    }

    fn required<T>(
        &mut self,
        key: &'static str,
        convert: impl Fn(&Yaml) -> Result<T, String>,
    ) -> Read<T> {
        self.read.push(key);
        match self.peek(key) {
            Some(value) => self.convert(key, value, convert),
            None => {
                self.wrong(key, "missing");
                None
            }
        }
    }

    /// `Some(None)` when absent, `None` when present and wrong.
    fn optional<T>(
        &mut self,
        key: &'static str,
        convert: impl Fn(&Yaml) -> Result<T, String>,
    ) -> Read<Option<T>> {
        self.read.push(key);
        match self.peek(key) {
            Some(value) => self.convert(key, value, convert).map(Some),
            None => Some(None),
        }
    }

    /// The value when every field passed and no key looks misspelled, or every error.
    fn finish<T>(mut self, value: Option<T>) -> Result<T, String> {
        for key in self.map.keys() {
            match key {
                Yaml::String(name) if self.read.contains(&name.as_str()) => {}
                Yaml::String(name) => {
                    if let Some(meant) = self.read.iter().find(|known| misspelled(name, known)) {
                        self.errors
                            .push(format!("unknown field: {name}; did you mean {meant}?"));
                    } else if let Some((theirs, kind)) =
                        OWN_FIELDS.iter().find(|(field, _)| misspelled(name, field))
                    {
                        self.errors.push(format!(
                            "unknown field: {name}; {theirs} is a field of a {kind}, not of this type"
                        ));
                    }
                }
                other => self.errors.push(format!("not a field name: {other:?}")),
            }
        }
        match value {
            Some(value) if self.errors.is_empty() => Ok(value),
            _ => Err(self.errors.join("; ")),
        }
    }
}

// --- converters: one YAML value to one typed value -----------------------------

/// A string. Strict as in the Python checks: a number or a boolean is not turned into text.
fn text(value: &Yaml) -> Result<String, String> {
    match value {
        Yaml::String(s) => Ok(s.clone()),
        other => Err(format!("not a string: {other:?}")),
    }
}

/// A string with something in it: spaces alone are as empty as nothing.
fn non_empty_text(value: &Yaml) -> Result<String, String> {
    text(value).and_then(|s| {
        if s.trim().is_empty() {
            Err("empty".into())
        } else {
            Ok(s)
        }
    })
}

/// A text the index lists on one line. A line break would end the entry there, and what follows it could read as a
/// heading or an entry of its own.
fn one_line(value: &Yaml) -> Result<String, String> {
    non_empty_text(value).and_then(|s| {
        if s.contains(['\n', '\r']) {
            Err(format!(
                "on more than one line, and the index lists it on one (in YAML, write it on one line, or fold it \
                 with >-, which keeps no line break at the end): {s:?}"
            ))
        } else {
            Ok(s)
        }
    })
}

fn one_of(allowed: &'static [&'static str]) -> impl Fn(&Yaml) -> Result<String, String> {
    move |value| {
        text(value).and_then(|s| {
            if allowed.contains(&s.as_str()) {
                Ok(s)
            } else {
                Err(format!("{s:?} is not one of {allowed:?}"))
            }
        })
    }
}

fn status(allowed: &'static [Status]) -> impl Fn(&Yaml) -> Result<Status, String> {
    move |value| {
        let s = text(value)?;
        allowed
            .iter()
            .copied()
            .find(|status| status.name() == s)
            .ok_or_else(|| {
                let names: Vec<&str> = allowed.iter().map(|status| status.name()).collect();
                format!("{s:?} is not one of {names:?}")
            })
    }
}

fn deadline_kind(value: &Yaml) -> Result<DeadlineKind, String> {
    match text(value)?.as_str() {
        "until" => Ok(DeadlineKind::Until),
        "none" => Ok(DeadlineKind::NoDeadline),
        other => Err(format!("{other:?} is not one of [\"until\", \"none\"]")),
    }
}

fn date(value: &Yaml) -> Result<NaiveDate, String> {
    let s = text(value)?;
    NaiveDate::parse_from_str(&s, "%Y-%m-%d").map_err(|_| format!("not a date (YYYY-MM-DD): {s}"))
}

/// A datetime with a time zone. A date alone fails: filling in 00:00 would invent a time nobody measured.
fn time(value: &Yaml) -> Result<Time, String> {
    let s = text(value)?;
    DateTime::parse_from_rfc3339(&s)
        .map_err(|_| format!("not a datetime with a time zone (ISO 8601): {s}"))
}

fn stamp(value: &Yaml) -> Result<Stamp, String> {
    let Yaml::Hash(map) = value else {
        return Err(format!("not a {{by, at}} mapping: {value:?}"));
    };
    let mut fields = Fields::new(map);
    let by = fields.required("by", actor);
    let at = fields.required("at", time);
    fields.finish(by.zip(at).map(|(by, at)| Stamp { by, at }))
}

fn actor(value: &Yaml) -> Result<String, String> {
    let s = text(value)?;
    if ACTOR.is_match(&s) {
        Ok(s)
    } else {
        Err(format!(
            "not an actor (human:<id>, process:<id> or <producer>/<version>): {s}"
        ))
    }
}

/// Whether `key` looks like a misspelling of `known`: the same once case, `_` and `-` are ignored, or a few edits
/// away (one for a name of up to 6 letters, two for a longer one; none for up to 3, where one edit makes another word).
fn misspelled(key: &str, known: &str) -> bool {
    let plain = |s: &str| -> Vec<char> {
        s.chars()
            .filter(|c| *c != '_' && *c != '-')
            .flat_map(char::to_lowercase)
            .collect()
    };
    let (key, known) = (plain(key), plain(known));
    let allowed = match known.len() {
        0..=3 => 0,
        4..=6 => 1,
        _ => 2,
    };
    edits(&key, &known) <= allowed
}

/// The number of insertions, deletions, substitutions and swaps of two neighbours that turn `a` into `b` (optimal
/// string alignment).
fn edits(a: &[char], b: &[char]) -> usize {
    let mut d = vec![vec![0; b.len() + 1]; a.len() + 1];
    for (i, row) in d.iter_mut().enumerate() {
        row[0] = i;
    }
    for (j, cell) in d[0].iter_mut().enumerate() {
        *cell = j;
    }
    for i in 1..=a.len() {
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            d[i][j] = (d[i - 1][j] + 1)
                .min(d[i][j - 1] + 1)
                .min(d[i - 1][j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                d[i][j] = d[i][j].min(d[i - 2][j - 2] + 1);
            }
        }
    }
    d[a.len()][b.len()]
}

/// How the content was produced: `by` is required, and `at` is optional (OKF 0.2, section 5.2).
fn generated(value: &Yaml) -> Result<(), String> {
    let Yaml::Hash(map) = value else {
        return Err(format!("not a {{by, at}} mapping: {value:?}"));
    };
    let mut fields = Fields::new(map);
    let by = fields.required("by", actor);
    let at = fields.optional("at", time);
    fields.finish(by.zip(at).map(|_| ()))
}

/// The materials a document derives from: a list of entries, each with a `resource` (OKF 0.2, section 5.1).
fn sources(value: &Yaml) -> Result<(), String> {
    let Yaml::Array(list) = value else {
        return Err(format!("not a list: {value:?}"));
    };
    list.iter().try_for_each(source)
}

fn source(value: &Yaml) -> Result<(), String> {
    let Yaml::Hash(map) = value else {
        return Err(format!("not a source mapping: {value:?}"));
    };
    let mut fields = Fields::new(map);
    let resource = fields.required("resource", non_empty_text);
    fields.optional("id", non_empty_text);
    fields.optional("title", text);
    // Not checked as an actor: OKF's own example of a source has `author: team:ga4-docs`, which is none of the forms
    // of section 7
    fields.optional("author", non_empty_text);
    fields.optional("usage_count", count);
    fields.optional("last_modified", time);
    fields.optional("usage_window", usage_window);
    fields.finish(resource.map(|_| ()))
}

/// The `{from, to}` datetime range that frames every `usage_count`.
fn usage_window(value: &Yaml) -> Result<(), String> {
    let Yaml::Hash(map) = value else {
        return Err(format!("not a {{from, to}} mapping: {value:?}"));
    };
    let mut fields = Fields::new(map);
    let from = fields.required("from", time);
    let to = fields.required("to", time);
    fields.finish(from.zip(to).map(|_| ()))
}

fn count(value: &Yaml) -> Result<i64, String> {
    match value {
        Yaml::Integer(n) if *n >= 0 => Ok(*n),
        other => Err(format!("not a count: {other:?}")),
    }
}

/// One `{by, at}` mapping or a list of them.
fn stamps(value: &Yaml) -> Result<Vec<Stamp>, String> {
    match value {
        Yaml::Array(list) => list.iter().map(stamp).collect(),
        other => stamp(other).map(|one| vec![one]),
    }
}

/// A list of tags, each on one line. An empty tag names no area.
fn text_list(value: &Yaml) -> Result<Vec<String>, String> {
    match value {
        Yaml::Array(list) => list.iter().map(one_line).collect(),
        other => Err(format!("not a list: {other:?}")),
    }
}

/// The slug of a document: its file name without `.md`, which never changes once the file exists. Not a path: the slug
/// is what commit messages and other documents name it by.
fn slug(value: &Yaml) -> Result<String, String> {
    let s = one_line(value)?;
    if s.contains(['/', '\\']) {
        Err(format!(
            "{s} is a path; write the slug, the file name without .md"
        ))
    } else if let Some(stem) = s.strip_suffix(".md") {
        Err(format!("{s} is a file name; write the slug, {stem}"))
    } else if s.trim() != s {
        Err(format!("{s:?} has a space at one end"))
    } else {
        Ok(s)
    }
}

/// Exactly one tag: the area the index groups a backlog item or a spec by. Whether it is declared is checked against
/// the declaration, which the document alone does not know (`bundle.rs`).
fn one_tag(value: &Yaml) -> Result<String, String> {
    let mut tags = text_list(value)?;
    match tags.len() {
        1 => Ok(tags.remove(0)),
        n => Err(format!(
            "{n} tags; a backlog item or a spec has exactly one area"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A valid item. Each bad input below changes exactly one thing in it
    const GOOD: &str = "---
type: Backlog Item
title: Some problem
description: Something is wrong.
tags: [operations]
status: stable
filed: 2026-09-27
verified: {by: human:someone, at: 2026-09-28T10:00:00+09:00}
deadline_kind: until
deadline: until the next deploy
---

# Trigger

The next deploy

# State

Not yet.

# Details

[somewhere](/log.md)
";
    const AT: &str = "at: 2026-09-28T10:00:00+09:00";

    fn good(from: &str, to: &str) -> String {
        assert!(
            GOOD.contains(from),
            "{from:?} is not in GOOD, so the case would change nothing"
        );
        GOOD.replacen(from, to, 1)
    }

    fn passing(cases: &[(&str, String)]) -> Vec<String> {
        cases
            .iter()
            .filter_map(|(name, text)| backlog_doc(text).ok().map(|_| name.to_string()))
            .collect()
    }

    fn failing(cases: &[(&str, String)]) -> Vec<(String, String)> {
        cases
            .iter()
            .filter_map(|(name, text)| backlog_doc(text).err().map(|why| (name.to_string(), why)))
            .collect()
    }

    #[test]
    fn the_good_input_passes() {
        // If the valid item did not pass, a failure below would not show that the one change was caught
        assert!(backlog_doc(GOOD).is_ok(), "{:?}", backlog_doc(GOOD).err());
    }

    const KNOWLEDGE: &str = "---
type: Knowledge
title: The API
description: How the API is shaped, and why.
tags: [operations]
status: stable
---

# Shape

Text.
";

    #[test]
    fn a_knowledge_document_is_read() {
        let Ok(KnowledgeDoc::Knowledge(doc, _)) = knowledge_doc(KNOWLEDGE) else {
            panic!("{:?}", knowledge_doc(KNOWLEDGE).err());
        };
        assert_eq!(
            (doc.tag.as_str(), doc.status),
            ("operations", Status::Stable)
        );
        let gone = closed_record(KNOWLEDGE, "Replaced by the v2 document.");
        assert!(
            knowledge_doc(&gone).is_ok(),
            "{:?}",
            knowledge_doc(&gone).err()
        );
        // The rules are a guide in the same directory
        let rules = "---\ntype: Guide\ntitle: R\ndescription: D.\n---\n";
        assert!(matches!(knowledge_doc(rules), Ok(KnowledgeDoc::Guide(_))));
    }

    #[test]
    fn a_broken_knowledge_document_is_caught() {
        let bad = [
            ("no area", KNOWLEDGE.replace("tags: [operations]\n", "")),
            (
                "a draft",
                KNOWLEDGE.replace("status: stable", "status: draft"),
            ),
            (
                "deprecated without a resolution",
                KNOWLEDGE.replace("status: stable", "status: deprecated"),
            ),
            (
                "the resolution after another heading",
                KNOWLEDGE.replace("status: stable", "status: deprecated")
                    + "\n# Resolution\n\nGone.\n",
            ),
            // A spec's own field: the wrong type
            (
                "an epic",
                KNOWLEDGE.replace("status: stable", "status: stable\nepic: big"),
            ),
            ("a misspelled title", KNOWLEDGE.replace("title:", "titel:")),
        ];
        for (name, text) in bad {
            assert!(knowledge_doc(&text).is_err(), "{name} passed");
        }
    }

    #[test]
    fn a_closed_item_opens_with_its_resolution() {
        let closed = closed_record(GOOD, "Fixed.");
        assert!(
            backlog_doc(&closed).is_ok(),
            "{:?}",
            backlog_doc(&closed).err()
        );
        let at_the_end =
            good("status: stable", "status: deprecated") + "\n# Resolution\n\nFixed.\n";
        assert_eq!(
            backlog_doc(&at_the_end).err(),
            Some(
                "a closed item opens with # Resolution, before every other heading (its first heading is # Trigger)"
                    .into()
            )
        );
        // A heading only in a comment does not count: the reader sees # Trigger first
        let hidden = at_the_end.replacen("\n# Trigger", "\n<!--\n# Resolution\n-->\n# Trigger", 1);
        assert!(backlog_doc(&hidden).is_err());
    }

    #[test]
    fn each_type_s_own_fields_are_found_from_the_readers() {
        assert_eq!(
            *OWN_FIELDS,
            [
                ("filed", "backlog item"),
                ("deadline_kind", "backlog item"),
                ("deadline", "backlog item"),
                ("epic", "spec"),
            ]
        );
        // Every field the item reads is found, optional ones included
        let item_keys = keys_read(|fields| {
            item(fields);
        });
        for key in [
            "stale_after",
            "generated",
            "sources",
            "usage_window",
            "resource",
        ] {
            assert!(item_keys.contains(&key), "{key} not found: {item_keys:?}");
        }
    }

    #[test]
    fn a_type_s_own_field_fails_on_every_other_type() {
        let guide = "---\ntype: Guide\ntitle: Rules\ndescription: What goes here.\n---\n\n# What goes here\n";
        assert!(OWN_FIELDS.len() >= 4, "{:?}", *OWN_FIELDS);
        for (field, kind) in OWN_FIELDS.iter() {
            let on_guide = guide.replace("type: Guide", &format!("type: Guide\n{field}: x"));
            assert!(backlog_doc(&on_guide).is_err(), "{field} passed on a guide");
            if *kind != "spec" {
                let on_spec =
                    SPEC.replace("status: stable", &format!("status: stable\n{field}: x"));
                assert!(spec("specs", &on_spec).is_err(), "{field} passed on a spec");
            }
            if *kind != "backlog item" {
                let on_item =
                    GOOD.replace("status: stable", &format!("status: stable\n{field}: x"));
                let why = backlog_doc(&on_item).err();
                assert!(
                    why.as_ref()
                        .is_some_and(|why| why.contains(&format!("a field of a {kind}"))),
                    "{field} on a backlog item: {why:?}"
                );
            }
        }
    }

    #[test]
    fn misspelled_reads_close_names_only() {
        let close = [
            ("stale_afer", "stale_after"),
            ("staleAfter", "stale_after"),
            ("STALE-AFTER", "stale_after"),
            ("taggs", "tags"),
            ("tgas", "tags"),
            ("BY", "by"),
            ("deadlien", "deadline"),
        ];
        for (key, known) in close {
            assert!(misspelled(key, known), "{key} not taken for {known}");
        }
        let far = [
            ("confidence", "description"),
            ("owner", "title"),
            ("bx", "by"),
            ("source_kind", "sources"),
            ("tagline", "tags"),
        ];
        for (key, known) in far {
            assert!(!misspelled(key, known), "{key} taken for {known}");
        }
    }

    #[test]
    fn what_okf_allows_passes() {
        let two = "verified:\n  - {by: human:someone, at: 2026-09-28T10:00:00+09:00}\
                   \n  - {by: process:nightly, at: 2026-09-29T02:00:00Z}";
        let one = "verified: {by: human:someone, at: 2026-09-28T10:00:00+09:00}";
        let okf = [
            ("verified as a list", good(one, two)),
            (
                "optional OKF fields",
                good(
                    "status: stable",
                    "status: stable\ngenerated: {by: agent/v1, at: 2026-09-27T09:00:00Z}\
                     \nresource: https://example.com/x\nsources: [{resource: https://example.com/doc}]",
                ),
            ),
            // OKF lets a producer add any key: one that is not close to a known field is an extension
            (
                "an extension field",
                good(
                    "status: stable",
                    "status: stable\nconfidence: high\nowner: team:records",
                ),
            ),
            (
                "an extension in a source",
                good(
                    "status: stable",
                    "status: stable\nsources: [{resource: x, accessed: 2026-10-01T00:00:00Z}]",
                ),
            ),
            // OKF requires only `by` in `generated`
            (
                "generated without at",
                good(
                    "status: stable",
                    "status: stable\ngenerated: {by: human:someone}",
                ),
            ),
            // The example of section 5.1, author and all
            (
                "sources with every signal, and a usage window",
                good(
                    "status: stable",
                    "status: stable\nsources:\n  - id: ga4-schema\
                     \n    resource: https://developers.google.com/analytics/bigquery/export-schema\
                     \n    title: GA4 BigQuery Export schema\n    author: team:ga4-docs\n    usage_count: 5000\
                     \n    last_modified: 2026-05-30T00:00:00Z\
                     \nusage_window: { from: 2026-06-01T00:00:00Z, to: 2026-06-30T00:00:00Z }",
                ),
            ),
            // A long text folded over lines in YAML is one line once read
            (
                "a description folded with >-",
                good(
                    "description: Something is wrong.",
                    "description: >-\n  Something\n  is wrong.",
                ),
            ),
            // The content may be in any language
            (
                "a deadline in Japanese",
                good(
                    "deadline: until the next deploy",
                    "deadline: 次のデプロイまで",
                ),
            ),
            // An event may carry a date; only a date alone is not an event
            (
                "an event with a date in it",
                good(
                    "deadline: until the next deploy",
                    "deadline: until the release planned for 2026/10/31",
                ),
            ),
            (
                "an event with a Japanese date in it",
                good(
                    "deadline: until the next deploy",
                    "deadline: 2026年10月31日のリリースまで",
                ),
            ),
            // Quoting does not change a value (YAML 1.2). PyYAML read a quoted datetime as a string, and it failed
            (
                "a quoted datetime",
                good(AT, "at: '2026-09-28T10:00:00+09:00'"),
            ),
            (
                "a quoted date",
                good("filed: 2026-09-27", "filed: '2026-09-27'"),
            ),
            // OKF does not limit the characters of an actor's id; its own samples use this one
            (
                "an actor with an at sign",
                good("human:someone", "human:jsmith@acme"),
            ),
            (
                "no deadline, with the reason",
                good("deadline_kind: until", "deadline_kind: none"),
            ),
        ];
        let failed = failing(&okf);
        assert!(failed.is_empty(), "{failed:?}");
        // The newest measurement is the last one (used for the index date and for stale_after)
        let Ok(BacklogDoc::Item(item, _)) = backlog_doc(&okf[0].1) else {
            panic!()
        };
        assert_eq!(item.last_verified().by, "process:nightly");
    }

    #[test]
    fn a_broken_document_is_caught() {
        let bad = [
            ("no trigger", good("# Trigger\n\nThe next deploy\n\n", "")),
            ("empty state", good("Not yet.\n", "")),
            // A reader of the rendered page sees neither a comment nor what a code block holds
            (
                "a state that is only a comment",
                good("Not yet.\n", "<!-- Not yet. -->\n"),
            ),
            (
                "headings only inside an unclosed code block",
                good("The next deploy\n", "The next deploy\n\n```\n"),
            ),
            ("no verified time", good(&format!(", {AT}"), "")),
            // OKF asks for a datetime. With a date only, someone would have to invent the time
            ("verified date only", good(AT, "at: 2026-09-28")),
            (
                "verified without a time zone",
                good(AT, "at: 2026-09-28T10:00:00"),
            ),
            (
                "empty verified list",
                good(
                    &format!("verified: {{by: human:someone, {AT}}}"),
                    "verified: []",
                ),
            ),
            ("actor in the wrong form", good("human:someone", "someone")),
            // OKF requires these within their mappings
            (
                "a source without a resource",
                good("status: stable", "status: stable\nsources: [{title: x}]"),
            ),
            (
                "generated without by",
                good(
                    "status: stable",
                    "status: stable\ngenerated: {at: 2026-09-27T09:00:00Z}",
                ),
            ),
            (
                "a usage window without to",
                good(
                    "status: stable",
                    "status: stable\nusage_window: {from: 2026-06-01T00:00:00Z}",
                ),
            ),
            (
                "a misspelled field in a source",
                good(
                    "status: stable",
                    "status: stable\nsources: [{resource: x, usage_cuont: 3}]",
                ),
            ),
            (
                "actor with a space",
                good("human:someone", "'human:some one'"),
            ),
            (
                "deadline is a date",
                good("deadline: until the next deploy", "deadline: 2026-12-31"),
            ),
            (
                "deadline is a quoted date",
                good(
                    "deadline: until the next deploy",
                    "deadline: \"2026-12-31\"",
                ),
            ),
            (
                "deadline is a datetime",
                good(
                    "deadline: until the next deploy",
                    "deadline: \"2026-12-31T00:00:00+09:00\"",
                ),
            ),
            // A date in another notation is still only a date
            (
                "deadline is a date with slashes",
                good("deadline: until the next deploy", "deadline: 2026/10/31"),
            ),
            (
                "deadline is a date with dots, day first",
                good("deadline: until the next deploy", "deadline: 31.10.2026"),
            ),
            (
                "deadline is a date, month first",
                good("deadline: until the next deploy", "deadline: 10/31/2026"),
            ),
            (
                "deadline is a month",
                good("deadline: until the next deploy", "deadline: 2026-10"),
            ),
            (
                "deadline is a date in Japanese",
                good(
                    "deadline: until the next deploy",
                    "deadline: 2026年10月31日",
                ),
            ),
            (
                "deadline is a month in Japanese",
                good("deadline: until the next deploy", "deadline: 2026年10月"),
            ),
            (
                "deadline is a date and a time with slashes",
                good(
                    "deadline: until the next deploy",
                    "deadline: \"2026/10/31 18:00\"",
                ),
            ),
            // The reason for having no deadline is not a date either
            (
                "no deadline, with a date for the reason",
                good(
                    "deadline_kind: until\ndeadline: until the next deploy",
                    "deadline_kind: none\ndeadline: 2026-12-31",
                ),
            ),
            ("no deadline kind", good("deadline_kind: until\n", "")),
            (
                "unknown field",
                good("status: stable", "status: stable\nstatu: stable"),
            ),
            // Misspellings of a field OKF defines, and of one Rotproof adds, in the forms they take
            (
                "a misspelled optional field",
                good(
                    "status: stable",
                    "status: stable\nstale_afer: 2026-12-31T00:00:00Z",
                ),
            ),
            (
                "a field in another case style",
                good(
                    "status: stable",
                    "status: stable\nstaleAfter: 2026-12-31T00:00:00Z",
                ),
            ),
            (
                "a field with a hyphen",
                good(
                    "status: stable",
                    "status: stable\nstale-after: 2026-12-31T00:00:00Z",
                ),
            ),
            (
                "two letters swapped",
                good("status: stable", "status: stable\nsoruces: []"),
            ),
            (
                "a field in capitals",
                good("status: stable", "status: stable\nTitle: x"),
            ),
            (
                "a misspelled field Rotproof adds",
                good(
                    "status: stable",
                    "status: stable\ndeadlin: until the next deploy",
                ),
            ),
            (
                "a key that is not a name",
                good("status: stable", "status: stable\n2026: x"),
            ),
            (
                "closed without a resolution",
                good("status: stable", "status: deprecated"),
            ),
            (
                "no frontmatter",
                GOOD.splitn(3, "---\n").nth(2).unwrap().to_string(),
            ),
            (
                "unquoted colon",
                good(
                    "description: Something is wrong.",
                    "description: how to measure: count",
                ),
            ),
            (
                "a number for a title",
                good("title: Some problem", "title: 123"),
            ),
            (
                "two tags",
                good("tags: [operations]", "tags: [operations, other]"),
            ),
            // Spaces alone are as empty as nothing
            ("a blank title", good("title: Some problem", "title: \" \"")),
            (
                "a blank deadline",
                good("deadline: until the next deploy", "deadline: \"  \""),
            ),
            ("an empty tag", good("tags: [operations]", "tags: [\"\"]")),
            ("a blank tag", good("tags: [operations]", "tags: [\" \"]")),
            // The index lists each of these on one line: a line break would end the entry and start another
            (
                "a description on two lines",
                good(
                    "description: Something is wrong.",
                    "description: \"Wrong.\\n\\n# INJECTED HEADING\\n\\n* Fake entry\"",
                ),
            ),
            (
                "a title on two lines",
                good("title: Some problem", "title: \"Some\\nproblem\""),
            ),
            (
                "a deadline on two lines",
                good(
                    "deadline: until the next deploy",
                    "deadline: \"until\\r\\nthe next deploy\"",
                ),
            ),
            (
                "a tag on two lines",
                good("tags: [operations]", "tags: [\"oper\\nations\"]"),
            ),
            // A folded text keeps one line break at its end unless it is folded with >-
            (
                "a folded description",
                good(
                    "description: Something is wrong.",
                    "description: >\n  Something\n  is wrong.",
                ),
            ),
            (
                "filed is not a date",
                good("filed: 2026-09-27", "filed: 2026-13-01"),
            ),
            (
                "stale_after equal to the verified time",
                good(
                    "deadline_kind:",
                    "stale_after: 2026-09-28T10:00:00+09:00\ndeadline_kind:",
                ),
            ),
            (
                "stale_after date only",
                good("deadline_kind:", "stale_after: 2027-03-31\ndeadline_kind:"),
            ),
            (
                "stale_after not a date",
                good(
                    "deadline_kind:",
                    "stale_after: in six months\ndeadline_kind:",
                ),
            ),
            (
                "a spec in the backlog",
                good("type: Backlog Item", "type: Spec"),
            ),
        ];
        let passed = passing(&bad);
        assert!(passed.is_empty(), "passed: {passed:?}");
    }

    #[test]
    fn a_guide_passes_with_its_own_fields() {
        let rules = "---\ntype: Guide\ntitle: Backlog rules\ndescription: What goes here.\n---\n\n# What goes here\n";
        let Ok(BacklogDoc::Guide(guide)) = backlog_doc(rules) else {
            panic!("{:?}", backlog_doc(rules))
        };
        assert_eq!(guide.status, Status::Stable);
        for bad in [
            rules.replace("description: What goes here.\n", ""),
            rules.replace("type: Guide", "type: Guide\ndeadline: never"),
        ] {
            assert!(backlog_doc(&bad).is_err(), "{bad}");
        }
    }

    const SPEC: &str = "---
type: Spec
title: Something
description: One sentence.
tags: [operations]
status: stable
---

# Goals

Something.
";

    #[test]
    fn a_good_spec_passes() {
        assert!(
            spec("specs", SPEC).is_ok(),
            "{:?}",
            spec("specs", SPEC).err()
        );
        let part = SPEC.replace("status: stable", "status: stable\nepic: big-work");
        assert_eq!(
            spec("specs", &part).map(|(spec, _)| spec.epic),
            Ok(Some("big-work".into()))
        );
        let closed = closed_record(SPEC, "Done.");
        assert!(
            spec("specs", &closed).is_ok(),
            "{:?}",
            spec("specs", &closed).err()
        );
        // The Resolution at the end, as it was written before it had to come first
        let at_the_end =
            SPEC.replace("status: stable", "status: deprecated") + "\n# Resolution\n\nDone.\n";
        assert_eq!(
            spec("specs", &at_the_end).err(),
            Some(
                "a closed spec opens with # Resolution, before every other heading (its first heading is # Goals)"
                    .into()
            )
        );
    }

    #[test]
    fn a_broken_spec_is_caught() {
        let closed = SPEC.replace("status: stable", "status: deprecated");
        let bad = [
            (
                "specs",
                "no description",
                SPEC.replace("description: One sentence.\n", ""),
            ),
            (
                "specs",
                "a blank description",
                SPEC.replace("description: One sentence.", "description: \" \""),
            ),
            (
                "specs",
                "an empty tag",
                SPEC.replace("tags: [operations]", "tags: [\"\"]"),
            ),
            // A spec has exactly one area, as a backlog item does
            ("specs", "no tag", SPEC.replace("tags: [operations]\n", "")),
            // An epic is named by its slug, not by a path
            (
                "specs",
                "an epic named by its path",
                SPEC.replace("status: stable", "status: stable\nepic: /specs/big.md"),
            ),
            (
                "specs",
                "an epic named by its path without .md",
                SPEC.replace("status: stable", "status: stable\nepic: specs/big"),
            ),
            (
                "specs",
                "an epic named by its file name",
                SPEC.replace("status: stable", "status: stable\nepic: big.md"),
            ),
            (
                "specs",
                "an empty epic",
                SPEC.replace("status: stable", "status: stable\nepic: \"\""),
            ),
            (
                "specs",
                "a misspelled epic",
                SPEC.replace("status: stable", "status: stable\nepik: big"),
            ),
            (
                "specs",
                "two tags",
                SPEC.replace("tags: [operations]", "tags: [operations, billing]"),
            ),
            (
                "specs",
                "unknown field",
                SPEC.replace("status: stable", "status: stable\nstatu: stable"),
            ),
            (
                "specs",
                "date-only verified",
                SPEC.replace(
                    "status: stable",
                    "status: stable\nverified: {by: human:a, at: 2026-10-01}",
                ),
            ),
            (
                "specs",
                "no frontmatter",
                "# Goals\n\nSomething.\n".to_string(),
            ),
            ("specs", "closed without a resolution", closed),
        ];
        for (folder, name, text) in bad {
            assert!(spec(folder, &text).is_err(), "{folder}: {name} passed");
        }
    }
}
