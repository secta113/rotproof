//! The check that the inner layers do no I/O, read from the tree: the code of `domain`, `protocol`, `application` and
//! `presentation`, read by the reader of its language, and judged by the rules in `domain::inner_io`. What it finds is
//! a [`Forbidden`] of its own rule, kept as an import is: `rotproof approve` approves it, and an approval that matches
//! nothing fails.
//!
//! In Python the net of method names leaves out the methods the project defines itself, which are read from every
//! file of the project (a test, a file in `unchecked` too), not from the inner layers alone.

use std::collections::BTreeSet;
use std::io;

use crate::tree::{code_files, read_code};
use domain::code::{Defined, Import, Source};
use domain::declaration::Declared;
use domain::direction::Forbidden;
use domain::inner_io::{
    Excluded, findings, in_python, in_rust, in_typescript, is_judged, python_excluded, unread,
};
use domain::layers::{Language, Layout, Place};
use domain::paths::{file_name, join, parent};
use protocol::outbound::{Parsers, Tree};

/// What the check found: the I/O of the inner layers, and every file that could not be read.
#[derive(Debug, Default)]
pub struct Judged {
    pub forbidden: Vec<Forbidden>,
    /// A file that is not UTF-8, which cannot be read
    pub problems: Vec<String>,
    /// The methods of the project that the net of method names leaves out, which `check` prints on every run
    pub excluded: Excluded,
}

/// A file of an inner layer, read.
struct Read {
    path: String,
    source: String,
    layer: String,
    parsed: Parsed,
}

/// What the reader of the language gave of a file.
enum Parsed {
    Python(Source<Vec<String>>),
    TypeScript(Source<Import>),
    Rust,
}

/// Every use of I/O in the inner layers of the project of `declared`. An error is a directory that could not be walked.
pub fn problems(tree: &dyn Tree, parsers: &dyn Parsers, declared: &Declared) -> io::Result<Judged> {
    let mut judged = Judged::default();
    let Some(layout) = &declared.layout else {
        // A repository of records only has no code to read
        return Ok(judged);
    };
    let mut files = Vec::new();
    for layer in declared
        .places
        .iter()
        .filter(|p| p.is_layer() && !declared.is_absent(p) && is_judged(p))
    {
        // The files of the layer, with their text, in the order of their paths
        let mut layer_files = Vec::new();
        for path in files_of(tree, layout, layer)? {
            if let Some(source) = read_code(tree, &path, "I/O", &mut judged.problems)? {
                layer_files.push((path, source));
            }
        }
        let tests = test_paths(layout, &layer_files);
        for (path, source) in layer_files {
            if tests.iter().any(|test| is_under(&path, test)) {
                continue;
            }
            let parsed = match layout.language {
                Language::Python => Parsed::Python(parsers.python(&source, &path)),
                Language::TypeScript => Parsed::TypeScript(parsers.typescript(&source, &path)),
                Language::Rust => Parsed::Rust,
            };
            files.push(Read {
                path,
                source,
                layer: layer.name.clone(),
                parsed,
            });
        }
    }
    if layout.language == Language::Python {
        judged.excluded = python_excluded(&defined_in(tree, parsers, declared, layout, &files)?);
    }
    for file in &files {
        let found = match &file.parsed {
            Parsed::Python(read) => {
                let mut found =
                    in_python(&read.imports, &read.uses, &read.methods, &judged.excluded);
                found.extend(unread(&read.unread));
                found
            }
            Parsed::TypeScript(read) => {
                let mut found = in_typescript(&read.imports, &read.uses);
                found.extend(unread(&read.unread));
                found
            }
            Parsed::Rust => in_rust(&file.source),
        };
        judged.forbidden.extend(findings(
            &file.path,
            &file.source,
            &file.layer,
            layout.language,
            &found,
        ));
    }
    // In the byte order of the path, as every check lists what it finds file by file (a file's lines keep their order)
    judged.forbidden.sort_by(|a, b| a.file.cmp(&b.file));
    judged.problems.sort();
    Ok(judged)
}

/// Every method a class of the project defines, in a Python project: the judged files as they were read, and every
/// other code file of the project (a test, a file of another layer, one in `unchecked`) read here, so that a method the
/// project defines is left out of the net wherever it is. A file that cannot be read as UTF-8 is not read here: the
/// check of the code that is judged says so.
fn defined_in(
    tree: &dyn Tree,
    parsers: &dyn Parsers,
    declared: &Declared,
    layout: &Layout,
    read: &[Read],
) -> io::Result<Vec<Defined>> {
    let mut defined: Vec<Defined> = read
        .iter()
        .filter_map(|file| match &file.parsed {
            Parsed::Python(source) => Some(source.defined.clone()),
            _ => None,
        })
        .flatten()
        .collect();
    let known: BTreeSet<&str> = read.iter().map(|file| file.path.as_str()).collect();
    for scope in declared.scopes() {
        for path in code_files(tree, &scope, |name| layout.has_comments(name))? {
            if known.contains(path.as_str()) {
                continue;
            }
            if let Some(source) = read_code(tree, &path, "I/O", &mut Vec::new())? {
                defined.extend(parsers.python(&source, &path).defined);
            }
        }
    }
    Ok(defined)
}

/// The files of `layer` that are code, but for a test by its name: the whole of the layer's directory in Python and
/// TypeScript, and in Rust the crate's `src/` alone, since Cargo keeps a crate's tests, benches, examples and build
/// script beside it.
fn files_of(tree: &dyn Tree, layout: &Layout, layer: &Place) -> io::Result<Vec<String>> {
    let dir = match layout.language {
        Language::Rust => join(&layer.path, "src"),
        Language::Python | Language::TypeScript => layer.path.clone(),
    };
    Ok(code_files(tree, &dir, |name| layout.has_comments(name))?
        .into_iter()
        .filter(|path| !layout.is_test_file(path))
        .collect())
}

/// The paths of the test modules of a Rust crate, from the files of `files` (path, text): a module declared
/// `#[cfg(test)] mod x;` in a file is `x.rs` or `x/mod.rs`, in the directory of the file for `lib.rs`, `main.rs` and
/// `mod.rs` and in the directory named as the file for any other. A file under such a path is a test. Each is a path
/// of the file, or of the directory, that is a test as a whole. None in a stack that tells no module apart.
fn test_paths(layout: &Layout, files: &[(String, String)]) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    if layout.language != Language::Rust {
        return found;
    }
    for (path, source) in files {
        let names = domain::rust::test_modules(&domain::rust::code(source));
        if names.is_empty() {
            continue;
        }
        let stem = file_name(path).trim_end_matches(".rs");
        let dir = match stem {
            "lib" | "main" | "mod" => parent(path).to_string(),
            _ => join(parent(path), stem),
        };
        for name in names {
            found.insert(join(&dir, &name));
        }
    }
    found
}

/// Whether `path` is the file `base.rs` or sits in the directory `base/`.
fn is_under(path: &str, base: &str) -> bool {
    path == format!("{base}.rs") || path.starts_with(&format!("{base}/"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::fake::Fake;
    use domain::direction::Rule;
    use infrastructure::outbound::readers::Readers;

    fn declared(stack: &str, ui: &str) -> Declared {
        Declared::new(
            domain::declaration::parse_declaration(&format!(
                "stack = \"{stack}\"\nareas = []\nui = \"{ui}\"\n"
            ))
            .unwrap(),
        )
        .unwrap()
    }

    /// The findings of a tree, as `file: entry (names)`, one for each line
    fn judged(files: &[(&str, &str)], stack: &str, ui: &str) -> Vec<String> {
        let tree = Fake::new(files);
        let found = problems(&tree, &Readers, &declared(stack, ui)).unwrap();
        assert!(found.problems.is_empty(), "{:?}", found.problems);
        found
            .forbidden
            .iter()
            .map(|f| {
                let names = match &f.rule {
                    Rule::InnerIo { names } if !names.is_empty() => {
                        format!(" ({})", names.join(", "))
                    }
                    _ => String::new(),
                };
                format!(
                    "{}:{}{names}",
                    f.file,
                    f.detail.split(':').nth(1).unwrap_or_default()
                ) + &format!(" {}", f.import)
            })
            .collect()
    }

    #[test]
    fn a_python_project_is_judged_in_its_four_layers_and_not_in_the_others() {
        let io = "import shutil\nshutil.rmtree('x')\n";
        let found = judged(
            &[
                ("domain/a.py", io),
                ("protocol/inbound/b.py", "from socket import socket\n"),
                ("protocol/outbound/c.py", "import sqlite3\n"),
                ("application/d.py", "import subprocess\n"),
                ("infrastructure/inbound/e.py", io),
                ("infrastructure/outbound/f.py", io),
                ("composition/g.py", io),
                ("__main__.py", io),
                ("tests/test_a.py", io),
            ],
            "python",
            "none",
        );
        assert_eq!(
            found,
            [
                "application/d.py:1 subprocess",
                "domain/a.py:1 shutil",
                "domain/a.py:2 (rmtree) shutil",
                "protocol/inbound/b.py:1 (socket) socket",
                "protocol/outbound/c.py:1 sqlite3",
            ]
        );
    }

    #[test]
    fn the_screens_of_a_project_with_a_ui_are_judged_in_presentation_and_not_in_ui() {
        let found = problems(
            &Fake::new(&[
                ("ui/pages/home.py", "import webbrowser\n"),
                ("presentation/models/m.py", "import shutil\n"),
                ("presentation/display/d.py", "import tempfile\n"),
            ]),
            &Readers,
            &declared("python", "atomic"),
        )
        .unwrap();
        let said: Vec<(&str, bool)> = found
            .forbidden
            .iter()
            .map(|f| {
                (
                    f.file.as_str(),
                    f.detail.contains("a screen's model does no I/O"),
                )
            })
            .collect();
        assert_eq!(
            said,
            [
                ("presentation/display/d.py", true),
                ("presentation/models/m.py", true)
            ]
        );
    }

    #[test]
    fn a_typescript_test_beside_the_code_is_free_and_the_code_is_not() {
        let found = judged(
            &[
                ("src/domain/index.ts", ""),
                ("src/domain/a.ts", "import { readFile } from 'node:fs';\n"),
                (
                    "src/domain/a.test.ts",
                    "import { readFile } from 'node:fs';\n",
                ),
                ("src/domain/__tests__/helper.ts", "import fs from 'fs';\n"),
                ("src/domain/b.ts", "import sqlite from 'bun:sqlite';\n"),
                ("src/infrastructure/outbound/c.ts", "import fs from 'fs';\n"),
                ("src/main.ts", "import fs from 'fs';\n"),
            ],
            "typescript",
            "none",
        );
        assert_eq!(
            found,
            ["src/domain/a.ts:1 (readFile) fs", "src/domain/b.ts:1 bun:"]
        );
    }

    #[test]
    fn a_package_that_the_manifest_lists_does_not_make_fs_another_module() {
        let tree = Fake::new(&[
            (
                "package.json",
                "{\"name\":\"x\",\"dependencies\":{\"fs\":\"1.0.0\"}}\n",
            ),
            ("src/domain/a.ts", "import fs from 'fs';\n"),
        ]);
        let found = problems(&tree, &Readers, &declared("typescript", "none")).unwrap();
        assert_eq!(found.forbidden.len(), 1);
        assert_eq!(found.forbidden[0].import, "fs");
    }

    #[test]
    fn a_rust_crate_is_judged_in_its_src_and_not_in_its_tests_or_what_cfg_test_marks() {
        let found = judged(
            &[
                ("crates/domain/Cargo.toml", "[package]\nname = \"domain\"\n"),
                (
                    "crates/domain/src/lib.rs",
                    "pub mod a;\n#[cfg(test)]\nmod tests;\n#[cfg(test)]\nmod helpers {\n    pub fn f() { std::fs::read(\"x\"); }\n}\npub fn g() { std::fs::read(\"y\"); }\n",
                ),
                (
                    "crates/domain/src/a.rs",
                    "pub fn a() { println!(\"a\"); }\n#[cfg(test)]\nmod tests;\n",
                ),
                ("crates/domain/src/tests.rs", "use std::process::Command;\n"),
                (
                    "crates/domain/src/a/tests.rs",
                    "use std::process::Command;\n",
                ),
                (
                    "crates/domain/src/a/tests/more.rs",
                    "use std::process::Command;\n",
                ),
                ("crates/domain/src/other.rs", "use std::process::Command;\n"),
                ("crates/domain/tests/it.rs", "use std::fs;\n"),
                ("crates/domain/benches/b.rs", "use std::fs;\n"),
                (
                    "crates/domain/build.rs",
                    "fn main() { std::fs::write(\"a\", \"b\"); }\n",
                ),
                (
                    "crates/infrastructure/Cargo.toml",
                    "[package]\nname = \"infrastructure\"\n",
                ),
                (
                    "crates/infrastructure/src/lib.rs",
                    "pub fn i() { std::fs::read(\"x\"); }\n",
                ),
                ("src/main.rs", "fn main() { println!(\"x\"); }\n"),
            ],
            "rust",
            "none",
        );
        assert_eq!(
            found,
            [
                "crates/domain/src/a.rs:1 (println!) std::io",
                "crates/domain/src/lib.rs:8 (read) std::fs",
                "crates/domain/src/other.rs:1 (Command) std::process",
            ]
        );
    }

    #[test]
    fn a_layer_declared_absent_and_a_repository_of_records_are_not_read() {
        let tree = Fake::new(&[("domain/a.py", "import shutil\n")]);
        let declared = Declared::new(
            domain::declaration::parse_declaration(
                "stack = \"python\"\nareas = []\nui = \"none\"\nabsent = [\"domain\"]\n",
            )
            .unwrap(),
        )
        .unwrap();
        assert!(
            problems(&tree, &Readers, &declared)
                .unwrap()
                .forbidden
                .is_empty()
        );
        let records = Declared::new(
            domain::declaration::parse_declaration("stack = \"none\"\nareas = [\"a\"]\n").unwrap(),
        )
        .unwrap();
        assert!(
            problems(&tree, &Readers, &records)
                .unwrap()
                .forbidden
                .is_empty()
        );
    }

    /// What a Python file in `domain/` is found to do, as `line (names) entry`.
    fn says(source: &str) -> Vec<String> {
        judged(&[("domain/a.py", source)], "python", "none")
            .iter()
            .map(|found| found.strip_prefix("domain/a.py:").unwrap().to_string())
            .collect()
    }

    fn nothing() -> Vec<String> {
        Vec::new()
    }

    #[test]
    fn the_pure_names_of_an_allowlist_pass_and_every_other_name_of_the_module_fails() {
        let pure = "\
import os
import os.path
import sys
import time
import logging
from os import PathLike
a = os.fspath('x')
b = os.path.join('a', 'b')
c = sys.version_info >= (3, 11)
d = time.strptime('1', '%S')
e = logging.getLogger('x')
time.sleep(1)
f = logging.INFO
g = sys.getsizeof(a)
h = os.path.splitext(b)
i = os.error
";
        assert_eq!(says(pure), nothing());
        let source = "\
import os
import os.path
import sys
import time
import logging
a = os.getcwd()
b = os.sep
c = os.path.exists('x')
d = sys.platform
e = sys.exit
f = logging.basicConfig()
g = logging.info('x')
h = time.mktime(t)
i = time.timezone
j = os.listdir('.')
k = logging.StreamHandler()
";
        assert_eq!(
            says(source),
            [
                "6 os.getcwd",
                "7 os.sep",
                "8 os.path.exists",
                "9 sys.platform",
                "10 sys.exit",
                "11 logging.basicConfig",
                "12 logging.info",
                "13 time.mktime",
                "14 time.timezone",
                "15 os.listdir",
                "16 logging.StreamHandler",
            ]
        );
    }

    #[test]
    fn a_name_taken_from_a_module_or_reached_through_an_alias_is_judged_as_the_module_names_it() {
        let source = "\
import os as o
from os import getcwd as cwd, path
from os.path import exists
import posixpath as pp
from time import time as now
o.listdir('.')
cwd()
path.exists('x')
path.join('a', 'b')
exists('y')
pp.getsize('z')
now()
";
        assert_eq!(
            says(source),
            [
                "2 os.getcwd",
                "3 os.path.exists",
                "5 (time.time) clock",
                "6 os.listdir",
                "7 os.getcwd",
                "8 os.path.exists",
                "10 os.path.exists",
                "11 posixpath.getsize",
                "12 (time.time) clock",
            ]
        );
    }

    #[test]
    fn what_the_environment_check_owns_is_not_reported_twice() {
        let source = "\
import os
from pathlib import Path
from os import *
from pathlib import *
a = os.environ['A']
b = os.getenv('B')
c = os.path.expanduser('~')
d = Path.home()
e = os.getcwd()
";
        // The import of `pathlib.Path` is a finding of its own; the names and the star imports are the environment's
        assert_eq!(says(source), ["2 pathlib.Path", "9 os.getcwd"]);
        let tree = Fake::new(&[("domain/a.py", source)]);
        let declared = declared("python", "none");
        let environment = crate::environment::problems(&tree, &Readers, &declared).unwrap();
        assert_eq!(environment.len(), 6, "{environment:?}");
    }

    #[test]
    fn the_clock_is_one_entry_as_a_call_and_as_a_reference() {
        let source = "\
import time
import uuid
import datetime as dt
from datetime import datetime, date
from dataclasses import field
from typing import Annotated
from pydantic import Field
class M:
    id: Annotated[str, Field(default_factory=uuid.uuid4)]
    at: datetime = field(default_factory=datetime.now)
a = datetime.now()
d = dt.datetime.utcnow()
e = date.today()
f = time.time()
g = time.gmtime()
h = time.gmtime(0)
i = time.strftime('%Y')
j = time.strftime('%Y', h)
k = time.monotonic
l = datetime.fromtimestamp(0)
";
        assert_eq!(
            says(source),
            [
                "9 (uuid.uuid4) random",
                "10 (datetime.datetime.now) clock",
                "11 (datetime.datetime.now) clock",
                "12 (datetime.datetime.utcnow) clock",
                "13 (datetime.date.today) clock",
                "14 (time.time) clock",
                "15 (time.gmtime) clock",
                "17 (time.strftime) clock",
                "19 (time.monotonic) clock",
            ]
        );
    }

    #[test]
    fn a_function_is_pure_with_its_argument_given_and_a_generator_with_its_seed() {
        let source = "\
import random
import time
a = random.Random()
b = random.Random(1)
c = random.Random(None)
d = random.Random(seed)
e = random.choice([1, 2])
f = random.SystemRandom()
g = time.localtime(None)
h = time.localtime(t)
i = time.ctime()
j = time.asctime(t)
";
        assert_eq!(
            says(source),
            [
                "3 (random.Random) random",
                "5 (random.Random) random",
                "7 (random.choice) random",
                "8 (random.SystemRandom) random",
                "9 (time.localtime) clock",
                "11 (time.ctime) clock",
            ]
        );
    }

    #[test]
    fn a_builtin_no_scope_binds_is_the_terminal_and_one_it_binds_is_not() {
        let shadowed = "\
def a(input):
    return input()
def b():
    print = log
    print('x')
class C:
    def open(self):
        pass
from app import open as app_open
app_open()
";
        assert_eq!(says(shadowed), nothing());
        let source = "\
x = open('f')
print('x')
y = input()
log = print
import io, codecs, sys, builtins
io.open('f')
codecs.open('f')
sys.stdout.write('x')
builtins.print('y')
breakpoint()
";
        assert_eq!(
            says(source),
            [
                "1 (open) terminal",
                "2 (print) terminal",
                "3 (input) terminal",
                "4 (print) terminal",
                "6 (io.open) terminal",
                "7 (codecs.open) terminal",
                "8 (sys.stdout) terminal",
                "9 (print) terminal",
                "10 (breakpoint) terminal",
            ]
        );
    }

    #[test]
    fn a_path_is_io_by_name_and_a_pure_path_is_not() {
        let source = "\
from pathlib import Path, PurePath, PurePosixPath
import pathlib
a = Path('x')
b = PurePath('x')
c = pathlib.PosixPath('x')
d = pathlib.PurePosixPath('x')
e = PurePosixPath('x').name
";
        assert_eq!(
            says(source),
            ["1 pathlib.Path", "3 pathlib.Path", "5 pathlib.PosixPath"]
        );
    }

    #[test]
    fn a_name_in_a_type_position_is_not_judged_and_a_call_in_an_annotation_is() {
        let source = "\
import os
import sys
import datetime
from pathlib import PurePath
x: sys.stdout = None
def f(a: os.PathLike, b: datetime.datetime, c: os.sep) -> sys.argv:
    pass
";
        assert_eq!(says(source), nothing());
        assert_eq!(
            says("import os\nx: list[os.getcwd()] = []\n"),
            ["2 os.getcwd"]
        );
    }

    #[test]
    fn the_command_line_arguments_are_one_entry_and_free_where_the_program_starts() {
        let found = judged(
            &[
                (
                    "domain/a.py",
                    "import sys\nx = sys.argv\ny = sys.orig_argv\n",
                ),
                ("application/b.py", "from sys import argv\nprint(argv)\n"),
                ("infrastructure/inbound/c.py", "import sys\nx = sys.argv\n"),
                ("composition/d.py", "import sys\nx = sys.argv\n"),
                ("__main__.py", "import sys\nx = sys.argv\n"),
                ("tests/test_a.py", "import sys\nx = sys.argv\n"),
            ],
            "python",
            "none",
        );
        assert_eq!(
            found,
            [
                "application/b.py:1 (sys.argv) argv",
                "application/b.py:2 (sys.argv) argv",
                "application/b.py:2 (print) terminal",
                "domain/a.py:2 (sys.argv) argv",
                "domain/a.py:3 (sys.orig_argv) argv",
            ]
        );
    }

    #[test]
    fn a_module_used_as_a_value_a_star_import_and_what_runs_unread_code_fail() {
        let source = "\
import os
import importlib
a = getattr(os, 'system')
b = os
c = vars(os)
d = eval('1')
exec('x')
importlib.import_module(name)
__import__(x)
from time import *
from datetime import *
from random import *
from typing import *
from os import *
";
        assert_eq!(
            says(source),
            [
                "3 os",
                "4 os",
                "5 os",
                "6 eval",
                "7 exec",
                "8 importlib.import_module",
                "9 __import__",
                "10 time",
                "11 datetime",
                "12 random",
            ]
        );
        // What is called by its name is judged by it, and a module the file binds is not the module
        assert_eq!(
            says("import os\ndef f(os):\n    return check(os)\nx = os.fspath('a')\n"),
            nothing()
        );
    }

    #[test]
    fn a_finding_of_a_group_says_what_to_do_and_names_the_entry() {
        let tree = Fake::new(&[
            ("domain/a.py", "import time\nx = time.time()\n"),
            (
                "presentation/models/m.py",
                "import uuid\nx = uuid.uuid4()\n",
            ),
        ]);
        let found = problems(&tree, &Readers, &declared("python", "atomic")).unwrap();
        let details: Vec<&str> = found.forbidden.iter().map(|f| f.detail.as_str()).collect();
        assert_eq!(
            details,
            [
                "domain/a.py:2: pass the time now in as an argument, or take it from a port in protocol.outbound that \
                 the use case is given (a port may be a callable alias, `Clock = Callable[[], datetime]`): reads the \
                 clock (time.time), which does I/O (entry `clock`)\nx = time.time()",
                "presentation/models/m.py:2: a screen's model does no I/O: it gets the id or the random value from its \
                 page, or through a use case of protocol.inbound it is given: draws on randomness (uuid.uuid4), which \
                 does I/O (entry `random`)\nx = uuid.uuid4()",
            ]
        );
    }

    #[test]
    fn a_method_of_a_path_is_judged_on_any_receiver_unless_the_project_defines_one() {
        let files = [
            (
                "domain/a.py",
                "\
def f(p, repo):
    p.read_text()
    p.exists()
    p.exists(1)
    repo.mkdir('x')
    p.open('w')
    p.open()
    p.stat()
    p.mkdir()
",
            ),
            (
                "infrastructure/store.py",
                "class Store:\n    def mkdir(self, name): ...\n    def stat(self): ...\n    def close(self): ...\n",
            ),
        ];
        let found = judged(&files, "python", "none");
        assert_eq!(
            found,
            [
                "domain/a.py:2 read_text",
                "domain/a.py:3 exists",
                "domain/a.py:6 open",
                "domain/a.py:9 mkdir",
            ]
        );
        let tree = Fake::new(&files);
        let judged = problems(&tree, &Readers, &declared("python", "none")).unwrap();
        assert_eq!(
            judged.excluded.line().as_deref(),
            Some(
                "I/O net: left out, as the project defines a method of the same name and number of parameters: \
                 mkdir(1), stat(0)"
            )
        );
        // Another file of the project defines it too: a test, a file in `unchecked`, a class in a layer
        for place in ["tests/helpers.py", "domain/b.py", "composition/c.py"] {
            let found = judged_with(
                place,
                "class Fake:\n    def exists(self): ...\n    def read_text(self, a, b): ...\n",
            );
            assert_eq!(
                found,
                [
                    "domain/a.py:2 read_text",
                    "domain/a.py:5 mkdir",
                    "domain/a.py:6 open",
                    "domain/a.py:8 stat",
                    "domain/a.py:9 mkdir",
                ],
                "{place}"
            );
        }
    }

    /// The findings of the method net's file with one file more that defines a method.
    fn judged_with(place: &str, source: &str) -> Vec<String> {
        judged(
            &[
                (
                    "domain/a.py",
                    "def f(p, repo):\n    p.read_text()\n    p.exists()\n    p.exists(1)\n    repo.mkdir('x')\n    \
                     p.open('w')\n    p.open()\n    p.stat()\n    p.mkdir()\n",
                ),
                (place, source),
            ],
            "python",
            "none",
        )
    }

    #[test]
    fn a_method_a_module_judges_is_not_judged_again_by_its_name() {
        let source = "import os\nimport shutil\nos.mkdir('x')\nshutil.rmtree('y')\n";
        assert_eq!(
            says(source),
            ["2 shutil", "3 os.mkdir", "4 (rmtree) shutil"]
        );
    }

    #[test]
    fn the_free_layers_and_the_tests_do_all_of_it() {
        let source = "\
import os, sys, time, random, uuid, logging
print(os.getcwd(), sys.argv, time.time(), random.random(), uuid.uuid4())
logging.basicConfig()
p.read_text()
eval('1')
from time import *
";
        let found = judged(
            &[
                ("infrastructure/outbound/a.py", source),
                ("composition/b.py", source),
                ("__main__.py", source),
                ("tests/test_a.py", source),
            ],
            "python",
            "none",
        );
        assert!(found.is_empty(), "{found:?}");
        // And the screens of a project with a UI are judged in `presentation` and not in `ui`
        let found = judged(
            &[
                ("ui/pages/home.py", "import time\nx = time.time()\n"),
                ("presentation/models/m.py", "import time\nx = time.time()\n"),
            ],
            "python",
            "atomic",
        );
        assert_eq!(found, ["presentation/models/m.py:2 (time.time) clock"]);
    }
}
