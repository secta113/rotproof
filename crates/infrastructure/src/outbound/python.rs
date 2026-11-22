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
//!   relative one (`".x"`), is built at run time and is not seen: the reader says so in `Source::unread`. How each
//!   entry was written is in `Source::forms` (`domain::code::Form`): the module and the name taken from it, none for
//!   an `import`, so that `from m import n` and `import m.n` are told apart.
//! - **Its comments,** `#` to the end of the line. A docstring is a string, so it is not a comment; its range is given
//!   apart (`Source::docstrings`), for the count of the lines of code, which leaves documentation out.
//! - **Its uses** (`uses.rs`, `domain::code::Use`): every name that resolves to something outside the file's own code,
//!   as Python's scopes resolve it, which the environment check, and the checks after it, judge.
//! - **Its outline** ([`outline`]), every function and class with its names and its range, for a knowledge document
//!   that follows one and for a link that names one, and the shape of a class at the top that is a vocabulary: bases
//!   from `enum` (with `str` or `int` beside them), and a body of literal or `auto()` members and dunder or sunder
//!   methods, for the rule that lets `protocol.inbound` read one.

mod uses;

use std::collections::HashMap;

use ruff_python_ast::statement_visitor::{StatementVisitor, walk_stmt};
use ruff_python_ast::token::TokenKind;
use ruff_python_ast::{
    ExceptHandler, Expr, Number, PySourceType, Stmt, StmtAssign, StmtClassDef, StmtFunctionDef,
    UnaryOp,
};

use domain::code::{Form, Source, module_parts};
use domain::definitions::{Definition, Kind, Shape};
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
    found.extend(read.dynamic.into_iter().map(|(line, module)| {
        let form = Form {
            module: module.clone(),
            name: None,
        };
        (line, module, form)
    }));
    // In the order of the lines, as the statements are read, with the calls among them
    found.sort_by_key(|(line, _, _)| *line);
    let (imports, forms): (Vec<_>, Vec<_>) = found
        .into_iter()
        .map(|(line, module, form)| ((line, module), form))
        .unzip();
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
        imports,
        forms,
        comments,
        docstrings: docstrings(&parsed.syntax().body),
        uses: read.uses,
        unread: read.unread,
        methods: read.methods,
        defined: read.defined,
        error,
    }
}

/// The byte range of every docstring in `body`: a string statement that stands first in the body of the module, of a
/// class or of a function, wherever the class or the function is defined.
fn docstrings(body: &[Stmt]) -> Vec<(usize, usize)> {
    let mut found = Docstrings { found: Vec::new() };
    found.first(body);
    found.visit_body(body);
    found.found
}

struct Docstrings {
    found: Vec<(usize, usize)>,
}

impl Docstrings {
    fn first(&mut self, body: &[Stmt]) {
        if let Some(Stmt::Expr(expr)) = body.first()
            && expr.value.is_string_literal_expr()
        {
            self.found
                .push((expr.range.start().to_usize(), expr.range.end().to_usize()));
        }
    }
}

impl<'a> StatementVisitor<'a> for Docstrings {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        match stmt {
            Stmt::FunctionDef(def) => self.first(&def.body),
            Stmt::ClassDef(class) => self.first(&class.body),
            _ => {}
        }
        walk_stmt(self, stmt);
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
    found: Vec<(usize, Vec<String>, Form)>,
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
                    let module: Vec<String> = alias.name.split('.').map(String::from).collect();
                    let form = Form {
                        module: module.clone(),
                        name: None,
                    };
                    self.found
                        .push((self.line(alias.range.start().to_usize()), module, form));
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
                    let form = Form {
                        module: base.clone(),
                        name: Some(alias.name.to_string()),
                    };
                    self.found
                        .push((self.line(alias.range.start().to_usize()), module, form));
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
/// outline, as a name after `::` leads through classes and functions alone. A class at the top that is a vocabulary
/// (see [`Enums::vocabulary`]) has the shape [`Shape::Vocabulary`]; every other definition is plain. A file with a
/// syntax error is read as far as the parser recovers.
pub fn outline(source: &str) -> Vec<Definition> {
    let module = ruff_python_parser::parse_unchecked_source(source, PySourceType::Python);
    let body = &module.syntax().body;
    let enums = Enums::of(body);
    let mut found = Vec::new();
    nested(body, &enums, &mut Vec::new(), &mut found);
    found
}

/// The definitions in `body`, under the names in `scope`, and in the body of each.
fn nested(body: &[Stmt], enums: &Enums, scope: &mut Vec<String>, found: &mut Vec<Definition>) {
    for stmt in body {
        let (name, range, decorators, inner, kind) = match stmt {
            Stmt::FunctionDef(def) => (
                &def.name,
                def.range,
                &def.decorator_list,
                &def.body,
                Kind::Function,
            ),
            Stmt::ClassDef(class) => (
                &class.name,
                class.range,
                &class.decorator_list,
                &class.body,
                Kind::Type,
            ),
            _ => continue,
        };
        let start = decorators
            .iter()
            .map(|d| d.range.start())
            .chain([range.start()])
            .min()
            .expect("the definition's own start is there");
        // A class inside another class or a function is no vocabulary: only the top of the module is read
        let shape = match stmt {
            Stmt::ClassDef(class) if scope.is_empty() && enums.vocabulary(class) => {
                Shape::Vocabulary
            }
            _ => Shape::Plain,
        };
        scope.push(name.to_string());
        found.push(Definition {
            names: scope.clone(),
            start: start.to_usize(),
            end: range.end().to_usize(),
            kind,
            shape,
        });
        nested(inner, enums, scope, found);
        scope.pop();
    }
}

/// What the top of a module binds, to tell a name of the module `enum` from any other thing of the same name. It is
/// read from the module's own statements and not from the scopes of `uses.rs`, as a vocabulary is a plain thing: a name
/// that is bound twice is not told apart, and is not taken for one of `enum`.
struct Enums {
    /// How many times each name is bound in the module outside any function or class: by an import, an assignment, a
    /// `def`, a `class`, a loop, and so on, in an `if` or a `try` too, as such a statement may rebind it
    bound: HashMap<String, usize>,
    /// The names that `import enum` or `import enum as e` binds
    modules: Vec<String>,
    /// The names that `from enum import Enum` or `from enum import Enum as E` binds, with the member they name
    members: HashMap<String, String>,
}

impl Enums {
    fn of(body: &[Stmt]) -> Self {
        let mut enums = Enums {
            bound: HashMap::new(),
            modules: Vec::new(),
            members: HashMap::new(),
        };
        enums.count(body);
        // Only the import statements that stand directly in the module's body: one in an `if` may not have run
        for stmt in body {
            match stmt {
                Stmt::Import(import) => {
                    for alias in &import.names {
                        match &alias.asname {
                            Some(name) if alias.name.as_str() == "enum" => {
                                enums.modules.push(name.to_string());
                            }
                            // `import enum.x` binds `enum`, the package, as `import enum` does
                            None if alias.name.split('.').next() == Some("enum") => {
                                enums.modules.push("enum".to_string());
                            }
                            _ => {}
                        }
                    }
                }
                Stmt::ImportFrom(import)
                    if import.level == 0 && import.module.as_deref() == Some("enum") =>
                {
                    for alias in &import.names {
                        let bound = alias.asname.as_ref().unwrap_or(&alias.name);
                        enums
                            .members
                            .insert(bound.to_string(), alias.name.to_string());
                    }
                }
                _ => {}
            }
        }
        enums
    }

    fn bind(&mut self, name: &str) {
        *self.bound.entry(name.to_string()).or_default() += 1;
    }

    /// Count what `body` binds, and what the statements in it that hold a body bind, but not what a function or a class
    /// binds inside: that is another scope.
    fn count(&mut self, body: &[Stmt]) {
        for stmt in body {
            match stmt {
                Stmt::Import(import) => {
                    for alias in &import.names {
                        match &alias.asname {
                            Some(name) => self.bind(name),
                            None => self.bind(alias.name.split('.').next().unwrap_or_default()),
                        }
                    }
                }
                Stmt::ImportFrom(import) => {
                    for alias in &import.names {
                        self.bind(alias.asname.as_ref().unwrap_or(&alias.name));
                    }
                }
                Stmt::FunctionDef(def) => self.bind(&def.name),
                Stmt::ClassDef(class) => self.bind(&class.name),
                Stmt::Assign(assign) => assign.targets.iter().for_each(|t| self.target(t)),
                Stmt::AnnAssign(assign) => self.target(&assign.target),
                Stmt::AugAssign(assign) => self.target(&assign.target),
                Stmt::For(stmt) => {
                    self.target(&stmt.target);
                    self.count(&stmt.body);
                    self.count(&stmt.orelse);
                }
                Stmt::While(stmt) => {
                    self.count(&stmt.body);
                    self.count(&stmt.orelse);
                }
                Stmt::If(stmt) => {
                    self.count(&stmt.body);
                    for clause in &stmt.elif_else_clauses {
                        self.count(&clause.body);
                    }
                }
                Stmt::With(stmt) => {
                    for item in &stmt.items {
                        if let Some(target) = &item.optional_vars {
                            self.target(target);
                        }
                    }
                    self.count(&stmt.body);
                }
                Stmt::Try(stmt) => {
                    self.count(&stmt.body);
                    for handler in &stmt.handlers {
                        let ExceptHandler::ExceptHandler(handler) = handler;
                        if let Some(name) = &handler.name {
                            self.bind(name);
                        }
                        self.count(&handler.body);
                    }
                    self.count(&stmt.orelse);
                    self.count(&stmt.finalbody);
                }
                Stmt::Match(stmt) => stmt.cases.iter().for_each(|case| self.count(&case.body)),
                Stmt::TypeAlias(alias) => self.target(&alias.name),
                _ => {}
            }
        }
    }

    fn target(&mut self, target: &Expr) {
        match target {
            Expr::Name(name) => self.bind(&name.id),
            Expr::Tuple(tuple) => tuple.elts.iter().for_each(|e| self.target(e)),
            Expr::List(list) => list.elts.iter().for_each(|e| self.target(e)),
            Expr::Starred(starred) => self.target(&starred.value),
            _ => {}
        }
    }

    fn bound_once(&self, name: &str) -> bool {
        self.bound.get(name) == Some(&1)
    }

    /// The member of `enum` that `expr` names, when it is a name or an attribute that resolves to one through the
    /// module's top-level imports: `Enum`, `enum.Enum`, `E` after `from enum import Enum as E`.
    fn member<'a>(&'a self, expr: &'a Expr) -> Option<&'a str> {
        match expr {
            Expr::Name(name) => {
                let name = name.id.as_str();
                if self.bound_once(name) {
                    self.members.get(name).map(String::as_str)
                } else {
                    None
                }
            }
            Expr::Attribute(attribute) => {
                let Expr::Name(module) = attribute.value.as_ref() else {
                    return None;
                };
                let module = module.id.as_str();
                (self.bound_once(module) && self.modules.iter().any(|m| m == module))
                    .then_some(attribute.attr.as_str())
            }
            _ => None,
        }
    }

    /// Whether `expr` is the builtin `name`: the module does not bind it itself.
    fn builtin(&self, expr: &Expr, name: &str) -> bool {
        matches!(expr, Expr::Name(n) if n.id.as_str() == name && !self.bound.contains_key(name))
    }

    /// Whether `expr` is the call `enum.auto()`, with no argument.
    fn auto(&self, expr: &Expr) -> bool {
        matches!(expr, Expr::Call(call)
            if call.arguments.args.is_empty()
                && call.arguments.keywords.is_empty()
                && self.member(&call.func) == Some("auto"))
    }

    /// Whether `class` is a vocabulary of values: a class that is an enum of the module `enum` (with `str` or `int`
    /// beside the enum) and holds nothing but its members, a docstring, and methods named `__x__` or `_x_`. The rule
    /// that reads it lets a vocabulary cross, as it holds no rule of the domain; a plain method may hold one, so a
    /// class with one is no vocabulary, nor is one whose metaclass, a decorator or a base could change what it is.
    fn vocabulary(&self, class: &StmtClassDef) -> bool {
        if class.type_params.is_some() {
            return false;
        }
        let Some(arguments) = &class.arguments else {
            return false;
        };
        if !arguments.keywords.is_empty() || arguments.args.is_empty() {
            return false;
        }
        let mut enum_base = false;
        for base in &arguments.args {
            if matches!(
                self.member(base),
                Some("Enum" | "StrEnum" | "IntEnum" | "Flag" | "IntFlag")
            ) {
                enum_base = true;
            } else if !(self.builtin(base, "str") || self.builtin(base, "int")) {
                return false;
            }
        }
        if !enum_base {
            return false;
        }
        let decorated = class
            .decorator_list
            .iter()
            .all(|decorator| match &decorator.expression {
                Expr::Call(call) => self.member(&call.func) == Some("verify"),
                expression => self.member(expression) == Some("unique"),
            });
        if !decorated {
            return false;
        }
        let mut members = 0;
        for (at, stmt) in class.body.iter().enumerate() {
            match stmt {
                Stmt::Expr(expr) if at == 0 && expr.value.is_string_literal_expr() => {}
                Stmt::Expr(expr) if matches!(*expr.value, Expr::EllipsisLiteral(_)) => {}
                Stmt::Pass(_) => {}
                Stmt::Assign(assign) if self.member_assignment(assign) => members += 1,
                Stmt::FunctionDef(def) if self.special_method(def) => {}
                _ => return false,
            }
        }
        members > 0
    }

    fn member_assignment(&self, assign: &StmtAssign) -> bool {
        let [Expr::Name(target)] = assign.targets.as_slice() else {
            return false;
        };
        let name = target.id.as_str();
        !is_dunder(name) && !is_sunder(name) && self.literal(&assign.value)
    }

    /// A string, an int or a float written out, a negative one, or `auto()`.
    fn literal(&self, value: &Expr) -> bool {
        let number = |expr: &Expr| matches!(expr, Expr::NumberLiteral(n) if matches!(n.value, Number::Int(_) | Number::Float(_)));
        value.is_string_literal_expr()
            || number(value)
            || matches!(value, Expr::UnaryOp(op) if op.op == UnaryOp::USub && number(&op.operand))
            || self.auto(value)
    }

    fn special_method(&self, def: &StmtFunctionDef) -> bool {
        (is_dunder(&def.name) || is_sunder(&def.name))
            && !def.is_async
            && def.decorator_list.iter().all(|decorator| {
                self.builtin(&decorator.expression, "classmethod")
                    || self.builtin(&decorator.expression, "staticmethod")
            })
    }
}

/// `__x__`, as Python's `enum` tells a dunder: two underscores each side, and the inside neither starts nor ends with
/// one.
fn is_dunder(name: &str) -> bool {
    name.len() > 4
        && name.starts_with("__")
        && name.ends_with("__")
        && !name[2..].starts_with('_')
        && !name[..name.len() - 2].ends_with('_')
}

/// `_x_`, as Python's `enum` tells a sunder: one underscore each side, and the inside neither starts nor ends with one.
fn is_sunder(name: &str) -> bool {
    name.len() > 2
        && name.starts_with('_')
        && name.ends_with('_')
        && !name[1..].starts_with('_')
        && !name[..name.len() - 1].ends_with('_')
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
    fn a_function_and_a_method_are_functions_and_a_class_is_a_type() {
        let source = r#"
X = 1

def f():
    pass

async def g():
    pass

class C:
    y = 1
    def m(self):
        pass
    class D:
        pass

class P(Protocol):
    def q(self): ...
"#;
        let kinds: Vec<(String, Kind)> = outline(source)
            .into_iter()
            .map(|d| (d.names.join("."), d.kind))
            .collect();
        assert_eq!(
            kinds,
            [
                ("f".to_string(), Kind::Function),
                ("g".to_string(), Kind::Function),
                ("C".to_string(), Kind::Type),
                ("C.m".to_string(), Kind::Function),
                ("C.D".to_string(), Kind::Type),
                ("P".to_string(), Kind::Type),
                ("P.q".to_string(), Kind::Function),
            ]
        );
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
    fn every_import_says_the_module_and_the_name_it_takes_and_an_import_takes_none() {
        let source = "\
import os, domain.song as song
from typing import TYPE_CHECKING
if TYPE_CHECKING:
    from application import play, load as l
def f():
    import infrastructure.db
    from domain.song import Kind as K
class C:
    from . import theme
from .. import organisms
from ..molecules.row import Row, Cell as C2
from ... import beyond
from ui import *
from .x import *
import importlib
importlib.import_module(\"a.b\")
try:
    from domain import (
        Dir,
        Level as L,
    )
except ImportError:
    import legacy as old
";
        let read = read(source, "ui/atoms/__init__.py");
        assert_eq!(read.error, None);
        // Each entry of the imports has its form, in the same order, and names the module and the name joined
        assert_eq!(read.forms.len(), read.imports.len());
        for ((_, module), form) in read.imports.iter().zip(&read.forms) {
            let mut named = form.module.clone();
            named.extend(form.name.iter().filter(|name| *name != "*").cloned());
            assert_eq!(*module, named);
        }
        let found: Vec<(usize, String, Option<String>)> = read
            .imports
            .iter()
            .zip(&read.forms)
            .map(|((line, _), form)| (*line, form.module.join("."), form.name.clone()))
            .collect();
        let expected = [
            (1, "os", None),
            (1, "domain.song", None),
            (2, "typing", Some("TYPE_CHECKING")),
            (4, "application", Some("play")),
            (4, "application", Some("load")),
            (6, "infrastructure.db", None),
            (7, "domain.song", Some("Kind")),
            (9, "ui.atoms", Some("theme")),
            (10, "ui", Some("organisms")),
            (11, "ui.molecules.row", Some("Row")),
            (11, "ui.molecules.row", Some("Cell")),
            // Line 12 climbs above the top package `ui`, which Python fails on: it imports nothing
            (13, "ui", Some("*")),
            (14, "ui.atoms.x", Some("*")),
            (15, "importlib", None),
            (16, "a.b", None),
            (19, "domain", Some("Dir")),
            (20, "domain", Some("Level")),
            (23, "legacy", None),
        ];
        let expected: Vec<(usize, String, Option<String>)> = expected
            .iter()
            .map(|(l, m, n)| (*l, m.to_string(), n.map(String::from)))
            .collect();
        assert_eq!(found, expected);
    }

    #[test]
    fn a_call_that_imports_a_module_by_a_literal_is_an_import_of_it_with_no_name() {
        let source = "\
from importlib import import_module as load
def f():
    load(\"billing.domain.x\")
";
        let read = read(source, "app/main.py");
        let found: Vec<(usize, String, Option<String>)> = read
            .forms
            .iter()
            .zip(&read.imports)
            .map(|(form, (line, _))| (*line, form.module.join("."), form.name.clone()))
            .collect();
        assert_eq!(
            found,
            [
                (
                    1,
                    "importlib".to_string(),
                    Some("import_module".to_string())
                ),
                (3, "billing.domain.x".to_string(), None),
            ]
        );
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
    fn a_docstring_is_a_string_standing_first_in_a_module_a_class_or_a_function_and_nothing_else() {
        let source = "\"\"\"Module.\"\"\"\nx = \"not\"\nclass C:\n    \"\"\"Class.\"\"\"\n    y = \"not\"\n    \
                      def m(self):\n        \"\"\"Method.\"\"\"\n        z = \"not\"\n        def inner():\n            \
                      \"inner\"\nif x:\n    def g():\n        \"\"\"G.\"\"\"\n    \"loose\"\n";
        let found: Vec<&str> = read(source, "x.py")
            .docstrings
            .into_iter()
            .map(|(from, to)| &source[from..to])
            .collect();
        assert_eq!(
            found,
            [
                "\"\"\"Module.\"\"\"",
                "\"\"\"Class.\"\"\"",
                "\"\"\"Method.\"\"\"",
                "\"inner\"",
                "\"\"\"G.\"\"\""
            ]
        );
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

    /// The shape of the definition of that name, which the file defines once.
    fn shape(source: &str, dotted: &str) -> Shape {
        let outline = outline(source);
        let mut named = outline.iter().filter(|d| d.names.join(".") == dotted);
        let definition = named
            .next()
            .unwrap_or_else(|| panic!("{dotted} is not defined in {source}"));
        assert!(named.next().is_none(), "{dotted} is defined twice");
        definition.shape.clone()
    }

    /// Whether the class `C` of the file is a vocabulary.
    fn vocabulary(source: &str) -> bool {
        shape(source, "C") == Shape::Vocabulary
    }

    #[test]
    fn a_class_of_members_with_an_enum_base_is_a_vocabulary_whichever_way_the_base_is_imported() {
        let cases = [
            ("from enum import Enum", "Enum"),
            ("import enum", "enum.Enum"),
            ("import enum as e", "e.Enum"),
            ("from enum import Enum as E", "E"),
            ("from enum import StrEnum", "StrEnum"),
            ("from enum import IntEnum", "IntEnum"),
            ("from enum import Flag", "Flag"),
            ("from enum import IntFlag", "IntFlag"),
            ("import enum", "enum.StrEnum"),
            ("import enum as e", "e.IntFlag"),
            ("from enum import Enum", "str, Enum"),
            ("from enum import Enum", "Enum, int"),
            ("import enum", "str, enum.Enum"),
            ("from enum import Enum as E", "str, E"),
            ("import enum.x", "enum.Enum"),
        ];
        for (import, bases) in cases {
            let source = format!("{import}\n\nclass C({bases}):\n    A = \"a\"\n    B = \"b\"\n");
            assert!(vocabulary(&source), "{import}: {bases}");
        }
    }

    #[test]
    fn a_member_is_a_string_a_number_a_negative_number_or_auto() {
        let source = r#"
import enum
from enum import Enum, auto

class C(Enum):
    A = "a"
    B = 1
    C = -2
    D = 1.5
    E = auto()
    F = enum.auto()
    G = 'one' 'two'
"#;
        assert!(vocabulary(source));
        let aliased = "import enum as e\nfrom enum import Enum, auto as a\n\nclass C(Enum):\n    A = a()\n    B = e.auto()\n";
        assert!(vocabulary(aliased));
    }

    #[test]
    fn a_docstring_pass_and_an_ellipsis_leave_a_vocabulary_one() {
        let source = r#"
from enum import Enum

class C(Enum):
    """The colours."""
    pass
    ...
    RED = "red"
"#;
        assert!(vocabulary(source));
    }

    #[test]
    fn dunder_and_sunder_methods_leave_a_vocabulary_one() {
        let source = r#"
from enum import Enum

class C(str, Enum):
    RED = "red"

    def __str__(self):
        return self.value

    @classmethod
    def _missing_(cls, value):
        return cls.RED

    @staticmethod
    def _generate_next_value_(name, start, count, last_values):
        return name

    def __init_subclass__(cls, **kwargs):
        pass
"#;
        assert!(vocabulary(source));
    }

    #[test]
    fn unique_and_verify_leave_a_vocabulary_one() {
        let cases = [
            ("from enum import Enum, unique", "@unique"),
            ("import enum\nfrom enum import Enum", "@enum.unique"),
            ("import enum as e\nfrom enum import Enum", "@e.unique"),
            ("from enum import Enum, verify, UNIQUE", "@verify(UNIQUE)"),
            (
                "import enum\nfrom enum import Enum",
                "@enum.verify(enum.UNIQUE)",
            ),
            ("from enum import Enum, unique", "@unique\n@unique"),
        ];
        for (import, decorator) in cases {
            let source = format!("{import}\n\n{decorator}\nclass C(Enum):\n    A = 1\n");
            assert!(vocabulary(&source), "{decorator}");
        }
    }

    #[test]
    fn only_a_class_at_the_top_has_a_shape_and_every_other_definition_is_plain() {
        let source = r#"
from enum import Enum

class Colour(Enum):
    RED = "red"

class Outer:
    class Inner(Enum):
        A = 1

def f():
    class Local(Enum):
        A = 1

def g(): ...
X = 1
"#;
        assert_eq!(shape(source, "Colour"), Shape::Vocabulary);
        for plain in ["Outer", "Outer.Inner", "f", "f.Local", "g"] {
            assert_eq!(shape(source, plain), Shape::Plain, "{plain}");
        }
        assert!(
            outline(source)
                .iter()
                .all(|d| d.shape == Shape::Plain || d.names == ["Colour"])
        );
    }

    #[test]
    fn a_class_with_a_plain_method_is_not_a_vocabulary() {
        let source = "from enum import Enum\n\nclass C(Enum):\n    A = 1\n\n    def label(self):\n        return 1\n";
        assert!(!vocabulary(source));
        let property = "from enum import Enum\n\nclass C(Enum):\n    A = 1\n\n    @property\n    def __x__(self):\n        return 1\n";
        assert!(!vocabulary(property));
        let asynchronous = "from enum import Enum\n\nclass C(Enum):\n    A = 1\n\n    async def __x__(self):\n        pass\n";
        assert!(!vocabulary(asynchronous));
        let shadowed = "from enum import Enum\nclassmethod = print\n\nclass C(Enum):\n    A = 1\n\n    @classmethod\n    def _x_(cls):\n        pass\n";
        assert!(!vocabulary(shadowed));
    }

    #[test]
    fn a_member_that_is_not_a_plain_name_bound_to_a_literal_is_not_a_member() {
        let bodies = [
            "_ignore_ = 1",
            "__order__ = 1",
            "A: int = 1",
            "A = f\"a{1}\"",
            "A = b\"a\"",
            "A = 1j",
            "A = None",
            "A = True",
            "A = -\"a\"",
            "A = +1",
            "A = 1 + 1",
            "A = auto(1)",
            "A = auto",
            "A = (1, 2)",
            "A = B = 1",
            "A, B = 1, 2",
            "A += 1",
            "class N:\n        pass",
            "if True:\n        A = 1",
            "import os",
            "A = other.auto()",
        ];
        for body in bodies {
            let source = format!(
                "from enum import Enum, auto\nimport other\n\nclass C(Enum):\n    Z = 1\n    {body}\n"
            );
            assert!(!vocabulary(&source), "{body}");
        }
    }

    #[test]
    fn a_docstring_that_is_not_first_is_not_one() {
        let source = "from enum import Enum\n\nclass C(Enum):\n    A = 1\n    \"later\"\n";
        assert!(!vocabulary(source));
    }

    #[test]
    fn an_enum_that_is_empty_or_holds_no_member_is_not_a_vocabulary() {
        for body in [
            "pass",
            "\"\"\"Doc.\"\"\"",
            "...",
            "def __str__(self):\n        return \"\"",
        ] {
            let source = format!("from enum import Enum\n\nclass C(Enum):\n    {body}\n");
            assert!(!vocabulary(&source), "{body}");
        }
    }

    #[test]
    fn a_class_whose_bases_are_not_all_known_is_not_a_vocabulary() {
        let cases = [
            ("from enum import Enum", "Enum, metaclass=Meta"),
            ("from enum import Enum", "Enum, boundary=STRICT"),
            (
                "from enum import Enum\nfrom project import Base",
                "Base, Enum",
            ),
            ("from enum import Enum\nfrom project import Base", "Base"),
            ("from enum import Enum", "str"),
            ("from enum import Enum", "int, str"),
            ("from enum import Enum", "float, Enum"),
            ("from enum import Enum", "Enum, object"),
            ("from enum import Enum", "Enum[int]"),
            ("from enum import Enum", "enum.Enum"),
            ("import enum", "Enum"),
            ("import enum", "enum.EnumType"),
            ("import enum", "enum.auto"),
            ("import enum", "enum.Enum.A"),
            ("import enum", "other.Enum"),
            ("from enum import Enum as E", "Enum"),
            ("from enum import auto", "auto"),
            ("from other import Enum", "Enum"),
            ("from .enum import Enum", "Enum"),
            ("from enum.sub import Enum", "Enum"),
            ("import other as enum", "enum.Enum"),
            ("import enum.sub as e", "e.Enum"),
        ];
        for (import, bases) in cases {
            let source = format!("{import}\n\nclass C({bases}):\n    A = 1\n");
            assert!(!vocabulary(&source), "{import}: {bases}");
        }
        let no_bases = "class C:\n    A = 1\n";
        assert!(!vocabulary(no_bases));
        let empty_call = "from enum import Enum\n\nclass C():\n    A = 1\n";
        assert!(!vocabulary(empty_call));
        let generic = "from enum import Enum\n\nclass C[T](Enum):\n    A = 1\n";
        assert!(!vocabulary(generic));
    }

    #[test]
    fn an_alias_made_by_assignment_is_not_followed() {
        let source = "import enum\nE = enum.Enum\n\nclass C(E):\n    A = 1\n";
        assert!(!vocabulary(source));
        let member = "import enum\nfrom enum import Enum\nauto = enum.auto\n\nclass C(Enum):\n    A = auto()\n";
        assert!(!vocabulary(member));
    }

    #[test]
    fn a_name_bound_more_than_once_at_the_top_is_not_resolved() {
        let cases = [
            "from enum import Enum\nfrom enum import Enum\n",
            "from enum import Enum\nfrom other import Enum\n",
            "from enum import Enum\nEnum = 1\n",
            "from enum import Enum\ndef Enum(): ...\n",
            "from enum import Enum\nclass Enum: ...\n",
            "from enum import Enum\nfor Enum in []: ...\n",
            "from enum import Enum\nwith open('f') as Enum: ...\n",
            "from enum import Enum\ntry:\n    pass\nexcept Exception as Enum:\n    pass\n",
            "from enum import Enum\nEnum: int = 1\n",
            "from enum import Enum\n(Enum, other) = 1, 2\n",
            "from enum import Enum\nif other:\n    Enum = 1\n",
            "from enum import Enum\nif other:\n    from other import Enum\n",
            "import enum\nimport enum\n",
            "import enum as e\nimport other as e\n",
        ];
        for head in cases {
            let base = if head.starts_with("import enum") && !head.contains("as e") {
                "enum.Enum"
            } else if head.contains("as e") {
                "e.Enum"
            } else {
                "Enum"
            };
            let source = format!("{head}\nclass C({base}):\n    A = 1\n");
            assert!(!vocabulary(&source), "{head}");
        }
    }

    #[test]
    fn a_name_bound_inside_a_function_or_a_class_does_not_count_against_the_top() {
        let source = r#"
from enum import Enum

def f():
    Enum = 1

class Other:
    Enum = 2

class C(Enum):
    A = 1
"#;
        assert!(vocabulary(source));
    }

    #[test]
    fn a_str_or_int_that_the_module_rebinds_is_not_the_builtin() {
        let cases = [
            "from enum import Enum\nfrom other import str\n",
            "from enum import Enum\nstr = 1\n",
            "from enum import Enum\nclass str: ...\n",
            "from enum import Enum\ndef str(): ...\n",
            "from enum import Enum\nimport str\n",
        ];
        for head in cases {
            let source = format!("{head}\nclass C(str, Enum):\n    A = 1\n");
            assert!(!vocabulary(&source), "{head}");
        }
        let int = "from enum import Enum\nint = 1\n\nclass C(int, Enum):\n    A = 1\n";
        assert!(!vocabulary(int));
        // A rebound `str` does no harm to a class that does not name it
        let unused = "from enum import Enum\nstr = 1\n\nclass C(Enum):\n    A = 1\n";
        assert!(vocabulary(unused));
    }

    #[test]
    fn an_enum_import_that_is_not_a_statement_of_the_module_body_is_not_used() {
        let cases = [
            "if True:\n    from enum import Enum\n",
            "try:\n    from enum import Enum\nexcept ImportError:\n    pass\n",
            "def f():\n    from enum import Enum\n",
        ];
        for head in cases {
            let source = format!("{head}\nclass C(Enum):\n    A = 1\n");
            assert!(!vocabulary(&source), "{head}");
        }
        let module = "if True:\n    import enum\n\nclass C(enum.Enum):\n    A = 1\n";
        assert!(!vocabulary(module));
        let auto = "from enum import Enum\nif True:\n    from enum import auto\n\nclass C(Enum):\n    A = auto()\n";
        assert!(!vocabulary(auto));
    }

    #[test]
    fn a_decorator_other_than_unique_or_verify_is_not_allowed() {
        let cases = [
            "@dataclass",
            "@unique()",
            "@verify",
            "@enum.verify",
            "@other.unique",
            "@decorate(1)",
            "@enum.unique\n@dataclass",
            "@enum.unique.x",
        ];
        for decorator in cases {
            let source = format!(
                "import enum\nimport other\nfrom enum import Enum, unique, verify\nfrom dataclasses import dataclass\n\n{decorator}\nclass C(Enum):\n    A = 1\n"
            );
            assert!(!vocabulary(&source), "{decorator}");
        }
        let rebound =
            "from enum import Enum, unique\nunique = print\n\n@unique\nclass C(Enum):\n    A = 1\n";
        assert!(!vocabulary(rebound));
    }

    #[test]
    fn a_syntax_error_below_a_vocabulary_leaves_its_shape() {
        let source =
            "from enum import Enum\n\nclass C(Enum):\n    A = 1\n\ndef broken(:\n    pass\n";
        assert!(vocabulary(source));
    }
}
