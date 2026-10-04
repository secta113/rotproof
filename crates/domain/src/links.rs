//! Whether a link in a record resolves: in the repository, by the exact names of its files, to a heading that is there,
//! and for a `.py` file to a definition the link names.
//!
//! The rules here judge a link by its target, and then by what the file it names holds; `application` looks the file
//! up and reads it.

use std::collections::BTreeSet;
use std::sync::LazyLock;

use percent_encoding::percent_decode_str;
use regex::Regex;

use utils::markdown::anchors;

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
        /// Whether it is a Python file, whose link names a definition rather than a heading
        is_python: bool,
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
        is_python: path.to_ascii_lowercase().ends_with(".py"),
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
        let file = |start: &str, rel: &str, fragment: &str, is_python| {
            Ok(Target::File {
                start: start.into(),
                rel: rel.into(),
                fragment: fragment.into(),
                is_python,
            })
        };
        let at = |t: &str| target(t, "docs/backlog", "docs");
        assert_eq!(at("/log.md#a%20b"), file("docs", "log.md", "a b", false));
        assert_eq!(
            at("../../src/x.PY"),
            file("docs/backlog", "../../src/x.PY", "", true)
        );
        assert_eq!(at("https://example.com"), Ok(Target::Url));
        assert_eq!(
            at("nothing.md:12"),
            file("docs/backlog", "nothing.md:12", "", false)
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
