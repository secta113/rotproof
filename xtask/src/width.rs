//! A line that holds a comment is at most 120 characters, in every `.rs` file of the repository. rustfmt wraps code but
//! not comments, so nothing else keeps them in step: 8 lines had run past it while the rule was written nowhere.
//!
//! The comments are found by the scanner the marker check uses, so text in a string is not a comment. A line is counted
//! whole, the code before a comment on it included, in characters, not bytes.

use std::collections::BTreeSet;

/// The most characters a line that holds a comment may have
pub const WIDTH: usize = 120;

/// The lines of `source` that hold a comment and are longer than [`WIDTH`], each as `path:line`, and how many lines
/// with a comment were read.
pub fn in_file(path: &str, source: &str) -> (Vec<String>, usize) {
    let starts: Vec<usize> = std::iter::once(0)
        .chain(source.match_indices('\n').map(|(at, _)| at + 1))
        .collect();
    let line_at = |offset: usize| starts.partition_point(|&start| start <= offset);
    let mut lines = BTreeSet::new();
    for (at, text) in utils::rust::comments(source) {
        let end = at + text.trim_end_matches('\n').len().max(1) - 1;
        lines.extend(line_at(at)..=line_at(end));
    }
    let all: Vec<&str> = source.lines().collect();
    let found = lines
        .iter()
        .filter_map(|&number| {
            let line = all.get(number - 1)?;
            let length = line.chars().count();
            (length > WIDTH).then(|| {
                format!("{path}:{number}: {length} characters, over {WIDTH}: wrap the comment")
            })
        })
        .collect();
    (found, lines.len())
}

/// Every line over the width in `files` (path, text), with the floor: at least one line with a comment read.
pub fn problems(files: &[(String, String)]) -> Vec<String> {
    let mut found = Vec::new();
    let mut read = 0;
    for (path, source) in files {
        let (here, lines) = in_file(path, source);
        found.extend(here);
        read += lines;
    }
    if read == 0 {
        found.push("no line with a comment was read in a .rs file".into());
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_with_a_comment_over_the_width_is_found_by_its_number() {
        let long = "x".repeat(WIDTH);
        let source = format!(
            "// {long}\nlet s = \"{long}\";\nfn f() {{}} // {long}\n/* one\n{long}x\n*/\n/// é{}\n",
            "é".repeat(WIDTH - 5)
        );
        let (found, read) = in_file("a.rs", &source);
        let numbers: Vec<&str> = found
            .iter()
            .map(|why| why.split(": ").next().unwrap())
            .collect();
        // The string on line 2 is not a comment, and line 7 is 120 characters in 240 bytes
        assert_eq!(numbers, ["a.rs:1", "a.rs:3", "a.rs:5"]);
        assert_eq!(read, 6);
    }

    #[test]
    fn nothing_read_fails_by_the_floor() {
        assert_eq!(problems(&[]).len(), 1);
        assert_eq!(problems(&[("a.rs".into(), "fn f() {}\n".into())]).len(), 1);
        assert_eq!(
            problems(&[("a.rs".into(), "// short\n".into())]),
            Vec::<String>::new()
        );
    }
}
