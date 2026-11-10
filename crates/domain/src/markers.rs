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
//! The rules here judge the comments of a file once they are read; `application` walks the files and reads them.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use regex::Regex;

use crate::code::is_source;
use crate::layers::{Declared, Language, Layout};
use crate::paths::within;
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
}

impl Markers {
    /// The heading `found` is printed under: the markers found, and what to do with the finding instead. Every marker
    /// of a finding put off when none was found (a file that could not be read, or no file at all).
    pub fn heading(&self) -> String {
        let words = if self.words.is_empty() {
            &MARKERS[..KNOWLEDGE]
        } else {
            &self.words[..]
        };
        format!(
            "no comment holds {} (fix the finding now, write it as one line in the task's issue, or let it go; \
             then take the comment out. Why the code is written so stays, without the word)",
            either(words)
        )
    }

    /// Sorts in `finding`, a line whose comments hold the markers at `positions` in [`MARKERS`]: into `found` when one
    /// of them puts off a finding, as taking the comment out settles the knowledge beside it too, and into
    /// `knowledge` otherwise.
    pub fn add(&mut self, finding: String, positions: &[usize]) {
        let put_off: Vec<&'static str> = positions
            .iter()
            .filter(|&&at| at != KNOWLEDGE)
            .map(|&at| MARKERS[at])
            .collect();
        if put_off.is_empty() {
            self.knowledge.push(finding);
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

/// Whether the file named `name`, in a project of `layout`, has comments to read.
pub fn has_comments(layout: &Layout, name: &str) -> bool {
    match layout.language {
        // Every code file of a Python layout is a `.py` file: a test reads every layout
        Language::Python => layout.is_code(name),
        // Every file in `src/` is code in the layout; only source has comments to read
        Language::TypeScript => layout.is_code(name) && is_source(name),
        // The code files of a Rust layout are its crates' manifests; the comments are in the `.rs` files beside them
        Language::Rust => crate::rust::is_source(name),
    }
}

/// Whether the file at `path` sits in a path the project lists in `unchecked`. A path that holds or sits in a layer
/// switches nothing off, as the structure check says of it: the markers in that layer are read all the same.
pub fn is_unchecked(declared: &Declared, path: &str) -> bool {
    declared
        .unchecked()
        .filter(|skip| declared.overlapped(skip).is_none())
        .any(|skip| within(path, skip))
}

/// The findings in the file at `path`, whose text is `source` and whose reader found `comments` (byte offset, text):
/// each line whose comments hold a marker, as `path:line` and the line, with the markers on it as positions in
/// [`MARKERS`].
pub fn in_file(
    path: &str,
    source: &str,
    comments: &[(usize, String)],
) -> Vec<(String, Vec<usize>)> {
    let lines: Vec<&str> = source.lines().collect();
    in_comments(source, comments)
        .into_iter()
        .map(|(line, words)| (format!("{path}:{line}\n{}", lines[line - 1].trim()), words))
        .collect()
}

/// Every line of a source whose comments hold a marker, and the markers on it as positions in [`MARKERS`], from the
/// comments the reader of its language found (byte offset, text). A comment of several lines is named at the line of
/// each marker in it.
fn in_comments(source: &str, comments: &[(usize, String)]) -> Vec<(usize, Vec<usize>)> {
    let mut lines: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (at, comment) in comments {
        for word in MARKER.find_iter(comment) {
            lines
                .entry(line_of(source, at + word.start()))
                .or_default()
                .push(MARKERS.iter().position(|m| *m == word.as_str()).unwrap());
        }
    }
    lines.into_iter().collect()
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
        let found = in_file("x.py", source, &comments);
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
        assert!(in_file("x.py", "# todo, TODOS, NOTES, NOTE_X, XXXL\n", &other).is_empty());
    }

    #[test]
    fn a_note_alone_is_sorted_apart_from_the_findings_put_off() {
        let mut markers = Markers::default();
        markers.add("x.py:1\n# TODO and NOTE".into(), &[0, KNOWLEDGE]);
        markers.add("x.py:6\n# NOTE why".into(), &[KNOWLEDGE]);
        markers.add("x.py:7\n# XXX and TODO".into(), &[2, 0]);
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
    fn a_file_in_a_path_listed_in_unchecked_is_skipped() {
        let declared = Declared::new(
            crate::layers::parse_declaration(
                "stack = \"python\"\nareas = []\nunchecked = [\"scripts/\", \"gen\"]\n",
            )
            .unwrap(),
        )
        .unwrap();
        assert!(is_unchecked(&declared, "scripts/run.py"));
        assert!(is_unchecked(&declared, "gen/a/b.py"));
        assert!(!is_unchecked(&declared, "generated/a.py"));
        assert!(!is_unchecked(&declared, "domain/scripts/x.py"));
    }

    #[test]
    fn a_path_in_unchecked_that_holds_or_sits_in_a_layer_skips_nothing() {
        let declared = Declared::new(
            crate::layers::parse_declaration(
                "stack = \"python\"\nareas = []\nunchecked = [\"domain\", \"ui/atoms/gen/\"]\n",
            )
            .unwrap(),
        )
        .unwrap();
        assert!(!is_unchecked(&declared, "domain/song.py"));
        assert!(!is_unchecked(&declared, "ui/atoms/gen/x.py"));
    }

    #[test]
    fn nothing_read_fails_by_the_floor() {
        let declared = Declared::new(
            crate::layers::parse_declaration("stack = \"python\"\nareas = []\n").unwrap(),
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
            let heading = found.heading();
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
}
