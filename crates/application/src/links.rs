//! Whether a link in a record resolves, read from the tree: the file it names looked up by its exact names and read,
//! and judged by the rules in `domain`.

use crate::tree::{Lookup, lookup, read_text};
use domain::definitions::Definition;
use domain::links::{
    Target, file_name, missing, named, names_file, target as pointed, to_definition, to_heading,
};
use protocol::outbound::Parsers;
use protocol::outbound::Tree;

/// Why the link with `text` to `target` does not resolve, or `None` when it does. `here` is the directory of the
/// document that holds it, and `bundle_root` the directory a target starting with `/` is from (see
/// [`domain::links::target`]).
pub fn broken(
    tree: &dyn Tree,
    parsers: &dyn Parsers,
    text: &str,
    target: &str,
    here: &str,
    bundle_root: &str,
) -> Option<String> {
    let (start, rel, fragment, language) = match pointed(target, here, bundle_root) {
        Err(why) => return Some(why),
        Ok(Target::Url) => return None,
        Ok(Target::File {
            start,
            rel,
            fragment,
            language,
        }) => (start, rel, fragment, language),
    };
    let full = match lookup(tree, &start, &rel) {
        Lookup::Found {
            path,
            is_dir: false,
        } => path,
        Lookup::Spelled(on_disk) => return Some(missing(target, Some(&on_disk))),
        _ => return Some(missing(target, None)),
    };
    let source = match read_text(tree, &full) {
        Ok(source) => source,
        Err(e) => return Some(format!("cannot read {target}: {e}")),
    };
    let Some(language) = language else {
        return to_heading(target, &fragment, &source);
    };
    // A file of code: the text is the file's name, or a definition in it as a key of `follows` names it
    let name = match named(text, target, &fragment) {
        Ok(name) => name,
        Err(why) => return Some(why),
    };
    let file = file_name(&rel);
    if names_file(&name, &rel) {
        return None;
    }
    let outline: Vec<Definition> = match parsers.outline(language, &source, &full) {
        Ok(outline) => outline,
        Err((line, why)) => {
            return Some(format!(
                "cannot parse {target}, so the definition `{name}` cannot be looked up: line {line}: {why}"
            ));
        }
    };
    to_definition(&name, target, file, &source, &outline)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use super::*;

    /// A bundle with a document, a log whose format guide is an HTML comment, and files of code outside the bundle:
    /// Python, Rust and TypeScript. Laid out as in a project: the links are written from `docs/work/`.
    fn tree() -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join("src")).unwrap();
        fs::write(
            root.path().join("src/table.rs"),
            concat!(
                "pub struct Table;\n",
                "impl Table {\n",
                "    pub fn add(&self) {}\n",
                "}\n",
                "impl Drop for Table {\n",
                "    fn drop(&mut self) {}\n",
                "}\n",
                "pub fn free() {}\n",
                "// fn mentioned() only in a comment\n",
            ),
        )
        .unwrap();
        fs::write(
            root.path().join("src/router.ts"),
            concat!(
                "export class Router {\n",
                "  add(path: string) {}\n",
                "}\n",
                "export function route() {}\n",
                "export const handler = () => 1;\n",
            ),
        )
        .unwrap();
        fs::write(root.path().join("src/broken.ts"), "export class {{{ (\n").unwrap();
        let docs = root.path().join("docs");
        fs::create_dir_all(docs.join("work")).unwrap();
        fs::create_dir_all(root.path().join("tests")).unwrap();
        fs::write(docs.join("work/rules.md"), "# Rules\n\n## What goes here\n").unwrap();
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
        let tree = infrastructure::outbound::disk::Disk::new(root);
        cases
            .iter()
            .map(|(text, target)| {
                broken(
                    &tree,
                    &infrastructure::outbound::readers::Readers,
                    text,
                    target,
                    "docs/work",
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
                "/work/rules.md#what-goes-here",
            ),
            ("heading by a relative path", "rules.md#what-goes-here"),
            ("a file", "/log.md"),
            ("`render_index`", "../../tests/backlog_bundle.py"),
            // A name is a key of `follows`: a method with its type, at any depth of functions and classes
            ("`Bundle`", "../../tests/backlog_bundle.py"),
            ("`Bundle.method`", "../../tests/backlog_bundle.py"),
            ("`Bundle.method.inner`", "../../tests/backlog_bundle.py"),
            ("`shouted`", "../../tests/upper.PY"),
            // The same in every language: a Rust type covers all its `impl` blocks
            ("`Table`", "../../src/table.rs"),
            ("`Table.add`", "../../src/table.rs"),
            ("`Table.drop`", "../../src/table.rs"),
            ("`free`", "../../src/table.rs"),
            ("`Router`", "../../src/router.ts"),
            ("`Router.add`", "../../src/router.ts"),
            ("`route`", "../../src/router.ts"),
            ("`handler`", "../../src/router.ts"),
            // The file's own name, which is read from no outline
            ("`backlog_bundle.py`", "../../tests/backlog_bundle.py"),
            ("`table.rs`", "../../src/table.rs"),
            ("`router.ts`", "../../src/router.ts"),
            ("`broken.ts`", "../../src/broken.ts"),
            ("`upper.PY`", "../../tests/upper.PY"),
            // A path that ends in the target's path, from a directory boundary
            ("`src/table.rs`", "../../src/table.rs"),
            ("`tests/backlog_bundle.py`", "../../tests/backlog_bundle.py"),
            ("`src/router.ts`", "../../src/router.ts"),
            ("`src/broken.ts`", "../../src/broken.ts"),
            ("`tests/upper.PY`", "../../tests/upper.PY"),
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
            ("missing heading", "/work/rules.md#no-such-heading"),
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
            // A link to a file of code names what it points at; with no name, any def would do
            ("", "../../tests/backlog_bundle.py"),
            ("``", "../../tests/backlog_bundle.py"),
            // Prose is not a name, even the name of a definition or the file written as prose
            ("render_index", "../../tests/backlog_bundle.py"),
            ("the index", "../../tests/backlog_bundle.py"),
            ("backlog_bundle.py", "../../tests/backlog_bundle.py"),
            ("`render_index` here", "../../tests/backlog_bundle.py"),
            ("the table", "../../src/table.rs"),
            ("Router", "../../src/router.ts"),
            ("a file", "../../src/broken.ts"),
            // A method is named with its type, as a key of `follows` is: the bare name is not one
            ("`method`", "../../tests/backlog_bundle.py"),
            ("`inner`", "../../tests/backlog_bundle.py"),
            ("`add`", "../../src/table.rs"),
            ("`add`", "../../src/router.ts"),
            // In an `if` or a `try` is not in the outline, as a key does not lead through them
            ("`conditional`", "../../tests/backlog_bundle.py"),
            // The file's name, in another case or without its extension
            ("`TABLE.rs`", "../../src/table.rs"),
            ("`table`", "../../src/table.rs"),
            ("`../../src/table.rs`", "../../src/table.rs"),
            // A path is its last parts whole, spelled exactly, with none that climbs or stays
            ("`rc/table.rs`", "../../src/table.rs"),
            ("`Src/table.rs`", "../../src/table.rs"),
            ("`/src/table.rs`", "../../src/table.rs"),
            ("`src//table.rs`", "../../src/table.rs"),
            ("`./table.rs`", "../../src/table.rs"),
            ("`../src/table.rs`", "../../src/table.rs"),
            ("`src/table`", "../../src/table.rs"),
            ("`tests/table.rs`", "../../src/table.rs"),
            ("`table/table.rs`", "../../src/table.rs"),
            ("`src/router.ts`", "../../src/table.rs"),
            // A .PY file is Python too: its link names a definition
            ("`nothing_here`", "../../tests/upper.PY"),
            // Only what is defined: a comment mentions a name, and a type's other name is not its
            ("`mentioned`", "../../src/table.rs"),
            ("`Drop`", "../../src/table.rs"),
            ("`Router.remove`", "../../src/router.ts"),
            // A fragment names no heading in a file of code, and a line number goes stale
            ("`render_index`", "../../tests/backlog_bundle.py#L9"),
            ("`table.rs`", "../../src/table.rs#add"),
            // A file of code that is not there
            ("`gone.rs`", "../../src/gone.rs"),
            // Each opens the file on Windows, and nothing on Linux or GitHub
            ("another case", "/work/Rules.md"),
            ("a directory in another case", "/Work/rules.md"),
            ("a dot at the end", "/log.md."),
            ("a space at the end", "/log.md%20"),
            ("a directory with a dot at the end", "../work./rules.md"),
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
            said("/work/Rules.md").contains("the disk has work/rules.md"),
            "{}",
            said("/work/Rules.md")
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
        let why =
            |text: &str, target: &str| reasons(root.path(), &[(text, target)]).remove(0).unwrap();
        let py = "../../tests/backlog_bundle.py";
        // A call: the name it ends with
        assert!(
            why("`render_index()`", py)
                .ends_with("write the name without the call: `render_index`")
        );
        // A method without its type: the keys it can be
        let typeless = why("`method`", py);
        assert!(
            typeless.contains("with its type") && typeless.contains("`Bundle.method`"),
            "{typeless}"
        );
        // Anything else: what the file defines, so the right name can be picked
        let listed = why("`render`", py);
        // A def inside a string, an `if` or a comment is not listed
        assert!(
            listed.ends_with(
                "or the file's name `backlog_bundle.py` or a path ending in it (4 defined: Bundle, Bundle.method, \
                 Bundle.method.inner, render_index)"
            ),
            "{listed}"
        );
        // Prose is not a name
        for text in ["the render", "render_index", "", "``", "`a` and `b`"] {
            assert!(why(text, py).contains("link"), "{text}");
        }
        assert!(
            why("the router", "../../src/router.ts")
                .contains("is code naming a definition, or the file or a path ending in it")
        );
        // A name that is mentioned, a variant of the file's name
        assert!(why("`mentioned`", "../../src/table.rs").contains("no definition named mentioned"));
        assert!(why("`table`", "../../src/table.rs").contains("no definition named table"));
        assert!(why("`Table.drop.x`", "../../src/table.rs").contains("no definition named"));
    }

    #[test]
    fn a_fragment_on_a_code_file_fails() {
        let root = tree();
        for target in [
            "../../tests/backlog_bundle.py#render_index",
            "../../tests/backlog_bundle.py#L9",
            "../../src/table.rs#L1",
            "../../src/router.ts#route",
        ] {
            let why = reasons(root.path(), &[("`render_index`", target)]).remove(0);
            assert!(
                why.as_ref()
                    .is_some_and(|why| why.contains("has no fragment")),
                "{target}: {why:?}"
            );
        }
    }

    #[test]
    fn a_file_that_cannot_be_parsed_is_named_by_its_file_name_alone() {
        let root = tree();
        let at = |text: &str| reasons(root.path(), &[(text, "../../src/broken.ts")]).remove(0);
        assert_eq!(at("`broken.ts`"), None);
        let why = at("`Router`").unwrap();
        assert!(why.contains("cannot parse ../../src/broken.ts"), "{why}");
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
