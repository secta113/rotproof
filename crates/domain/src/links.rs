//! Whether a link in a record resolves: in the repository, by the exact names of its files, to a heading that is there,
//! and for a file of code to the name its text shows.
//!
//! A link to a file of code (one of the languages whose definitions Rotproof reads, [`language_of`]) is judged by what
//! the reader sees: its text is code (`` `Router.add` ``) and is either a definition of the file, named as a key of
//! `follows` names it, or the file's name, or a path that ends in it (`` `lib.rs` ``, `` `src/lib.rs` ``). Prose is
//! not a name, so a link whose text is prose fails; it is the name the reader reads that is checked, and not a second
//! place that names it.
//!
//! The rules here judge a link by its target, and then by what the file it names holds; `application` looks the file
//! up and reads it.

use std::collections::BTreeSet;
use std::sync::LazyLock;

use percent_encoding::percent_decode_str;
use regex::Regex;

use crate::definitions::{Definition, selected};
use crate::follows::language_of;
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
        /// The language of the file when it is code, by its extension in any case, whose link names a definition or the
        /// file rather than a heading
        language: Option<Language>,
    },
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
        // In any case: `bundle.PY` is a Python file on Windows, and its link names a definition as any other does
        language: language_of(path),
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

/// The name a link to a file of code gives, or why it names nothing. The text is one code span (`` `Router.add` ``):
/// prose is not a name, and a name not shown as code cannot be told from the words around it. A fragment fails: a
/// heading is not in a file of code, and a line number goes stale without a word.
pub fn named(text: &str, target: &str, fragment: &str) -> Result<String, String> {
    if !fragment.is_empty() {
        return Err(format!(
            "a link to a code file has no fragment (a line number goes stale without a word); name a definition or \
             the file in the link text instead: {target}"
        ));
    }
    let shown = text.trim();
    let Some(code) = shown
        .strip_prefix('`')
        .and_then(|rest| rest.strip_suffix('`'))
        .filter(|code| !code.contains('`'))
    else {
        return Err(format!(
            "the text of a link to a code file is code naming a definition, or the file or a path ending in it \
             (`Router.add`, `lib.rs`, `src/lib.rs`), not {shown:?}: {target}"
        ));
    };
    let name = code.trim();
    if name.is_empty() {
        return Err(format!(
            "no definition or file named in the link text: {target}"
        ));
    }
    Ok(name.to_string())
}

/// The name of the file a path from a directory ends in: `lib.rs` for `../src/lib.rs`.
pub fn file_name(rel: &str) -> &str {
    rel.rsplit('/').next().unwrap_or(rel)
}

/// Whether `name` is the file `rel` names, or a path that ends in it: its parts are the last parts of `rel`, matched
/// at the boundaries of the directories and spelled exactly (`lib.rs`, `src/lib.rs` and `rotproof/src/lib.rs` for
/// `../rotproof/src/lib.rs`; not `rc/lib.rs`, `Lib.rs` or `/src/lib.rs`). A part that is empty, `.` or `..` is no part
/// of a name: a path that climbs is the link's target, not what a reader is shown.
pub fn names_file(name: &str, rel: &str) -> bool {
    let parts: Vec<&str> = name.split('/').collect();
    if parts.iter().any(|p| matches!(*p, "" | "." | "..")) {
        return false;
    }
    let held: Vec<&str> = rel.split('/').collect();
    held.len() >= parts.len() && held[held.len() - parts.len()..] == parts[..]
}

/// Why a link whose text names `name` does not resolve in the file `file` whose text is `source` and whose outline is
/// `outline`: it is no definition there, as a key of `follows` names one (`Router.add`; a Rust type covers all its
/// `impl` blocks). The name has to be defined, not only mentioned: a call, a comment or a string can keep a name after
/// the definition was renamed. The file's own name is judged before, by the caller, which reads no outline for it.
pub fn to_definition(
    name: &str,
    target: &str,
    file: &str,
    source: &str,
    outline: &[Definition],
) -> Option<String> {
    selected(source, outline, name).is_none().then(|| {
        format!(
            "no definition named {name} in {target}; {}",
            how_to_name(name, &defined(outline), file)
        )
    })
}

/// Every name the outline gives, as a key of `follows` writes it after `::`.
fn defined(outline: &[Definition]) -> BTreeSet<String> {
    outline.iter().map(|d| d.names.join(".")).collect()
}

/// How to fix a link text that names nothing the file defines, so whoever wrote it can fix it from the message alone.
/// A call (`f()`) names its callee; a method without its type (`add`) names the methods of that name; otherwise the
/// message lists what the file defines.
fn how_to_name(name: &str, defined: &BTreeSet<String>, file: &str) -> String {
    let bare = name.split('(').next().unwrap_or(name).trim();
    if bare != name && defined.contains(bare) {
        return format!("write the name without the call: `{bare}`");
    }
    let last = bare.rsplit('.').next().unwrap_or(bare);
    let methods: Vec<String> = defined
        .iter()
        .filter(|d| d.rsplit('.').next() == Some(last) && d.as_str() != bare && d.contains('.'))
        .take(5)
        .map(|d| format!("`{d}`"))
        .collect();
    if !methods.is_empty() {
        return format!(
            "a method is named with its type, as a key of follows names it: {}",
            methods.join(", ")
        );
    }
    const SHOWN: usize = 20;
    if defined.is_empty() {
        return format!(
            "the file defines no name a key can follow; the text can be the file's name: `{file}`, or a path ending in it"
        );
    }
    let mut names: Vec<&str> = defined.iter().take(SHOWN).map(String::as_str).collect();
    if defined.len() > SHOWN {
        names.push("...");
    }
    format!(
        "the text is a name as a key of follows writes it, or the file's name `{file}` or a path ending in it ({} defined: {})",
        defined.len(),
        names.join(", ")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_target_is_read_before_any_file_is() {
        let file = |start: &str, rel: &str, fragment: &str, language| {
            Ok(Target::File {
                start: start.into(),
                rel: rel.into(),
                fragment: fragment.into(),
                language,
            })
        };
        let at = |t: &str| target(t, "docs/backlog", "docs");
        assert_eq!(at("/log.md#a%20b"), file("docs", "log.md", "a b", None));
        assert_eq!(
            at("../../src/x.PY"),
            file("docs/backlog", "../../src/x.PY", "", Some(Language::Python))
        );
        assert_eq!(
            at("../x.ts#L3"),
            file("docs/backlog", "../x.ts", "L3", Some(Language::TypeScript))
        );
        assert_eq!(
            at("../x.rs"),
            file("docs/backlog", "../x.rs", "", Some(Language::Rust))
        );
        assert_eq!(at("https://example.com"), Ok(Target::Url));
        assert_eq!(
            at("nothing.md:12"),
            file("docs/backlog", "nothing.md:12", "", None)
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

    #[test]
    fn the_text_of_a_link_to_code_is_one_code_span() {
        for (text, name) in [
            ("`Router.add`", "Router.add"),
            ("  `lib.rs` ", "lib.rs"),
            ("` render `", "render"),
        ] {
            assert_eq!(named(text, "x.py", ""), Ok(name.to_string()), "{text}");
        }
        for text in [
            "",
            "``",
            "the router",
            "Router.add",
            "`a` and `b`",
            "see `a`",
            "`a` here",
        ] {
            assert!(named(text, "x.py", "").is_err(), "{text:?}");
        }
        assert!(
            named("the router", "x.py", "")
                .unwrap_err()
                .contains("is code naming a definition, or the file or a path ending in it")
        );
        assert!(
            named("``", "x.py", "")
                .unwrap_err()
                .starts_with("no definition or file named")
        );
        // A fragment: whatever it is, with a name that would pass
        assert!(
            named("`f`", "x.py#L3", "L3")
                .unwrap_err()
                .contains("has no fragment")
        );
    }

    #[test]
    fn a_name_is_a_key_of_follows() {
        let source = "fn f() {}\nstruct T;\nimpl T { fn f() {} }\n";
        let at = |names: &[&str], start, end| Definition {
            names: names.iter().map(|n| n.to_string()).collect(),
            start,
            end,
            kind: crate::definitions::Kind::Other,
        };
        let outline = [
            at(&["f"], 0, 9),
            at(&["T"], 10, 19),
            at(&["T"], 20, 40),
            at(&["T", "f"], 29, 38),
        ];
        let to = |name: &str| to_definition(name, "../x.rs", "x.rs", source, &outline);
        // A type covers its `impl` blocks, and a method is named with its type
        for name in ["f", "T", "T.f"] {
            assert_eq!(to(name), None, "{name}");
        }
        let why = to("T.g").unwrap();
        assert!(
            why.starts_with("no definition named T.g in ../x.rs"),
            "{why}"
        );
        // A call: the name it ends with
        assert!(
            to("f()")
                .unwrap()
                .ends_with("write the name without the call: `f`")
        );
        // A method without its type: the keys it can be
        let typeless = to("g.f").unwrap();
        assert!(
            typeless.contains("with its type") && typeless.contains("`T.f`"),
            "{typeless}"
        );
        // Anything else: what the file defines, so the right name can be picked
        let listed = to("g").unwrap();
        assert!(
            listed.ends_with(
                "or the file's name `x.rs` or a path ending in it (3 defined: T, T.f, f)"
            ),
            "{listed}"
        );
        assert!(
            to_definition("g", "x.rs", "x.rs", "", &[])
                .unwrap()
                .contains("defines no name a key can follow")
        );
    }

    #[test]
    fn a_list_of_what_is_defined_is_cut() {
        let many: BTreeSet<String> = (0..25).map(|n| format!("f{n:02}")).collect();
        assert!(how_to_name("x", &many, "x.py").contains("(25 defined: f00, f01,"));
        assert!(how_to_name("x", &many, "x.py").ends_with("f19, ...)"));
    }

    #[test]
    fn a_file_is_named_by_its_last_part() {
        assert_eq!(file_name("../src/lib.rs"), "lib.rs");
        assert_eq!(file_name("lib.rs"), "lib.rs");
    }

    #[test]
    fn a_path_names_a_file_by_its_last_parts_whole() {
        let rel = "../rotproof/src/lib.rs";
        for name in ["lib.rs", "src/lib.rs", "rotproof/src/lib.rs"] {
            assert!(names_file(name, rel), "{name}");
        }
        assert!(names_file("lib.rs", "lib.rs"));
        for name in [
            // Not from a directory boundary, or not spelled exactly
            "rc/lib.rs",
            // The file's name with another directory before it: a record whose directory moved
            "lib/lib.rs",
            "b.rs",
            "Lib.rs",
            "SRC/lib.rs",
            "lib",
            "src/lib",
            // Nothing before the first part, nothing empty, nothing that stays or climbs
            "/src/lib.rs",
            "src//lib.rs",
            "src/lib.rs/",
            "./lib.rs",
            "../src/lib.rs",
            "../rotproof/src/lib.rs",
            "",
            // More parts than the path has, or another file
            "x/rotproof/src/lib.rs",
            "src/main.rs",
        ] {
            assert!(!names_file(name, rel), "{name:?}");
        }
        // A path that starts at the bundle root has no leading part to name
        assert!(!names_file("x/lib.rs", "lib.rs"));
    }
}
