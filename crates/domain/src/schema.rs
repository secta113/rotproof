//! The frontmatter of each record type: work items, specs, guides and knowledge documents.
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

use utils::frontmatter::{Sections, first_heading, split};

/// A datetime with a time zone, as OKF writes every timestamp.
pub type Time = DateTime<FixedOffset>;

/// Body headings every work item needs. A closed item also needs `CLOSED_SECTION`
pub const SECTIONS: [&str; 2] = ["State", "Details"];
/// A body heading a work item may have: what starts it, when that is not a record its `after` can name. Not empty when
/// it is there
pub const TRIGGER_SECTION: &str = "Trigger";
pub const CLOSED_SECTION: &str = "Resolution";
/// The fields some types read and others do not, each with the types that read it: a work item's `filed`, the
/// `parent` of a work item and a spec, the arrows of the records of `docs/work/`, a milestone's `date` and a knowledge
/// document's `follows`. Found from the readers, so a field added to a type is in it with no list to update. On a type
/// that does not read it, one is not an extension: it is a sign of the wrong `type`, under which the record's place in
/// the tree or in the order of the work would go unchecked
static OWN_FIELDS: LazyLock<Vec<(&'static str, Vec<&'static str>)>> = LazyLock::new(|| {
    let readers = [
        (
            "work item",
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
            "milestone",
            keys_read(|fields| {
                milestone_fields(fields);
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

/// Fields Rotproof read once and reads no more, each with what to write instead. One left in a record would otherwise
/// pass as an extension, and what it said would go unchecked without a word
const RETIRED: [(&str, &str); 3] = [
    (
        "epic",
        "no longer read; write the slug of the epic in parent",
    ),
    (
        "deadline_kind",
        "no longer read; write the moment the deadline names as a milestone, and its slug in until",
    ),
    (
        "deadline",
        "no longer read; write the moment it names as a milestone, and its slug in until",
    ),
];

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

/// Who and when: one entry of OKF's `verified`, or `generated`.
#[derive(Debug, Clone, PartialEq)]
pub struct Stamp {
    pub by: String,
    pub at: Time,
}

/// How far the work of a record of `docs/work/` has come: its `progress`. It decides whether the record is open, apart
/// from `status`, which says, as in OKF, whether the document is current: an implemented spec still describes how
/// things are, so it may close and stay `stable`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Progress {
    /// No `progress`: open, not started
    #[default]
    NotStarted,
    /// Open, and someone has started its work. Who and when are in `# State` and `verified`
    Started,
    /// Closed, done or dropped
    Closed(ClosedAs),
}

impl Progress {
    /// How the record closed, or `None` while it is open
    pub fn closed_as(self) -> Option<ClosedAs> {
        match self {
            Progress::Closed(closed_as) => Some(closed_as),
            Progress::NotStarted | Progress::Started => None,
        }
    }
}

/// How a record of `docs/work/` closed: its `progress` once it is closed. A dropped record is `deprecated`; a done one
/// is too, unless it is a spec or a milestone that is still current
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClosedAs {
    /// Done, implemented, or happened
    Done,
    /// Dropped or withdrawn: what waited on it did not get it
    Dropped,
}

impl ClosedAs {
    pub fn name(self) -> &'static str {
        match self {
            ClosedAs::Done => "done",
            ClosedAs::Dropped => "dropped",
        }
    }
}

/// The frontmatter of one work item.
#[derive(Debug, Clone)]
pub struct Item {
    pub title: String,
    /// What the problem is, in one sentence
    pub description: String,
    /// The area. The index groups items by it
    pub tag: String,
    /// `Draft` = nobody has sorted it yet / `Stable` = sorted into a spec / `Deprecated` = closed: what it waited for
    /// no longer holds. A closed item stays, so references to it keep working
    pub status: Status,
    /// Closed exactly when `status` is `Deprecated`
    pub progress: Progress,
    /// The slug of the spec it is a part of: required once it is sorted (`Stable`). Whether it names one is checked
    /// against the other records, which the document alone does not know (`bundle.rs`)
    pub parent: Option<String>,
    pub arrows: Arrows,
    pub filed: NaiveDate,
    /// Never empty. As in OKF, written as one mapping or a list of them
    pub verified: Vec<Stamp>,
    /// When the recorded state goes out of date with time alone, the time after which it should be measured again.
    /// It does not end the item, and passing it does not fail the check
    pub stale_after: Option<Time>,
}

impl Item {
    /// Whether the item is open: its `progress` is neither done nor dropped.
    pub fn is_open(&self) -> bool {
        self.progress.closed_as().is_none()
    }

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

/// A document that is not an item, such as the rules of a directory.
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
    /// `Draft` = proposed / `Stable` = agreed, or done and still current / `Deprecated` = dropped, or done and replaced
    pub status: Status,
    pub progress: Progress,
    /// The slug of the epic this spec is a part of. Whether it names one is checked against the other records, which
    /// the document alone does not know (`bundle.rs`)
    pub parent: Option<String>,
    pub arrows: Arrows,
}

impl Spec {
    /// Whether the spec is open: its `progress` is neither done nor dropped, whatever its status.
    pub fn is_open(&self) -> bool {
        self.progress.closed_as().is_none()
    }
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

/// A moment the work waits for: a release, an agreement, something outside the project. It holds no work and no
/// decision, only whether it has happened.
#[derive(Debug, Clone)]
pub struct Milestone {
    pub title: String,
    pub description: String,
    /// The area
    pub tag: String,
    /// `Draft` = proposed / `Stable` = placed, or happened and still current / `Deprecated` = withdrawn, or happened
    /// and kept as history
    pub status: Status,
    /// Closed as `Done` when it happened, as `Dropped` when it was withdrawn
    pub progress: Progress,
    /// The day it is planned or announced for. Passing it fails nothing: the index and `rotproof index` show it
    pub date: Option<NaiveDate>,
    pub arrows: Arrows,
}

impl Milestone {
    /// Whether the moment is still waited for: its `progress` is neither done nor dropped.
    pub fn is_open(&self) -> bool {
        self.progress.closed_as().is_none()
    }
}

/// The arrows a record of `docs/work/` writes, each "this one before that one". The records they name, and whether
/// they keep their order, are checked against the other records, which the document alone does not know (`bundle.rs`)
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Arrows {
    /// The records that come before this one: it starts once they are done
    pub after: Vec<String>,
    /// The records that come after this one: it is done by the time they close
    pub until: Vec<String>,
}

/// The body heading every milestone needs: what to look at to know it has happened
pub const CONDITION_SECTION: &str = "Condition";

/// A document in `docs/work/`.
#[derive(Debug, Clone)]
pub enum WorkDoc {
    Item(Item, Sections),
    Spec(Spec, Sections),
    Milestone(Milestone, Sections),
    Guide(Guide),
}

/// A document in `docs/work/`, read by its `type`, or why it breaks the format of that type. A document whose type is
/// none of `Spec`, `Milestone` and `Guide` is read as a work item, which says what its type should be.
pub fn work_doc(text: &str) -> Result<WorkDoc, String> {
    let (meta, sections) = split(text)?;
    let mut fields = Fields::new(&meta);
    match fields.peek("type") {
        Some(Yaml::String(kind)) if kind == "Guide" => {
            let guide = guide(&mut fields);
            return fields.finish(guide).map(WorkDoc::Guide);
        }
        Some(Yaml::String(kind)) if kind == "Spec" => {
            return spec(text).map(|(spec, sections)| WorkDoc::Spec(spec, sections));
        }
        Some(Yaml::String(kind)) if kind == "Milestone" => {
            let milestone = milestone_fields(&mut fields);
            let milestone = fields.finish(milestone)?;
            let required = [CONDITION_SECTION]
                .into_iter()
                .chain((!milestone.is_open()).then_some(CLOSED_SECTION));
            let empty: Vec<&str> = required
                .filter(|s| sections.get(*s).is_none_or(|text| text.is_empty()))
                .collect();
            if !empty.is_empty() {
                return Err(format!("body headings missing or empty: {empty:?}"));
            }
            if !milestone.is_open() {
                resolution_first(text, "milestone")?;
            }
            return Ok(WorkDoc::Milestone(milestone, sections));
        }
        _ => {}
    }
    let item = item(&mut fields);
    let item = fields.finish(item)?;
    let required = SECTIONS
        .iter()
        .copied()
        .chain((!item.is_open()).then_some(CLOSED_SECTION));
    let mut empty: Vec<&str> = required
        .filter(|s| sections.get(*s).is_none_or(|text| text.is_empty()))
        .collect();
    if sections
        .get(TRIGGER_SECTION)
        .is_some_and(|text| text.is_empty())
    {
        empty.insert(0, TRIGGER_SECTION);
    }
    if !empty.is_empty() {
        return Err(format!("body headings missing or empty: {empty:?}"));
    }
    if !item.is_open() {
        resolution_first(text, "work item")?;
    }
    Ok(WorkDoc::Item(item, sections))
}

/// For tests: a record closed as it should be, `status: deprecated` and `# Resolution` as the first heading of its
/// body.
#[cfg(test)]
pub(crate) fn closed_record(text: &str, resolution: &str) -> String {
    // A record of docs/work/ says how it closed; a knowledge document only that it no longer holds
    let closed = if text.contains("\ntype: Knowledge\n") {
        "status: deprecated"
    } else {
        "status: deprecated\nprogress: done"
    };
    let text = text
        .replace("status: stable", closed)
        .replace("status: draft", closed);
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

/// A spec in `docs/work/`, or why it breaks the format. Open or closed: every spec stays there when it closes, so its
/// path, and every link to it, never changes, and its `progress` alone says it is closed.
pub fn spec(text: &str) -> Result<(Spec, Sections), String> {
    let (meta, sections) = split(text)?;
    let mut fields = Fields::new(&meta);
    let spec = spec_fields(&mut fields);
    let spec = fields.finish(spec)?;
    if !spec.is_open()
        && sections
            .get(CLOSED_SECTION)
            .is_none_or(|text| text.is_empty())
    {
        return Err(format!(
            "a closed spec needs a non-empty # {CLOSED_SECTION}"
        ));
    }
    if !spec.is_open() {
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

fn spec_fields(fields: &mut Fields) -> Option<Spec> {
    fields.required("type", one_of(&["Spec"]));
    let title = fields.required("title", one_line);
    let description = fields.required("description", one_line);
    let status = fields.required(
        "status",
        status(&[Status::Draft, Status::Stable, Status::Deprecated]),
    );
    let tag = fields.required("tags", one_tag);
    let progress = progress(fields, status, WorkKind::Spec);
    let parent = fields.optional("parent", slug);
    let arrows = arrows(fields);
    fields.optional("verified", stamps);
    fields.optional("stale_after", time);
    okf_optional(fields);
    Some(Spec {
        title: title?,
        description: description?,
        tag: tag?,
        status: status?,
        progress: progress?,
        parent: parent?,
        arrows: arrows?,
    })
}

/// The type of a record of `docs/work/`, as far as its `progress` depends on it.
#[derive(Clone, Copy, PartialEq, Eq)]
enum WorkKind {
    Spec,
    Item,
    Milestone,
}

/// How far the work of a record with `status` has come, and whether the two agree: a deprecated record is closed, a
/// dropped one is deprecated, a done one is deprecated unless it is a spec or a milestone (which may still describe
/// how things are), and a milestone is never started. When the status itself could not be read, only the value is
/// checked.
fn progress(fields: &mut Fields, status: Option<Status>, kind: WorkKind) -> Read<Progress> {
    let progress = fields.optional("progress", |value| match text(value)?.as_str() {
        "started" => Ok(Progress::Started),
        "done" => Ok(Progress::Closed(ClosedAs::Done)),
        "dropped" => Ok(Progress::Closed(ClosedAs::Dropped)),
        other => Err(format!(
            "{other:?} is not one of [\"started\", \"done\", \"dropped\"]"
        )),
    })?;
    let progress = progress.unwrap_or_default();
    let done_may_stay_current = kind != WorkKind::Item;
    let wrong = match (status?, progress) {
        (Status::Deprecated, Progress::NotStarted) => Some(
            "missing: a deprecated record is closed, and says how: done (implemented, done or happened) or dropped \
             (dropped or withdrawn)"
                .to_string(),
        ),
        (Status::Deprecated, Progress::Started) => Some(
            "a deprecated record is closed, and says how: done or dropped; started is open".to_string(),
        ),
        (_, Progress::Started) if kind == WorkKind::Milestone => Some(
            "a milestone is never started: while it is open, it is the moment the work waits for".to_string(),
        ),
        (Status::Deprecated, Progress::Closed(_)) | (_, Progress::NotStarted | Progress::Started) => None,
        (status, Progress::Closed(ClosedAs::Dropped)) => Some(format!(
            "a dropped record is status: deprecated, so it does not read as current; this one is {}",
            status.name()
        )),
        (Status::Stable, Progress::Closed(ClosedAs::Done)) if done_may_stay_current => None,
        (status, Progress::Closed(ClosedAs::Done)) if done_may_stay_current => Some(format!(
            "a done record is status: stable (still current) or deprecated (replaced, kept as history); this one \
             is {}",
            status.name()
        )),
        (status, Progress::Closed(ClosedAs::Done)) => Some(format!(
            "a done work item is status: deprecated: what it waited for no longer holds; this one is {}",
            status.name()
        )),
    };
    match wrong {
        Some(why) => {
            fields.wrong("progress", why);
            None
        }
        None => Some(progress),
    }
}

fn milestone_fields(fields: &mut Fields) -> Option<Milestone> {
    fields.required("type", one_of(&["Milestone"]));
    let title = fields.required("title", one_line);
    let description = fields.required("description", one_line);
    let tag = fields.required("tags", one_tag);
    let status = fields.required(
        "status",
        status(&[Status::Draft, Status::Stable, Status::Deprecated]),
    );
    let progress = progress(fields, status, WorkKind::Milestone);
    let date = fields.optional("date", date);
    let arrows = arrows(fields);
    fields.optional("verified", stamps);
    fields.optional("stale_after", time);
    okf_optional(fields);
    Some(Milestone {
        title: title?,
        description: description?,
        tag: tag?,
        status: status?,
        progress: progress?,
        date: date?,
        arrows: arrows?,
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
    let follows = fields.optional("follows", followed_hashes);
    let matches = fields.optional("match", match_strings);
    okf_optional(fields);
    Some(Knowledge {
        title: title?,
        description: description?,
        tag: tag?,
        status: status?,
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

fn item(fields: &mut Fields) -> Option<Item> {
    fields.required("type", one_of(&["Work Item"]));
    let title = fields.required("title", one_line);
    let description = fields.required("description", one_line);
    let tag = fields.required("tags", one_tag);
    let status = fields.required(
        "status",
        status(&[Status::Draft, Status::Stable, Status::Deprecated]),
    );
    let progress = progress(fields, status, WorkKind::Item);
    let parent = fields.optional("parent", slug);
    let arrows = arrows(fields);
    let filed = fields.required("filed", date);
    let verified = fields.required("verified", stamps);
    let stale_after = fields.optional("stale_after", time);
    okf_optional(fields);
    let item = Item {
        title: title?,
        description: description?,
        tag: tag?,
        status: status?,
        progress: progress?,
        parent: parent?,
        arrows: arrows?,
        filed: filed?,
        verified: verified?,
        stale_after: stale_after?,
    };
    if item.status == Status::Stable && item.parent.is_none() {
        fields.wrong(
            "parent",
            "missing: a sorted work item (status: stable) is a part of a spec; one nobody has sorted yet is \
             status: draft",
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
                    if let Some((_, instead)) = RETIRED
                        .iter()
                        .find(|(retired, _)| misspelled(name, retired))
                    {
                        self.errors.push(format!("{name}: {instead}"));
                    } else if let Some(meant) =
                        self.read.iter().find(|known| misspelled(name, known))
                    {
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

/// The `after` and `until` of a record. Each is a list of slugs, each named once.
fn arrows(fields: &mut Fields) -> Read<Arrows> {
    let after = fields.optional("after", slugs);
    let until = fields.optional("until", slugs);
    Some(Arrows {
        after: after?.unwrap_or_default(),
        until: until?.unwrap_or_default(),
    })
}

/// A list of slugs, each once. A list, even of one: a field that is sometimes a list and sometimes not is read two ways
/// by every tool that reads it.
fn slugs(value: &Yaml) -> Result<Vec<String>, String> {
    let Yaml::Array(list) = value else {
        return Err(format!(
            "not a list: write the slugs in [], even one ([{}])",
            text(value).unwrap_or_default()
        ));
    };
    let found: Vec<String> = list.iter().map(slug).collect::<Result<_, _>>()?;
    if found.is_empty() {
        return Err("empty: leave the field out when there is no arrow".into());
    }
    for (i, slug) in found.iter().enumerate() {
        if found[..i].contains(slug) {
            return Err(format!("{slug} is named twice"));
        }
    }
    Ok(found)
}

/// Exactly one tag: the area the index groups a work item, a spec or a knowledge document by. Whether it is
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

    /// A valid item. Each bad input below changes exactly one thing in it
    const GOOD: &str = "---
type: Work Item
title: Some problem
description: Something is wrong.
tags: [operations]
status: stable
parent: big-work
filed: 2026-09-27
verified: {by: human:someone, at: 2026-09-28T10:00:00+09:00}
until: [next-release]
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
            .filter_map(|(name, text)| work_doc(text).ok().map(|_| name.to_string()))
            .collect()
    }

    fn failing(cases: &[(&str, String)]) -> Vec<(String, String)> {
        cases
            .iter()
            .filter_map(|(name, text)| work_doc(text).err().map(|why| (name.to_string(), why)))
            .collect()
    }

    #[test]
    fn the_good_input_passes() {
        // If the valid item did not pass, a failure below would not show that the one change was caught
        assert!(work_doc(GOOD).is_ok(), "{:?}", work_doc(GOOD).err());
    }

    const MILESTONE: &str = "---
type: Milestone
title: Rotproof 0.3.0
description: The release that puts the records in docs/work/.
tags: [operations]
status: stable
date: 2026-11-01
---

# Condition

The release workflow of tag v0.3.0 passes every job.
";

    #[test]
    fn a_milestone_holds_only_whether_it_happened() {
        let Ok(WorkDoc::Milestone(milestone, _)) = work_doc(MILESTONE) else {
            panic!("{:?}", work_doc(MILESTONE).err());
        };
        assert_eq!(
            (milestone.status, milestone.progress, milestone.date),
            (
                Status::Stable,
                Progress::NotStarted,
                NaiveDate::from_ymd_opt(2026, 11, 1)
            )
        );
        for good in [
            MILESTONE.replace("status: stable", "status: draft"),
            MILESTONE.replace("date: 2026-11-01\n", ""),
            closed_record(MILESTONE, "Released on 2026-11-01."),
            closed_record(MILESTONE, "Withdrawn.").replace("progress: done", "progress: dropped"),
        ] {
            assert!(
                work_doc(&good).is_ok(),
                "{good}: {:?}",
                work_doc(&good).err()
            );
        }
        let bad = [
            (
                "no condition",
                MILESTONE.replace(
                    "# Condition\n\nThe release workflow of tag v0.3.0 passes every job.\n",
                    "",
                ),
                "body headings missing or empty: [\"Condition\"]",
            ),
            // What create writes: a comment is not seen, so the heading is empty
            (
                "only a comment",
                MILESTONE
                    .replace("The release workflow", "<!-- The release workflow")
                    .replace("every job.", "every job. -->"),
                "missing or empty: [\"Condition\"]",
            ),
            (
                "closed without a resolution",
                MILESTONE.replace("status: stable", "status: deprecated\nprogress: done"),
                "missing or empty: [\"Resolution\"]",
            ),
            (
                "closed without saying how",
                closed_record(MILESTONE, "Released.").replace("progress: done\n", ""),
                "progress: missing",
            ),
            (
                "a date and time",
                MILESTONE.replace("date: 2026-11-01", "date: 2026-11-01T00:00:00+09:00"),
                "date: not a date",
            ),
            // A moment is not a part of the work: it is what the work waits for
            (
                "a parent",
                MILESTONE.replace("status: stable", "status: stable\nparent: big"),
                "parent is a field of a work item or a spec",
            ),
            (
                "a deadline",
                MILESTONE.replace("status: stable", "status: stable\ndeadline: x"),
                "deadline: no longer read",
            ),
        ];
        for (name, text, said) in bad {
            let why = work_doc(&text).err().unwrap_or_default();
            assert!(why.contains(said), "{name}: {why:?}");
        }
        // The date is the milestone's own field
        let why = spec(&SPEC.replace("status: stable", "status: stable\ndate: 2026-11-01"))
            .err()
            .unwrap_or_default();
        assert!(why.contains("date is a field of a milestone"), "{why:?}");
    }

    #[test]
    fn a_trigger_is_written_only_when_an_arrow_cannot_say_it() {
        // What starts most work is a record before it, which after names; the trigger is for what is no record
        let without = good("# Trigger\n\nThe next deploy\n\n", "");
        assert!(work_doc(&without).is_ok(), "{:?}", work_doc(&without).err());
        let empty = good("# Trigger\n\nThe next deploy\n", "# Trigger\n");
        assert_eq!(
            work_doc(&empty).err(),
            Some("body headings missing or empty: [\"Trigger\"]".into())
        );
        // The state is measured and the history linked, whatever started the work
        for heading in [
            "# State\n\nNot yet.\n",
            "# Details\n\n[somewhere](/log.md)\n",
        ] {
            assert!(work_doc(&good(heading, "")).is_err(), "{heading}");
        }
    }

    #[test]
    fn an_arrow_is_a_list_of_slugs() {
        let Ok(WorkDoc::Item(item, _)) = work_doc(&good(
            "until: [next-release]",
            "after: [a, b]\nuntil: [release]",
        )) else {
            panic!("a list of slugs did not pass");
        };
        assert_eq!(
            item.arrows,
            Arrows {
                after: vec!["a".into(), "b".into()],
                until: vec!["release".into()]
            }
        );
        let bad = [
            (
                "one slug, not in a list",
                "until: release",
                "until: not a list: write the slugs in [], even one ([release])",
            ),
            (
                "an empty list",
                "after: []",
                "after: empty: leave the field out",
            ),
            ("a slug twice", "after: [a, a]", "after: a is named twice"),
            (
                "a path",
                "until: [/work/release.md]",
                "is a path; write the slug",
            ),
        ];
        for (name, field, said) in bad {
            let text = good("until: [next-release]", field);
            let why = work_doc(&text).err().unwrap_or_default();
            assert!(why.contains(said), "{name}: {why:?}");
        }
    }

    #[test]
    fn a_closed_record_says_how_it_closed() {
        for (closed_as, expected) in [("done", ClosedAs::Done), ("dropped", ClosedAs::Dropped)] {
            let item = closed_record(GOOD, "Nothing found.")
                .replace("progress: done", &format!("progress: {closed_as}"));
            let Ok(WorkDoc::Item(item, _)) = work_doc(&item) else {
                panic!("{:?}", work_doc(&item).err());
            };
            assert_eq!(item.progress, Progress::Closed(expected));
            assert!(!item.is_open());
            let spec_text = closed_record(SPEC, "Dropped.")
                .replace("progress: done", &format!("progress: {closed_as}"));
            assert_eq!(
                spec(&spec_text).map(|(spec, _)| spec.progress),
                Ok(Progress::Closed(expected))
            );
        }
        let bad = [
            (
                "closed without it",
                closed_record(GOOD, "Fixed.").replace("progress: done\n", ""),
                "progress: missing: a deprecated record is closed, and says how",
            ),
            (
                "a spec closed without it",
                closed_record(SPEC, "Done.").replace("progress: done\n", ""),
                "progress: missing",
            ),
            (
                "another value",
                closed_record(GOOD, "Fixed.").replace("progress: done", "progress: fixed"),
                "progress: \"fixed\" is not one of [\"started\", \"done\", \"dropped\"]",
            ),
        ];
        for (name, text, said) in bad {
            let why = work_doc(&text).err().unwrap_or_default();
            assert!(why.contains(said), "{name}: {why:?}");
        }
        // A knowledge document only stops holding: it has no work to finish or drop
        let knowledge = closed_record(KNOWLEDGE, "Replaced.")
            .replace("status: deprecated", "status: deprecated\nprogress: done");
        let why = knowledge_doc(&knowledge).err().unwrap_or_default();
        assert!(
            why.contains("progress is a field of a work item or a spec or a milestone"),
            "{why:?}"
        );
    }

    /// The status says whether the document is current, the progress how far its work has come: a done spec may stay
    /// current, a done work item no longer holds, and nothing dropped reads as current.
    #[test]
    fn status_and_progress_agree() {
        let closed_spec = |status: &str, progress: &str| {
            closed_record(SPEC, "Implemented in abc1234.")
                .replace("status: deprecated", &format!("status: {status}"))
                .replace("progress: done", &format!("progress: {progress}"))
        };
        let closed_item = |status: &str, progress: &str| {
            closed_record(GOOD, "Fixed.")
                .replace("status: deprecated", &format!("status: {status}"))
                .replace("progress: done", &format!("progress: {progress}"))
        };
        let current = closed_spec("stable", "done");
        let Ok((done, _)) = spec(&current) else {
            panic!("{:?}", spec(&current).err());
        };
        assert_eq!(
            (done.status, done.is_open()),
            (Status::Stable, false),
            "a done spec that still holds is closed and current"
        );
        // Started: open, whether sorted or agreed yet or not
        for status in ["draft", "stable"] {
            let started = |text: &str| {
                text.replace(
                    "status: stable",
                    &format!("status: {status}\nprogress: started"),
                )
            };
            let Ok(WorkDoc::Item(item, _)) = work_doc(&started(GOOD)) else {
                panic!("{status}: {:?}", work_doc(&started(GOOD)).err());
            };
            assert_eq!((item.progress, item.is_open()), (Progress::Started, true));
            assert_eq!(
                spec(&started(SPEC)).map(|(spec, _)| spec.is_open()),
                Ok(true),
                "{status}"
            );
        }
        let milestone =
            closed_record(MILESTONE, "Released.").replace("status: deprecated", "status: stable");
        assert!(
            work_doc(&milestone).is_ok(),
            "{:?}",
            work_doc(&milestone).err()
        );
        let bad = [
            (
                "a draft spec done",
                spec(&closed_spec("draft", "done")).err(),
                "progress: a done record is status: stable (still current) or deprecated (replaced, kept as \
                 history); this one is draft",
            ),
            (
                "a stable spec dropped",
                spec(&closed_spec("stable", "dropped")).err(),
                "progress: a dropped record is status: deprecated, so it does not read as current; this one is \
                 stable",
            ),
            (
                "a stable item done",
                work_doc(&closed_item("stable", "done")).err(),
                "progress: a done work item is status: deprecated: what it waited for no longer holds; this one is \
                 stable",
            ),
            (
                "a draft item dropped",
                work_doc(&closed_item("draft", "dropped")).err(),
                "this one is draft",
            ),
            (
                "a draft milestone done",
                work_doc(&milestone.replace("status: stable", "status: draft")).err(),
                "a done record is status: stable (still current) or deprecated",
            ),
            (
                "a stable milestone dropped",
                work_doc(&milestone.replace("progress: done", "progress: dropped")).err(),
                "a dropped record is status: deprecated",
            ),
            (
                "a started milestone",
                work_doc(&MILESTONE.replace("status: stable", "status: stable\nprogress: started"))
                    .err(),
                "progress: a milestone is never started",
            ),
            (
                "a deprecated item started",
                work_doc(&closed_item("deprecated", "started")).err(),
                "progress: a deprecated record is closed, and says how: done or dropped; started is open",
            ),
            // A spec that stays current still says where it closed, first
            (
                "a current spec without a resolution",
                spec(&SPEC.replace("status: stable", "status: stable\nprogress: done")).err(),
                "a closed spec needs a non-empty # Resolution",
            ),
        ];
        for (name, why, said) in bad {
            let why = why.unwrap_or_default();
            assert!(why.contains(said), "{name}: {why:?}");
        }
    }

    #[test]
    fn a_sorted_item_has_a_parent_and_an_unsorted_one_is_a_draft() {
        let unsorted = good("status: stable\nparent: big-work", "status: draft");
        let Ok(WorkDoc::Item(item, _)) = work_doc(&unsorted) else {
            panic!("{:?}", work_doc(&unsorted).err());
        };
        assert!(item.is_open());
        assert_eq!((item.status, item.parent), (Status::Draft, None));
        // Sorted into a spec before anyone wrote down that it was sorted: a draft may name its spec already
        assert!(work_doc(&good("status: stable", "status: draft")).is_ok());
        let Some(why) = work_doc(&good("parent: big-work\n", "")).err() else {
            panic!("a sorted item without a parent passed");
        };
        assert!(
            why.contains("parent: missing: a sorted work item (status: stable)"),
            "{why}"
        );
        // Closed, it needs none: an item closed with nothing found was never sorted
        let dropped = closed_record(&good("parent: big-work\n", ""), "Nothing found.");
        assert!(work_doc(&dropped).is_ok(), "{:?}", work_doc(&dropped).err());
        for (name, text) in [
            (
                "a path",
                good("parent: big-work", "parent: /work/big-work.md"),
            ),
            (
                "the retired field",
                good("parent: big-work", "epic: big-work"),
            ),
        ] {
            assert!(work_doc(&text).is_err(), "{name} passed");
        }
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

File a draft work item with until: [the next release], and check the trusted publisher on PyPI.
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
                ALARM.replace("# Do\n\nFile a draft", "# Then\n\nFile a draft"),
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
        // Only a knowledge document holds alarms
        let why = work_doc(&GOOD.replace("status: stable", "status: stable\nmatch: [403]"))
            .err()
            .unwrap_or_default();
        assert!(
            why.contains("match is a field of a knowledge document"),
            "{why:?}"
        );
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
            // A field of a work item and a spec: the wrong type
            (
                "a parent",
                KNOWLEDGE.replace("status: stable", "status: stable\nparent: big"),
            ),
            ("a misspelled title", KNOWLEDGE.replace("title:", "titel:")),
        ];
        for (name, text) in bad {
            assert!(knowledge_doc(&text).is_err(), "{name} passed");
        }
        // The area is shared by three types, so its message names none of them
        let why = knowledge_doc(&KNOWLEDGE.replace("tags: [operations]", "tags: []"))
            .err()
            .unwrap();
        assert!(
            why.contains("0 tags; a record has exactly one area"),
            "{why}"
        );
    }

    #[test]
    fn a_closed_item_opens_with_its_resolution() {
        let closed = closed_record(GOOD, "Fixed.");
        assert!(work_doc(&closed).is_ok(), "{:?}", work_doc(&closed).err());
        let at_the_end = good("status: stable", "status: deprecated\nprogress: done")
            + "\n# Resolution\n\nFixed.\n";
        assert_eq!(
            work_doc(&at_the_end).err(),
            Some(
                "a closed work item opens with # Resolution, before every other heading (its first heading is # Trigger)"
                    .into()
            )
        );
        // A heading only in a comment does not count: the reader sees # Trigger first
        let hidden = at_the_end.replacen("\n# Trigger", "\n<!--\n# Resolution\n-->\n# Trigger", 1);
        assert!(work_doc(&hidden).is_err());
    }

    #[test]
    fn each_type_s_own_fields_are_found_from_the_readers() {
        assert_eq!(
            *OWN_FIELDS,
            [
                ("progress", vec!["work item", "spec", "milestone"]),
                ("parent", vec!["work item", "spec"]),
                ("after", vec!["work item", "spec", "milestone"]),
                ("until", vec!["work item", "spec", "milestone"]),
                ("filed", vec!["work item"]),
                ("date", vec!["milestone"]),
                ("follows", vec!["knowledge document"]),
                ("match", vec!["knowledge document"]),
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
        for (field, kinds) in OWN_FIELDS.iter() {
            let said = format!("a field of a {}", kinds.join(" or a "));
            let on_guide = guide.replace("type: Guide", &format!("type: Guide\n{field}: x"));
            assert!(work_doc(&on_guide).is_err(), "{field} passed on a guide");
            if !kinds.contains(&"spec") {
                let on_spec =
                    SPEC.replace("status: stable", &format!("status: stable\n{field}: x"));
                assert!(spec(&on_spec).is_err(), "{field} passed on a spec");
            }
            if !kinds.contains(&"milestone") {
                let on_milestone =
                    MILESTONE.replace("status: stable", &format!("status: stable\n{field}: x"));
                let why = work_doc(&on_milestone).err();
                assert!(
                    why.as_ref().is_some_and(|why| why.contains(&said)),
                    "{field} on a milestone: {why:?}"
                );
            }
            if !kinds.contains(&"knowledge document") {
                let on_knowledge =
                    KNOWLEDGE.replace("status: stable", &format!("status: stable\n{field}: x"));
                let why = knowledge_doc(&on_knowledge).err();
                assert!(
                    why.as_ref().is_some_and(|why| why.contains(&said)),
                    "{field} on a knowledge document: {why:?}"
                );
            }
            if !kinds.contains(&"work item") {
                let on_item =
                    GOOD.replace("status: stable", &format!("status: stable\n{field}: x"));
                let why = work_doc(&on_item).err();
                assert!(
                    why.as_ref().is_some_and(|why| why.contains(&said)),
                    "{field} on a work item: {why:?}"
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
                "a title in Japanese",
                good("title: Some problem", "title: 次のデプロイで直す"),
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
        ];
        let failed = failing(&okf);
        assert!(failed.is_empty(), "{failed:?}");
        // The newest measurement is the last one (used for the index date and for stale_after)
        let Ok(WorkDoc::Item(item, _)) = work_doc(&okf[0].1) else {
            panic!()
        };
        assert_eq!(item.last_verified().by, "process:nightly");
    }

    #[test]
    fn a_broken_document_is_caught() {
        let bad = [
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
            // The fields before arrows and milestones: each says what to write instead
            (
                "a deadline",
                good(
                    "status: stable",
                    "status: stable\ndeadline: until the next deploy",
                ),
            ),
            (
                "a deadline kind",
                good("status: stable", "status: stable\ndeadline_kind: none"),
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
                good("filed:", "stale_after: 2026-09-28T10:00:00+09:00\nfiled:"),
            ),
            (
                "stale_after date only",
                good("filed:", "stale_after: 2027-03-31\nfiled:"),
            ),
            (
                "stale_after not a date",
                good("filed:", "stale_after: in six months\nfiled:"),
            ),
            (
                "a work item's fields under type Spec",
                good("type: Work Item", "type: Spec"),
            ),
        ];
        let passed = passing(&bad);
        assert!(passed.is_empty(), "passed: {passed:?}");
    }

    #[test]
    fn a_guide_passes_with_its_own_fields() {
        let rules = "---\ntype: Guide\ntitle: Work rules\ndescription: What goes here.\n---\n\n# What goes here\n";
        let Ok(WorkDoc::Guide(guide)) = work_doc(rules) else {
            panic!("{:?}", work_doc(rules))
        };
        assert_eq!(guide.status, Status::Stable);
        for bad in [
            rules.replace("description: What goes here.\n", ""),
            rules.replace("type: Guide", "type: Guide\ndeadline: never"),
        ] {
            assert!(work_doc(&bad).is_err(), "{bad}");
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
        assert!(spec(SPEC).is_ok(), "{:?}", spec(SPEC).err());
        let part = SPEC.replace("status: stable", "status: stable\nparent: big-work");
        assert_eq!(
            spec(&part).map(|(spec, _)| spec.parent),
            Ok(Some("big-work".into()))
        );
        let closed = closed_record(SPEC, "Done.");
        assert!(spec(&closed).is_ok(), "{:?}", spec(&closed).err());
        // The Resolution at the end, as it was written before it had to come first
        let at_the_end = SPEC.replace("status: stable", "status: deprecated\nprogress: done")
            + "\n# Resolution\n\nDone.\n";
        assert_eq!(
            spec(&at_the_end).err(),
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
                "no description",
                SPEC.replace("description: One sentence.\n", ""),
            ),
            (
                "a blank description",
                SPEC.replace("description: One sentence.", "description: \" \""),
            ),
            (
                "an empty tag",
                SPEC.replace("tags: [operations]", "tags: [\"\"]"),
            ),
            // A spec has exactly one area, as a work item does
            ("no tag", SPEC.replace("tags: [operations]\n", "")),
            // A parent is named by its slug, not by a path
            (
                "a parent named by its path",
                SPEC.replace("status: stable", "status: stable\nparent: /work/big.md"),
            ),
            (
                "a parent named by its path without .md",
                SPEC.replace("status: stable", "status: stable\nparent: work/big"),
            ),
            (
                "a parent named by its file name",
                SPEC.replace("status: stable", "status: stable\nparent: big.md"),
            ),
            (
                "an empty parent",
                SPEC.replace("status: stable", "status: stable\nparent: \"\""),
            ),
            (
                "a misspelled parent",
                SPEC.replace("status: stable", "status: stable\nparnet: big"),
            ),
            // The field before parent
            (
                "an epic",
                SPEC.replace("status: stable", "status: stable\nepic: big"),
            ),
            (
                "two tags",
                SPEC.replace("tags: [operations]", "tags: [operations, billing]"),
            ),
            (
                "unknown field",
                SPEC.replace("status: stable", "status: stable\nstatu: stable"),
            ),
            (
                "date-only verified",
                SPEC.replace(
                    "status: stable",
                    "status: stable\nverified: {by: human:a, at: 2026-10-01}",
                ),
            ),
            ("no frontmatter", "# Goals\n\nSomething.\n".to_string()),
            ("closed without a resolution", closed),
        ];
        for (name, text) in bad {
            assert!(spec(&text).is_err(), "{name} passed");
        }
    }
}
