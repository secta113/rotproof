//! A record split into its YAML frontmatter and the text under each `# heading` of its body.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use regex::Regex;
use yaml_rust2::yaml::Hash;
use yaml_rust2::{Yaml, YamlLoader};

use crate::markdown::{heading, visible};

/// Body heading -> the visible text under it, trimmed. A repeated heading collects the text under every occurrence.
/// Comments and code blocks are left out, as a reader of the rendered page does not see them as text: a heading in
/// one starts no section, and a section that holds only one is empty.
pub type Sections = BTreeMap<String, String>;

// The closing `---` is on its own line: followed by a newline, or by the end of a file with no body
static FRONT_MATTER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?s)\A---\n(.*?)\n---(?:\n(.*))?\z").unwrap());

/// The frontmatter, and the text under each `# heading` of the body.
///
/// The YAML is read as YAML 1.2: a date or a datetime is a string, quoted or not, and the schema parses it. YAML 1.1
/// (PyYAML) types an unquoted one and leaves a quoted one a string, so the same value could pass or fail by its quotes.
pub fn split(text: &str) -> Result<(Hash, Sections), String> {
    let text = text.replace("\r\n", "\n");
    let caps = FRONT_MATTER
        .captures(&text)
        .ok_or("no frontmatter (a YAML block between `---` lines)")?;
    let docs = YamlLoader::load_from_str(&caps[1])
        .map_err(|e| format!("frontmatter is not valid YAML: {e}"))?;
    let Some(Yaml::Hash(meta)) = docs.into_iter().next() else {
        return Err("frontmatter is not a mapping".into());
    };
    let mut lines: BTreeMap<String, Vec<&str>> = BTreeMap::new();
    let mut current: Option<String> = None;
    let body = visible(caps.get(2).map_or("", |body| body.as_str()));
    for line in body.split('\n') {
        if let Some((1, heading)) = heading(line) {
            let name = heading.to_string();
            lines.entry(name.clone()).or_default();
            current = Some(name);
        } else if let Some(name) = &current {
            lines.get_mut(name).unwrap().push(line);
        }
    }
    let sections = lines
        .into_iter()
        .map(|(name, lines)| (name, lines.join("\n").trim().to_string()))
        .collect();
    Ok((meta, sections))
}

/// The first `# heading` of the body, as a reader sees the rendered page: one inside a comment or a code block does not
/// count. `None` when the text has no frontmatter, or its body no heading.
pub fn first_heading(text: &str) -> Option<String> {
    let text = text.replace("\r\n", "\n");
    let caps = FRONT_MATTER.captures(&text)?;
    let body = visible(caps.get(2).map_or("", |body| body.as_str()));
    body.split('\n').find_map(|line| match heading(line) {
        Some((1, heading)) => Some(heading.to_string()),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sections_are_split_by_heading() {
        let text = "---\ntype: X\n---\n\nignored\n# A\n\n one \n\n## sub\n# B\n# A\nagain\n";
        let (meta, sections) = split(text).unwrap();
        assert_eq!(meta[&Yaml::String("type".into())], Yaml::String("X".into()));
        assert_eq!(sections["A"], "one \n\n## sub\nagain");
        assert_eq!(sections["B"], "");
        assert_eq!(sections.len(), 2);
    }

    #[test]
    fn sections_hold_only_what_is_shown() {
        let text =
            "---\ntype: X\n---\n# A\n\nshown <!-- not shown -->\n\n```\n# B\n```\n<!--\n# C\n-->\n";
        let (_, sections) = split(text).unwrap();
        assert_eq!(sections.keys().collect::<Vec<_>>(), ["A"]);
        assert_eq!(sections["A"], "shown");
    }

    #[test]
    fn an_indented_heading_starts_a_section() {
        let (_, sections) =
            split("---\ntype: X\n---\n  # A\nx\n   # B ##\ny\n    # code\n").unwrap();
        assert_eq!(sections.keys().collect::<Vec<_>>(), ["A", "B"]);
        assert_eq!(sections["B"], "y\n    # code");
    }

    #[test]
    fn a_file_that_ends_at_the_closing_line_has_frontmatter() {
        let (meta, sections) = split("---\ntype: X\n---").unwrap();
        assert_eq!(meta[&Yaml::String("type".into())], Yaml::String("X".into()));
        assert!(sections.is_empty());
    }

    #[test]
    fn the_first_heading_is_the_first_one_shown() {
        let text = "---\ntype: X\n---\n<!--\n# Hidden\n-->\n```\n# Code\n```\n## Sub\n# Resolution\nx\n# Goals\n";
        assert_eq!(first_heading(text).as_deref(), Some("Resolution"));
        assert_eq!(first_heading("---\ntype: X\n---\ntext only\n"), None);
        assert_eq!(first_heading("# no frontmatter\n"), None);
    }

    #[test]
    fn crlf_is_read_as_lf() {
        assert!(split("---\r\ntype: X\r\n---\r\n# A\r\nb\r\n").is_ok());
    }

    #[test]
    fn a_bad_frontmatter_is_caught() {
        let bad = [
            "# no frontmatter\n",
            "---\n---\n",
            // The closing line is `---` alone, not the start of a longer line
            "---\ntype: X\n----\n",
            "---\ntype: X\n---x",
            "---\n- a list\n---\n",
            "---\njust text\n---\n",
            // In YAML, a colon followed by a space starts a mapping, so an unquoted one breaks a text field
            "---\ndescription: how to measure: count\n---\n",
        ];
        for text in bad {
            assert!(split(text).is_err(), "{text:?} passed");
        }
    }
}
