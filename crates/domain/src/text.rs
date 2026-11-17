//! Positions in a text, for messages.

/// The line (from 1) at a byte offset.
pub fn line_of(source: &str, at: usize) -> usize {
    source.as_bytes()[..at.min(source.len())]
        .iter()
        .filter(|&&b| b == b'\n')
        .count()
        + 1
}

/// The line breaks of a text, found once, for a reader that asks the line of many offsets of the same text: each answer
/// is the one [`line_of`] gives, found by a search instead of a count from the start, so that a file is read in a time
/// that grows with its size and not with its size times the names it uses.
pub struct Lines {
    breaks: Vec<usize>,
    len: usize,
}

impl Lines {
    pub fn new(source: &str) -> Self {
        Lines {
            breaks: source
                .bytes()
                .enumerate()
                .filter(|&(_, b)| b == b'\n')
                .map(|(at, _)| at)
                .collect(),
            len: source.len(),
        }
    }

    /// The line (from 1) at a byte offset, as [`line_of`] says it.
    pub fn line(&self, at: usize) -> usize {
        self.breaks.partition_point(|&b| b < at.min(self.len)) + 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_line_counts_the_line_breaks_before_the_offset() {
        assert_eq!(line_of("a\nb\nc", 0), 1);
        assert_eq!(line_of("a\nb\nc", 2), 2);
        assert_eq!(line_of("a\nb\nc", 99), 3);
    }

    #[test]
    fn the_lines_of_a_text_found_once_say_what_line_of_says_at_every_offset() {
        for source in [
            "",
            "a",
            "\n",
            "a\nb\nc",
            "a\n\n\nb\n",
            "\n\nx",
            "é\nü\r\nz\n",
        ] {
            let lines = Lines::new(source);
            for at in 0..source.len() + 3 {
                assert_eq!(lines.line(at), line_of(source, at), "{source:?} at {at}");
            }
        }
    }
}
