//! The checks of the comments in the code, read from the tree: every code file walked and read, its comments found by
//! the reader of its language, and judged by the rules in `domain` for markers (`markers.rs`) and for the comments
//! that switch the type checker off (`suppressions.rs`), which read the same files.

use std::io;

use crate::tree::{code_files, read_code};
use domain::declaration::Declared;
use domain::layers::Language;
use domain::markers::{Markers, in_file};
use domain::suppressions::Effect;
use protocol::outbound::Parsers;
use protocol::outbound::Tree;

/// What the comments of the code hold.
#[derive(Debug, Default)]
pub struct Comments {
    /// The markers, and every file that could not be read
    pub markers: Markers,
    /// Every comment that switches the type checker off, as `path:line` and the line under it
    pub silenced: Vec<String>,
    /// Every comment that may make the type checker check less, as `silenced` names it: a warning
    pub weakened: Vec<String>,
}

/// Every marker in a comment of the code of `declared`, and every comment that switches the type checker off or may
/// make it check less. An error is a directory that could not be walked.
pub fn problems(
    tree: &dyn Tree,
    parsers: &dyn Parsers,
    declared: &Declared,
) -> io::Result<Comments> {
    let Some(layout) = &declared.layout else {
        // A repository of records only has no code to read
        return Ok(Comments::default());
    };
    let mut found = Comments::default();
    let mut read = 0;
    for path in code_files(tree, &layout.scope, |name| layout.has_comments(name))? {
        if declared.is_unchecked(&path) {
            continue;
        }
        let Some(source) = read_code(tree, &path, "comments", &mut found.markers.found)? else {
            continue;
        };
        read += 1;
        let comments = comments(parsers, layout.language, &source, &path);
        for (finding, words) in in_file(&path, &source, &comments) {
            found.markers.add(finding, &words);
        }
        for (finding, effect) in
            domain::suppressions::in_file(layout.language, &path, &source, &comments)
        {
            match effect {
                Effect::Silences => found.silenced.push(finding),
                Effect::Weakens => found.weakened.push(finding),
            }
        }
    }
    found.markers.floor(declared, read);
    Ok(found)
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
        Language::Rust => domain::rust::comments(source),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::markers::MARKERS;
    use domain::suppressions::Effect;
    use infrastructure::outbound::readers::Readers;

    /// The lines of a source with a marker in a comment, as the reader of `language` finds its comments, and the
    /// markers on them in order
    fn words(language: Language, source: &str, path: &str) -> (Vec<usize>, Vec<&'static str>) {
        let found = in_file(path, source, &comments(&Readers, language, source, path));
        let lines = found
            .iter()
            .map(|(finding, _)| {
                let at = finding.split_once('\n').unwrap().0;
                at.rsplit(':').next().unwrap().parse().unwrap()
            })
            .collect();
        let markers = found
            .into_iter()
            .flat_map(|(_, positions)| positions)
            .map(|i| MARKERS[i])
            .collect();
        (lines, markers)
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

    /// The comments that switch the checker off and the ones that may make it check less, as the reader of `language`
    /// finds the comments of `source`
    fn effects(language: Language, source: &str, path: &str) -> Vec<(String, Effect)> {
        domain::suppressions::in_file(
            language,
            path,
            source,
            &comments(&Readers, language, source, path),
        )
    }

    #[test]
    fn a_comment_that_switches_the_checker_off_is_read_as_a_comment_only() {
        let source = "\
import json
x = json.loads(s)  # type: ignore[no-any-return]
note = \"# type: ignore\"
\"\"\"# pyright: ignore in a docstring\"\"\"
y = 1  # pyright: basic
";
        assert_eq!(
            effects(Language::Python, source, "x.py"),
            [
                (
                    "x.py:2\nx = json.loads(s)  # type: ignore[no-any-return]".to_string(),
                    Effect::Silences
                ),
                ("x.py:5\ny = 1  # pyright: basic".into(), Effect::Weakens),
            ]
        );
        let source = "\
const s = '// @ts-ignore';
// @ts-expect-error: the test passes a number
f(1);
const view = <p>// @ts-nocheck in text</p>;
";
        assert_eq!(
            effects(Language::TypeScript, source, "src/a.tsx"),
            [(
                "src/a.tsx:2\n// @ts-expect-error: the test passes a number".to_string(),
                Effect::Silences
            )]
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
