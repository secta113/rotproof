//! Reading Python with Ruff's parser: what a file imports, as the modules it names, its comments, the names it uses,
//! and the functions and classes it defines.
//!
//! - **What a file imports:** every module an `import` or `from ... import` names, inside functions, classes and
//!   `if TYPE_CHECKING:` too. `from m import n` names `m.n`, which is the module `n` when there is one and `m`
//!   otherwise; both sit in the place of `m` unless `m.n` is a place itself (`from ui import atoms`). A relative import
//!   is resolved against the file's package, and one that climbs above the top package is left out: Python fails on it.
//!   A call that resolves to `importlib.import_module` or to the builtin `__import__` (`builtins.__import__`, or a name
//!   bound to either by `from importlib import import_module`) with a string literal as its module is an import of
//!   that module, as `import` of it is. A name that is not a literal (a variable, an f-string, a concatenation), and a
//!   relative one (`".x"`), is built at run time and is not seen: the reader says so in `Source::unread`.
//! - **Its comments,** `#` to the end of the line. A docstring is a string, so it is not a comment.
//! - **Its uses** (`uses.rs`, `domain::code::Use`): every name that resolves to something outside the file's own code,
//!   as Python's scopes resolve it, which the environment check, and the checks after it, judge.
//! - **Its outline** ([`outline`]), every function and class with its names and its range, for a knowledge document
//!   that follows one and for a link that names one.

mod uses;

use ruff_python_ast::statement_visitor::{StatementVisitor, walk_stmt};
use ruff_python_ast::token::TokenKind;
use ruff_python_ast::{PySourceType, Stmt};

use domain::code::{Source, module_parts};
use domain::definitions::Definition;
use domain::text::line_of;

/// Read one file, whose path from the root names the package its relative imports start from.
pub fn read(source: &str, path: &str) -> Source<Vec<String>> {
    let parsed = ruff_python_parser::parse_unchecked_source(source, PySourceType::Python);
    let package = package(path);
    let mut imports = Imports {
        source,
        package: &package,
        found: Vec::new(),
    };
    imports.visit_body(&parsed.syntax().body);
    let mut found = imports.found;
    let read = uses::read(source, &parsed.syntax().body, &package);
    found.extend(read.dynamic);
    // In the order of the lines, as the statements are read, with the calls among them
    found.sort_by_key(|(line, _)| *line);
    let comments = parsed
        .tokens()
        .iter()
        .map(|token| token.as_tuple())
        .filter(|(kind, _)| *kind == TokenKind::Comment)
        .map(|(_, range)| {
            let (start, end) = (range.start().to_usize(), range.end().to_usize());
            (start, source[start..end].to_string())
        })
        .collect();
    let error = parsed.errors().first().map(|e| {
        (
            line_of(source, e.location.start().to_usize()),
            e.error.to_string(),
        )
    });
    Source {
        imports: found,
        comments,
        uses: read.uses,
        unread: read.unread,
        error,
    }
}

/// The package a relative import of `level` dots starts from, followed by the module it names: `None` when it climbs
/// above the top package, which Python fails on.
fn base_of(package: &[String], level: u32, module: Option<&str>) -> Option<Vec<String>> {
    let level = level as usize;
    let mut base: Vec<String> = if level == 0 {
        Vec::new()
    } else if level - 1 < package.len() {
        package[..package.len() - (level - 1)].to_vec()
    } else {
        return None;
    };
    if let Some(module) = module {
        base.extend(module.split('.').map(String::from));
    }
    Some(base)
}

struct Imports<'s> {
    source: &'s str,
    package: &'s [String],
    found: Vec<(usize, Vec<String>)>,
}

impl Imports<'_> {
    fn line(&self, at: usize) -> usize {
        line_of(self.source, at)
    }
}

impl<'a> StatementVisitor<'a> for Imports<'_> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        match stmt {
            Stmt::Import(import) => {
                for alias in &import.names {
                    let module = alias.name.split('.').map(String::from).collect();
                    self.found
                        .push((self.line(alias.range.start().to_usize()), module));
                }
            }
            Stmt::ImportFrom(import) => {
                let Some(base) = base_of(self.package, import.level, import.module.as_deref())
                else {
                    // Above the top package: Python fails on it, so it imports nothing
                    walk_stmt(self, stmt);
                    return;
                };
                for alias in &import.names {
                    let mut module = base.clone();
                    if alias.name.as_str() != "*" {
                        module.push(alias.name.to_string());
                    }
                    self.found
                        .push((self.line(alias.range.start().to_usize()), module));
                }
            }
            _ => {}
        }
        walk_stmt(self, stmt);
    }
}

/// The package a file's relative imports start from: the module itself for `__init__.py`, its parent otherwise.
fn package(path: &str) -> Vec<String> {
    let mut parts = module_parts(path);
    let is_init = path
        .rsplit('/')
        .next()
        .is_some_and(|name| name.eq_ignore_ascii_case("__init__.py"));
    if !is_init {
        parts.pop();
    }
    parts
}

/// The outline of a Python file: every function and class at the top, and inside a class or a function (a method, a
/// function in a function), each with its names from the outermost and its range from its first decorator to its end,
/// for a knowledge document that follows it (`follows.rs` in `domain`). What sits in an `if` or a `try` is not in the
/// outline, as a name after `::` leads through classes and functions alone. A file with a syntax error is read as far
/// as the parser recovers.
pub fn outline(source: &str) -> Vec<Definition> {
    let module = ruff_python_parser::parse_unchecked_source(source, PySourceType::Python);
    let mut found = Vec::new();
    nested(&module.syntax().body, &mut Vec::new(), &mut found);
    found
}

/// The definitions in `body`, under the names in `scope`, and in the body of each.
fn nested(body: &[Stmt], scope: &mut Vec<String>, found: &mut Vec<Definition>) {
    for stmt in body {
        let (name, range, decorators, inner) = match stmt {
            Stmt::FunctionDef(def) => (&def.name, def.range, &def.decorator_list, &def.body),
            Stmt::ClassDef(class) => (&class.name, class.range, &class.decorator_list, &class.body),
            _ => continue,
        };
        let start = decorators
            .iter()
            .map(|d| d.range.start())
            .chain([range.start()])
            .min()
            .expect("the definition's own start is there");
        scope.push(name.to_string());
        found.push(Definition {
            names: scope.clone(),
            start: start.to_usize(),
            end: range.end().to_usize(),
        });
        nested(inner, scope, found);
        scope.pop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::definitions::pieces;

    /// The source of every definition of that name in a file, as a key hashes it.
    fn found(source: &str, dotted: &str) -> Vec<String> {
        pieces(source, &outline(source), dotted)
            .into_iter()
            .map(str::to_string)
            .collect()
    }

    #[test]
    fn a_definition_is_cut_from_its_first_decorator_to_its_end() {
        let source = "import x\n\n@first\n@second(1)\ndef f(a):\n    return a\n\n\nclass C:\n    \
                      @staticmethod\n    def m():\n        pass\n\n    def n(self):\n        pass\n";
        assert_eq!(
            found(source, "f"),
            ["@first\n@second(1)\ndef f(a):\n    return a"]
        );
        assert_eq!(
            found(source, "C.m"),
            ["@staticmethod\n    def m():\n        pass"]
        );
        let class = found(source, "C");
        assert_eq!(class.len(), 1);
        assert!(class[0].ends_with("def n(self):\n        pass"));
        for missing in ["g", "C.f", "f.a", "x", "C.m.z"] {
            assert_eq!(found(source, missing), Vec::<String>::new(), "{missing}");
        }
    }

    #[test]
    fn the_outline_names_each_definition_from_the_outermost() {
        let source = "def f():\n    def inner():\n        pass\n\nclass C:\n    class D:\n        def m(self):\n            \
                      pass\n\nif True:\n    def hidden():\n        pass\n";
        let names: Vec<Vec<String>> = outline(source).into_iter().map(|d| d.names).collect();
        assert_eq!(
            names,
            [
                vec!["f"],
                vec!["f", "inner"],
                vec!["C"],
                vec!["C", "D"],
                vec!["C", "D", "m"],
            ]
        );
    }

    #[test]
    fn a_name_defined_more_than_once_gives_every_piece_in_the_order_they_appear() {
        let source = "class C:\n    @property\n    def x(self):\n        return 1\n\n    @x.setter\n    \
                      def x(self, v):\n        pass\n\n\ndef f():\n    return 1\n\n\ndef f():\n    return 2\n\n\n\
                      class C:\n    def x(self):\n        return 3\n";
        assert_eq!(
            found(source, "C.x"),
            [
                "@property\n    def x(self):\n        return 1",
                "@x.setter\n    def x(self, v):\n        pass",
                "def x(self):\n        return 3",
            ]
        );
        assert_eq!(
            found(source, "f"),
            ["def f():\n    return 1", "def f():\n    return 2"]
        );
        // A class opened twice gives both of its pieces
        let classes = found(source, "C");
        assert_eq!(classes.len(), 2, "{classes:?}");
        assert!(classes[0].starts_with("class C:\n    @property"));
        assert_eq!(classes[1], "class C:\n    def x(self):\n        return 3");
    }

    fn parts(dotted: &str) -> Vec<String> {
        dotted.split('.').map(String::from).collect()
    }

    #[test]
    fn a_module_is_named_from_its_path() {
        assert_eq!(module_parts("ui/pages/home.py"), parts("ui.pages.home"));
        assert_eq!(module_parts("ui/pages/__init__.py"), parts("ui.pages"));
        assert_eq!(module_parts("domain/Stray.PY"), parts("domain.Stray"));
        assert_eq!(package("ui/pages/home.py"), parts("ui.pages"));
        assert_eq!(package("ui/pages/__init__.py"), parts("ui.pages"));
        assert_eq!(package("domain/__INIT__.PY"), parts("domain"));
    }

    #[test]
    fn every_form_of_import_is_read() {
        let source = "\
import os, domain.song as song
from typing import TYPE_CHECKING
if TYPE_CHECKING:
    from application import play
def f():
    import infrastructure.db
class C:
    from . import theme
from .. import organisms
from ..molecules.row import Row
from ... import beyond
from .... import further
from ui import *
";
        let read = read(source, "ui/atoms/__init__.py");
        assert_eq!(read.error, None);
        let found: Vec<(usize, String)> = read
            .imports
            .into_iter()
            .map(|(l, m)| (l, m.join(".")))
            .collect();
        let expected = [
            (1, "os"),
            (1, "domain.song"),
            (2, "typing.TYPE_CHECKING"),
            (4, "application.play"),
            (6, "infrastructure.db"),
            (8, "ui.atoms.theme"),
            (9, "ui.organisms"),
            (10, "ui.molecules.row.Row"),
            // Lines 11 and 12 climb above the top package `ui`, which Python fails on: they import nothing
            (13, "ui"),
        ];
        let expected: Vec<(usize, String)> =
            expected.iter().map(|(l, m)| (*l, m.to_string())).collect();
        assert_eq!(found, expected);
    }

    #[test]
    fn a_call_that_imports_a_module_by_a_literal_is_an_import_of_it() {
        let source = "\
import importlib
import importlib.util as util
import builtins as b
from importlib import import_module as load
def f():
    importlib.import_module(\"billing.domain.x\")
    __import__(\"billing.domain.y\")
    b.__import__(name=\"billing.domain.z\")
    return load(\"shipping.\" \"domain\")
class C:
    m = [importlib.import_module(\"one\") for _ in ()]
";
        let found: Vec<(usize, String)> = read(source, "app/main.py")
            .imports
            .into_iter()
            .map(|(l, m)| (l, m.join(".")))
            .collect();
        let expected = [
            (1, "importlib"),
            (2, "importlib.util"),
            (3, "builtins"),
            (4, "importlib.import_module"),
            (6, "billing.domain.x"),
            (7, "billing.domain.y"),
            (8, "billing.domain.z"),
            (9, "shipping.domain"),
            (11, "one"),
        ];
        let expected: Vec<(usize, String)> =
            expected.iter().map(|(l, m)| (*l, m.to_string())).collect();
        assert_eq!(found, expected);
    }

    #[test]
    fn a_call_whose_module_is_built_at_run_time_or_is_not_importlibs_is_not_an_import() {
        let source = "\
import importlib
import mylib
name = \"billing.domain.x\"
importlib.import_module(name)
importlib.import_module(f\"billing.{name}\")
importlib.import_module(\"billing.\" + name)
importlib.import_module(\".x\", package=\"billing\")
importlib.import_module()
__import__(*[\"billing\"])
mylib.import_module(\"billing.domain.x\")
import_module(\"billing.domain.x\")
other.__import__(\"billing.domain.x\")
";
        let found: Vec<String> = read(source, "app/main.py")
            .imports
            .into_iter()
            .map(|(_, m)| m.join("."))
            .collect();
        assert_eq!(found, ["importlib", "mylib"]);
    }

    #[test]
    fn a_syntax_error_is_named_with_its_line() {
        let read = read("import domain\ndef (:\nimport utils\n", "ui/x.py");
        assert_eq!(read.imports.first().map(|(l, _)| *l), Some(1));
        assert_eq!(read.error.map(|(line, _)| line), Some(2));
    }

    #[test]
    fn comments_are_read_and_docstrings_are_not() {
        let source = "\"\"\"# not a comment\"\"\"\nx = 1  # one\n# two\n";
        let read = read(source, "x.py");
        let texts: Vec<&str> = read.comments.iter().map(|(_, t)| t.as_str()).collect();
        assert_eq!(texts, ["# one", "# two"]);
        assert_eq!(read.comments[0].0, source.find("# one").unwrap());
    }

    #[test]
    fn definitions_after_a_syntax_error_are_read() {
        // A half-written file still names what it defines below the error
        let source = "def broken(:\n    pass\n\ndef after():\n    pass\n";
        assert_eq!(found(source, "after"), ["def after():\n    pass"]);
    }
}
