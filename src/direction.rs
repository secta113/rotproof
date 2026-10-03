//! The direction check: every layer imports only what the table (`layers/table.toml`) allows.
//!
//! - An import from layer A of layer B fails unless B is in A's `imports`. Inside `ui`, a level may import the levels
//!   below it and the layers in its own `imports`, and not `ui` beside its levels. A place may always import itself,
//!   and a layer the levels inside it.
//! - Only direct imports are judged. What the table allows is closed under chaining (a test keeps it so), so a chain
//!   of allowed imports never reaches what its first module may not import.
//! - An import of a module in no layer present (the standard library, a third-party package, a layer declared absent)
//!   is not judged. Imports built at run time (`importlib.import_module`, `__import__`) are not seen.
//!
//! For `python`, a module is named by its dotted name, and sits in the place its parts start with. For `typescript`,
//! an import is resolved to a path (`typescript.rs`), and sits in the place that path starts with. For `rust`, the
//! check says that it did not run.

use std::io;
use std::path::Path;

use ruff_python_ast::statement_visitor::{StatementVisitor, walk_stmt};
use ruff_python_ast::{PySourceType, Stmt};

use crate::layers::{Declared, Entry, Layout, Place, Table, listed, table};
use crate::source::{code_files, line_of, read_code};

/// What the direction check found.
#[derive(Debug, Default)]
pub struct Direction {
    /// Every import a layer may not make, and every file that could not be read
    pub found: Vec<String>,
    /// What was not checked, and why
    pub skipped: Option<String>,
}

/// Every import in the layers of `declared` that the table does not allow. An error is a directory that could not be
/// walked.
pub fn problems(root: &Path, declared: &Declared) -> io::Result<Direction> {
    let Some(layout) = &declared.layout else {
        // A repository of records only: the structure check already says that the layers were not checked
        return Ok(Direction::default());
    };
    // Only the places that are there: an import of a layer declared absent names a module the project does not have
    let places: Vec<&Place> = declared
        .places
        .iter()
        .filter(|p| !declared.is_absent(p) && crate::source::exactly(root, &p.path).is_ok())
        .collect();
    match declared.declaration.stack.as_str() {
        "python" => python(root, layout, &places),
        "typescript" => typescript(root, layout, &places),
        stack => Ok(Direction {
            found: Vec::new(),
            skipped: Some(format!(
                "the direction of imports is not checked: Rotproof does not read the imports of a {stack} project yet"
            )),
        }),
    }
}

/// The finding for an import from `from` that lands in `to`, or `None` when the table allows it.
fn judged(table: &Table, at: &str, what: &str, from: &Place, to: &Place) -> Option<String> {
    (!allowed(table, from, to)).then(|| {
        format!(
            "{at}: imports {what}, {}; {}",
            described(from, to),
            what_it_may_import(table, from)
        )
    })
}

/// The direction check of a TypeScript project: every import of a source file in a layer, resolved to a path
/// (`typescript.rs`), judged by the place that path sits in.
fn typescript(root: &Path, layout: &Layout, places: &[&Place]) -> io::Result<Direction> {
    let table = table();
    let aliases = crate::typescript::aliases(root)?;
    let mut found: Vec<String> = aliases
        .problems
        .iter()
        .map(|why| format!("{why}; imports through it are not checked"))
        .collect();
    let parts = |path: &str| -> Vec<String> { path.split('/').map(String::from).collect() };
    // Every source file of the layers with the place it sits in, and each file a starter puts outside the layers with
    // the layer it belongs to, while that layer is there
    let mut files: Vec<(String, &Place)> = Vec::new();
    for layer in places.iter().filter(|p| p.parent.is_none()) {
        for path in code_files(root, &layer.path, |name| layout.is_code(name))? {
            if let Some(from) = place_of(&parts(&path), places) {
                files.push((path, from));
            }
        }
    }
    for (path, layer) in &layout.belongs {
        let place = places.iter().find(|p| &p.name == layer);
        if let Some(place) = place
            && crate::source::exactly(root, path).is_ok_and(|full| full.is_file())
        {
            files.push((path.clone(), place));
        }
    }
    for (path, from) in files {
        if !crate::typescript::is_source(&path) {
            continue;
        }
        let Some(source) = read_code(root, &path, "imports", &mut found)? else {
            continue;
        };
        let read = crate::typescript::read(&source, &path);
        if let Some((line, why)) = read.error {
            found.push(format!(
                "{path}:{line}: cannot be read as TypeScript ({why}), so the imports after it may be misread"
            ));
        }
        for (line, specifier) in read.imports {
            let Some(target) = aliases.resolve(root, &path, &specifier) else {
                continue;
            };
            let Some(to) = place_of(&parts(&target), places) else {
                continue;
            };
            found.extend(judged(
                &table,
                &format!("{path}:{line}"),
                &format!("{specifier} ({target})"),
                from,
                to,
            ));
        }
    }
    Ok(Direction {
        found,
        skipped: None,
    })
}

/// The direction check of a Python project: every import of a `.py` file in a layer, as the module it names.
fn python(root: &Path, layout: &Layout, places: &[&Place]) -> io::Result<Direction> {
    let table = table();
    let mut found = Vec::new();
    for layer in places.iter().filter(|p| p.parent.is_none()) {
        for path in code_files(root, &layer.path, |name| layout.is_code(name))? {
            let Some(from) = place_of(&module_parts(&path), places) else {
                continue;
            };
            let Some(source) = read_code(root, &path, "imports", &mut found)? else {
                continue;
            };
            let (imports, error) = imports(&source, &package(&path));
            if let Some((line, why)) = error {
                found.push(format!(
                    "{path}:{line}: cannot be read as Python ({why}), so the imports after it are not checked"
                ));
            }
            for (line, module) in imports {
                let Some(to) = place_of(&module, places) else {
                    continue;
                };
                found.extend(judged(
                    &table,
                    &format!("{path}:{line}"),
                    &module.join("."),
                    from,
                    to,
                ));
            }
        }
    }
    Ok(Direction {
        found,
        skipped: None,
    })
}

/// The module a file is, as parts: `ui/pages/home.py` is `ui.pages.home`, and `ui/pages/__init__.py` is `ui.pages`.
fn module_parts(path: &str) -> Vec<String> {
    let mut parts: Vec<String> = path.split('/').map(String::from).collect();
    let last = parts.pop().unwrap_or_default();
    // `.PY` too: Windows runs it with Python
    let stem = &last[..last.len() - ".py".len()];
    if !stem.eq_ignore_ascii_case("__init__") {
        parts.push(stem.to_string());
    }
    parts
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

/// The place a module sits in: the one whose path is the longest prefix of the module's parts.
fn place_of<'a>(module: &[String], places: &[&'a Place]) -> Option<&'a Place> {
    places
        .iter()
        .copied()
        .filter(|place| {
            let parts: Vec<&str> = place.path.split('/').collect();
            module.len() >= parts.len() && parts.iter().zip(module).all(|(a, b)| a == b)
        })
        .max_by_key(|place| place.path.len())
}

/// The line of an import, and the module it names as parts.
type Import = (usize, Vec<String>);
/// The line of a syntax error, and what the parser says.
type SyntaxError = (usize, String);

/// What a file imports, with the line of each import: every module an `import` or `from ... import` names, inside
/// functions, classes and `if TYPE_CHECKING:` too. `from m import n` names `m.n`, which is the module `n` when there is
/// one and `m` otherwise; both sit in the place of `m` unless `m.n` is a place itself (`from ui import atoms`). A
/// relative import is resolved against `package`, and one that climbs above the top package is left out: Python fails
/// on it. The first syntax error, if any, with its line: the parser recovers, but what follows may be misread.
fn imports(source: &str, package: &[String]) -> (Vec<Import>, Option<SyntaxError>) {
    struct Imports<'s> {
        source: &'s str,
        package: &'s [String],
        found: Vec<Import>,
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
    let parsed = ruff_python_parser::parse_unchecked_source(source, PySourceType::Python);
    let mut visitor = Imports {
        source,
        package,
        found: Vec::new(),
    };
    visitor.visit_body(&parsed.syntax().body);
    let error = parsed.errors().first().map(|e| {
        (
            line_of(source, e.location.start().to_usize()),
            e.error.to_string(),
        )
    });
    (visitor.found, error)
}

/// The table's entry for a place: a layer by its name, a level of `ui` by the part after the dot.
fn entry<'t>(table: &'t Table, place: &Place) -> &'t Entry {
    let (list, name) = match &place.parent {
        None => (&table.layers, place.name.as_str()),
        Some(_) => (
            &table.levels,
            place.name.rsplit('.').next().unwrap_or(&place.name),
        ),
    };
    list.iter()
        .find(|e| e.name == name)
        .unwrap_or_else(|| panic!("every place is in the table: {}", place.name))
}

/// The layer a place sits in: itself, or the layer of a level.
fn layer_name(place: &Place) -> &str {
    place.parent.as_deref().unwrap_or(&place.name)
}

/// Whether the place `from` may import the place `to`.
fn allowed(table: &Table, from: &Place, to: &Place) -> bool {
    if from.name == to.name {
        return true;
    }
    let imports = &entry(table, from).imports;
    match &from.parent {
        // A layer may import itself, the levels inside it included, and the layers in its imports with their levels
        None => layer_name(to) == from.name || imports.iter().any(|i| i == layer_name(to)),
        // A level may import the levels below it, and the layers in its imports; not its layer beside the levels
        Some(parent) => {
            if layer_name(to) != parent {
                return imports.iter().any(|i| i == layer_name(to));
            }
            if to.parent.is_none() {
                return false;
            }
            let rank = |place: &Place| {
                table
                    .levels
                    .iter()
                    .position(|l| entry(table, place).name == l.name)
            };
            rank(to) > rank(from)
        }
    }
}

/// The place an import lands in, for the message. A level that imports its own layer imports what sits beside the
/// levels, which has no place in their order.
fn described(from: &Place, to: &Place) -> String {
    if to.parent.is_none() && from.parent.as_deref() == Some(to.name.as_str()) {
        return format!("in {} outside its levels", to.name);
    }
    format!("in {}", to.name)
}

/// What a place may import, for the message.
fn what_it_may_import(table: &Table, place: &Place) -> String {
    let entry = entry(table, place);
    let below = place.parent.is_some()
        && table
            .levels
            .last()
            .is_some_and(|last| last.name != entry.name);
    match (below, entry.imports.is_empty()) {
        (false, true) => format!("{} imports no other layer", place.name),
        (false, false) => format!("{} may import {}", place.name, listed(&entry.imports)),
        (true, true) => format!("{} may import the levels below it", place.name),
        (true, false) => format!(
            "{} may import the levels below it and {}",
            place.name,
            listed(&entry.imports)
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layers::layout;

    fn python_places() -> Vec<Place> {
        layout("python").unwrap().places(&table())
    }

    fn parts(dotted: &str) -> Vec<String> {
        dotted.split('.').map(String::from).collect()
    }

    #[test]
    fn what_the_table_allows_is_closed_under_chaining() {
        // Only direct imports are judged, so a chain of allowed imports must never reach what its first may not
        let table = table();
        let places = python_places();
        let mut chains = 0;
        for a in &places {
            for b in &places {
                for c in &places {
                    if allowed(&table, a, b) && allowed(&table, b, c) {
                        chains += 1;
                        assert!(
                            allowed(&table, a, c),
                            "{} may import {}, which may import {}, but {} may not import {}",
                            a.name,
                            b.name,
                            c.name,
                            a.name,
                            c.name
                        );
                    }
                }
            }
        }
        assert!(chains > places.len(), "{chains} chains");
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
    fn a_module_sits_in_the_longest_place_it_starts_with() {
        let places = python_places();
        let places: Vec<&Place> = places.iter().collect();
        let name = |dotted: &str| place_of(&parts(dotted), &places).map(|p| p.name.clone());
        assert_eq!(name("ui.atoms.button"), Some("ui.atoms".into()));
        assert_eq!(name("ui.atoms"), Some("ui.atoms".into()));
        assert_eq!(name("ui.helpers"), Some("ui".into()));
        assert_eq!(name("ui"), Some("ui".into()));
        assert_eq!(name("domain.song"), Some("domain".into()));
        // Whole names only, and not the standard library
        assert_eq!(name("domains.song"), None);
        assert_eq!(name("ui_kit"), None);
        assert_eq!(name("os.path"), None);
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
        let (found, error) = imports(source, &parts("ui.atoms"));
        assert_eq!(error, None);
        let found: Vec<(usize, String)> =
            found.into_iter().map(|(l, m)| (l, m.join("."))).collect();
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
        let (found, error) = imports("import domain\ndef (:\nimport utils\n", &parts("ui"));
        assert_eq!(found.first().map(|(l, _)| *l), Some(1));
        assert_eq!(error.map(|(line, _)| line), Some(2));
    }

    #[test]
    fn the_message_says_what_a_place_may_import() {
        let table = table();
        let places = python_places();
        let say = |name: &str| {
            what_it_may_import(&table, places.iter().find(|p| p.name == name).unwrap())
        };
        assert_eq!(say("utils"), "utils imports no other layer");
        assert_eq!(say("domain"), "domain may import `utils`");
        assert_eq!(say("ui.atoms"), "ui.atoms may import `utils`");
        assert_eq!(
            say("ui.molecules"),
            "ui.molecules may import the levels below it and `utils`"
        );
    }
}
