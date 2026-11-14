//! Whether a link in a record resolves, read from the tree: the file it names looked up by its exact names and read,
//! and judged by the rules in `domain`.

use crate::tree::{Lookup, lookup, read_text};
use domain::links::{
    Names, Target, missing, target as pointed, to_definition, to_heading, to_outlined_definition,
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
    let (start, rel, fragment, names) = match pointed(target, here, bundle_root) {
        Err(why) => return Some(why),
        Ok(Target::Url) => return None,
        Ok(Target::File {
            start,
            rel,
            fragment,
            names,
        }) => (start, rel, fragment, names),
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
    match names {
        Names::DefinitionByText => {
            to_definition(text, target, &parsers.python_definitions(&source))
        }
        // A link with no fragment names the file, which is not parsed for it
        Names::DefinitionByFragment(_) if fragment.is_empty() => None,
        // The outline `follows` reads the same file as, so the link resolves when a document could follow its name. A
        // file the parser gives up on has no definitions to read, as for `follows`: it is told apart from one that
        // lacks the name
        Names::DefinitionByFragment(language) => match parsers.outline(language, &source, &full) {
            Ok(outline) => to_outlined_definition(target, &fragment, language, &source, &outline),
            Err((line, why)) => Some(format!(
                "{target} cannot be parsed, so its definitions cannot be read: line {line}: {why}"
            )),
        },
        Names::Heading => to_heading(target, &fragment, &source),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use super::*;

    /// A bundle with a document, a log whose format guide is an HTML comment, and a Python file outside the bundle.
    /// Laid out as in a project: the links are written from `docs/work/`.
    fn tree() -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
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
        // TypeScript, JavaScript and Rust: a definition is named in the fragment, in the form of a key of `follows`
        fs::write(
            root.path().join("tests/router.ts"),
            concat!(
                "export function route() {}\n",
                "class Router {\n",
                "  add() {}\n",
                "}\n",
                "export namespace Api { export function get() {} }\n",
                "// gone() is only mentioned\n",
                "const text = `function in_string() {}`;\n",
            ),
        )
        .unwrap();
        fs::write(
            root.path().join("tests/plain.mjs"),
            "export const make = () => 1;\n",
        )
        .unwrap();
        fs::write(
            root.path().join("tests/lib.rs"),
            concat!(
                "pub struct Table;\n",
                "impl Table {\n",
                "    pub fn add(&self) {}\n",
                "}\n",
                "impl Drop for Table {\n",
                "    fn drop(&mut self) {}\n",
                "}\n",
                "fn helper() {}\n",
                "mod tests {\n",
                "    fn inner() {}\n",
                "}\n",
                "// fn gone() {} is only mentioned\n",
            ),
        )
        .unwrap();
        // oxc gives up on it, so every definition in it would read as gone
        fs::write(
            root.path().join("tests/broken.ts"),
            "export function f() {}\nclass { {{ \n",
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
    fn a_link_to_a_definition_of_typescript_javascript_or_rust_resolves() {
        let root = tree();
        let resolved = [
            // The file itself, which has no fragment, and what its text says does not matter
            ("a TypeScript file", "../../tests/router.ts"),
            ("a Rust file", "../../tests/lib.rs"),
            (
                "a file read as it is, however broken",
                "../../tests/broken.ts",
            ),
            // A definition, exported or not, and a member after its type, as a key of `follows` writes it
            ("`route`", "../../tests/router.ts#route"),
            ("`Router`", "../../tests/router.ts#Router"),
            ("`Router.add`", "../../tests/router.ts#Router.add"),
            ("`Api.get`", "../../tests/router.ts#Api.get"),
            ("a variable", "../../tests/plain.mjs#make"),
            // A Rust type is its definition and every `impl` block for it, and `Type.method` a method of any
            ("`Table`", "../../tests/lib.rs#Table"),
            ("`Table.add`", "../../tests/lib.rs#Table.add"),
            ("`Table.drop`", "../../tests/lib.rs#Table.drop"),
            ("`helper`", "../../tests/lib.rs#helper"),
            ("an inline module", "../../tests/lib.rs#tests.inner"),
            // From the root of the bundle, and encoded
            ("`route`", "/../tests/router.ts#route"),
            ("`Table.add`", "../../tests/lib.rs#Table%2Eadd"),
        ];
        let found = reasons(root.path(), &resolved);
        let missing: Vec<_> = resolved
            .iter()
            .zip(&found)
            .filter(|(_, why)| why.is_some())
            .collect();
        assert!(missing.is_empty(), "{missing:?}");
    }

    #[test]
    fn a_link_to_a_definition_of_typescript_javascript_or_rust_that_is_not_there_is_caught() {
        let root = tree();
        let said = |target: &str| {
            reasons(root.path(), &[("`x`", target)])
                .remove(0)
                .unwrap_or_else(|| panic!("{target} passed"))
        };
        // Mentioned in a comment, in a string and in a template literal, which define nothing
        for target in [
            "router.ts#gone",
            "router.ts#in_string",
            "lib.rs#gone",
            // A name that is only the end or the start of one
            "router.ts#add",
            "router.ts#Router.add.x",
            "lib.rs#add",
            "lib.rs#Table.helper",
            "lib.rs#inner",
            // Another case
            "lib.rs#table",
            "plain.mjs#Make",
        ] {
            let why = said(&format!("../../tests/{target}"));
            let (file, name) = target.split_once('#').unwrap();
            assert!(
                why.starts_with(&format!(
                    "no definition named {name} in ../../tests/{file}#{name}; "
                )),
                "{why}"
            );
        }
        // How to write it: the names that end as it does, and otherwise what the file defines
        assert!(
            said("../../tests/lib.rs#add")
                .ends_with("the fragment is a name as the file defines it: Table.add")
        );
        let listed = said("../../tests/lib.rs#nothing");
        assert!(
            listed.ends_with(
                "the fragment is a name as the file defines it (6 defined: Table, Table.add, Table.drop, helper, \
                 tests, tests.inner)"
            ),
            "{listed}"
        );
        // A line anchor is no name of a definition, and what is no name at all says so
        assert!(said("../../tests/lib.rs#L12").contains("the fragment is a name"));
        assert!(said("../../tests/lib.rs#L12-L20").contains("names no definition"));
        // A file that does not parse says so, as `follows` does, not that the definition is gone
        let why = said("../../tests/broken.ts#f");
        assert!(
            why.starts_with("../../tests/broken.ts#f cannot be parsed, so its definitions cannot be read: line 2:"),
            "{why}"
        );
        // The file has to be there as it is for any link
        assert!(said("../../tests/Lib.rs#Table").contains("the disk has ../../tests/lib.rs"));
        assert!(said("../../tests/none.rs#Table").starts_with("no such file"));
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
            // A link to a .py file names what it points at; with no name, any def would do
            ("", "../../tests/backlog_bundle.py"),
            ("``", "../../tests/backlog_bundle.py"),
            // A .PY file is Python too: its link names a definition
            ("`nothing_here`", "../../tests/upper.PY"),
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
