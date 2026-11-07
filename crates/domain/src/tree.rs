//! The rules Rotproof keeps on how a file's text and name are read, and how a text is written back as its file was.
//! The ports to a project's files are in `protocol::outbound`.

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

/// How a file was written, in what [`with_lf`] drops: whether its lines end with `\r\n`, and whether it starts with a
/// byte order mark. A file a person wrote and Rotproof writes again keeps both, so a document checked out with CRLF
/// or saved by a Windows tool does not change on every line where Rotproof changed one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Written {
    crlf: bool,
    bom: bool,
}

impl Written {
    /// How `raw`, the text of a file as it is, was written. A file with any line ending `\r\n` is written back with
    /// `\r\n` on every line, and a line ending `\r` alone, which no tool writes now, comes back as `\n`.
    pub fn of(raw: &str) -> Self {
        Self {
            crlf: raw.contains("\r\n"),
            bom: raw.starts_with('\u{feff}'),
        }
    }

    /// `text`, read as [`with_lf`] reads, written as the file was: every `\n` as `\r\n` when its lines ended so, and
    /// after its byte order mark when it had one.
    pub fn restore(self, text: &str) -> String {
        let text = if self.crlf {
            text.replace('\n', "\r\n")
        } else {
            text.to_string()
        };
        if self.bom {
            format!("\u{feff}{text}")
        } else {
            text
        }
    }
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
    fn a_text_read_with_lf_is_written_back_as_the_file_was() {
        for raw in [
            "---\na\n",
            "---\r\na\r\n",
            "\u{feff}---\na\n",
            "\u{feff}---\r\na\r\n",
        ] {
            assert_eq!(Written::of(raw).restore(&with_lf(raw)), raw, "{raw:?}");
        }
        // A line added in Rotproof's text ends as the others do
        let raw = "\u{feff}a\r\nb\r\n";
        assert_eq!(
            Written::of(raw).restore(&format!("{}c\n", with_lf(raw))),
            "\u{feff}a\r\nb\r\nc\r\n"
        );
        // One line ending with CRLF makes every line end so; a byte order mark only counts at the start
        assert_eq!(Written::of("a\nb\r\n").restore("a\nb\n"), "a\r\nb\r\n");
        assert_eq!(
            Written::of("a\u{feff}\n").restore("a\u{feff}\n"),
            "a\u{feff}\n"
        );
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
