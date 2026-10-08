//! The marker check: no comment in the code holds a marker, one of the words in [`MARKERS`].
//!
//! - The first four mark work left to do that no record holds: nothing lists it, and nothing asks when it is done.
//!   The last marks knowledge kept where nothing can filter or link it.
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

use std::collections::{BTreeMap, BTreeSet};
use std::sync::LazyLock;

use regex::Regex;

use crate::code::is_source;
use crate::layers::{Declared, Language, Layout};
use utils::paths::within;
use utils::text::line_of;

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
        format!(
            "no comment holds {} (work left to do goes in docs/work/, a decision and its reason in the spec or the \
             log entry of the change, how to read the code in a plain comment without the word)",
            either(words)
        )
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
        Language::Rust => utils::rust::is_source(name),
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
/// each line whose comments hold a marker, as `path:line` and the line, and the markers on them as positions in
/// [`MARKERS`].
pub fn in_file(
    path: &str,
    source: &str,
    comments: &[(usize, String)],
) -> (Vec<String>, Vec<usize>) {
    let lines: Vec<&str> = source.lines().collect();
    let mut found = Vec::new();
    let mut words = Vec::new();
    for (line, in_comment) in in_comments(source, comments) {
        found.push(format!("{path}:{line}\n{}", lines[line - 1].trim()));
        words.extend(in_comment);
    }
    (found, words)
}

/// What the marker check found in a project of `declared`, from the findings and the positions of the markers in
/// them, with its floor: `read` files read, of which at least one.
pub fn markers(
    declared: &Declared,
    mut found: Vec<String>,
    words: BTreeSet<usize>,
    read: usize,
) -> Markers {
    if read == 0 {
        found.push(format!(
            "no code file was read: the {} layout finds none in the tree",
            declared.declaration.stack
        ));
    }
    Markers {
        found,
        // In the order of MARKERS: a set of their positions
        words: words.into_iter().map(|i| MARKERS[i]).collect(),
    }
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
        let source = "x = 1  # TODO(me) and NOTE\ny = 2\n\"\"\"\nFIXME in a string\n\"\"\"\n# a plain\n# HACK\n";
        // As a reader gives them: the docstring is no comment
        let comments = [
            (7, "# TODO(me) and NOTE".to_string()),
            (59, "# a plain".into()),
            (69, "# HACK".into()),
        ];
        let (found, words) = in_file("x.py", source, &comments);
        assert_eq!(
            found,
            ["x.py:1\nx = 1  # TODO(me) and NOTE", "x.py:7\n# HACK"]
        );
        assert_eq!(words, [0, 4, 3]);
        // Upper case and whole words only
        let other = [(0, "# todo, TODOS, NOTES, NOTE_X, XXXL".to_string())];
        assert_eq!(
            in_file("x.py", "# todo, TODOS, NOTES, NOTE_X, XXXL\n", &other).0,
            Vec::<String>::new()
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
        let none = markers(&declared, Vec::new(), BTreeSet::new(), 0);
        assert_eq!(
            none.found,
            ["no code file was read: the python layout finds none in the tree"]
        );
        let some = markers(&declared, Vec::new(), [4, 0].into(), 1);
        assert!(some.found.is_empty());
        assert_eq!(some.words, ["TODO", "NOTE"]);
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
}
