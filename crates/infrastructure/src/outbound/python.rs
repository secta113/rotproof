//! Reading Python with Ruff's parser: what a file imports, as the modules it names, its comments, and the functions
//! and classes it defines.
//!
//! - **What a file imports:** every module an `import` or `from ... import` names, inside functions, classes and
//!   `if TYPE_CHECKING:` too. `from m import n` names `m.n`, which is the module `n` when there is one and `m`
//!   otherwise; both sit in the place of `m` unless `m.n` is a place itself (`from ui import atoms`). A relative import
//!   is resolved against the file's package, and one that climbs above the top package is left out: Python fails on it.
//!   A call of `importlib.import_module` or `__import__` (`builtins.__import__`, or a name bound to either by `from
//!   importlib import import_module`) with a string literal as its module is an import of that module, as `import` of
//!   it is. A name that is not a literal (a variable, an f-string, a concatenation), and a relative one (`".x"`), is
//!   built at run time and is not seen.
//! - **Its comments,** `#` to the end of the line. A docstring is a string, so it is not a comment.
//! - **Its reads of the environment** (`domain::environment`): a name of `os` that touches it, imported or reached
//!   through a name bound to `os`.
//! - **What it defines** ([`definitions`]), for a link to a `.py` file in the records.
//! - **Its outline** ([`outline`]), every function and class with its names and its range, for a knowledge document
//!   that follows one.

use std::collections::BTreeSet;

use ruff_python_ast::statement_visitor::{StatementVisitor, walk_stmt};
use ruff_python_ast::token::TokenKind;
use ruff_python_ast::visitor::{Visitor, walk_expr};
use ruff_python_ast::{Expr, PySourceType, Stmt};

use domain::code::{Source, module_parts};
use domain::definitions::Definition;
use domain::environment::PYTHON as PYTHON_ENVIRONMENT;
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
    found.extend(dynamic(source, &parsed.syntax().body));
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
        environment: environment(source, &parsed.syntax().body),
        error,
    }
}

/// Every read or write of the environment in a file (`domain::environment`): a name of `os` in
/// [`domain::environment::PYTHON`], as `from os import environ` imports it, or as an attribute of a name the file
/// binds to `os` (`import os`, `import os as o`, `import os.path`). The line, and the name as `os.<name>`.
fn environment(source: &str, body: &[Stmt]) -> Vec<(usize, String)> {
    /// The names a file binds to the module `os`, and the names of `os` it imports
    struct Bindings<'s> {
        source: &'s str,
        os: BTreeSet<String>,
        found: Vec<(usize, String)>,
    }
    impl<'a> StatementVisitor<'a> for Bindings<'_> {
        fn visit_stmt(&mut self, stmt: &'a Stmt) {
            match stmt {
                Stmt::Import(import) => {
                    for alias in &import.names {
                        let name = alias.name.as_str();
                        match &alias.asname {
                            // `import os.path as p` binds `p` to `os.path`, not to `os`
                            Some(local) if name == "os" => {
                                self.os.insert(local.to_string());
                            }
                            None if name == "os" || name.starts_with("os.") => {
                                self.os.insert("os".into());
                            }
                            _ => {}
                        }
                    }
                }
                Stmt::ImportFrom(import)
                    if import.level == 0 && import.module.as_ref().is_some_and(|m| m == "os") =>
                {
                    for alias in &import.names {
                        if PYTHON_ENVIRONMENT.contains(&alias.name.as_str()) {
                            self.found.push((
                                line_of(self.source, alias.range.start().to_usize()),
                                format!("os.{}", alias.name),
                            ));
                        }
                    }
                }
                _ => {}
            }
            walk_stmt(self, stmt);
        }
    }
    /// The attributes of those names that read or write the environment
    struct Reads<'b, 's> {
        source: &'s str,
        os: &'b BTreeSet<String>,
        found: &'b mut Vec<(usize, String)>,
    }
    impl<'a> Visitor<'a> for Reads<'_, '_> {
        fn visit_expr(&mut self, expr: &'a Expr) {
            if let Expr::Attribute(attribute) = expr
                && let Expr::Name(name) = attribute.value.as_ref()
                && self.os.contains(name.id.as_str())
                && PYTHON_ENVIRONMENT.contains(&attribute.attr.as_str())
            {
                self.found.push((
                    line_of(self.source, attribute.range.start().to_usize()),
                    format!("os.{}", attribute.attr),
                ));
            }
            walk_expr(self, expr);
        }
    }
    let mut bindings = Bindings {
        source,
        os: BTreeSet::new(),
        found: Vec::new(),
    };
    bindings.visit_body(body);
    let mut found = bindings.found;
    if !bindings.os.is_empty() {
        let mut reads = Reads {
            source,
            os: &bindings.os,
            found: &mut found,
        };
        for stmt in body {
            reads.visit_stmt(stmt);
        }
    }
    found.sort();
    found
}

/// The modules a file imports by calling `importlib.import_module` or `__import__` with a string literal, each with the
/// line of the call. `import_module` is `importlib`'s, reached as an attribute of a name the file binds to `importlib`
/// (`import importlib`, `import importlib as il`, `import importlib.util`) or as a name imported from it; `__import__`
/// is a builtin, reached by its name, as an attribute of a name bound to `builtins`, or imported from it. The module is
/// the first argument, or `name=`, when it is one literal string (an implicit concatenation is one); a relative name
/// (`".x"`), which needs the `package` to resolve, and anything but a literal is built at run time and left out.
fn dynamic(source: &str, body: &[Stmt]) -> Vec<(usize, Vec<String>)> {
    /// The names a file binds to `importlib` and `builtins`, and to the two functions
    #[derive(Default)]
    struct Bindings {
        importlib: BTreeSet<String>,
        builtins: BTreeSet<String>,
        functions: BTreeSet<String>,
    }
    impl<'a> StatementVisitor<'a> for Bindings {
        fn visit_stmt(&mut self, stmt: &'a Stmt) {
            match stmt {
                Stmt::Import(import) => {
                    for alias in &import.names {
                        let name = alias.name.as_str();
                        let top = name.split('.').next().unwrap_or(name);
                        // `import a.b as c` binds `c` to `a.b`, and `import a.b` binds `a`
                        let (bound, module) = match &alias.asname {
                            Some(local) if name == "importlib" || name == "builtins" => {
                                (local.to_string(), name)
                            }
                            Some(_) => continue,
                            None => (top.to_string(), top),
                        };
                        match module {
                            "importlib" => self.importlib.insert(bound),
                            "builtins" => self.builtins.insert(bound),
                            _ => continue,
                        };
                    }
                }
                Stmt::ImportFrom(import) if import.level == 0 => {
                    let wanted = match import.module.as_ref().map(|m| m.as_str()) {
                        Some("importlib") => "import_module",
                        Some("builtins") => "__import__",
                        _ => "",
                    };
                    for alias in &import.names {
                        if alias.name.as_str() == wanted {
                            let local = alias.asname.as_ref().unwrap_or(&alias.name);
                            self.functions.insert(local.to_string());
                        }
                    }
                }
                _ => {}
            }
            walk_stmt(self, stmt);
        }
    }
    /// The calls of those, wherever an expression can be
    struct Calls<'b, 's> {
        source: &'s str,
        bindings: &'b Bindings,
        found: Vec<(usize, Vec<String>)>,
    }
    impl<'a> Visitor<'a> for Calls<'_, '_> {
        fn visit_expr(&mut self, expr: &'a Expr) {
            if let Expr::Call(call) = expr {
                let imports = match call.func.as_ref() {
                    Expr::Name(name) => {
                        name.id.as_str() == "__import__"
                            || self.bindings.functions.contains(name.id.as_str())
                    }
                    Expr::Attribute(attribute) => match attribute.value.as_ref() {
                        Expr::Name(name) => {
                            let (name, attr) = (name.id.as_str(), attribute.attr.as_str());
                            (attr == "import_module" && self.bindings.importlib.contains(name))
                                || (attr == "__import__" && self.bindings.builtins.contains(name))
                        }
                        _ => false,
                    },
                    _ => false,
                };
                if imports
                    && let Some(Expr::StringLiteral(module)) =
                        call.arguments.find_argument_value("name", 0)
                {
                    let module = module.value.to_str();
                    let parts: Vec<String> = module.split('.').map(String::from).collect();
                    if parts.iter().all(|part| !part.is_empty()) {
                        self.found.push((
                            line_of(self.source, call.arguments.range.start().to_usize()),
                            parts,
                        ));
                    }
                }
            }
            walk_expr(self, expr);
        }
    }
    let mut bindings = Bindings::default();
    bindings.visit_body(body);
    let mut calls = Calls {
        source,
        bindings: &bindings,
        found: Vec::new(),
    };
    for stmt in body {
        calls.visit_stmt(stmt);
    }
    calls.found
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
        let names = definitions("def broken(:\n    pass\n\ndef after():\n    pass\n");
        assert!(names.contains("after"), "{names:?}");
    }
}
