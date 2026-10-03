//! The marker check: no comment in the code holds `TODO`, `FIXME`, `XXX`, `HACK` or `NOTE`.
//!
//! - A `TODO` (and `FIXME`, `XXX`, `HACK`) is work left to do that no record holds: nothing lists it, and nothing
//!   asks when it is done. A `NOTE` is knowledge kept where nothing can filter or link it.
//! - The words count in upper case, as whole words, and only in comments: `Status.TODO` in a task board and
//!   `"XXX-XXXX"` in a phone format are not markers. Docstrings are strings, so they are not read.
//! - Every code file of the layout is read, `tests/` and the other paths that are not layers included, except the
//!   paths the project lists in `unchecked` (generated code is written by a tool the project does not edit).
//!
//! The layout's `language` says how the code is read: a Python file's `#` comments with Ruff's parser (`python.rs`), a
//! TypeScript or JavaScript file's `//` and `/* */` comments with oxc (`typescript.rs`), where JSX text is not a
//! comment. For `rust`, whose code files are the crates' manifests, the check says that it did not run. The floor:
//! at least one source file is read, or the check fails instead of passing with nothing read.

use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;

use crate::layers::{Declared, Language, not_read};
use crate::source::{code_files, line_of, read_code, within};

/// The words that fail in a comment.
pub const MARKERS: [&str; 5] = ["TODO", "FIXME", "XXX", "HACK", "NOTE"];

static MARKER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(&format!(r"\b(?:{})\b", MARKERS.join("|"))).unwrap());

/// What the marker check found.
#[derive(Debug, Default)]
pub struct Markers {
    /// Every comment that holds a marker, as `path:line` and the line under it as a traceback shows it, and every
    /// file that could not be read
    pub found: Vec<String>,
    /// The markers found, in the order of [`MARKERS`], each once
    pub words: Vec<&'static str>,
    /// What was not checked, and why
    pub skipped: Option<String>,
}

impl Markers {
    /// The heading the findings are printed under: the markers found, and where the text goes instead. All of them
    /// when none was found (a file that could not be read, or no file at all).
    pub fn heading(&self) -> String {
        let words = if self.words.is_empty() {
            &MARKERS[..]
        } else {
            &self.words[..]
        };
        let listed = match words {
            [one] => one.to_string(),
            [rest @ .., last] => format!("{} or {last}", rest.join(", ")),
            [] => unreachable!("MARKERS is not empty"),
        };
        format!(
            "no comment holds {listed} (work left to do goes in docs/backlog/, a decision and its reason in the spec \
             or the log entry of the change, how to read the code in a plain comment without the word)"
        )
    }
}

/// Every marker in a comment of the code of `declared`. An error is a directory that could not be walked.
pub fn problems(root: &Path, declared: &Declared) -> io::Result<Markers> {
    let Some(layout) = &declared.layout else {
        // A repository of records only has no code to read
        return Ok(Markers::default());
    };
    // Which code files have comments to read, and how to read them
    type Comments = fn(&str, &str) -> Vec<(usize, String)>;
    let (is_source, comments): (fn(&str) -> bool, Comments) = match layout.language {
        // Every code file of a Python layout is a `.py` file: a test reads every layout
        Language::Python => (
            |_| true,
            |source, path| crate::python::read(source, path).comments,
        ),
        // Every file in `src/` is code in the layout; only source has comments to read
        Language::TypeScript => (crate::typescript::is_source, |source, path| {
            crate::typescript::read(source, path).comments
        }),
        // The code files of a Rust layout are its manifests, and the comments of `.rs` files are not read yet
        Language::Rust => {
            return Ok(Markers {
                skipped: Some(format!(
                    "comments are not checked for {}: {}",
                    MARKERS.join(", "),
                    not_read(&declared.declaration.stack, "comments")
                )),
                ..Markers::default()
            });
        }
    };
    let unchecked: Vec<&str> = declared
        .declaration
        .unchecked
        .iter()
        .map(|path| path.trim_end_matches('/'))
        .collect();
    let mut found = Vec::new();
    let mut words = BTreeSet::new();
    let mut read = 0;
    for path in code_files(root, &layout.scope, |name| layout.is_code(name))? {
        if unchecked.iter().any(|skip| within(&path, skip)) {
            continue;
        }
        if !is_source(&path) {
            continue;
        }
        let Some(source) = read_code(root, &path, "comments", &mut found)? else {
            continue;
        };
        read += 1;
        let lines: Vec<&str> = source.lines().collect();
        for (line, in_comment) in in_comments(&source, &comments(&source, &path)) {
            found.push(format!("{path}:{line}\n{}", lines[line - 1].trim()));
            words.extend(in_comment);
        }
    }
    if read == 0 {
        found.push(format!(
            "no code file was read: the {} layout finds none in the tree",
            declared.declaration.stack
        ));
    }
    Ok(Markers {
        found,
        // In the order of MARKERS: a set of their positions
        words: words.into_iter().map(|i| MARKERS[i]).collect(),
        skipped: None,
    })
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

    fn named(found: Vec<(usize, Vec<usize>)>) -> Vec<(usize, Vec<&'static str>)> {
        found
            .into_iter()
            .map(|(line, at)| (line, at.into_iter().map(|i| MARKERS[i]).collect()))
            .collect()
    }

    fn words(source: &str) -> Vec<(usize, Vec<&'static str>)> {
        named(in_comments(
            source,
            &crate::python::read(source, "x.py").comments,
        ))
    }

    #[test]
    fn every_marker_fails_in_a_comment() {
        for marker in MARKERS {
            assert_eq!(
                words(&format!("x = 1\n# {marker}: later\n")),
                [(2, vec![marker])],
                "{marker}"
            );
        }
        // One comment, one finding, with every marker in it
        assert_eq!(
            words("x = 1  # TODO(me) and NOTE\n"),
            [(1, vec!["TODO", "NOTE"])]
        );
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
        assert_eq!(heading(vec!["NOTE"]), "no comment holds NOTE");
        assert_eq!(
            heading(vec!["TODO", "NOTE"]),
            "no comment holds TODO or NOTE"
        );
        assert_eq!(
            heading(vec!["TODO", "XXX", "NOTE"]),
            "no comment holds TODO, XXX or NOTE"
        );
        // Nothing found but a file that could not be read: every marker
        assert_eq!(
            heading(Vec::new()),
            "no comment holds TODO, FIXME, XXX, HACK or NOTE"
        );
    }

    #[test]
    fn a_marker_outside_a_comment_or_in_another_word_passes() {
        let source = "\
class Status:
    TODO = 1
    \"\"\"TODO in a docstring is a string.\"\"\"
phone = \"XXX-XXXX\"
note = Status.TODO  # todo in lower case, TODOS and NOTES, NOTE_X and XXXL are other words
";
        assert_eq!(words(source), []);
    }

    #[test]
    fn comments_after_a_syntax_error_are_read() {
        assert_eq!(words("def f(:\n    pass\n# FIXME\n"), [(3, vec!["FIXME"])]);
    }

    fn typescript_words(source: &str) -> Vec<(usize, Vec<&'static str>)> {
        named(in_comments(
            source,
            &crate::typescript::read(source, "src/a.tsx").comments,
        ))
    }

    #[test]
    fn typescript_comments_are_read_and_jsx_text_and_strings_are_not() {
        let source = "\
const status = 'TODO';
// TODO: one
const view = <p>NOTE in text, // HACK in text</p>;
/* FIXME
   and XXX on the next line */
const done = Status.TODO; // a plain comment
const empty = <div>{/* NOTE in a JSX comment */}</div>;
";
        assert_eq!(
            typescript_words(source),
            [
                (2, vec!["TODO"]),
                (4, vec!["FIXME"]),
                (5, vec!["XXX"]),
                (7, vec!["NOTE"]),
            ]
        );
    }
}
