//! A new knowledge document, as `rotproof new` starts it: the frontmatter, written by a machine because it is easy to
//! write wrongly, with the `description` left empty so that `rotproof check` fails until a person writes it, and, for
//! an alarm, its two headings, empty for the same reason. There is no other body: a heading left empty would be a
//! record gone stale from its first day.

use std::sync::LazyLock;

use regex::Regex;

use crate::bundle::RESERVED;
use crate::schema::{DO_SECTION, WHEN_SECTION};

// Lower-case words of letters and digits joined by single hyphens, as the slugs of a repository read in a list
static SLUG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[a-z0-9]+(?:-[a-z0-9]+)*$").unwrap());

/// The slug of the knowledge rules, which Rotproof writes into `docs/knowledge/` besides the names OKF reserves
/// ([`RESERVED`]).
const RULES: &str = "rules";

/// Words YAML reads as something other than a string when they stand alone
const YAML_WORDS: [&str; 10] = [
    "true", "false", "null", "yes", "no", "on", "off", "y", "n", "~",
];

/// Why `slug` cannot be the name of a new document, or `Ok`. The slug is the file's name without `.md` and never
/// changes once the file exists, because commit messages and other documents refer to it.
pub fn slug_problem(slug: &str) -> Result<(), String> {
    if slug.ends_with(".md") {
        return Err(format!(
            "{slug:?} is the name of the file: write the slug without .md ({:?})",
            slug.trim_end_matches(".md")
        ));
    }
    if !SLUG.is_match(slug) {
        return Err(format!(
            "{slug:?} is not a slug: write lower-case words of letters and digits joined by single hyphens \
             (rates-are-cached), the name docs/knowledge/<slug>.md will have for good"
        ));
    }
    if RESERVED.contains(&format!("{slug}.md").as_str()) || slug == RULES {
        return Err(format!(
            "docs/knowledge/{slug}.md is a name Rotproof or OKF keeps for a file of its own: choose another slug"
        ));
    }
    Ok(())
}

/// The area a new document is tagged with: `given` when it is one of `areas`, else the only area there is. `Err`
/// names the areas to choose from, or says there is none to choose.
pub fn pick_area(areas: &[String], given: Option<&str>) -> Result<String, String> {
    match (given, areas) {
        (_, []) => Err(
            "the declaration lists no area: add one to areas in .config/rotproof.toml, as every document has exactly \
             one"
                .into(),
        ),
        (Some(area), areas) => areas
            .iter()
            .find(|declared| declared.as_str() == area)
            .cloned()
            .ok_or_else(|| {
                format!(
                    "{area:?} is not an area the declaration lists (areas: {})",
                    areas.join(", ")
                )
            }),
        (None, [only]) => Ok(only.clone()),
        (None, areas) => Err(format!(
            "the declaration lists more than one area, so name the document's with --area (areas: {})",
            areas.join(", ")
        )),
    }
}

/// The title a new document starts with, from its slug: the words, the first one capitalised. It is a stand-in for the
/// person to change when the slug says less than a title would.
pub fn title_of(slug: &str) -> String {
    let words = slug.replace('-', " ");
    let mut chars = words.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// `text` as a YAML value on one line that reads back as the same string: as it is when it is plain words (a letter,
/// then letters, digits, spaces, `_` and `-`) that YAML reads as a string, else in double quotes.
fn scalar(text: &str) -> String {
    let plain = text.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, ' ' | '_' | '-'))
        && !text.ends_with(' ')
        && !YAML_WORDS.contains(&text.to_ascii_lowercase().as_str());
    if plain {
        text.to_string()
    } else {
        quoted(text)
    }
}

/// `text` in double quotes, with `\` and `"` escaped, which YAML reads back as it is.
fn quoted(text: &str) -> String {
    format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
}

/// The strings of `match`, trimmed, each on one line and named once, or why they cannot be written: `match` is read by
/// `schema.rs` with the same rules.
fn match_strings(strings: &[String]) -> Result<Vec<String>, String> {
    let mut found: Vec<String> = Vec::new();
    for string in strings {
        let string = string.trim();
        if string.is_empty() {
            return Err(
                "a --match string is empty: write the part of the output that stays".into(),
            );
        }
        if string.chars().any(char::is_control) {
            return Err(format!(
                "--match {string:?} holds a line break or another control character: a string is one line of what is \
                 seen"
            ));
        }
        if found.iter().any(|seen| seen == string) {
            return Err(format!("--match {string:?} is named twice"));
        }
        found.push(string.to_string());
    }
    Ok(found)
}

/// The text of a new document `slug` tagged with `area`, as `rotproof check` reads it (every line ending `\n`).
/// `alarm` is `Some` with the strings of `match` (none, when it has none) for an alarm, which also gets `# When` and
/// `# Do`; `None` for a document with no body. The `description` is empty, and so are an alarm's two headings, so the
/// check fails until each is written.
pub fn document(slug: &str, area: &str, alarm: Option<&[String]>) -> Result<String, String> {
    slug_problem(slug)?;
    let mut text = format!(
        "---\ntype: Knowledge\ntitle: {}\ndescription: \"\"\ntags: [{}]\nstatus: stable\n",
        scalar(&title_of(slug)),
        scalar(area)
    );
    if let Some(strings) = alarm {
        let strings = match_strings(strings)?;
        if !strings.is_empty() {
            let list: Vec<String> = strings.iter().map(|s| quoted(s)).collect();
            text.push_str(&format!("match: [{}]\n", list.join(", ")));
        }
    }
    text.push_str("---\n");
    if alarm.is_some() {
        text.push_str(&format!("\n# {WHEN_SECTION}\n\n# {DO_SECTION}\n"));
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{KnowledgeDoc, knowledge_doc};

    fn strings(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    /// The document with its description written and, for an alarm, each of its sections.
    fn filled(text: &str) -> String {
        let text = text.replace("description: \"\"", "description: What it tells.");
        text.replace("# When\n", "# When\n\nA 403.\n")
            .replace("# Do\n", "# Do\n\nOpen an issue.\n")
    }

    #[test]
    fn a_document_has_no_body_and_fails_until_its_description_is_written() {
        let text = document("rates-are-cached", "billing", None).unwrap();
        assert_eq!(
            text,
            "---\ntype: Knowledge\ntitle: Rates are cached\ndescription: \"\"\ntags: [billing]\nstatus: stable\n---\n"
        );
        let why = knowledge_doc(&text).unwrap_err();
        assert!(why.contains("description: empty"), "{why}");
        match knowledge_doc(&filled(&text)) {
            Ok(KnowledgeDoc::Knowledge(doc, sections)) => {
                assert_eq!(doc.title, "Rates are cached");
                assert_eq!(doc.tag, "billing");
                assert!(doc.follows.is_empty() && doc.matches.is_empty());
                assert!(sections.is_empty(), "{sections:?}");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn an_alarm_has_its_two_headings_empty_and_fails_until_each_is_written() {
        let text = document(
            "pypi-403",
            "ops",
            Some(&strings(&["403", "trusted publisher"])),
        )
        .unwrap();
        assert!(
            text.contains("match: [\"403\", \"trusted publisher\"]\n---\n\n# When\n\n# Do\n"),
            "{text}"
        );
        // The description first, as the scaffold fails on both until they are written
        let described = text.replace("description: \"\"", "description: D.");
        let why = knowledge_doc(&described).unwrap_err();
        assert!(
            why.contains("missing or empty") && why.contains("When") && why.contains("Do"),
            "{why}"
        );
        match knowledge_doc(&filled(&text)) {
            Ok(KnowledgeDoc::Knowledge(doc, sections)) => {
                assert_eq!(doc.matches, ["403", "trusted publisher"]);
                assert_eq!(sections["When"], "A 403.");
            }
            other => panic!("{other:?}"),
        }
        // No strings: no `match`, which an empty list would fail
        let bare = document("pypi-403", "ops", Some(&[])).unwrap();
        assert!(!bare.contains("match:"), "{bare}");
        assert!(knowledge_doc(&filled(&bare)).is_ok());
    }

    #[test]
    fn the_strings_of_match_read_back_as_written() {
        let odd = [
            "a: b",
            "x #y",
            "say \"hi\"",
            "back\\slash",
            "[x]",
            "-1",
            "日本語",
            "'q'",
            "  padded  ",
        ];
        let text = document("odd", "ops", Some(&strings(&odd))).unwrap();
        match knowledge_doc(&filled(&text)) {
            Ok(KnowledgeDoc::Knowledge(doc, _)) => {
                let want: Vec<&str> = odd.iter().map(|s| s.trim()).collect();
                assert_eq!(doc.matches, want);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn match_strings_that_the_check_would_not_read_are_refused() {
        for (bad, says) in [
            (vec![""], "empty"),
            (vec!["   "], "empty"),
            (vec!["a\nb"], "control"),
            (vec!["a\tb"], "control"),
            (vec!["403", "403"], "twice"),
            (vec!["403", " 403 "], "twice"),
        ] {
            let why = document("a", "ops", Some(&strings(&bad))).unwrap_err();
            assert!(why.contains(says), "{bad:?}: {why}");
        }
    }

    #[test]
    fn a_title_and_an_area_that_yaml_would_not_read_as_text_are_quoted() {
        for (slug, area, title, tag) in [
            ("on", "ops", "\"On\"", "ops"),
            ("y", "true", "\"Y\"", "\"true\""),
            ("2026-plan", "2024", "\"2026 plan\"", "\"2024\""),
            ("null", "area one", "\"Null\"", "area one"),
            ("a", "billing/eu", "A", "\"billing/eu\""),
            ("a", "a: b", "A", "\"a: b\""),
            ("a", "x #y", "A", "\"x #y\""),
        ] {
            let text = document(slug, area, None).unwrap();
            assert!(text.contains(&format!("title: {title}\n")), "{text}");
            assert!(text.contains(&format!("tags: [{tag}]\n")), "{text}");
            match knowledge_doc(&filled(&text)) {
                Ok(KnowledgeDoc::Knowledge(doc, _)) => assert_eq!(doc.tag, area, "{text}"),
                other => panic!("{text}: {other:?}"),
            }
        }
    }

    #[test]
    fn a_slug_is_lower_case_words_joined_by_hyphens_and_never_a_reserved_name() {
        for good in ["a", "rates", "rates-are-cached", "adr-0007", "2026-plan"] {
            assert!(slug_problem(good).is_ok(), "{good}");
        }
        for (bad, says) in [
            ("", "not a slug"),
            ("Rates", "not a slug"),
            ("rates_are", "not a slug"),
            ("-rates", "not a slug"),
            ("rates-", "not a slug"),
            ("rates--are", "not a slug"),
            ("rates are", "not a slug"),
            ("a/b", "not a slug"),
            ("knowledge/a", "not a slug"),
            ("..", "not a slug"),
            ("rates.md", "without .md"),
            ("index", "keeps for a file of its own"),
            ("log", "keeps for a file of its own"),
            ("rules", "keeps for a file of its own"),
        ] {
            let why = slug_problem(bad).unwrap_err();
            assert!(why.contains(says), "{bad:?}: {why}");
            assert!(document(bad, "ops", None).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn the_area_is_the_only_one_or_the_one_named_and_declared() {
        let areas = strings(&["billing", "ops"]);
        assert_eq!(pick_area(&strings(&["ops"]), None).unwrap(), "ops");
        assert_eq!(pick_area(&areas, Some("ops")).unwrap(), "ops");
        assert_eq!(pick_area(&strings(&["ops"]), Some("ops")).unwrap(), "ops");
        let several = pick_area(&areas, None).unwrap_err();
        assert!(
            several.contains("--area") && several.contains("billing, ops"),
            "{several}"
        );
        let other = pick_area(&areas, Some("sales")).unwrap_err();
        assert!(
            other.contains("\"sales\"") && other.contains("billing, ops"),
            "{other}"
        );
        // Names are compared exactly, as the check compares a tag with an area
        assert!(pick_area(&areas, Some("Ops")).is_err());
        assert!(pick_area(&[], None).unwrap_err().contains("no area"));
        assert!(pick_area(&[], Some("ops")).unwrap_err().contains("no area"));
    }
}
