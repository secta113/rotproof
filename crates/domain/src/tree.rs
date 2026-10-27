//! The rules Rotproof keeps on how a file's text and name are read. The ports to a project's files are in
//! `protocol::outbound`.

/// The text of a file as Rotproof reads it: every line ending as `\n`, and no byte order mark.
///
/// Line endings are normalized as Python's text mode does, which the original checks read with: a file checked out with
/// CRLF would otherwise keep `\r` at the end of each heading, and its anchors would differ. A byte order mark, which
/// Windows tools write before UTF-8 (PowerShell 5.1's `-Encoding UTF8`), is dropped as Python's `utf-8-sig` drops it:
/// a record would otherwise not start with `---`, and fail as one with no frontmatter.
pub fn with_lf(text: &str) -> String {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    text.replace("\r\n", "\n").replace('\r', "\n")
}

/// A name as Windows reads it: case does not count, and dots and spaces at the end are dropped. Two names that read
/// the same open the same entry on Windows, and different ones on Linux and GitHub.
pub fn as_windows_reads(name: &str) -> String {
    name.trim_end_matches(['.', ' ']).to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_endings_become_lf() {
        assert_eq!(with_lf("# A\r\nb\rc\n"), "# A\nb\nc\n");
    }

    #[test]
    fn a_byte_order_mark_at_the_start_is_dropped() {
        assert_eq!(with_lf("\u{feff}---\r\n"), "---\n");
        // Anywhere else it is text
        assert_eq!(with_lf("a\u{feff}"), "a\u{feff}");
    }

    #[test]
    fn windows_reads_a_name_in_any_case_and_without_dots_or_spaces_at_its_end() {
        for name in ["readme.md", "README.md.", "README.md ", "Readme.MD. ."] {
            assert_eq!(
                as_windows_reads(name),
                as_windows_reads("README.md"),
                "{name}"
            );
        }
        assert_ne!(
            as_windows_reads("README.md:secret"),
            as_windows_reads("README.md")
        );
    }
}
