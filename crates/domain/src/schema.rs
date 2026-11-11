//! The frontmatter of each document type Rotproof reads in `docs/knowledge/`: knowledge documents and guides.
//!
//! OKF lets a producer add any key, and tells readers not to reject one they do not know. So an unknown field passes
//! as an extension, unless it looks like a misspelling of a field the type reads (OKF's or Rotproof's): that one
//! fails, as a misspelled optional field would otherwise be silently dropped. Every field OKF defines for a concept
//! (sections 4 and 5) passes as OKF writes it, so a document another OKF tool wrote correctly does not fail. The
//! fields of an Attested Computation (section 10) do not, as no such document belongs in the records.

use std::fmt::Display;
use std::sync::LazyLock;

use chrono::{DateTime, FixedOffset, NaiveDate};
use regex::Regex;
use yaml_rust2::Yaml;
use yaml_rust2::yaml::Hash;

use crate::frontmatter::{Sections, first_heading, split};

/// A datetime with a time zone, as OKF writes every timestamp.
pub type Time = DateTime<FixedOffset>;

pub const CLOSED_SECTION: &str = "Resolution";
/// The fields one type reads and the other does not, each with the type that reads it, such as a knowledge document's
/// `follows`. Found from the readers, so a field added to a type is in it with no list to update. On a type that does
/// not read it, one is not an extension: it is a sign of the wrong `type`, under which what it says would go unchecked
static OWN_FIELDS: LazyLock<Vec<(&'static str, Vec<&'static str>)>> = LazyLock::new(|| {
    let readers = [
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
    let mut own: Vec<(&'static str, Vec<&'static str>)> = Vec::new();
    for (_, keys) in &readers {
        for key in keys {
            let kinds: Vec<&'static str> = readers
                .iter()
                .filter(|(_, keys)| keys.contains(key))
                .map(|(kind, _)| *kind)
                .collect();
            if kinds.len() < readers.len() && !own.iter().any(|(known, _)| known == key) {
                own.push((*key, kinds));
            }
        }
    }
    own
});

// OKF actors (section 7): `<producer>/<version>` for an agent, `human:<id>` for a person, `process:<id>`. OKF does not
// limit the characters of `<id>` (its own samples use `human:jsmith@acme`), so only whitespace is excluded
static ACTOR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(?:human:\S+|process:\S+|[^\s:/]+/\S+)$").unwrap());

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

/// Who and on which day: one entry of OKF's `verified`. Only the day is kept ([`day`]).
#[derive(Debug, Clone, PartialEq)]
pub struct Stamp {
    pub by: String,
    pub at: NaiveDate,
}

#[derive(Debug, Clone)]
pub struct Guide {
    pub title: String,
    pub description: String,
    pub status: Status,
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
    /// The day from which to look at it again, for what it says of outside the code, which `follows` cannot watch.
    /// `rotproof check` warns from that day and fails nothing ([`crate::records::due`])
    pub stale_after: Option<NaiveDate>,
    /// What it follows, each key with its hash when the document was last reviewed (`follows.rs`), in the order written
    pub follows: Vec<(String, String)>,
    /// For an alarm (a document with `# When`): short strings, each as it appears in what is observed, so a search for
    /// the message met finds the document even when the rest of the message changed
    pub matches: Vec<String>,
}

/// The body headings of an alarm: a knowledge document that matters only when something is seen. `# When` says what
/// is seen and where, as observed; `# Do` what to do then. One without the other is not an alarm
pub const WHEN_SECTION: &str = "When";
pub const DO_SECTION: &str = "Do";

/// For tests: a record closed as it should be, `status: deprecated` and `# Resolution` as the first heading of its
/// body.
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
    let alarm = [WHEN_SECTION, DO_SECTION].map(|heading| sections.get(heading));
    if alarm.iter().any(Option::is_some) {
        let empty: Vec<&str> = [WHEN_SECTION, DO_SECTION]
            .into_iter()
            .zip(alarm)
            .filter(|(_, text)| text.is_none_or(|text| text.is_empty()))
            .map(|(heading, _)| heading)
            .collect();
        if !empty.is_empty() {
            return Err(format!(
                "an alarm has both # {WHEN_SECTION} (what is seen, and where) and # {DO_SECTION} (what to do then); \
                 missing or empty: {empty:?}"
            ));
        }
    } else if !knowledge.matches.is_empty() {
        return Err(format!(
            "match: only an alarm, a document with # {WHEN_SECTION} and # {DO_SECTION}, has strings to match"
        ));
    }
    Ok(KnowledgeDoc::Knowledge(knowledge, sections))
}

// Each reader below reads every field before it can return, so running it on an empty mapping lists its fields
// (`keys_read`)

fn knowledge_fields(fields: &mut Fields) -> Option<Knowledge> {
    fields.required("type", one_of(&["Knowledge"]));
    let title = fields.required("title", one_line);
    let description = fields.required("description", one_line);
    let tag = fields.required("tags", one_tag);
    let status = fields.required("status", status(&[Status::Stable, Status::Deprecated]));
    fields.optional("verified", stamps);
    let stale_after = fields.optional("stale_after", day);
    let follows = fields.optional("follows", followed_hashes);
    let matches = fields.optional("match", match_strings);
    okf_optional(fields);
    Some(Knowledge {
        title: title?,
        description: description?,
        tag: tag?,
        status: status?,
        stale_after: stale_after?,
        follows: follows?.unwrap_or_default(),
        matches: matches?.unwrap_or_default(),
    })
}

/// The strings an alarm is found by: a list of short texts, each on one line and named once, as they appear in what is
/// observed (an error message, an exit code, an HTTP status).
fn match_strings(value: &Yaml) -> Result<Vec<String>, String> {
    let Yaml::Array(list) = value else {
        return Err(format!(
            "not a list: write the strings in [], even one ([\"{}\"])",
            text(value).unwrap_or_default()
        ));
    };
    // A number is a string here: an exit code or an HTTP status is matched as the text it is printed as
    let found: Vec<String> = list
        .iter()
        .map(|value| match value {
            Yaml::Integer(n) => Ok(n.to_string()),
            other => one_line(other),
        })
        .collect::<Result<_, _>>()?;
    if found.is_empty() {
        return Err("empty: leave the field out when there is nothing to match".into());
    }
    for (i, string) in found.iter().enumerate() {
        if found[..i].contains(string) {
            return Err(format!("{string:?} is named twice"));
        }
    }
    Ok(found)
}

/// What a knowledge document follows: a mapping of keys `follows.rs` can read to hashes of 8 lower-case hex digits.
fn followed_hashes(value: &Yaml) -> Result<Vec<(String, String)>, String> {
    let Yaml::Hash(map) = value else {
        return Err(format!(
            "not a mapping of what it follows to hashes: {value:?}"
        ));
    };
    let mut found = Vec::new();
    for (key, hash) in map {
        let Yaml::String(key) = key else {
            return Err(format!("not a path: {key:?}"));
        };
        crate::follows::followed(key)?;
        let Yaml::String(hash) = hash else {
            return Err(format!("{key}: not a hash of 8 hex digits: {hash:?}"));
        };
        if hash.len() != 8 || !hash.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')) {
            return Err(format!(
                "{key}: {hash:?} is not a hash of 8 lower-case hex digits"
            ));
        }
        found.push((key.clone(), hash.clone()));
    }
    if found.is_empty() {
        return Err("empty: leave the field out when the document follows nothing".into());
    }
    Ok(found)
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
    fields.optional("stale_after", day);
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
                    } else if let Some((theirs, kinds)) =
                        OWN_FIELDS.iter().find(|(field, _)| misspelled(name, field))
                    {
                        self.errors.push(format!(
                            "unknown field: {name}; {theirs} is a field of a {}, not of this type",
                            kinds.join(" or a ")
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

/// A datetime with a time zone, for the timestamps of OKF that Rotproof only checks (`generated`, `sources`,
/// `usage_window`). A date alone fails: filling in 00:00 would invent a time nobody wrote.
fn time(value: &Yaml) -> Result<Time, String> {
    let s = text(value)?;
    DateTime::parse_from_rfc3339(&s)
        .map_err(|_| format!("not a datetime with a time zone (ISO 8601): {s}"))
}

/// A day of `verified` or `stale_after`: written as a day (YYYY-MM-DD), or as a datetime with a time zone, read as its
/// day in that zone. The time of day is never kept. An agent has no clock, so a time of day it writes may be a guess,
/// and no check can tell a guess a few minutes early from a measurement; a day is what a reader acts on. OKF asks for a
/// datetime here (a SHOULD, section 5): the records depart from it on purpose, and a datetime written by a process
/// with a clock still passes.
fn day(value: &Yaml) -> Result<NaiveDate, String> {
    let s = text(value)?;
    NaiveDate::parse_from_str(&s, "%Y-%m-%d")
        .or_else(|_| DateTime::parse_from_rfc3339(&s).map(|at| at.date_naive()))
        .map_err(|_| {
            format!("not a day (YYYY-MM-DD) or a datetime with a time zone (ISO 8601): {s}")
        })
}

fn stamp(value: &Yaml) -> Result<Stamp, String> {
    let Yaml::Hash(map) = value else {
        return Err(format!("not a {{by, at}} mapping: {value:?}"));
    };
    let mut fields = Fields::new(map);
    let by = fields.required("by", actor);
    let at = fields.required("at", day);
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

/// Exactly one tag: the area the index groups a knowledge document by. Whether it is
/// declared is checked against the declaration, which the document alone does not know (`bundle.rs`).
fn one_tag(value: &Yaml) -> Result<String, String> {
    let mut tags = text_list(value)?;
    match tags.len() {
        1 => Ok(tags.remove(0)),
        n => Err(format!("{n} tags; a record has exactly one area")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A valid knowledge document. Each bad input below changes exactly one thing in it
    const GOOD: &str = "---
type: Knowledge
title: Some decision
description: Why it is so.
tags: [operations]
status: stable
verified: {by: human:someone, at: 2026-09-28T10:00:00+09:00}
---

# Why

Because.
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
            .filter_map(|(name, text)| knowledge_doc(text).ok().map(|_| name.to_string()))
            .collect()
    }

    fn failing(cases: &[(&str, String)]) -> Vec<(String, String)> {
        cases
            .iter()
            .filter_map(|(name, text)| knowledge_doc(text).err().map(|why| (name.to_string(), why)))
            .collect()
    }

    #[test]
    fn the_good_input_passes() {
        // If the valid document did not pass, a failure below would not show that the one change was caught
        assert!(
            knowledge_doc(GOOD).is_ok(),
            "{:?}",
            knowledge_doc(GOOD).err()
        );
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

    const ALARM: &str = "---
type: Knowledge
title: PyPI refuses the upload
description: What to do when the release workflow cannot publish to PyPI.
tags: [operations]
status: stable
match: [403, trusted publisher]
---

# When

The publish job of the release workflow prints `HTTPError: 403 Forbidden` and `trusted publisher`.

# Do

Check the trusted publisher on PyPI, and run the release workflow again.
";

    #[test]
    fn an_alarm_says_what_is_seen_and_what_to_do() {
        let Ok(KnowledgeDoc::Knowledge(doc, _)) = knowledge_doc(ALARM) else {
            panic!("{:?}", knowledge_doc(ALARM).err());
        };
        // An exit code or a status is matched as the text it is printed as
        assert_eq!(doc.matches, ["403", "trusted publisher"]);
        // The strings are optional; the headings are what make an alarm
        assert!(knowledge_doc(&ALARM.replace("match: [403, trusted publisher]\n", "")).is_ok());
        let bad = [
            (
                "no # Do",
                ALARM.replace("# Do\n\nCheck", "# Then\n\nCheck"),
                "missing or empty: [\"Do\"]",
            ),
            (
                "an empty # When",
                ALARM.replace(
                    "The publish job of the release workflow prints `HTTPError: 403 Forbidden` and `trusted publisher`.\n",
                    "",
                ),
                "missing or empty: [\"When\"]",
            ),
            (
                "only # Do",
                ALARM.replace("# When\n", "# Seen\n"),
                "missing or empty: [\"When\"]",
            ),
            (
                "strings without # When",
                KNOWLEDGE.replace("status: stable", "status: stable\nmatch: [403]"),
                "match: only an alarm",
            ),
            (
                "one string, not in a list",
                ALARM.replace("match: [403, trusted publisher]", "match: \"403\""),
                "match: not a list",
            ),
            ("an empty list", ALARM.replace("match: [403, trusted publisher]", "match: []"), "match: empty"),
            (
                "a string twice",
                ALARM.replace("match: [403, trusted publisher]", "match: [403, \"403\"]"),
                "\"403\" is named twice",
            ),
            (
                "a string on two lines",
                ALARM.replace("match: [403, trusted publisher]", "match: [\"a\\nb\"]"),
                "on more than one line",
            ),
        ];
        for (name, text, said) in bad {
            let why = knowledge_doc(&text).err().unwrap_or_default();
            assert!(why.contains(said), "{name}: {why:?}");
        }
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
            ("a misspelled title", KNOWLEDGE.replace("title:", "titel:")),
        ];
        for (name, text) in bad {
            assert!(knowledge_doc(&text).is_err(), "{name} passed");
        }
        let why = knowledge_doc(&KNOWLEDGE.replace("tags: [operations]", "tags: []"))
            .err()
            .unwrap();
        assert!(
            why.contains("0 tags; a record has exactly one area"),
            "{why}"
        );
    }

    #[test]
    fn a_deprecated_document_opens_with_its_resolution() {
        let at_the_end = KNOWLEDGE.replace("status: stable", "status: deprecated")
            + "\n# Resolution\n\nReplaced.\n";
        assert_eq!(
            knowledge_doc(&at_the_end).err(),
            Some(
                "a closed knowledge document opens with # Resolution, before every other heading (its first \
                 heading is # Shape)"
                    .into()
            )
        );
        // A heading only in a comment does not count: the reader sees # Shape first
        let hidden = at_the_end.replacen("\n# Shape", "\n<!--\n# Resolution\n-->\n# Shape", 1);
        assert!(knowledge_doc(&hidden).is_err());
    }

    #[test]
    fn each_type_s_own_fields_are_found_from_the_readers() {
        assert_eq!(
            *OWN_FIELDS,
            [
                ("follows", vec!["knowledge document"]),
                ("match", vec!["knowledge document"]),
            ]
        );
        // Every field the knowledge document reads is found, optional ones included
        let keys = keys_read(|fields| {
            knowledge_fields(fields);
        });
        for key in [
            "stale_after",
            "generated",
            "sources",
            "usage_window",
            "resource",
        ] {
            assert!(keys.contains(&key), "{key} not found: {keys:?}");
        }
    }

    #[test]
    fn a_type_s_own_field_fails_on_the_other_type() {
        let guide = "---\ntype: Guide\ntitle: Rules\ndescription: What goes here.\n---\n\n# What goes here\n";
        for (field, kinds) in OWN_FIELDS.iter() {
            let said = format!("a field of a {}", kinds.join(" or a "));
            let on_guide = guide.replace("type: Guide", &format!("type: Guide\n{field}: x"));
            let why = knowledge_doc(&on_guide).err();
            assert!(
                why.as_ref().is_some_and(|why| why.contains(&said)),
                "{field} on a guide: {why:?}"
            );
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
            ("folows", "follows"),
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
            // A day, as the records are written, or a datetime read as its day in the zone written
            ("verified on a day", good(AT, "at: 2026-09-28")),
            (
                "verified at a time in another zone",
                good(AT, "at: 2026-09-28T23:30:00-05:00"),
            ),
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
                    "description: Why it is so.",
                    "description: >-\n  Why it\n  is so.",
                ),
            ),
            // The content may be in any language
            (
                "a title in Japanese",
                good("title: Some decision", "title: 次のデプロイで直す"),
            ),
            // Quoting does not change a value (YAML 1.2). PyYAML read a quoted datetime as a string, and it failed
            (
                "a quoted datetime",
                good(AT, "at: '2026-09-28T10:00:00+09:00'"),
            ),
            (
                "a quoted date",
                good(
                    "status: stable",
                    "status: stable\nstale_after: '2026-12-01'",
                ),
            ),
            // OKF does not limit the characters of an actor's id; its own samples use this one
            (
                "an actor with an at sign",
                good("human:someone", "human:jsmith@acme"),
            ),
        ];
        let failed = failing(&okf);
        assert!(failed.is_empty(), "{failed:?}");
    }

    #[test]
    fn a_broken_document_is_caught() {
        let bad = [
            ("no verified time", good(&format!(", {AT}"), "")),
            ("verified not a day", good(AT, "at: 2026-09-31")),
            (
                "verified without a time zone",
                good(AT, "at: 2026-09-28T10:00:00"),
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
                good("status: stable", "status: stable\nfolows: []"),
            ),
            (
                "a key that is not a name",
                good("status: stable", "status: stable\n2026: x"),
            ),
            (
                "deprecated without a resolution",
                good("status: stable", "status: deprecated"),
            ),
            (
                "no frontmatter",
                GOOD.splitn(3, "---\n").nth(2).unwrap().to_string(),
            ),
            (
                "unquoted colon",
                good(
                    "description: Why it is so.",
                    "description: how to measure: count",
                ),
            ),
            (
                "a number for a title",
                good("title: Some decision", "title: 123"),
            ),
            (
                "two tags",
                good("tags: [operations]", "tags: [operations, other]"),
            ),
            // Spaces alone are as empty as nothing
            (
                "a blank title",
                good("title: Some decision", "title: \" \""),
            ),
            ("an empty tag", good("tags: [operations]", "tags: [\"\"]")),
            ("a blank tag", good("tags: [operations]", "tags: [\" \"]")),
            // The index lists each of these on one line: a line break would end the entry and start another
            (
                "a description on two lines",
                good(
                    "description: Why it is so.",
                    "description: \"Wrong.\\n\\n# INJECTED HEADING\\n\\n* Fake entry\"",
                ),
            ),
            (
                "a title on two lines",
                good("title: Some decision", "title: \"Some\\ndecision\""),
            ),
            (
                "a tag on two lines",
                good("tags: [operations]", "tags: [\"oper\\nations\"]"),
            ),
            // A folded text keeps one line break at its end unless it is folded with >-
            (
                "a folded description",
                good(
                    "description: Why it is so.",
                    "description: >\n  Why it\n  is so.",
                ),
            ),
            (
                "stale_after not a date",
                good(
                    "status: stable",
                    "status: stable\nstale_after: in six months",
                ),
            ),
            // A record of a type Rotproof no longer reads, in docs/knowledge/
            ("another type", good("type: Knowledge", "type: Spec")),
        ];
        let passed = passing(&bad);
        assert!(passed.is_empty(), "passed: {passed:?}");
    }

    #[test]
    fn a_guide_passes_with_its_own_fields() {
        let rules = "---\ntype: Guide\ntitle: Rules\ndescription: What goes here.\n---\n\n# What goes here\n";
        let Ok(KnowledgeDoc::Guide(guide)) = knowledge_doc(rules) else {
            panic!("{:?}", knowledge_doc(rules))
        };
        assert_eq!(guide.status, Status::Stable);
        for bad in [
            rules.replace("description: What goes here.\n", ""),
            rules.replace("type: Guide", "type: Guide\nfollows: x"),
        ] {
            assert!(knowledge_doc(&bad).is_err(), "{bad}");
        }
    }
}
