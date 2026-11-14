//! The environment check, read from the tree: the code of every layer but `infrastructure`, and the entry file, read
//! by the reader of its language, and judged by the rules in `domain`.

use std::io;

use crate::tree::{code_files, exactly, read_code};
use domain::declaration::Declared;
use domain::environment::{BUILD_SCRIPT, finding, in_rust, is_judged};
use domain::layers::{Language, table};
use domain::markers::has_comments;
use domain::paths::join;
use protocol::outbound::Parsers;
use protocol::outbound::Tree;

/// Every read or write of the environment outside `infrastructure` in the project of `declared`, and every file that
/// could not be read. An error is a directory that could not be walked.
pub fn problems(
    tree: &dyn Tree,
    parsers: &dyn Parsers,
    declared: &Declared,
) -> io::Result<Vec<String>> {
    let Some(layout) = &declared.layout else {
        // A repository of records only has no code to read
        return Ok(Vec::new());
    };
    let mut files = Vec::new();
    // A layer's levels sit in its directory, so walking each layer reads them
    for layer in declared
        .places
        .iter()
        .filter(|p| p.parent.is_none() && !declared.is_absent(p) && is_judged(p))
    {
        // A crate's build script runs before Rust compiles it, and reads what Cargo sets for it
        let build_script = join(&layer.path, BUILD_SCRIPT);
        files.extend(
            code_files(tree, &layer.path, |name| has_comments(layout, name))?
                .into_iter()
                .filter(|path| layout.language != Language::Rust || *path != build_script),
        );
    }
    if let Some(entry) = layout.entry(&table())
        && exactly(tree, &entry.path).is_ok_and(|(_, is_dir)| !is_dir)
    {
        files.push(entry.path);
    }
    let mut found = Vec::new();
    for path in files {
        let Some(source) = read_code(tree, &path, "reads of the environment", &mut found)? else {
            continue;
        };
        let reads = match layout.language {
            Language::Python => parsers.python(&source, &path).environment,
            Language::TypeScript => parsers.typescript(&source, &path).environment,
            Language::Rust => in_rust(&source),
        };
        found.extend(
            reads
                .into_iter()
                .map(|(line, what)| finding(&path, &source, line, &what)),
        );
    }
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;
    use infrastructure::outbound::readers::Readers;

    fn python(source: &str) -> Vec<(usize, String)> {
        Readers.python(source, "domain/a.py").environment
    }

    fn typescript(source: &str) -> Vec<(usize, String)> {
        Readers.typescript(source, "src/domain/a.ts").environment
    }

    #[test]
    fn python_reads_are_found_through_every_binding_of_os() {
        let source = "\
import os
import os as o
from os import environ, getenv as g, path
import os.path
def f():
    a = os.environ['A']
    b = o.getenv('B')
    os.putenv('C', '1')
    return os.path.join(a, b)
";
        assert_eq!(
            python(source),
            [
                (3, "os.environ".to_string()),
                (3, "os.getenv".into()),
                (6, "os.environ".into()),
                (7, "os.getenv".into()),
                (8, "os.putenv".into()),
            ]
        );
    }

    #[test]
    fn python_forms_built_at_run_time_or_of_another_module_pass() {
        let source = "\
import os.path as p
from os import *
import sys
x = getattr(os, 'environ')
y = sys.argv
z = p.environ
class C:
    environ = 1
w = C.environ
";
        assert!(python(source).is_empty());
    }

    #[test]
    fn typescript_reads_are_found_in_every_form() {
        let source = "\
import { env } from 'node:process';
import proc from 'process';
const a = process.env.A;
const b = import.meta.env.VITE_B;
const { env: e } = process;
const c = globalThis.process.env.C;
const d = proc.env.D;
const f = process['env'];
";
        assert_eq!(
            typescript(source),
            [
                (1, "process.env".to_string()),
                (3, "process.env".into()),
                (4, "import.meta.env".into()),
                (5, "process.env".into()),
                (6, "process.env".into()),
                (7, "process.env".into()),
                (8, "process.env".into()),
            ]
        );
    }

    #[test]
    fn typescript_forms_of_other_objects_pass() {
        let source = "\
const config = { env: 'dev' };
const a = config.env;
const { env } = config;
const b = import.meta.url;
const c = process.argv;
";
        assert!(typescript(source).is_empty());
    }

    #[test]
    fn the_values_vite_sets_itself_pass_and_the_ones_passed_in_do_not() {
        let source = "\
if (import.meta.env.DEV) {}
const base = import.meta.env.BASE_URL;
const { MODE, PROD } = import.meta.env;
const url = import.meta.env.VITE_API_URL;
const { DEV, VITE_KEY } = import.meta.env;
const { SSR, ...rest } = import.meta.env;
const all = import.meta.env;
";
        assert_eq!(
            typescript(source),
            [
                (4, "import.meta.env".to_string()),
                (5, "import.meta.env".into()),
                (6, "import.meta.env".into()),
                (7, "import.meta.env".into()),
            ]
        );
    }

    #[test]
    fn a_rust_crates_build_script_is_not_judged() {
        let tree = crate::tree::fake::Fake::new(&[
            ("crates/domain/Cargo.toml", "[package]\nname = \"domain\"\n"),
            (
                "crates/domain/build.rs",
                "fn main() { let _ = std::env::var(\"OUT_DIR\"); }\n",
            ),
            (
                "crates/domain/src/lib.rs",
                "pub fn a() { let _ = std::env::var(\"A\"); }\n",
            ),
        ]);
        let declared = Declared::new(
            domain::declaration::parse_declaration("stack = \"rust\"\nareas = []\nui = \"none\"\n")
                .unwrap(),
        )
        .unwrap();
        assert_eq!(
            problems(&tree, &Readers, &declared).unwrap(),
            [
                "crates/domain/src/lib.rs:1: std::env::var\npub fn a() { let _ = std::env::var(\"A\"); }"
            ]
        );
    }
}
