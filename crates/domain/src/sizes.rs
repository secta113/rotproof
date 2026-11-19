//! The size of code: how many lines of code a file holds. The counts are facts and no verdict: what a number says of
//! the design is for the reader to judge.
//!
//! A line of code is a line with something on it that is neither white space, nor a comment, nor documentation (a
//! Python docstring), nor a test: a line that holds code and a trailing comment is a line of code, and a line inside a
//! string is too, as it is part of the program.

/// How many lines of `source` hold code: those with a byte that is not white space outside every range of `skipped`
/// (byte ranges of comments, docstrings and tests, which are not code). A range past the end of the source is cut.
pub fn lines_of_code(source: &str, skipped: &[(usize, usize)]) -> usize {
    let mut bytes = source.as_bytes().to_vec();
    for &(start, end) in skipped {
        let end = end.min(bytes.len());
        if start >= end {
            continue;
        }
        for byte in &mut bytes[start..end] {
            if *byte != b'\n' {
                *byte = b' ';
            }
        }
    }
    bytes
        .split(|byte| *byte == b'\n')
        .filter(|line| line.iter().any(|byte| !byte.is_ascii_whitespace()))
        .count()
}

/// The ranges of the comments `comments` gives (byte offset and text), as [`lines_of_code`] skips them.
pub fn spans(comments: &[(usize, String)]) -> Vec<(usize, usize)> {
    comments
        .iter()
        .map(|(at, text)| (*at, at + text.len()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blank_lines_and_lines_of_skipped_ranges_are_no_lines_of_code() {
        let source = "a = 1\n\n# note\nb = 2  # trailing\n   \n";
        let comments = [
            (source.find("# note").unwrap(), "# note".to_string()),
            (source.find("# trailing").unwrap(), "# trailing".to_string()),
        ];
        assert_eq!(lines_of_code(source, &spans(&comments)), 2);
        // With nothing skipped the comment line counts too
        assert_eq!(lines_of_code(source, &[]), 3);
    }

    #[test]
    fn a_block_comment_over_several_lines_keeps_its_line_breaks() {
        let source = "x\n/* one\n   two\n   three */\ny\n";
        let start = source.find("/*").unwrap();
        let end = source.find("*/").unwrap() + 2;
        assert_eq!(lines_of_code(source, &[(start, end)]), 2);
    }

    #[test]
    fn a_range_past_the_end_or_empty_is_harmless() {
        assert_eq!(lines_of_code("a\nb\n", &[(2, 99), (5, 4), (9, 12)]), 1);
        assert_eq!(lines_of_code("", &[(0, 3)]), 0);
    }
}
