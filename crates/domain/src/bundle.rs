//! The part of `docs/` Rotproof keeps, as one OKF 0.2 bundle: `docs/knowledge/` sorted out once its documents are
//! read, the log, and the index files.
//!
//! Every `.md` in `docs/knowledge/` except the reserved names is a document with frontmatter. The index files are never
//! written by hand: `rotproof index` writes them, and `rotproof check` fails when one differs from what it would write.
//! The rest of `docs/` is the project's: Rotproof reads none of it, `docs/work/` of a project made before 1.0 included.

use std::collections::{BTreeMap, BTreeSet};

use chrono::NaiveDate;

use crate::frontmatter::{Sections, split};
use crate::layers::DECLARATION;
use crate::schema::{
    CLOSED_SECTION, Guide, Knowledge, KnowledgeDoc, Status, Time, WHEN_SECTION, knowledge_doc,
};
use yaml_rust2::Yaml;

/// File names OKF reserves. Never used for a document
pub const RESERVED: [&str; 2] = ["index.md", "log.md"];
/// The notice at the top of every generated index. An HTML comment, so OKF readers do not see it
pub const GENERATED: &str = "<!-- Generated from the frontmatter by `rotproof index`. Do not edit: `rotproof check` \
                             fails when this file differs from what `rotproof index` writes. -->";
/// The rules of `docs/knowledge/`. Rotproof writes them like an index file, so the rules a project reads are the rules
/// its Rotproof checks
pub const KNOWLEDGE_RULES: &str = include_str!("../../../records/knowledge-rules.md");
/// The log as `rotproof create` makes it. From then on it is the project's
pub const LOG: &str = include_str!("../../../records/log.md");
/// The bundle-root index links to these, in this order
const ROOT_ENTRIES: [(&str, &str, &str); 2] = [
    ("Knowledge", "knowledge/", "How things are now, and why."),
    ("Log", "log.md", "What was done, newest first."),
];

/// File name -> text.
pub type Docs = BTreeMap<String, String>;
/// File name -> why it was left out.
pub type Problems = BTreeMap<String, String>;

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
/// documents left out of the index files, from the documents read in `docs/knowledge/`.
pub fn expected(knowledge_docs: &Docs, areas: &[String]) -> (Vec<(String, String)>, Problems) {
    let read = knowledge_with_rules(knowledge_docs, areas);
    let files = vec![
        (in_docs("index.md"), render_root()),
        (in_docs("knowledge/rules.md"), KNOWLEDGE_RULES.into()),
        (
            in_docs("knowledge/index.md"),
            render_knowledge(&read.documents, &read.guides, &read.problems, areas),
        ),
    ];
    let problems = read
        .problems
        .into_iter()
        .map(|(name, why)| (format!("knowledge/{name}"), why))
        .collect();
    (files, problems)
}

/// The documents of `docs/knowledge/`, from those read there, with the knowledge rules read as Rotproof writes them, so
/// the index lists them on the run that writes them.
pub fn knowledge_with_rules(docs: &Docs, areas: &[String]) -> KnowledgeFolder {
    let mut docs = docs.clone();
    docs.insert("rules.md".into(), KNOWLEDGE_RULES.into());
    knowledge(&docs, areas)
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

/// The first sentence of the first paragraph of `text`, for the index. Bold text at the start counts as a sentence on
/// its own. Works for any language: a sentence ends at "。" or at "." followed by a space or the end of the paragraph,
/// but not inside parentheses ("（）" or "()", nested or not, either kind closing either) or inline code, where a full
/// stop ends a sentence of the aside, not of the paragraph: "実装した（PR #1。CI は pass）。" is one sentence. A
/// parenthesis or inline code left open is read as text, so the sentence ends at the first full stop after it that no
/// closed bracket holds, as it would without the bracket, and not at the end of the paragraph.
///
/// The paragraph is read as markdown reads it (see `paragraph`), so a sentence wrapped onto the next line is one
/// sentence, and a paragraph with no full stop is shown whole.
pub fn first_sentence(text: &str) -> String {
    let paragraph = paragraph(text);
    if let Some(rest) = paragraph.strip_prefix("**")
        && let Some(end) = rest.find('*')
        && end > 0
        && rest[end..].starts_with("**")
    {
        return paragraph[..end + 4].to_string();
    }
    match sentence_end(&paragraph) {
        Some(end) => paragraph[..end].to_string(),
        None => paragraph,
    }
}

/// The first paragraph of `text` as one line: its lines trimmed (which takes a `\r` too) and joined with a space,
/// except between two lines of Japanese, which are joined with none, as a space would split a word that was wrapped
/// and Japanese is not written with spaces. The paragraph is the first line, and the lines after it up to the first
/// that markdown would not read as part of it: a blank line, or one that starts a heading, a fenced code block, a
/// quote, a table row or a list item (so a list's first item is a paragraph of its own, as is the line before a list).
fn paragraph(text: &str) -> String {
    let mut lines = text.split('\n').map(str::trim);
    let mut paragraph = lines.next().unwrap_or("").to_string();
    if paragraph.is_empty() {
        return paragraph;
    }
    for line in lines {
        if line.is_empty() || starts_a_block(line) {
            break;
        }
        let both_wide = paragraph.chars().next_back().is_some_and(is_wide)
            && line.chars().next().is_some_and(is_wide);
        if !both_wide {
            paragraph.push(' ');
        }
        paragraph.push_str(line);
    }
    paragraph
}

/// Whether a trimmed line starts a block that interrupts a paragraph: `#` (a heading, with a space or nothing after),
/// a fence of three backticks or tildes, `>`, `|`, a bullet (`-`, `*` or `+` with a space or nothing after) or a number
/// and a `.` or `)` with a space or nothing after. A line that only starts with such a character is text:
/// `**bold**`, `#123`, `1.5 times`.
fn starts_a_block(line: &str) -> bool {
    // What follows a marker: nothing, or a space (the line is trimmed, so a tab or more spaces came from inside it)
    let alone = |rest: &str| rest.is_empty() || rest.starts_with(' ');
    let hashes = line.chars().take_while(|&c| c == '#').count();
    let digits = line.chars().take_while(char::is_ascii_digit).count();
    line.starts_with("```")
        || line.starts_with("~~~")
        || line.starts_with(['>', '|'])
        || ((1..=6).contains(&hashes) && alone(&line[hashes..]))
        || line.strip_prefix(['-', '*', '+']).is_some_and(alone)
        || (digits > 0 && line[digits..].strip_prefix(['.', ')']).is_some_and(alone))
}

/// Whether `c` is written without spaces between words: kana, ideographs, and the punctuation and forms of full width.
fn is_wide(c: char) -> bool {
    matches!(c,
        '\u{3000}'..='\u{30FF}' // punctuation, hiragana, katakana
        | '\u{3400}'..='\u{4DBF}' // ideographs, extension A
        | '\u{4E00}'..='\u{9FFF}' // ideographs
        | '\u{F900}'..='\u{FAFF}' // compatibility ideographs
        | '\u{FF00}'..='\u{FF60}' // forms of full width
        | '\u{FFE0}'..='\u{FFE6}')
}

/// Where the first sentence of a paragraph ends, after its full stop, as `first_sentence` reads it; none when no full
/// stop ends one, and the paragraph is one sentence.
fn sentence_end(line: &str) -> Option<usize> {
    // How deep in parentheses; the length of the run of backticks that opened inline code; and, while inside either,
    // where the text after the outermost opening starts, which is read again as text when that opening is not closed
    let mut depth = 0usize;
    let mut code: Option<usize> = None;
    let mut opened: Option<usize> = None;
    let mut chars = line.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        let after = i + c.len_utf8();
        if c == '`' {
            let mut run = 1;
            while chars.next_if(|&(_, next)| next == '`').is_some() {
                run += 1;
            }
            // A run of another length inside inline code is code too
            match code {
                Some(length) if length == run => {
                    code = None;
                    if depth == 0 {
                        opened = None;
                    }
                }
                None => {
                    code = Some(run);
                    if depth == 0 {
                        opened = Some(i + run);
                    }
                }
                Some(_) => {}
            }
            continue;
        }
        if code.is_some() {
            continue;
        }
        match c {
            '（' | '(' => {
                if depth == 0 {
                    opened = Some(after);
                }
                depth += 1;
            }
            '）' | ')' if depth > 0 => {
                depth -= 1;
                if depth == 0 {
                    opened = None;
                }
            }
            '。' if depth == 0 => return Some(after),
            '.' if depth == 0 && chars.peek().is_none_or(|(_, next)| next.is_whitespace()) => {
                return Some(after);
            }
            _ => {}
        }
    }
    let start = opened?;
    sentence_end(&line[start..]).map(|end| start + end)
}

/// The documents of a directory left out of its index, each with why, after its guides: what the index lists and counts
/// below may be short of them (an alarm is not shown), and a reader of
/// the index alone would not know it. Nothing when none is.
fn left_out_section(problems: &Problems) -> Vec<String> {
    if problems.is_empty() {
        return Vec::new();
    }
    let mut lines = vec!["".into(), "# Left out".into(), "".into()];
    lines.extend(problems.iter().map(|(key, why)| {
        // `knowledge/<file name>`: the index of docs/knowledge/ links to the file name
        let name = key.rsplit('/').next().unwrap_or(key);
        format!("* [{name}]({name}) - Not listed or counted below until it is fixed: {why}")
    }));
    lines
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

/// The knowledge index: the guides, the documents left out, the alarms that hold under `# When something happens`, the
/// other documents that hold by area in the order `areas` declares them, and the deprecated ones last under `# Closed`,
/// with what replaced them.
pub fn render_knowledge(
    documents: &BTreeMap<String, (Knowledge, Sections)>,
    guides: &BTreeMap<String, Guide>,
    problems: &Problems,
    areas: &[String],
) -> String {
    let mut out = vec![GENERATED.to_string()];
    out.extend(guide_section(guides));
    out.extend(left_out_section(problems));
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

/// Every tag the knowledge documents among `docs` use. A document that cannot be
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
        if kind != Some("Knowledge") {
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

/// The latest day it is anywhere at `now`, the day in UTC+14. A day of the records is checked against it, not against
/// the day of `now` itself: before 09:00 in Tokyo, the day written there is still to come in UTC, where CI runs.
pub fn latest_day(now: Time) -> NaiveDate {
    (now.naive_utc() + chrono::Duration::hours(14)).date()
}

#[cfg(test)]
mod tests {
    use chrono::{FixedOffset, TimeZone};

    use super::*;
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
    fn first_sentence_does_not_end_inside_brackets() {
        let pr = "実装した、push 済み・未 merge（2026-10-24、secta113/rotproof の PR #1。CI は `3be3976` で全ジョブ pass）。";
        let cases = [
            // A full stop inside parentheses, full-width or ASCII, ends a sentence of the aside, not of the line
            (format!("{pr}Rotproof のブランチ。"), pr),
            (
                "Done (see PR #1. CI passed). Then more.".into(),
                "Done (see PR #1. CI passed).",
            ),
            // Nested, and either kind closing either
            (
                "済んだ（基準は「a」（ユーザー。全文）。b）。次。".into(),
                "済んだ（基準は「a」（ユーザー。全文）。b）。",
            ),
            ("A (b (c. d) e. f) g. Next.".into(), "A (b (c. d) e. f) g."),
            ("A（b (c。d）e)。Next。".into(), "A（b (c。d）e)。"),
            // Inside inline code, with any run of backticks; a parenthesis in code is code
            (
                "Ran `a. b` and passed. Then.".into(),
                "Ran `a. b` and passed.",
            ),
            ("`x。y` を足した。次。".into(), "`x。y` を足した。"),
            ("Grep `(foo` found 3. Next.".into(), "Grep `(foo` found 3."),
            (
                "Wrote ``a ` b. c`` here. Next.".into(),
                "Wrote ``a ` b. c`` here.",
            ),
            // Left open on the line, a bracket is read as text: the sentence does not run to the end of the line
            ("A (unclosed. Next sentence. Third".into(), "A (unclosed."),
            ("前置き（閉じない。次の文。".into(), "前置き（閉じない。"),
            ("Ran `a. b and more. Next".into(), "Ran `a."),
            ("A (b. c) d (e. f".into(), "A (b. c) d (e."),
            ("A (x (b. c) d. e".into(), "A (x (b. c) d."),
            ("A (no full stop".into(), "A (no full stop"),
            // A closing parenthesis with no opening one is text
            ("1) first. 2) second.".into(), "1) first."),
            // No full stop outside the brackets: the line is one sentence
            ("Text (a. b) more".into(), "Text (a. b) more"),
        ];
        for (text, expected) in cases {
            assert_eq!(first_sentence(&text), expected, "{text:?}");
        }
    }

    #[test]
    fn first_sentence_reads_a_wrapped_paragraph_as_one() {
        let cases = [
            // The first sentence wrapped before its full stop
            (
                "Done from the tag `v1.0.0` (2026-01-02, run\n12345 passed). Then more.",
                "Done from the tag `v1.0.0` (2026-01-02, run 12345 passed).",
            ),
            (
                "It was wrapped\nonto two lines. Next.",
                "It was wrapped onto two lines.",
            ),
            // Wrapped inside parentheses, and inside inline code
            (
                "Done (see the\nrun. It passed). Then.",
                "Done (see the run. It passed).",
            ),
            ("Ran `a.\nb` and passed. Then.", "Ran `a. b` and passed."),
            // Bold wrapped; and a sentence that ends in the second line of a paragraph of three
            ("**Bold\nfirst.** Then more.", "**Bold first.**"),
            ("One\ntwo. Three\nfour.", "One two."),
            // A full stop at the end of a line ends the sentence, as before
            ("First.\nSecond line.", "First."),
            // Line endings and the padding of each line are not text
            ("  One\r\n  two.\r\nThree.", "One two."),
            ("One\r\ntwo", "One two"),
            // The paragraph ends at a blank line: with no full stop it is shown whole, and not the next paragraph
            (
                "No full stop\nhere\n\nSecond paragraph. More.",
                "No full stop here",
            ),
            ("Padded  \n\n  \nnext.", "Padded"),
            // Or where another block starts: a heading, a fence, a quote, a table row, a list item
            ("Before the list\n- one. two\n- three.", "Before the list"),
            ("- one\n  wrapped. Two\n- three.", "- one wrapped."),
            ("- one\n- two.", "- one"),
            ("1) one\n2) two.", "1) one"),
            ("Before a heading\n## Sub\nText.", "Before a heading"),
            ("Before a fence\n```\ncode.\n```", "Before a fence"),
            ("Before a quote\n> quoted.", "Before a quote"),
            ("| a | b |\n|---|---|\n| 1 | 2 |", "| a | b |"),
            // A line that only starts with such a character is text
            (
                "Fixed\n#123 and\n1.5 times\n**bold** too. Next.",
                "Fixed #123 and 1.5 times **bold** too.",
            ),
            ("A\n-not a bullet. B", "A -not a bullet."),
            // Japanese is joined with no space between two lines of it, and with one next to anything else
            (
                "実装した、push 済み・未\nmerge。次。",
                "実装した、push 済み・未 merge。",
            ),
            ("実装した。\n次。", "実装した。"),
            (
                "実装した、手元の\n環境で確かめた。次。",
                "実装した、手元の環境で確かめた。",
            ),
            (
                "実装した（PR #1。\nCI は通った）。次。",
                "実装した（PR #1。 CI は通った）。",
            ),
            (
                "実装した（確認。\n通った）。次。",
                "実装した（確認。通った）。",
            ),
            ("Rust の\nビルド。", "Rust のビルド。"),
            ("Rust\nのビルド。", "Rust のビルド。"),
            ("ビルドは\nRust で。", "ビルドは Rust で。"),
            ("全角（\n括弧）。", "全角（括弧）。"),
        ];
        for (text, expected) in cases {
            assert_eq!(first_sentence(text), expected, "{text:?}");
        }
    }

    #[test]
    fn the_latest_day_is_the_day_in_utc_plus_14() {
        let tokyo = FixedOffset::east_opt(9 * 3600).unwrap();
        // 08:00 in Tokyo is 23:00 of the day before in UTC, and already the same day in UTC+14
        let morning = tokyo.with_ymd_and_hms(2026, 10, 28, 8, 0, 0).unwrap();
        assert_eq!(
            latest_day(morning),
            NaiveDate::from_ymd_opt(2026, 10, 28).unwrap()
        );
        let evening = tokyo.with_ymd_and_hms(2026, 10, 28, 19, 0, 0).unwrap();
        assert_eq!(
            latest_day(evening),
            NaiveDate::from_ymd_opt(2026, 10, 29).unwrap()
        );
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
        let index = render_knowledge(&read.documents, &read.guides, &read.problems, &areas());
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
    fn an_index_names_what_it_left_out_before_its_lists() {
        // An alarm whose title a comment cuts: missing from the alarms, a reader of the index would never watch for it
        let alarm = "---\ntype: Knowledge\ntitle: Publish #403\ndescription: D.\ntags: [billing]\nstatus: stable\n\
                     ---\n\n# When\n\nThe publish job prints 403.\n\n# Do\n\nRenew the token.\n";
        let read = knowledge(
            &[("publish.md".to_string(), alarm.to_string())].into(),
            &areas(),
        );
        let index = render_knowledge(&read.documents, &read.guides, &read.problems, &areas());
        assert!(
            index.contains(
                "\n# Left out\n\n* [publish.md](publish.md) - Not listed or counted below until it is fixed: line 3: \
                 the frontmatter holds a YAML comment"
            ),
            "{index}"
        );
        // Nothing left out, no heading
        let read = knowledge(&Docs::new(), &areas());
        let index = render_knowledge(&read.documents, &read.guides, &read.problems, &areas());
        assert!(!index.contains("Left out"), "{index}");
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
        let index = render_knowledge(&read.documents, &read.guides, &read.problems, &areas());
        assert_eq!(
            index
                .lines()
                .filter(|l| l.contains("](") || l.starts_with('#'))
                .collect::<Vec<_>>(),
            [
                "# Guides",
                "* [Knowledge rules](rules.md) - What goes in docs/knowledge/ and how each document and alarm is \
                 written. The format is OKF 0.2; rotproof check checks it.",
                "# operations",
                "* [Ops](ops.md) - D.",
                "# billing",
                "* [API](api.md) - D.",
                "# Closed",
                "* [Old](old.md) - D. | Resolution: Replaced by the API.",
            ]
        );
    }

    /// The areas the tests declare: not in alphabetical order, so an index sorted by name would differ
    fn areas() -> Vec<String> {
        ["operations", "billing", "unused"].map(String::from).into()
    }

    #[test]
    fn the_tags_of_the_knowledge_documents_are_collected_and_others_are_not() {
        let docs: Docs = [
            ("a.md", "---\ntype: Knowledge\ntags: [billing, ops]\n---\n"),
            ("b.md", "---\ntype: Knowledge\ntags: records\n---\n"),
            ("rules.md", "---\ntype: Guide\ntags: [guides]\n---\n"),
            ("old.md", "---\ntype: Work Item\ntags: [work]\n---\n"),
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
    fn a_title_with_brackets_stays_the_text_of_its_own_entry() {
        let doc = "---\ntype: Knowledge\ntitle: 'Evil ](fake.md) [hacked \\ end'\ndescription: D.\ntags: [billing]\n\
                   status: stable\n---\n\n# Why\n\nW.\n";
        let read = knowledge(&[("x.md".to_string(), doc.to_string())].into(), &areas());
        let index = render_knowledge(&read.documents, &read.guides, &read.problems, &areas());
        let line = index.lines().find(|l| l.starts_with("* ")).unwrap();
        assert!(
            line.starts_with(r"* [Evil \](fake.md) \[hacked \\ end](x.md) - "),
            "{line}"
        );
        // Read back as a reader reads it, the entry is one link to its own document, with the title as its text
        let found = crate::markdown::links(line);
        assert_eq!(
            found,
            [(
                r"Evil ](fake.md) [hacked \ end".to_string(),
                "x.md".to_string()
            )]
        );
    }

    #[test]
    fn every_generated_file_is_expected_with_the_rules_listed() {
        let none = Docs::new();
        let (files, problems) = expected(&none, &areas());
        let paths: Vec<&str> = files.iter().map(|(path, _)| path.as_str()).collect();
        assert_eq!(
            paths,
            [
                "docs/index.md",
                "docs/knowledge/rules.md",
                "docs/knowledge/index.md",
            ]
        );
        assert!(problems.is_empty(), "{problems:?}");
        // The rules are listed on the run that writes them
        let index = |path: &str| &files.iter().find(|(p, _)| p == path).unwrap().1;
        assert!(index("docs/knowledge/index.md").contains("](rules.md) - "));
        // The root index links to what Rotproof keeps, and to no docs/work/
        let root = index("docs/index.md");
        assert!(
            root.contains("](knowledge/) - ") && root.contains("](log.md) - "),
            "{root}"
        );
        assert!(!root.contains("work/"), "{root}");
        assert!(is_document("a.md", false));
        assert!(!is_document("index.md", false) && !is_document("log.md", false));
        assert!(!is_document("a.md", true) && !is_document("a.txt", false));
    }
}
