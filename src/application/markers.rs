//! The marker check, read from the tree: every code file walked and read, its comments found by the reader of its
//! language, and judged by the rules in `domain`.

use std::collections::BTreeSet;
use std::io;

use crate::application::tree::{code_files, read_code};
use domain::code::Parsers;
use domain::layers::{Declared, Language};
use domain::markers::{Markers, has_comments, in_file, is_unchecked, markers};
use domain::tree::Tree;

/// Every marker in a comment of the code of `declared`. An error is a directory that could not be walked.
pub fn problems(
    tree: &dyn Tree,
    parsers: &dyn Parsers,
    declared: &Declared,
) -> io::Result<Markers> {
    let Some(layout) = &declared.layout else {
        // A repository of records only has no code to read
        return Ok(Markers::default());
    };
    let mut found = Vec::new();
    let mut words = BTreeSet::new();
    let mut read = 0;
    for path in code_files(tree, &layout.scope, |name| has_comments(layout, name))? {
        if is_unchecked(declared, &path) {
            continue;
        }
        let Some(source) = read_code(tree, &path, "comments", &mut found)? else {
            continue;
        };
        read += 1;
        let (in_this, words_here) = in_file(
            &path,
            &source,
            &comments(parsers, layout.language, &source, &path),
        );
        found.extend(in_this);
        words.extend(words_here);
    }
    Ok(markers(declared, found, words, read))
}

/// The comments of a source file, by the reader of its language: offset and text.
fn comments(
    parsers: &dyn Parsers,
    language: Language,
    source: &str,
    path: &str,
) -> Vec<(usize, String)> {
    match language {
        Language::Python => parsers.python(source, path).comments,
        Language::TypeScript => parsers.typescript(source, path).comments,
        Language::Rust => utils::rust::comments(source),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::readers::Readers;
    use domain::markers::MARKERS;

    /// The lines of a source with a marker in a comment, as the reader of `language` finds its comments, and the
    /// markers on them in order
    fn words(language: Language, source: &str, path: &str) -> (Vec<usize>, Vec<&'static str>) {
        let (found, positions) = in_file(path, source, &comments(&Readers, language, source, path));
        let lines = found
            .iter()
            .map(|finding| {
                let at = finding.split_once('\n').unwrap().0;
                at.rsplit(':').next().unwrap().parse().unwrap()
            })
            .collect();
        (lines, positions.into_iter().map(|i| MARKERS[i]).collect())
    }

    #[test]
    fn every_marker_fails_in_a_comment() {
        for marker in MARKERS {
            assert_eq!(
                words(
                    Language::Python,
                    &format!("x = 1\n# {marker}: later\n"),
                    "x.py"
                ),
                (vec![2], vec![marker]),
                "{marker}"
            );
        }
        // One comment, one finding, with every marker in it
        assert_eq!(
            words(Language::Python, "x = 1  # TODO(me) and NOTE\n", "x.py"),
            (vec![1], vec!["TODO", "NOTE"])
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
        assert_eq!(words(Language::Python, source, "x.py"), (vec![], vec![]));
    }

    #[test]
    fn comments_after_a_syntax_error_are_read() {
        assert_eq!(
            words(Language::Python, "def f(:\n    pass\n# FIXME\n", "x.py"),
            (vec![3], vec!["FIXME"])
        );
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
            words(Language::TypeScript, source, "src/a.tsx"),
            (vec![2, 4, 5, 7], vec!["TODO", "FIXME", "XXX", "NOTE"])
        );
    }
}
