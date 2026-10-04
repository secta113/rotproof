//! Whether a link in a record resolves: in the repository, by the exact names of its files, to a heading that is there,
//! and for a `.py` file to a definition the link names.

use std::collections::BTreeSet;
use std::sync::LazyLock;

use percent_encoding::percent_decode_str;
use regex::Regex;

use crate::code::Parsers;
use crate::tree::{Lookup, Tree, lookup, read_text};
use utils::markdown::anchors;

// A URL scheme. RFC 3986 allows `.` in one, but no scheme in use has it, while a file name with a line number
// (`check.rs:104`) always does: read as a URL, that path would never be checked
static URL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[A-Za-z][A-Za-z0-9+-]*:").unwrap());
// A drive letter has the form of a one-letter URL scheme. No scheme has one letter
static DRIVE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[A-Za-z]:").unwrap());
static FILE_URL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)^file:").unwrap());

/// Why the link does not resolve, or `None` when it does.
///
/// `here` is the directory of the document that holds the link, for relative targets. A target starting with `/` is
/// relative to `bundle_root`. A URL is not checked. A path with a drive letter (`C:/...`) or a `file:` URL fails: it
/// names a file on one machine, which no other checkout and no reader on GitHub can follow. A path with `\` fails: only
/// Windows reads it as a separator, so the same link would resolve on one machine and not on another. For the same
/// reason the file is found by its exact name (`tree::lookup`), and a path that climbs above the repository fails.
/// A link to a `.py` file names in its text a function or class that the file defines.
pub fn broken(
    tree: &dyn Tree,
    parsers: &dyn Parsers,
    text: &str,
    target: &str,
    here: &str,
    bundle_root: &str,
) -> Option<String> {
    // A file: URL names a file on one machine as surely as a drive letter does
    if DRIVE.is_match(target) || FILE_URL.is_match(target) {
        return Some(format!(
            "a path on one machine; link with / from the bundle root, or with a relative path: {target}"
        ));
    }
    if URL.is_match(target) {
        return None;
    }
    let decoded = percent_decode_str(target).decode_utf8_lossy();
    let (path, fragment) = decoded.split_once('#').unwrap_or((&decoded, ""));
    if path.contains('\\') {
        return Some(format!("a path with \\; separate with /: {target}"));
    }
    if path.is_empty() {
        return Some(format!("no file in the link: {target}"));
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
            return Some(format!("a path outside the repository: {target}"));
        }
    }
    let full = match lookup(tree, start, rel) {
        Lookup::Found {
            path,
            is_dir: false,
        } => path,
        Lookup::Spelled(on_disk) => {
            return Some(format!(
                "no such file: {target} (the disk has {on_disk}: names are compared exactly, as Linux and GitHub \
                 compare them)"
            ));
        }
        _ => return Some(format!("no such file: {target}")),
    };
    let source = match read_text(tree, &full) {
        Ok(source) => source,
        Err(e) => return Some(format!("cannot read {target}: {e}")),
    };
    // In any case: `bundle.PY` is a Python file on Windows, and its link names a definition as any other does
    if path.to_ascii_lowercase().ends_with(".py") {
        let name = text.trim().trim_matches('`').trim();
        if name.is_empty() {
            return Some(format!(
                "no function or class named in the link text: {target}"
            ));
        }
        // The name has to be defined, not only mentioned: a call, a comment or a string can keep a name after the
        // definition was renamed
        let defined = parsers.python_definitions(&source);
        if !defined.contains(name) {
            return Some(format!(
                "no def or class named {name} in {target}; {}",
                how_to_name(name, &defined)
            ));
        }
    } else if !fragment.is_empty() && !anchors(&source).contains(fragment) {
        return Some(format!("no heading with this anchor: {target}"));
    }
    None
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
    use std::fs;
    use std::path::Path;

    use super::*;

    /// A bundle with a document, a log whose format guide is an HTML comment, and a Python file outside the bundle.
    /// Laid out as in a project: the links are written from `docs/backlog/`.
    fn tree() -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        let docs = root.path().join("docs");
        fs::create_dir_all(docs.join("backlog")).unwrap();
        fs::create_dir_all(root.path().join("tests")).unwrap();
        fs::write(
            docs.join("backlog/rules.md"),
            "# Rules\n\n## What goes here\n",
        )
        .unwrap();
        fs::write(docs.join("log.md"), "# Log\n\n<!--\n### Task name\n-->\n").unwrap();
        fs::write(
            root.path().join("tests/upper.PY"),
            "def shouted():\n    pass\n",
        )
        .unwrap();
        fs::write(
            root.path().join("tests/backlog_bundle.py"),
            concat!(
                "\"\"\"\n",
                "def in_docstring():\n",
                "\"\"\"\n",
                "SOURCE = '''\n",
                "def in_string():\n",
                "    pass\n",
                "'''\n",
                "\n",
                "def render_index():\n",
                "    pass\n",
                "\n",
                "# gone() is only mentioned\n",
                "\n",
                "class Bundle:\n",
                "    async def method(self):\n",
                "        def inner():\n",
                "            pass\n",
                "\n",
                "if True:\n",
                "    def conditional():\n",
                "        pass\n",
            ),
        )
        .unwrap();
        root
    }

    fn reasons(root: &Path, cases: &[(&str, &str)]) -> Vec<Option<String>> {
        let tree = crate::disk::Disk::new(root);
        cases
            .iter()
            .map(|(text, target)| {
                broken(
                    &tree,
                    &crate::readers::Readers,
                    text,
                    target,
                    "docs/backlog",
                    "docs",
                )
            })
            .collect()
    }

    #[test]
    fn a_resolving_link_passes() {
        let root = tree();
        let resolved = [
            (
                "heading from the bundle root",
                "/backlog/rules.md#what-goes-here",
            ),
            ("heading by a relative path", "rules.md#what-goes-here"),
            ("a file", "/log.md"),
            ("`render_index`", "../../tests/backlog_bundle.py"),
            // Defined at any depth
            ("`Bundle`", "../../tests/backlog_bundle.py"),
            ("`method`", "../../tests/backlog_bundle.py"),
            ("`inner`", "../../tests/backlog_bundle.py"),
            ("`conditional`", "../../tests/backlog_bundle.py"),
            ("`shouted`", "../../tests/upper.PY"),
            ("a URL is not checked", "https://example.com/okf"),
            ("a scheme with a plus", "coap+tcp://example.com/x"),
            ("mail", "mailto:someone@example.com"),
        ];
        assert_eq!(reasons(root.path(), &resolved), vec![None; resolved.len()]);
    }

    #[test]
    fn a_dangling_link_is_caught() {
        let root = tree();
        let bad = [
            ("missing heading", "/backlog/rules.md#no-such-heading"),
            ("missing file", "/no_such_file.md"),
            (
                "`no_such_function_anywhere`",
                "../../tests/backlog_bundle.py",
            ),
            // Mentioned in a comment, not defined
            ("`gone`", "../../tests/backlog_bundle.py"),
            // A def line inside a docstring or a string is text, not a definition
            ("`in_docstring`", "../../tests/backlog_bundle.py"),
            ("`in_string`", "../../tests/backlog_bundle.py"),
            // The format guide at the top of the log is an HTML comment, so GitHub gives its headings no anchor
            ("heading inside a comment", "/log.md#task-name"),
            ("only a fragment", "#what-goes-here"),
            // A drive letter is not a URL scheme: the path names a file on one machine
            ("drive letter", "C:/no/such/file.md"),
            ("drive letter with backslashes", "c:\\no\\such\\file.md"),
            // A file name with a line number is a path, not a URL: no scheme contains a dot
            ("a file and a line", "nothing.md:12"),
            ("an existing file and a line", "rules.md:3"),
            // The file exists, but only Windows reads `\` as a separator
            ("backslashes", "..\\log.md"),
            // A link to a .py file names what it points at; with no name, any def would do
            ("", "../../tests/backlog_bundle.py"),
            ("``", "../../tests/backlog_bundle.py"),
            // A .PY file is Python too: its link names a definition
            ("`nothing_here`", "../../tests/upper.PY"),
            // Each opens the file on Windows, and nothing on Linux or GitHub
            ("another case", "/backlog/Rules.md"),
            ("a directory in another case", "/Backlog/rules.md"),
            ("a dot at the end", "/log.md."),
            ("a space at the end", "/log.md%20"),
            ("a directory with a dot at the end", "../backlog./rules.md"),
            ("a stream", "/log.md:hidden"),
            // A file: URL is a path on one machine too
            (
                "a file URL",
                "file:///C:/Windows/System32/drivers/etc/hosts",
            ),
            ("a file URL in capitals", "FILE:///etc/hosts"),
            // GitHub serves only the repository
            ("above the repository", "../../../outside.md"),
        ];
        let found = reasons(root.path(), &bad);
        for ((text, target), why) in bad.iter().zip(&found) {
            assert!(why.is_some(), "{text} ({target}) passed");
        }
        let said = |target: &str| reasons(root.path(), &[("x", target)]).remove(0).unwrap();
        assert!(
            said("/backlog/Rules.md").contains("the disk has backlog/rules.md"),
            "{}",
            said("/backlog/Rules.md")
        );
        assert!(said("../../../outside.md").contains("outside the repository"));
    }

    #[test]
    fn a_stream_that_exists_is_not_found() {
        // On Windows, `log.md:hidden` opens a stream of log.md once one is written: the operating system would find it.
        // On Linux the same write makes a file of that name, which GitHub serves too, so the link resolves there
        let root = tree();
        let docs = root.path().join("docs");
        fs::write(docs.join("log.md:hidden"), "hidden\n").unwrap();
        let a_file: bool = fs::read_dir(&docs)
            .unwrap()
            .any(|entry| entry.unwrap().file_name() == "log.md:hidden");
        let why = reasons(root.path(), &[("x", "/log.md:hidden")]).remove(0);
        assert_eq!(why.is_none(), a_file, "{why:?}");
    }

    #[test]
    fn a_wrong_name_says_how_to_write_it() {
        let root = tree();
        let why = |text: &str| {
            reasons(root.path(), &[(text, "../../tests/backlog_bundle.py")])
                .remove(0)
                .unwrap()
        };
        // A call or a dotted path: the bare name it ends with
        assert!(
            why("`render_index()`")
                .ends_with("write the bare name as the link text: `render_index`")
        );
        assert!(why("`Bundle.method`").ends_with("write the bare name as the link text: `method`"));
        // Anything else: what the file defines, so the right name can be picked
        let listed = why("`render`");
        // A def inside a string is not listed
        assert!(
            listed.ends_with("(5 defined: Bundle, conditional, inner, method, render_index)"),
            "{listed}"
        );
        assert_eq!(
            how_to_name("x", &BTreeSet::new()),
            "the file defines no function or class"
        );
        let many: BTreeSet<String> = (0..25).map(|n| format!("f{n:02}")).collect();
        assert!(how_to_name("x", &many).contains("(25 defined: f00, f01,"));
        assert!(how_to_name("x", &many).ends_with("f19, ...)"));
    }

    #[test]
    fn a_backslash_fails_on_every_platform() {
        // Off Windows, `..\log.md` is a missing file anyway; the reason shows it fails for the separator everywhere
        let root = tree();
        let why = reasons(root.path(), &[("x", "..\\log.md")]).remove(0);
        assert!(
            why.as_ref()
                .is_some_and(|why| why.contains("separate with /")),
            "{why:?}"
        );
    }
}
