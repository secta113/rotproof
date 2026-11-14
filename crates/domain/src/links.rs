//! Whether a link in a record resolves: in the repository, by the exact names of its files, to a heading that is there,
//! for a `.py` file to a definition its text names, and for a TypeScript, JavaScript or Rust file, when it has a
//! fragment, to the definition its fragment names.
//!
//! The rules here judge a link by its target, and then by what the file it names holds; `application` looks the file
//! up and reads it.

use std::collections::BTreeSet;
use std::sync::LazyLock;

use percent_encoding::percent_decode_str;
use regex::Regex;

use crate::definitions::{Definition, selected};
use crate::follows::{is_name, language_of};
use crate::layers::Language;
use crate::markdown::anchors;

// A URL scheme. RFC 3986 allows `.` in one, but no scheme in use has it, while a file name with a line number
// (`check.rs:104`) always does: read as a URL, that path would never be checked
static URL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[A-Za-z][A-Za-z0-9+-]*:").unwrap());
// A drive letter has the form of a one-letter URL scheme. No scheme has one letter
static DRIVE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[A-Za-z]:").unwrap());
static FILE_URL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)^file:").unwrap());

/// Where a link's target points, as far as the target itself says.
#[derive(Debug, PartialEq, Eq)]
pub enum Target {
    /// A URL, which is not checked
    Url,
    /// A file of the repository, to be looked up by its exact names: `rel` from the directory `start` (both from the
    /// root), and the fragment after `#` ("" for none)
    File {
        start: String,
        rel: String,
        fragment: String,
        names: Names,
    },
}

/// What the link to a file names in it, by the language of the file.
#[derive(Debug, PartialEq, Eq)]
pub enum Names {
    /// A heading, by the fragment: any file that is not code Rotproof reads definitions in
    Heading,
    /// A function or class of a Python file, by the text of the link
    DefinitionByText,
    /// A definition of a file of this language (TypeScript, JavaScript or Rust), by the fragment, in the form a key of
    /// `follows` has after `::`; a link with no fragment names the file, and is not read further
    DefinitionByFragment(Language),
}

/// Where `target` points, or why it cannot resolve anywhere.
///
/// `here` is the directory of the document that holds the link, for relative targets. A target starting with `/` is
/// relative to `bundle_root`. A URL is not checked. A path with a drive letter (`C:/...`) or a `file:` URL fails: it
/// names a file on one machine, which no other checkout and no reader on GitHub can follow. A path with `\` fails: only
/// Windows reads it as a separator, so the same link would resolve on one machine and not on another. For the same
/// reason the file is found by its exact name, and a path that climbs above the repository fails.
pub fn target(target: &str, here: &str, bundle_root: &str) -> Result<Target, String> {
    // A file: URL names a file on one machine as surely as a drive letter does
    if DRIVE.is_match(target) || FILE_URL.is_match(target) {
        return Err(format!(
            "a path on one machine; link with / from the bundle root, or with a relative path: {target}"
        ));
    }
    if URL.is_match(target) {
        return Ok(Target::Url);
    }
    let decoded = percent_decode_str(target).decode_utf8_lossy();
    let (path, fragment) = decoded.split_once('#').unwrap_or((&decoded, ""));
    if path.contains('\\') {
        return Err(format!("a path with \\; separate with /: {target}"));
    }
    if path.is_empty() {
        return Err(format!("no file in the link: {target}"));
    }
    let (start, rel) = match path.strip_prefix('/') {
        Some(rest) => (bundle_root, rest),
        None => (here, path),
    };
    // GitHub serves the files of the repository only: a path that climbs above its root names nothing there
    let mut depth = start.split('/').filter(|part| !part.is_empty()).count() as i64;
    for part in rel.split('/') {
        match part {
            "" | "." => {}
            ".." => depth -= 1,
            _ => depth += 1,
        }
        if depth < 0 {
            return Err(format!("a path outside the repository: {target}"));
        }
    }
    Ok(Target::File {
        start: start.to_string(),
        rel: rel.to_string(),
        fragment: fragment.to_string(),
        // By the extension in any case, as `follows` does: `bundle.PY` is a Python file on Windows, and its link names
        // a definition as any other does
        names: match language_of(path) {
            Some(Language::Python) => Names::DefinitionByText,
            Some(language) => Names::DefinitionByFragment(language),
            None => Names::Heading,
        },
    })
}

/// What to say of a `target` that names no file, when the tree holds `spelled`: the path up to a name it holds only
/// in another case, or with dots or spaces at its end.
pub fn missing(target: &str, spelled: Option<&str>) -> String {
    match spelled {
        Some(on_disk) => format!(
            "no such file: {target} (the disk has {on_disk}: names are compared exactly, as Linux and GitHub compare \
             them)"
        ),
        None => format!("no such file: {target}"),
    }
}

/// Why a link to a heading of the file whose text is `source` does not resolve: no heading has its `fragment`.
pub fn to_heading(target: &str, fragment: &str, source: &str) -> Option<String> {
    (!fragment.is_empty() && !anchors(source).contains(fragment))
        .then(|| format!("no heading with this anchor: {target}"))
}

/// Why a link to a Python file whose definitions are `defined` does not resolve: its `text` names no function or
/// class the file defines. The name has to be defined, not only mentioned: a call, a comment or a string can keep a
/// name after the definition was renamed.
pub fn to_definition(text: &str, target: &str, defined: &BTreeSet<String>) -> Option<String> {
    let name = text.trim().trim_matches('`').trim();
    if name.is_empty() {
        return Some(format!(
            "no function or class named in the link text: {target}"
        ));
    }
    (!defined.contains(name)).then(|| {
        format!(
            "no def or class named {name} in {target}; {}",
            how_to_name(name, defined)
        )
    })
}

/// Why a link whose `fragment` names a definition of a file of `language` does not resolve: `outline` is the outline of
/// the `source` of the file (see [`crate::definitions`]) and the fragment is no key of `follows` there. `fragment` is
/// the name a key has after `::` (`Router.add`), so a link resolves exactly when a document could follow its target as
/// a key: a Rust type is its definition and every `impl` block for it, `Type.method` a method in any of them, and an
/// unexported name counts. `None` for a link with no fragment, which names the file. A file whose definitions
/// cannot be read is judged by `application`, which has the parser's error.
pub fn to_outlined_definition(
    target: &str,
    fragment: &str,
    language: Language,
    source: &str,
    outline: &[Definition],
) -> Option<String> {
    if fragment.is_empty() {
        return None;
    }
    if !fragment.split('.').all(|part| is_name(language, part)) {
        return Some(format!(
            "the fragment names no definition: {target}; it is a name, or names joined by dots (Class.method), as \
             after :: in a key of follows"
        ));
    }
    if selected(source, outline, fragment).is_some() {
        return None;
    }
    let defined: BTreeSet<String> = outline.iter().map(|d| d.names.join(".")).collect();
    Some(format!(
        "no definition named {fragment} in {target}; {}",
        how_to_name_outlined(fragment, &defined)
    ))
}

/// How to fix a fragment that names nothing the file defines: the names that end as it does, when there are any (a
/// method written without its type), and otherwise what the file defines, listed as far as `SHOWN`.
fn how_to_name_outlined(name: &str, defined: &BTreeSet<String>) -> String {
    let last = name.rsplit('.').next().unwrap_or(name);
    let near: Vec<&str> = defined
        .iter()
        .map(String::as_str)
        .filter(|d| d.rsplit('.').next() == Some(last) && *d != name)
        .take(5)
        .collect();
    if !near.is_empty() {
        return format!(
            "the fragment is a name as the file defines it: {}",
            near.join(", ")
        );
    }
    const SHOWN: usize = 20;
    if defined.is_empty() {
        return "the file defines nothing a document can follow".into();
    }
    let mut names: Vec<&str> = defined.iter().take(SHOWN).map(String::as_str).collect();
    if defined.len() > SHOWN {
        names.push("...");
    }
    format!(
        "the fragment is a name as the file defines it ({} defined: {})",
        defined.len(),
        names.join(", ")
    )
}

/// How to fix a link text that names nothing the file defines, so whoever wrote it can fix it from the message alone.
/// The text is one name exactly as defined: a call (`f()`) or a dotted path (`Class.method`) names its last part.
/// Otherwise the message lists what the file defines.
fn how_to_name(name: &str, defined: &BTreeSet<String>) -> String {
    let bare = name.split('(').next().unwrap_or(name);
    let bare = bare.rsplit('.').next().unwrap_or(bare).trim();
    if bare != name && defined.contains(bare) {
        return format!("write the bare name as the link text: `{bare}`");
    }
    const SHOWN: usize = 20;
    if defined.is_empty() {
        return "the file defines no function or class".into();
    }
    let mut names: Vec<&str> = defined.iter().take(SHOWN).map(String::as_str).collect();
    if defined.len() > SHOWN {
        names.push("...");
    }
    format!(
        "the link text is one name as the file defines it ({} defined: {})",
        defined.len(),
        names.join(", ")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_target_is_read_before_any_file_is() {
        let file = |start: &str, rel: &str, fragment: &str, names| {
            Ok(Target::File {
                start: start.into(),
                rel: rel.into(),
                fragment: fragment.into(),
                names,
            })
        };
        let at = |t: &str| target(t, "docs/backlog", "docs");
        assert_eq!(
            at("/log.md#a%20b"),
            file("docs", "log.md", "a b", Names::Heading)
        );
        assert_eq!(
            at("../../src/x.PY"),
            file(
                "docs/backlog",
                "../../src/x.PY",
                "",
                Names::DefinitionByText
            )
        );
        // The language is the one `follows` reads the file as, by its extension in any case
        for (path, language) in [
            ("x.rs", Language::Rust),
            ("X.RS", Language::Rust),
            ("x.ts", Language::TypeScript),
            ("x.tsx", Language::TypeScript),
            ("x.mjs", Language::TypeScript),
            ("x.JS", Language::TypeScript),
        ] {
            assert_eq!(
                at(&format!("../{path}#T.f")),
                file(
                    "docs/backlog",
                    &format!("../{path}"),
                    "T.f",
                    Names::DefinitionByFragment(language)
                ),
                "{path}"
            );
        }
        assert_eq!(at("https://example.com"), Ok(Target::Url));
        assert_eq!(
            at("nothing.md:12"),
            file("docs/backlog", "nothing.md:12", "", Names::Heading)
        );
        // A file name with a line number is no source file: the line is part of its name
        assert_eq!(
            at("x.rs:12"),
            file("docs/backlog", "x.rs:12", "", Names::Heading)
        );
        for (t, why) in [
            ("C:/x.md", "a path on one machine"),
            ("file:///etc/hosts", "a path on one machine"),
            ("..\\log.md", "separate with /"),
            ("#only", "no file in the link"),
            ("../../../x.md", "outside the repository"),
        ] {
            assert!(at(t).unwrap_err().contains(why), "{t}");
        }
    }

    fn definition(names: &[&str], start: usize, end: usize) -> Definition {
        Definition {
            names: names.iter().map(|n| n.to_string()).collect(),
            start,
            end,
        }
    }

    #[test]
    fn a_fragment_resolves_when_a_document_could_follow_it() {
        let source = "struct T;\nimpl T { fn f() {} }\nfn g() {}\n";
        let outline = [
            definition(&["T"], 0, 9),
            definition(&["T"], 10, 29),
            definition(&["T", "f"], 20, 28),
            definition(&["g"], 30, 40),
        ];
        let said = |fragment: &str| {
            to_outlined_definition("x.rs", fragment, Language::Rust, source, &outline)
        };
        for fragment in ["", "T", "T.f", "g"] {
            assert_eq!(said(fragment), None, "{fragment}");
        }
        // Not a prefix, not the end of a longer name, not a name of another case
        for fragment in ["f", "T.g", "T.f.x", "t", "T.", ".f"] {
            assert!(said(fragment).is_some(), "{fragment} passed");
        }
        // A method written without its type says where it is
        assert_eq!(
            said("f").unwrap(),
            "no definition named f in x.rs; the fragment is a name as the file defines it: T.f"
        );
        // A line anchor is not a name a file defines: it lists what is
        assert_eq!(
            said("L12").unwrap(),
            "no definition named L12 in x.rs; the fragment is a name as the file defines it (3 defined: T, T.f, g)"
        );
        // What is no name at all is told apart from a name the file lacks
        assert!(said("L12-L20").unwrap().contains("names no definition"));
        assert!(said("a b").unwrap().contains("names no definition"));
        // Neither a range outside the text nor a file with nothing in it
        assert!(
            to_outlined_definition("x.rs", "g", Language::Rust, "fn g() {}", &outline).is_some()
        );
        assert!(
            to_outlined_definition("x.rs", "g", Language::Rust, "", &[])
                .unwrap()
                .ends_with("the file defines nothing a document can follow")
        );
    }

    #[test]
    fn a_dollar_is_a_letter_in_typescript_and_not_in_rust() {
        let source = "const $x = 1;\n";
        let outline = [definition(&["$x"], 0, 13)];
        let at = |language| to_outlined_definition("x", "$x", language, source, &outline);
        assert_eq!(at(Language::TypeScript), None);
        assert!(at(Language::Rust).unwrap().contains("names no definition"));
    }

    #[test]
    fn a_wrong_name_says_how_to_write_it() {
        let defined: BTreeSet<String> = ["Bundle", "method"].map(String::from).into();
        assert_eq!(to_definition("`Bundle`", "x.py", &defined), None);
        assert!(
            to_definition("`Bundle.method`", "x.py", &defined)
                .unwrap()
                .ends_with("write the bare name as the link text: `method`")
        );
        assert!(
            to_definition("``", "x.py", &defined)
                .unwrap()
                .starts_with("no function or class named")
        );
        assert_eq!(
            how_to_name("x", &BTreeSet::new()),
            "the file defines no function or class"
        );
        let many: BTreeSet<String> = (0..25).map(|n| format!("f{n:02}")).collect();
        assert!(how_to_name("x", &many).contains("(25 defined: f00, f01,"));
        assert!(how_to_name("x", &many).ends_with("f19, ...)"));
    }
}
