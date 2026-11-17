//! The environment check, read from the tree: the code of every layer but `infrastructure`, and the entry file, read
//! by the reader of its language, and judged by the rules in `domain`.

use std::io;

use crate::tree::{code_files, is_file, read_code};
use domain::declaration::Declared;
use domain::environment::{BUILD_SCRIPT, finding, in_python, in_rust, in_typescript, is_judged};
use domain::layers::{Language, table};
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
        .filter(|p| p.is_layer() && !declared.is_absent(p) && is_judged(p))
    {
        // A crate's build script runs before Rust compiles it, and reads what Cargo sets for it
        let build_script = join(&layer.path, BUILD_SCRIPT);
        // A test beside the code (a TypeScript `*.test.ts`) is judged no more than the files of Python's `tests/`
        files.extend(
            code_files(tree, &layer.path, |name| layout.has_comments(name))?
                .into_iter()
                .filter(|path| layout.language != Language::Rust || *path != build_script)
                .filter(|path| !layout.is_test_file(path)),
        );
    }
    files.extend(
        declared
            .entries(&table())
            .into_iter()
            .filter(|entry| is_file(tree, &entry.path))
            .map(|entry| entry.path),
    );
    let mut found = Vec::new();
    for path in files {
        let Some(source) = read_code(tree, &path, "reads of the environment", &mut found)? else {
            continue;
        };
        let reads = match layout.language {
            Language::Python => in_python(&parsers.python(&source, &path).uses),
            Language::TypeScript => in_typescript(&parsers.typescript(&source, &path).uses),
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
        in_python(&Readers.python(source, "domain/a.py").uses)
    }

    fn typescript(source: &str) -> Vec<(usize, String)> {
        in_typescript(&Readers.typescript(source, "src/domain/a.ts").uses)
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
import sys
x = getattr(os, 'environ')
y = sys.argv
z = p.environ
class C:
    environ = 1
w = C.environ
v = os.path.join('a', 'b')
u = os.getcwd()
";
        assert!(python(source).is_empty());
    }

    #[test]
    fn a_python_read_through_a_name_taken_from_os_is_found_at_the_import_and_at_each_use() {
        let source = "\
from os import getenv as g, environ
def f():
    return g('A') or environ['B']
";
        assert_eq!(
            python(source),
            [
                (1, "os.environ".to_string()),
                (1, "os.getenv".into()),
                (3, "os.environ".into()),
                (3, "os.getenv".into()),
            ]
        );
    }

    #[test]
    fn a_python_name_that_is_not_the_module_called_os_passes() {
        let source = "\
import os
def a(os):
    return os.environ
def b():
    os = make()
    return os.getenv('A')
def c(env=None):
    import os
    return os.getenv('A')
class D:
    os = make()
    x = os.environ
d = [os.environ for os in range(3)]
";
        // Only the `import os` of `c` is the module
        assert_eq!(python(source), [(9, "os.getenv".to_string())]);
        // The module `os` itself, bound by an import, is found in a method too, which the class body does not shadow
        assert_eq!(
            python(
                "import os\nclass D:\n    os = 1\n    def m(self):\n        return os.environ\n"
            ),
            [(5, "os.environ".to_string())]
        );
    }

    #[test]
    fn a_python_star_import_of_a_module_whose_names_count_fails() {
        let found = python(
            "from os import *\nfrom os.path import *\nfrom pathlib import *\nfrom typing import *\n",
        );
        assert_eq!(
            found.iter().map(|(line, _)| *line).collect::<Vec<_>>(),
            [1, 2, 3]
        );
        assert!(found[0].1.starts_with("os.*"));
        assert!(found[0].1.contains("cannot be read") && found[0].1.contains("name them"));
    }

    #[test]
    fn python_names_that_expand_the_environment_or_give_the_home_directory_are_reads() {
        let source = "\
import os
import os.path as p
import posixpath
import ntpath
import pathlib
from os.path import expandvars as ev
from pathlib import Path, PosixPath as P, WindowsPath
a = os.path.expandvars('$A')
b = os.path.expanduser('~')
c = p.expanduser('~')
d = posixpath.expandvars('$A')
e = ntpath.expanduser('~')
f = pathlib.Path.home()
g = Path.home()
h = P.home()
i = WindowsPath.home
j = ev('$A')
";
        assert_eq!(
            python(source),
            [
                (6, "os.path.expandvars".to_string()),
                (8, "os.path.expandvars".into()),
                (9, "os.path.expanduser".into()),
                (10, "os.path.expanduser".into()),
                (11, "posixpath.expandvars".into()),
                (12, "ntpath.expanduser".into()),
                (13, "pathlib.Path.home".into()),
                (14, "pathlib.Path.home".into()),
                (15, "pathlib.PosixPath.home".into()),
                (16, "pathlib.WindowsPath.home".into()),
                (17, "os.path.expandvars".into()),
            ]
        );
    }

    #[test]
    fn python_names_of_the_same_modules_that_read_nothing_from_the_environment_pass() {
        let source = "\
import os
import pathlib
from pathlib import Path
a = os.path.join('a', 'b')
b = os.path.exists('a')
c = os.path.abspath('a')
d = Path.cwd()
e = Path('a').exists()
f = pathlib.PurePath('a')
g = os.getcwd()
h = os.listdir('.')
";
        assert!(python(source).is_empty());
    }

    #[test]
    fn a_python_project_is_judged_through_its_uses_in_every_layer_and_the_entry_file() {
        let tree = crate::tree::fake::Fake::new(&[
            (
                "domain/a.py",
                "from pathlib import Path\n\n\ndef home():\n    return Path.home()\n",
            ),
            (
                "infrastructure/b.py",
                "import os\n\nVALUE = os.environ['A']\n",
            ),
            ("application/c.py", "def f(os):\n    return os.environ\n"),
            ("application/d.py", "from os import *\n"),
        ]);
        let declared = Declared::new(
            domain::declaration::parse_declaration(
                "stack = \"python\"\nareas = []\nui = \"none\"\n",
            )
            .unwrap(),
        )
        .unwrap();
        let found: Vec<String> = problems(&tree, &Readers, &declared)
            .unwrap()
            .iter()
            .map(|finding| {
                finding
                    .lines()
                    .next()
                    .unwrap()
                    .split(" (")
                    .next()
                    .unwrap()
                    .to_string()
            })
            .collect();
        assert_eq!(
            found,
            [
                "application/d.py:1: os.*",
                "domain/a.py:5: pathlib.Path.home"
            ]
        );
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
    fn typescript_reads_through_the_global_object_and_a_cast_are_found() {
        let source = "\
const a = window.process.env.A;
const b = self['process']['env'].B;
const c = (globalThis as any).process.env.C;
const d = (<any>global).process?.env!.D;
const { env: { E } } = process;
import * as p from 'node:process';
const f = p.env.F;
import { env as g } from 'process';
const h = g.H;
";
        assert_eq!(
            typescript(source),
            [
                (1, "process.env".to_string()),
                (2, "process.env".into()),
                (3, "process.env".into()),
                (4, "process.env".into()),
                (5, "process.env".into()),
                (7, "process.env".into()),
                (8, "process.env".into()),
                (9, "process.env".into()),
            ]
        );
    }

    #[test]
    fn typescript_reads_through_a_name_require_or_a_dynamic_import_binds() {
        let source = "\
const process = require('process');
const a = process.env.A;
const { env } = require('node:process');
const b = env.B;
const m = await import('node:process');
const c = m.env.C;
const d = (await import('process')).default.env.D; // not a declaration: not followed
function f() { const p = require('node:process'); return p.env.E; }
const x = require('path');
const g = x.env.G;
";
        assert_eq!(
            typescript(source),
            [
                (2, "process.env".to_string()),
                (3, "process.env".into()),
                (4, "process.env".into()),
                (6, "process.env".into()),
                (8, "process.env".into()),
            ]
        );
    }

    #[test]
    fn a_name_the_file_binds_is_not_the_global_and_a_use_outside_its_scope_is() {
        let source = "\
function a(process) { return process.env.A; }
function b() { const process = { env: {} }; return process.env.B; }
const c = (process) => process.env.C;
try { a(); } catch (process) { process.env.D; }
{ const process = { env: {} }; process.env.E; }
const f = process.env.F;
class G { m(process) { return process.env.G; } }
type T = typeof process.env;
";
        // Only the use in the module's own scope is the global
        assert_eq!(typescript(source), [(6, "process.env".to_string())]);
    }

    #[test]
    fn a_process_the_module_binds_hides_the_global_everywhere_in_the_module() {
        let source = "\
function a() { return process.env.A; }
const process = { env: {} };
";
        assert!(typescript(source).is_empty());
        let source = "\
import process from 'node:process';
function a(process) { return process.env.A; }
function b() { return process.env.B; }
";
        assert_eq!(typescript(source), [(3, "process.env".to_string())]);
    }

    #[test]
    fn what_a_file_computes_is_not_seen() {
        let source = "\
const a = globalThis['pro' + 'cess'].env.A;
const b = eval('process.env.B');
const key = 'env';
const c = process[key];
";
        assert!(typescript(source).is_empty());
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
    fn a_rust_rename_a_glob_and_a_longer_path_of_std_env_fail_in_a_layer_and_in_the_entry_file() {
        let tree = crate::tree::fake::Fake::new(&[
            ("crates/domain/Cargo.toml", "[package]\nname = \"domain\"\n"),
            (
                "crates/domain/src/lib.rs",
                "use std::env as e;\npub fn a() { let _ = e::var(\"A\"); }\n\
                 use std as s;\npub fn b() { let _ = s::env::vars().count(); }\n\
                 use std::env::*;\npub fn c() { let _ = env!(\"X\"); let _ = std::env::args(); }\n",
            ),
            (
                "src/main.rs",
                "use std::env as e;\nfn main() { let _ = e::var_os(\"M\"); }\n",
            ),
        ]);
        let declared = Declared::new(
            domain::declaration::parse_declaration("stack = \"rust\"\nareas = []\nui = \"none\"\n")
                .unwrap(),
        )
        .unwrap();
        let found = problems(&tree, &Readers, &declared).unwrap();
        let heads: Vec<&str> = found
            .iter()
            .map(|f| f.lines().next().unwrap().split(" (").next().unwrap())
            .collect();
        assert_eq!(
            heads,
            [
                "crates/domain/src/lib.rs:2: std::env::var",
                "crates/domain/src/lib.rs:4: std::env::vars",
                "crates/domain/src/lib.rs:5: std::env::*",
                "src/main.rs:2: std::env::var_os",
            ]
        );
        assert!(found[2].contains("cannot be read"));
    }

    #[test]
    fn a_typescript_test_beside_the_code_is_not_judged_and_the_code_is() {
        let tree = crate::tree::fake::Fake::new(&[
            ("src/domain/index.ts", ""),
            ("src/domain/a.ts", "export const a = process.env.A;\n"),
            ("src/domain/a.test.ts", "export const a = process.env.A;\n"),
            ("src/domain/a.spec.tsx", "export const a = process.env.A;\n"),
            (
                "src/domain/__tests__/helper.ts",
                "export const a = import.meta.env.VITE_A;\n",
            ),
            // The name is the whole of what makes it one
            ("src/domain/b.tests.ts", "export const a = process.env.A;\n"),
        ]);
        let declared = Declared::new(
            domain::declaration::parse_declaration(
                "stack = \"typescript\"\nareas = []\nui = \"none\"\n",
            )
            .unwrap(),
        )
        .unwrap();
        let found: Vec<String> = problems(&tree, &Readers, &declared)
            .unwrap()
            .iter()
            .map(|finding| finding.lines().next().unwrap().to_string())
            .collect();
        assert_eq!(
            found,
            [
                "src/domain/a.ts:1: process.env",
                "src/domain/b.tests.ts:1: process.env"
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
