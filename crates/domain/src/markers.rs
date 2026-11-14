//! The marker check: no comment in the code holds a marker, one of the words in [`MARKERS`].
//!
//! - The first four mark a finding put off where nothing lists it and nothing asks when it is done. The last, at
//!   [`KNOWLEDGE`], marks knowledge kept in the code, which only someone reading that code finds: it is printed under
//!   a heading of its own, which asks where it belongs.
//! - The words count in upper case, as whole words, and only in comments: a task board's status named after one, or
//!   a phone format written with the letter X, is not a marker. Docstrings are strings, so they are not read.
//! - Every code file of the layout is read, `tests/` and the other paths that are not layers included, except the
//!   paths the project lists in `unchecked` (generated code is written by a tool the project does not edit).
//!
//! The layout's `language` says how the code is read: a Python file's `#` comments with Ruff's parser (`python.rs`), a
//! TypeScript or JavaScript file's `//` and `/* */` comments with oxc (`typescript.rs`), where JSX text is not a
//! comment, and a Rust file's `//` and `/* */` comments, doc comments included, with Rotproof's own scanner
//! (`rust.rs`): the code files of a Rust layout are its crates' manifests, so its `.rs` files are read wherever the
//! layout looks for code (`crates/`), a crate's `tests/` and `build.rs` included. The floor: at least one source file
//! is read, or the check fails instead of passing with nothing read.
//!
//! A project that declares `todo_link` ([`TodoLink`]) may keep the first marker in a comment when it names the issue
//! that tracks the work: the marker, a reference in parentheses, a colon and a description, all on one line. Such a
//! comment is not a finding: it is listed, on every run, in [`Markers::linked`], so work put off has a place that
//! tracks it. Whether the issue exists or is open is not judged here: the check reads files alone and gives the same
//! answer every time.
//!
//! The rules here judge the comments of a file once they are read; `application` walks the files and reads them.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use regex::Regex;

use crate::declaration::Declared;
use crate::text::line_of;

/// The words that fail in a comment: the first four put off a finding, and the last, at [`KNOWLEDGE`], keeps
/// knowledge in the code.
pub const MARKERS: [&str; 5] = ["TODO", "FIXME", "XXX", "HACK", "NOTE"];

/// The position in [`MARKERS`] of the marker that keeps knowledge in the code.
pub const KNOWLEDGE: usize = 4;

/// The heading a comment that holds the marker at [`KNOWLEDGE`] is printed under: where what it says belongs.
pub const KNOWLEDGE_HEADING: &str = "no comment holds NOTE (if it explains only the code beside it, keep it as a plain \
                                comment without the word; if it spans files, says how to run or operate something, \
                                or lies outside the repository, move it to docs/knowledge/ and tie it to the code by \
                                follows)";

static MARKER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(&format!(r"\b(?:{})\b", MARKERS.join("|"))).unwrap());

/// The position in [`MARKERS`] of the marker that a reference may tie to an issue.
const LINKABLE: usize = 0;

/// What follows that marker for [`TodoLink::Number`]: `(#`, an issue number without a leading zero, `): `.
static NUMBER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\((#[1-9][0-9]*)\): ").unwrap());

/// What follows that marker for [`TodoLink::Key`]: `(`, an upper-case letter, upper-case letters or digits, `-`, the
/// number of the issue, `): `.
static KEY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\(([A-Z][A-Z0-9]+-[0-9]+)\): ").unwrap());

/// How a project writes the reference of a comment that keeps the first marker of [`MARKERS`] and names the issue
/// that tracks the work: the value of `todo_link` in the declaration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TodoLink {
    /// The number of an issue: `(#123)`
    Number,
    /// The key of a tracker's ticket: `(ABC-123)`
    Key,
}

impl TodoLink {
    /// The values `todo_link` takes, in the order a message names them
    pub const KNOWN: [&'static str; 2] = ["number", "key"];

    /// The link a declaration's `value` names, or `None` for a value that is none of [`TodoLink::KNOWN`].
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "number" => Some(Self::Number),
            "key" => Some(Self::Key),
            _ => None,
        }
    }

    /// The accepted form as a comment writes it, with a reference that stands for any
    pub fn example(self) -> &'static str {
        match self {
            Self::Number => "TODO(#123): what is left",
            Self::Key => "TODO(ABC-123): what is left",
        }
    }

    /// What the reference is called in a sentence: the issue's number, or its key
    pub fn reference_name(self) -> &'static str {
        match self {
            Self::Number => "number",
            Self::Key => "key",
        }
    }

    /// The reference at the start of `rest`, the text of a comment after the marker, and how many bytes of `rest` it
    /// takes with its parentheses; `None` unless `rest` is the whole form. The description is the rest of the line,
    /// and has a character that is no white space (the end of a block comment is none).
    fn reference(self, rest: &str) -> Option<(String, usize)> {
        let found = match self {
            Self::Number => &NUMBER,
            Self::Key => &KEY,
        }
        .captures(rest)?;
        let line = rest[found.get(0)?.end()..].lines().next().unwrap_or("");
        let description = line.trim();
        if description
            .strip_suffix("*/")
            .unwrap_or(description)
            .trim()
            .is_empty()
        {
            return None;
        }
        let reference = found.get(1)?;
        // The parenthesis that closes it is the one byte after the reference
        Some((reference.as_str().to_string(), reference.end() + 1))
    }
}

/// A comment that keeps the first marker with the reference of the issue that tracks the work: where it is, and the
/// reference as written (`#123`, `ABC-123`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Linked {
    pub path: String,
    pub line: usize,
    pub reference: String,
}

/// What the marker check found.
#[derive(Debug, Default)]
pub struct Markers {
    /// Every comment that holds a marker of a finding put off, as `path:line` and the line under it as a traceback
    /// shows it, and every file that could not be read
    pub found: Vec<String>,
    /// The markers of a finding put off that were found, in the order of [`MARKERS`], each once
    pub words: Vec<&'static str>,
    /// Every comment that holds the marker at [`KNOWLEDGE`] and no other, as `found` names it
    pub knowledge: Vec<String>,
    /// Every comment that keeps the first marker with the reference of an issue, on a line that holds no marker of a
    /// finding put off: listed on every run, and a finding of no kind. Always empty unless the project declares
    /// `todo_link`
    pub linked: Vec<Linked>,
}

impl Markers {
    /// The heading `found` is printed under: the markers found, and what to do with the finding instead. Every marker
    /// of a finding put off when none was found (a file that could not be read, or no file at all). With `link`, a
    /// project that declares `todo_link` is told after the three ways out, when the first marker was among the words,
    /// that work an existing issue tracks may stay in the form it accepts.
    pub fn heading(&self, link: Option<TodoLink>) -> String {
        let words = if self.words.is_empty() {
            &MARKERS[..KNOWLEDGE]
        } else {
            &self.words[..]
        };
        let tracked = match link {
            Some(link) if self.words.contains(&MARKERS[LINKABLE]) => format!(
                ". Work that an existing issue already tracks may stay as {}, with that issue's {}",
                link.example(),
                link.reference_name()
            ),
            _ => String::new(),
        };
        format!(
            "no comment holds {} (fix the finding now, write it as one line in the task's issue, or let it go; \
             then take the comment out. Why the code is written so stays, without the word{tracked})",
            either(words)
        )
    }

    /// Sorts in `line`, whose comments hold the markers at its `positions` in [`MARKERS`]: into `found` when one of
    /// them puts off a finding, as taking the comment out settles the knowledge beside it too, and into `knowledge`
    /// when the only one is at [`KNOWLEDGE`]. The references on a line that holds no marker of a finding put off are
    /// listed in `linked`, whether or not the line also holds the marker at [`KNOWLEDGE`], which fails by itself.
    pub fn add(&mut self, line: Line) {
        let finding = line.finding();
        let put_off: Vec<&'static str> = line
            .positions
            .iter()
            .filter(|&&at| at != KNOWLEDGE)
            .map(|&at| MARKERS[at])
            .collect();
        if put_off.is_empty() {
            if !line.positions.is_empty() {
                self.knowledge.push(finding);
            }
            self.linked
                .extend(line.linked.into_iter().map(|reference| Linked {
                    path: line.path.clone(),
                    line: line.number,
                    reference,
                }));
            return;
        }
        self.found.push(finding);
        self.words.extend(put_off);
        self.words
            .sort_by_key(|word| MARKERS.iter().position(|m| m == word));
        self.words.dedup();
    }

    /// The floor of the check in a project of `declared`: `read` files read, of which at least one.
    pub fn floor(&mut self, declared: &Declared, read: usize) {
        if read == 0 {
            self.found.push(format!(
                "no code file was read: the {} layout finds none in the tree",
                declared.declaration.stack
            ));
        }
    }
}

/// Markers as a sentence says any of them: the words joined with commas, and the last with "or".
pub fn either(words: &[&str]) -> String {
    match words {
        [one] => one.to_string(),
        [rest @ .., last] => format!("{} or {last}", rest.join(", ")),
        [] => unreachable!("MARKERS is not empty, and a finding has a word"),
    }
}

/// One line of a source whose comments hold a marker, or the reference of an issue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    pub path: String,
    /// Counted from 1
    pub number: usize,
    /// The line as written, without the white space at its ends
    pub text: String,
    /// The markers on it that are no reference of an issue, as positions in [`MARKERS`], in the order they come
    pub positions: Vec<usize>,
    /// The references of the issues that comments on it keep the first marker with, as written
    pub linked: Vec<String>,
}

impl Line {
    /// `path:line` and the line under it, as a traceback shows it.
    pub fn finding(&self) -> String {
        format!("{}:{}\n{}", self.path, self.number, self.text)
    }
}

/// What the lines of the file at `path`, whose text is `source` and whose reader found `comments` (byte offset,
/// text), hold: each line whose comments hold a marker, or, with `link`, a reference of an issue.
pub fn in_file(
    path: &str,
    source: &str,
    comments: &[(usize, String)],
    link: Option<TodoLink>,
) -> Vec<Line> {
    let lines: Vec<&str> = source.lines().collect();
    let mut found: BTreeMap<usize, Line> = BTreeMap::new();
    for (at, comment) in comments {
        // Where the reference last taken ends: a word inside it is no marker. A comment of several lines is named at
        // the line of each marker in it
        let mut inside = 0;
        for word in MARKER.find_iter(comment) {
            if word.start() < inside {
                continue;
            }
            let position = MARKERS.iter().position(|m| *m == word.as_str()).unwrap();
            let number = line_of(source, at + word.start());
            let line = found.entry(number).or_insert_with(|| Line {
                path: path.to_string(),
                number,
                text: lines[number - 1].trim().to_string(),
                positions: Vec::new(),
                linked: Vec::new(),
            });
            // With `link`, the first marker followed by the form it accepts is a reference and no marker
            let reference = link
                .filter(|_| position == LINKABLE)
                .and_then(|link| link.reference(&comment[word.end()..]));
            match reference {
                Some((reference, taken)) => {
                    line.linked.push(reference);
                    inside = word.end() + taken;
                }
                None => line.positions.push(position),
            }
        }
    }
    found.into_values().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_marker_is_named_at_its_line_in_the_comments_given() {
        let source = "x = 1  # TODO(me) and NOTE\ny = 2\n\"\"\"\nFIXME in a string\n\"\"\"\n# NOTE why\n# XXX\n";
        // As a reader gives them: the docstring is no comment
        let comments = [
            (7, "# TODO(me) and NOTE".to_string()),
            (59, "# NOTE why".into()),
            (70, "# XXX".into()),
        ];
        let found: Vec<(String, Vec<usize>)> = in_file("x.py", source, &comments, None)
            .into_iter()
            .map(|line| (line.finding(), line.positions))
            .collect();
        assert_eq!(
            found,
            [
                ("x.py:1\nx = 1  # TODO(me) and NOTE".to_string(), vec![0, 4]),
                ("x.py:6\n# NOTE why".into(), vec![4]),
                ("x.py:7\n# XXX".into(), vec![2]),
            ]
        );
        // Upper case and whole words only
        let other = [(0, "# todo, TODOS, NOTES, NOTE_X, XXXL".to_string())];
        assert!(in_file("x.py", "# todo, TODOS, NOTES, NOTE_X, XXXL\n", &other, None).is_empty());
    }

    /// A line as `in_file` gives it for a one-line comment of a Python file
    fn line(text: &str, link: Option<TodoLink>) -> Line {
        let source = format!("{text}\n");
        let comments = [(0, text.to_string())];
        let mut lines = in_file("x.py", &source, &comments, link);
        assert_eq!(lines.len(), 1, "{text}");
        lines.remove(0)
    }

    /// What a comment of one line is read as with `link`: its references, and the markers that are none
    fn read(text: &str, link: TodoLink) -> (Vec<String>, Vec<usize>) {
        let line = line(text, Some(link));
        (line.linked, line.positions)
    }

    #[test]
    fn a_marker_with_the_form_of_an_issue_is_linked() {
        let number = |text: &str| read(text, TodoLink::Number);
        let key = |text: &str| read(text, TodoLink::Key);
        let linked = |reference: &str| (vec![reference.to_string()], Vec::new());
        assert_eq!(number("# TODO(#123): split this"), linked("#123"));
        assert_eq!(number("x = 1  # TODO(#7):  two spaces"), linked("#7"));
        assert_eq!(number("# TODO(#10): a"), linked("#10"));
        assert_eq!(key("# TODO(ABC-123): split this"), linked("ABC-123"));
        assert_eq!(key("# TODO(A1-7): one digit in the key"), linked("A1-7"));
        assert_eq!(key("# TODO(AB-0): a number may be 0"), linked("AB-0"));
        // The description is read as any comment: another marker in it fails, and is no part of the reference
        assert_eq!(
            number("# TODO(#1): and HACK"),
            (vec!["#1".to_string()], vec![3])
        );
        // The end of a block comment is no description, a character of the description is
        assert_eq!(number("/* TODO(#1): split this */"), linked("#1"));
        // Two references on a line
        assert_eq!(
            number("# TODO(#1): a TODO(#2): b"),
            (vec!["#1".to_string(), "#2".to_string()], Vec::new())
        );
    }

    #[test]
    fn the_reference_is_not_read_for_markers() {
        let key = |text: &str| read(text, TodoLink::Key);
        let linked = |reference: &str| (vec![reference.to_string()], Vec::new());
        assert_eq!(key("# TODO(XXX-12): x"), linked("XXX-12"));
        assert_eq!(key("# TODO(NOTE-3): x"), linked("NOTE-3"));
        assert_eq!(key("# TODO(HACK-3): x"), linked("HACK-3"));
        assert_eq!(key("# TODO(FIXME-3): x"), linked("FIXME-3"));
        // Only the reference is exempt: the description is read
        assert_eq!(
            key("# TODO(ABC-1): XXX"),
            (vec!["ABC-1".to_string()], vec![2])
        );
        // Not the form, so every word counts, the one in the parentheses too
        assert_eq!(key("# TODO(XXX-12) x"), (Vec::new(), vec![0, 2]));
    }

    #[test]
    fn what_is_not_the_whole_form_stays_a_marker() {
        let none: Vec<String> = Vec::new();
        for text in [
            "# TODO later",
            "# TODO #123",
            "# TODO: x",
            "# TODO(): x",
            "# TODO(#123)",
            "# TODO(#123):",
            "# TODO(#123): ",
            "# TODO(#123):   ",
            "# TODO(#123)  x",
            "# TODO(#123) x",
            "# TODO(#123):x",
            "# TODO(#0): x",
            "# TODO(#012): x",
            "# TODO(#): x",
            "# TODO(#1a): x",
            "# TODO( #1): x",
            "# TODO(#1 ): x",
            "# TODO (#1): x",
            "# TODO(me): x",
            "# TODO(#1, #2): x",
            "# TODO(#1) (#2): x",
            "# TODO(ABC-123): x",
            "# TODO(#1)): x",
            "# TODO((#1): x",
            "/* TODO(#1): */",
            "/* TODO(#1):*/",
        ] {
            assert_eq!(
                read(text, TodoLink::Number),
                (none.clone(), vec![0]),
                "{text}"
            );
        }
        for text in [
            "# TODO(#123): x",
            "# TODO(abc-123): x",
            "# TODO(A-123): one letter is no key",
            "# TODO(ABC123): x",
            "# TODO(ABC-): x",
            "# TODO(1AB-1): x",
            "# TODO(ABC-1a): x",
            "# TODO(ABC-1)",
            "# TODO(ABC-1) x",
            "# TODO(ABC-1):",
            "# TODO(ABC-1, ABC-2): x",
            "# TODO(me): x",
            "# TODO(): x",
        ] {
            assert_eq!(read(text, TodoLink::Key), (none.clone(), vec![0]), "{text}");
        }
    }

    #[test]
    fn a_comment_of_several_lines_is_judged_at_the_line_of_each_marker() {
        let source = "/* TODO(#1): a\n   TODO(#2):\n   TODO(#3): c */\n";
        let comments = [(0, source.trim_end().to_string())];
        let lines = in_file("x.ts", source, &comments, Some(TodoLink::Number));
        let judged: Vec<(usize, Vec<String>, Vec<usize>)> = lines
            .into_iter()
            .map(|line| (line.number, line.linked, line.positions))
            .collect();
        assert_eq!(
            judged,
            [
                (1, vec!["#1".to_string()], vec![]),
                (2, vec![], vec![0]),
                (3, vec!["#3".to_string()], vec![]),
            ]
        );
    }

    #[test]
    fn nothing_is_linked_unless_the_project_declares_it() {
        let line = line("# TODO(#123): split this", None);
        assert!(line.linked.is_empty());
        assert_eq!(line.positions, [0]);
        // Only the first marker has a form; the others fail in either mode
        for word in &MARKERS[1..] {
            for link in [TodoLink::Number, TodoLink::Key] {
                for reference in ["(#1)", "(ABC-1)"] {
                    let line = line_with(&format!("# {word}{reference}: x"), link);
                    assert!(line.linked.is_empty(), "{word}{reference}");
                    assert_eq!(line.positions.len(), 1, "{word}{reference}");
                }
            }
        }
    }

    fn line_with(text: &str, link: TodoLink) -> Line {
        line(text, Some(link))
    }

    /// The same, at line `number`
    fn line_at(number: usize, text: &str) -> Line {
        Line {
            number,
            ..line(text, None)
        }
    }

    #[test]
    fn a_line_with_a_linked_marker_and_another_fails_as_it_does_today() {
        let mut markers = Markers::default();
        // Alone: listed, no finding
        markers.add(line("# TODO(#1): a", Some(TodoLink::Number)));
        assert_eq!(markers.linked.len(), 1);
        assert!(markers.found.is_empty() && markers.knowledge.is_empty());
        // With another marker of a finding put off, or a second one not in the form: a finding, not listed
        markers.add(line("# TODO(#2): a, FIXME", Some(TodoLink::Number)));
        markers.add(line("# TODO(#3): a, TODO later", Some(TodoLink::Number)));
        assert_eq!(markers.linked.len(), 1);
        assert_eq!(
            markers.found,
            [
                "x.py:1\n# TODO(#2): a, FIXME",
                "x.py:1\n# TODO(#3): a, TODO later"
            ]
        );
        assert_eq!(markers.words, ["TODO", "FIXME"]);
        // With the marker that keeps knowledge: listed, and that marker is a finding of its own kind, as when alone
        markers.add(line("# TODO(#4): a, NOTE", Some(TodoLink::Number)));
        assert_eq!(markers.linked.len(), 2);
        assert_eq!(markers.knowledge, ["x.py:1\n# TODO(#4): a, NOTE"]);
        assert_eq!(markers.found.len(), 2);
        assert_eq!(
            markers.linked[1],
            Linked {
                path: "x.py".into(),
                line: 1,
                reference: "#4".into()
            }
        );
    }

    #[test]
    fn a_declaration_value_is_one_of_the_two_links() {
        assert_eq!(TodoLink::parse("number"), Some(TodoLink::Number));
        assert_eq!(TodoLink::parse("key"), Some(TodoLink::Key));
        for other in ["", "Number", "keys", "issue", "#"] {
            assert_eq!(TodoLink::parse(other), None, "{other}");
        }
        for name in TodoLink::KNOWN {
            assert!(TodoLink::parse(name).is_some(), "{name}");
        }
        assert_eq!(TodoLink::Number.example(), "TODO(#123): what is left");
        assert_eq!(TodoLink::Key.example(), "TODO(ABC-123): what is left");
    }

    #[test]
    fn a_note_alone_is_sorted_apart_from_the_findings_put_off() {
        let mut markers = Markers::default();
        markers.add(line("# TODO and NOTE", None));
        markers.add(line_at(6, "# NOTE why"));
        markers.add(line_at(7, "# XXX and TODO"));
        // Taking out the comment of a finding put off settles the knowledge in it too
        assert_eq!(
            markers.found,
            ["x.py:1\n# TODO and NOTE", "x.py:7\n# XXX and TODO"]
        );
        assert_eq!(markers.words, ["TODO", "XXX"]);
        assert_eq!(markers.knowledge, ["x.py:6\n# NOTE why"]);
        assert!(
            KNOWLEDGE_HEADING.starts_with("no comment holds NOTE (")
                && KNOWLEDGE_HEADING.contains("docs/knowledge/")
        );
    }

    #[test]
    fn nothing_read_fails_by_the_floor() {
        let declared = Declared::new(
            crate::declaration::parse_declaration(
                "stack = \"python\"\nareas = []\nui = \"atomic\"\n",
            )
            .unwrap(),
        )
        .unwrap();
        let mut none = Markers::default();
        none.floor(&declared, 0);
        assert_eq!(
            none.found,
            ["no code file was read: the python layout finds none in the tree"]
        );
        let mut some = Markers::default();
        some.floor(&declared, 1);
        assert!(some.found.is_empty());
    }

    #[test]
    fn the_heading_names_only_the_markers_found() {
        let heading = |words: Vec<&'static str>| {
            let found = Markers {
                words,
                ..Markers::default()
            };
            let heading = found.heading(None);
            heading[..heading.find(" (").unwrap()].to_string()
        };
        assert_eq!(heading(vec!["HACK"]), "no comment holds HACK");
        assert_eq!(
            heading(vec!["TODO", "HACK"]),
            "no comment holds TODO or HACK"
        );
        assert_eq!(
            heading(vec!["TODO", "XXX", "HACK"]),
            "no comment holds TODO, XXX or HACK"
        );
        // Nothing found but a file that could not be read: every marker of a finding put off
        assert_eq!(
            heading(Vec::new()),
            "no comment holds TODO, FIXME, XXX or HACK"
        );
    }

    #[test]
    fn the_heading_says_what_may_stay_only_for_a_project_that_declares_a_link() {
        let found = |words: Vec<&'static str>| Markers {
            words,
            ..Markers::default()
        };
        let plain = "no comment holds TODO or HACK (fix the finding now, write it as one line in the task's issue, or \
                     let it go; then take the comment out. Why the code is written so stays, without the word)";
        assert_eq!(found(vec!["TODO", "HACK"]).heading(None), plain);
        // The three ways out come first, then the form, for the declared value
        let number = found(vec!["TODO", "HACK"]).heading(Some(TodoLink::Number));
        assert_eq!(
            number,
            plain.replace(
                "without the word)",
                "without the word. Work that an existing issue already tracks may stay as TODO(#123): what is left, \
                 with that issue's number)"
            )
        );
        // The end names what the declared value writes: a key is no number
        assert_eq!(
            found(vec!["TODO", "HACK"]).heading(Some(TodoLink::Key)),
            plain.replace(
                "without the word)",
                "without the word. Work that an existing issue already tracks may stay as TODO(ABC-123): what is \
                 left, with that issue's key)"
            )
        );
        // The form is no first offer, and names no number to copy
        assert!(number.find("fix the finding now").unwrap() < number.find("TODO(#123)").unwrap());
        // No marker that has a form among the words, so none is offered
        for words in [vec!["HACK"], vec!["FIXME", "XXX"], Vec::new()] {
            let heading = found(words).heading(Some(TodoLink::Number));
            assert_eq!(heading.matches("TODO(").count(), 0, "{heading}");
            assert!(heading.ends_with("without the word)"), "{heading}");
        }
    }
}
