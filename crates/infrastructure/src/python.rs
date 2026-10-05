//! Reading Python with Ruff's parser: what a file imports, as the modules it names, its comments, and the functions
//! and classes it defines.
//!
//! - **What a file imports:** every module an `import` or `from ... import` names, inside functions, classes and
//!   `if TYPE_CHECKING:` too. `from m import n` names `m.n`, which is the module `n` when there is one and `m`
//!   otherwise; both sit in the place of `m` unless `m.n` is a place itself (`from ui import atoms`). A relative import
//!   is resolved against the file's package, and one that climbs above the top package is left out: Python fails on it.
//!   Imports built at run time (`importlib.import_module`, `__import__`) are not seen.
//! - **Its comments,** `#` to the end of the line. A docstring is a string, so it is not a comment.
//! - **What it defines** ([`definitions`]), for a link to a `.py` file in the records.

use std::collections::BTreeSet;

use ruff_python_ast::statement_visitor::{StatementVisitor, walk_stmt};
use ruff_python_ast::token::TokenKind;
use ruff_python_ast::{PySourceType, Stmt};

use domain::code::{Source, module_parts};
use utils::text::line_of;

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
        imports: imports.found,
        comments,
        error,
    }
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
                let level = import.level as usize;
                let mut base: Vec<String> = if level == 0 {
                    Vec::new()
                } else if level - 1 < self.package.len() {
                    self.package[..self.package.len() - (level - 1)].to_vec()
                } else {
                    // Above the top package: Python fails on it, so it imports nothing
                    walk_stmt(self, stmt);
                    return;
                };
                if let Some(module) = &import.module {
                    base.extend(module.split('.').map(String::from));
                }
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

/// The names of the functions and classes a Python file defines, at any depth (methods, and definitions inside
/// functions, `if` and `try`). The file is parsed, so a `def` line inside a string or a docstring is not a definition.
/// A file with a syntax error is read past the error, as far as the parser recovers.
pub fn definitions(source: &str) -> BTreeSet<String> {
    #[derive(Default)]
    struct Definitions(BTreeSet<String>);
    impl<'a> StatementVisitor<'a> for Definitions {
        fn visit_stmt(&mut self, stmt: &'a Stmt) {
            match stmt {
                Stmt::FunctionDef(def) => {
                    self.0.insert(def.name.to_string());
                }
                Stmt::ClassDef(class) => {
                    self.0.insert(class.name.to_string());
                }
                _ => {}
            }
            walk_stmt(self, stmt);
        }
    }
    let module = ruff_python_parser::parse_unchecked_source(source, PySourceType::Python);
    let mut definitions = Definitions::default();
    definitions.visit_body(&module.syntax().body);
    definitions.0
}

/// The source text of the definition `dotted` names in a Python file: a function or class at the top, or one inside
/// a class or a function named before it (`Class.method`), from its first decorator to its end. `None` when the file
/// does not define it so.
pub fn definition(source: &str, dotted: &str) -> Option<String> {
    let module = ruff_python_parser::parse_unchecked_source(source, PySourceType::Python);
    let mut body: &[Stmt] = &module.syntax().body;
    let mut found = None;
    for name in dotted.split('.') {
        let (range, decorators, inner) = body.iter().find_map(|stmt| match stmt {
            Stmt::FunctionDef(def) if def.name.as_str() == name => {
                Some((def.range, &def.decorator_list, &def.body))
            }
            Stmt::ClassDef(class) if class.name.as_str() == name => {
                Some((class.range, &class.decorator_list, &class.body))
            }
            _ => None,
        })?;
        let start = decorators
            .iter()
            .map(|d| d.range.start())
            .chain([range.start()])
            .min()
            .expect("the definition's own start is there");
        found = Some((start.to_usize(), range.end().to_usize()));
        body = inner;
    }
    let (start, end) = found?;
    source.get(start..end).map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_definition_is_cut_from_its_first_decorator_to_its_end() {
        let source = "import x\n\n@first\n@second(1)\ndef f(a):\n    return a\n\n\nclass C:\n    \
                      @staticmethod\n    def m():\n        pass\n\n    def n(self):\n        pass\n";
        assert_eq!(
            definition(source, "f").as_deref(),
            Some("@first\n@second(1)\ndef f(a):\n    return a")
        );
        assert_eq!(
            definition(source, "C.m").as_deref(),
            Some("@staticmethod\n    def m():\n        pass")
        );
        assert!(
            definition(source, "C")
                .unwrap()
                .ends_with("def n(self):\n        pass")
        );
        for missing in ["g", "C.f", "f.a", "x", "C.m.z"] {
            assert_eq!(definition(source, missing), None, "{missing}");
        }
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
        let names = definitions("def broken(:\n    pass\n\ndef after():\n    pass\n");
        assert!(names.contains("after"), "{names:?}");
    }
}
