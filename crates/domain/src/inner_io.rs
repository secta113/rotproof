//! The inner layers do no I/O: `domain`, `protocol`, `application` and `presentation` import nothing that reads or
//! writes outside the program.
//!
//! - **What it judges, by import:** an import of an entry of Rotproof's table of what does I/O wholly ([`table`]: the
//!   modules of the standard library that are sockets, files, processes, the terminal and the system around the
//!   program, by stack), through every spelling the file binds it by. A project cannot change the table. What is partly
//!   pure (`os`, `pathlib`, `time`, `logging` itself) is not in it: a later rule judges it by the attribute or call.
//! - **The key of an entry is what an approval is made for:** a file and an entry of the table, whatever the names
//!   it uses under it (`shutil`, `urllib.request`, `std::fs`, `std::os::fs` for the `fs` of every platform, `std::io`
//!   for the terminal items and the print macros together, `fs` for Node's `fs` and `fs/promises`, `bun:` for a
//!   scheme). The names are said on every run an approval is printed, so that a reason that no longer fits is seen.
//! - **Where:** the code of the four layers and their levels; not `infrastructure`, which does the I/O, not
//!   `composition` and `ui`, which wire and draw, not the entry file, and not a test (Python's `tests/`, a TypeScript
//!   file named as a test, and in Rust the items under `#[cfg(test)]`, the files of a `#[cfg(test)] mod x;` and
//!   everything outside a crate's `src/`, where Cargo keeps its tests, benches, examples and build script).
//! - **A finding says what to do, by layer,** before it says what it found: the I/O goes in `infrastructure.outbound`,
//!   behind a port in `protocol.outbound` the use case is given, and a screen's model gets what it needs from its page
//!   or through a use case. The finding is a [`Forbidden`] of its own [`Rule`], so that `rotproof approve` and the
//!   approvals file treat it as any other. It is **one for each entry of a file**, the key of an approval, with every
//!   line the entry is on ([`findings`]): the key and the files that fail are what they were, only the print differs.
//! - **A package is imported only where it is allowed** (Python and TypeScript, `allowed`): the code of the four layers
//!   imports no package of PyPI or npm unless the declaration, or the layout of the stack, lists it for the layer or
//!   level, since what a package does is unknown. The finding is a [`Forbidden`] too, approved by the file and the
//!   package, and what is wrong with the list is in `allowed`.
//!
//! - **What is I/O in part is judged by the name,** in Python: the allowlists of `os`, `os.path`, `sys`, `time` and
//!   `logging`, the clock, randomness, the terminal, the command-line arguments, `pathlib.Path`, the net of method
//!   names and what cannot be read (`python`); in TypeScript: the allowlist of the globals (a global not on it is
//!   I/O), `process`, the clock, randomness, `console`, the arguments and what cannot be read (`typescript`). A group
//!   of one act (the clock, randomness, the terminal, the arguments) is one entry, whichever name it is reached by
//!   (`groups`); in Rust: the allowlist of `std::env`, the clocks, the net of `Path` method names and the `extern`
//!   blocks (`rust`).
//!
//! Python and TypeScript are read by their readers of imports and uses ([`in_python`], [`in_typescript`]); Rust by
//! Rotproof's own scanner ([`in_rust`]).

mod allowed;
mod groups;
mod python;
mod rust;
mod table;
mod typescript;

pub use allowed::{Allowlist, Imported, Placed};
pub use groups::{Excluded, Group};
pub use python::excluded as python_excluded;
pub use rust::{defined as rust_defined, excluded as rust_excluded};
pub use table::listing;

use std::collections::{BTreeMap, BTreeSet};

use crate::code::{How, Import, Method, Use, split_typescript_name};
use crate::direction::{Forbidden, Rule};
use crate::layers::{Language, Place};
use crate::rust::{Item, Token, code, items, paths, tokens, without_tests};
use crate::text::line_of;
use table::{
    NODE, NODE_PREFIXED, NODE_SUBPATHS, PYTHON, RUST, RUST_IO_KEY, RUST_MACROS, RUST_OS_MODULES,
    RUST_PLATFORMS,
};

/// The heading a finding is printed under.
pub const HEADING: &str = "the inner layers do no I/O (domain, protocol, application and presentation import \
                           nothing that touches files, the network, processes or the terminal: the I/O is done in \
                           infrastructure.outbound behind a port the use case is given, and approving it, renaming \
                           it or moving the code to a path in `unchecked` or `moving` is no fix; and a package is \
                           imported only where `allowed` in the declaration lists it, which is the person's decision)";

/// The layers judged, with their levels.
pub const LAYERS: [&str; 4] = ["domain", "protocol", "application", "presentation"];

/// Whether the code of `place` is judged: a layer of [`LAYERS`] or a level of one. `infrastructure`, `composition`,
/// `ui` and the entry file are not.
pub fn is_judged(place: &Place) -> bool {
    LAYERS.contains(&place.layer_name())
}

const GUIDE_TEXT: &str = include_str!("../../../project/rotproof/inner-io.md.in");
/// The part of it on the packages of Python and TypeScript, and the one of Rust (`allowed.rs` fills them in)
const GUIDE_ALLOWED: &str = include_str!("../../../project/rotproof/inner-io-allowed.md.in");
const GUIDE_ALLOWED_RUST: &str =
    include_str!("../../../project/rotproof/inner-io-allowed-rust.md.in");

/// What the guide says of this rule in the section on the layers, for a project of `language`: the whole table, since
/// an agent reads the guide and not the specification.
pub fn guide(language: Language) -> String {
    let tests = match language {
        Language::Python => "a test (a file in `tests/`)",
        Language::TypeScript => {
            "a test (a source file named `*.test.*` or `*.spec.*`, or a file in a `__tests__/` directory)"
        }
        Language::Rust => {
            "a test (the items under `#[cfg(test)]`, the file of a `#[cfg(test)] mod x;`, and what is outside a \
             crate's `src/`: `tests/`, `benches/`, `examples/` and `build.rs`)"
        }
    };
    // What a stack says of the names it judges besides the table
    let (names, passed, keys) = match language {
        Language::Python => (
            python::listing(),
            groups::passed_in(language),
            groups::KEYS.to_string(),
        ),
        Language::TypeScript => (
            typescript::listing(),
            groups::passed_in(language),
            groups::KEYS_TYPESCRIPT.to_string(),
        ),
        Language::Rust => (
            rust::listing(),
            groups::passed_in(language),
            groups::KEYS_RUST.to_string(),
        ),
    };
    let hook = match language {
        Language::TypeScript => ", through a hook in `ui.atoms`",
        Language::Python | Language::Rust => "",
    };
    GUIDE_TEXT
        .replace("{tests}", tests)
        .replace("{hook}", hook)
        .replace("{table}", &listing(language))
        .replace("{names}", &names)
        .replace("{passed}", &passed)
        .replace("{keys}", &keys)
        .replace("{allowed}", &allowed::guide(language))
}

/// How a finding says what it found.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Cause {
    /// A name of the table is used: imported, or taken at an import, or reached through a path
    Used,
    /// A glob of the module at this path (`std::fs`, or `std` above it): the names it brings in cannot be read
    Glob(String),
    /// An import by a scheme that is not `node:`, as written: what it names cannot be judged
    Scheme(String),
    /// The act of a group (the clock, randomness, the terminal, the arguments), by the names under the entry
    Group(Group),
    /// A module used as a value (`sh = os`, `getattr(os, "system")`): what is done through it cannot be read
    Module,
    /// A star import of this module, which has names that are I/O among those it brings in
    Star(String),
    /// What the reader cannot read (`eval`, an import by a name built at run time): what it runs is unknown
    Unread(String),
    /// A method called by a name that is a path's, on a receiver whose type cannot be read
    Method,
    /// An `extern` block (Rust), which declares what another language defines
    Extern,
}

/// One place a file does I/O by import: the line (from 1), the entry of the table, the name used under it, and how.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Found {
    pub line: usize,
    /// The key of the entry: what an approval is made for
    pub entry: String,
    /// The name used under the entry (`copyfile` of `shutil`), when one is
    pub name: Option<String>,
    pub cause: Cause,
}

/// The entry of Python's table that a dotted name is, or is under, and the first name under it (`shutil` and `rmtree`
/// for `shutil.rmtree.avoids_symlink_attacks`).
fn python_entry(dotted: &str) -> Option<(&'static str, Option<String>)> {
    PYTHON.iter().find_map(|entry| {
        let rest = dotted.strip_prefix(entry)?;
        if rest.is_empty() {
            return Some((*entry, None));
        }
        let rest = rest.strip_prefix('.')?;
        let name = rest.split('.').next().filter(|name| !name.is_empty());
        Some((*entry, name.map(String::from)))
    })
}

/// Every use of an entry of the table in a Python file, from what it imports (the modules, as parts, each with its
/// line: `from m import n` is `m.n`) and from the names it uses: `import asyncio` then `asyncio.open_connection(...)`
/// is a use of the entry `asyncio.open_connection`, and `import urllib` then `urllib.request.urlopen(...)` one of
/// `urllib.request`. A use in a type position is never judged, by the table or by the name: an annotation runs
/// nothing, and the import it names fails at its own line, but for the import of a class of `pathlib` (`Path`) that the
/// file uses in a type position alone, which is not judged either.
///
/// What is I/O in part is judged by the name after the table ([`python`]): the allowlists, the groups, `pathlib.Path`,
/// a module used as a value and a star import of one, and the calls of an attribute (`methods`) whose name is a path's,
/// unless `excluded` (the methods the project defines) leaves it out.
pub fn in_python(
    imports: &[(usize, Vec<String>)],
    uses: &[Use],
    methods: &[Method],
    excluded: &Excluded,
) -> Vec<Found> {
    let mut found: Vec<Found> = imports
        .iter()
        .filter_map(|(line, parts)| {
            python_found(*line, &parts.join(".")).or_else(|| python::judge_import(*line, parts))
        })
        .collect();
    // A class of `pathlib` that the file uses in a type position alone is free to import: nothing runs it
    let type_only = python::type_only_imports(uses);
    for used in uses {
        // An annotation runs nothing, and the import it names is found at its own line
        if used.in_type {
            continue;
        }
        // The module `from m import *` brings the names of is its import, which is already one when it is in the table
        if used.how == How::Star {
            found.extend(python::judge_star(used));
            continue;
        }
        if used.how == How::Imported && type_only.contains(used.name.as_str()) {
            continue;
        }
        match python_found(used.line, &used.name) {
            Some(one) => found.push(one),
            None => found.extend(python::judge(used)),
        }
    }
    found.extend(
        methods
            .iter()
            .filter_map(|method| python::net(method, excluded)),
    );
    sorted(found)
}

/// What a reader could not read, as findings: the function a file calls to import a module by a name built at run time,
/// `eval` and `exec`, `Function`. What they run is unknown, so it fails, and `eval` and `exec` are the entries (a
/// `builtins.` in front of a name is taken away).
pub fn unread(unread: &[(usize, String)]) -> Vec<Found> {
    let found = unread
        .iter()
        .map(|(line, what)| {
            let what = what.strip_prefix("builtins.").unwrap_or(what);
            Found {
                line: *line,
                entry: what.to_string(),
                name: None,
                cause: Cause::Unread(what.to_string()),
            }
        })
        .collect();
    sorted(found)
}

fn python_found(line: usize, dotted: &str) -> Option<Found> {
    let (entry, name) = python_entry(dotted)?;
    Some(Found {
        line,
        entry: entry.to_string(),
        name,
        cause: Cause::Used,
    })
}

/// The entry of Node's table a specifier is, with `node:` or without it, and what it is of when it is another scheme.
fn typescript_entry(specifier: &str) -> Option<(String, Cause)> {
    let Some((scheme, rest)) = specifier
        .split_once(':')
        .filter(|(scheme, _)| is_scheme(scheme))
    else {
        return node_entry(specifier, false).map(|entry| (entry, Cause::Used));
    };
    if scheme == "node" {
        return node_entry(rest, true).map(|entry| (entry, Cause::Used));
    }
    Some((format!("{scheme}:"), Cause::Scheme(specifier.to_string())))
}

/// Whether `text` is a scheme as RFC 3986 writes one.
fn is_scheme(text: &str) -> bool {
    let mut chars = text.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
}

/// The module of Node's table that `name` is, with `node:` in front of it or not.
fn node_entry(name: &str, prefixed: bool) -> Option<String> {
    let module = NODE
        .iter()
        .find(|module| {
            **module == name
                || NODE_SUBPATHS
                    .iter()
                    .any(|(of, path)| of == *module && *path == name)
        })
        .or_else(|| {
            prefixed
                .then(|| NODE_PREFIXED.iter().find(|module| **module == name))
                .flatten()
        })?;
    Some((*module).to_string())
}

/// Every use of an entry of the table in a TypeScript or JavaScript file, from the specifiers it imports (with the
/// names it takes) and from the names it uses of a module (`"node:fs".readFile`). The manifest is not asked: `fs` is
/// Node's `fs` whatever `package.json` lists. A scheme other than `node:` fails, as what it names cannot be judged. An
/// `import type` is an import as any.
///
/// What is I/O in part is judged by the name after the table ([`typescript`]): a global (a name no scope binds, which
/// `window.` and `globalThis.` do not change) is I/O unless it is on the allowlist, and the clock, randomness, the
/// terminal and the arguments are groups. A name in a type position and the operand of `typeof` are never judged, by
/// the table or by the name.
pub fn in_typescript(imports: &[(usize, Import)], uses: &[Use]) -> Vec<Found> {
    let mut found = Vec::new();
    for (line, import) in imports {
        let Some((entry, cause)) = typescript_entry(&import.specifier) else {
            continue;
        };
        // The module itself, and each name taken from it
        let mut names: Vec<Option<String>> = import
            .names
            .iter()
            .filter(|name| !matches!(name.as_str(), "*" | "default"))
            .map(|name| Some(name.clone()))
            .collect();
        if names.is_empty() {
            names.push(None);
        }
        for name in names {
            found.push(Found {
                line: *line,
                entry: entry.clone(),
                name,
                cause: cause.clone(),
            });
        }
    }
    for used in uses {
        // A type position and the operand of `typeof` run nothing, and the import is found at its own line
        if used.in_type || used.how == How::TypeOf {
            continue;
        }
        let (Some(module), parts) = split_typescript_name(&used.name) else {
            found.extend(typescript::judge(used));
            continue;
        };
        let Some((entry, cause)) = typescript_entry(&module) else {
            continue;
        };
        let parts: Vec<&str> = parts.iter().map(String::as_str).collect();
        let name = parts
            .strip_prefix(&["default"])
            .unwrap_or(&parts)
            .first()
            .map(|name| name.to_string());
        found.push(Found {
            line: used.line,
            entry,
            name,
            cause,
        });
    }
    sorted(found)
}

/// Every use of an entry of the table in a Rust source, found as the scanner finds the items of a module: by the whole
/// path, through a `use` of the module or of one above it, through a rename of either (`use std::fs as f;`, `use std as
/// s;`), and as a glob of the module or of one above it, which fails whatever is used since the names it brings in
/// cannot be read. The items under `#[cfg(test)]` are not read. Not followed: a name bound in one file and used through
/// another, a path a macro builds from pieces, and a macro renamed with `use std::println as p;` (the `use` is found).
///
/// What is I/O in part is judged by the name after the table ([`rust`]): the allowlist of `std::env`, the clocks, the
/// calls of a method whose name is a path's unless `excluded` (the methods the project defines) leaves it out, and
/// the `extern` blocks.
pub fn in_rust(source: &str, excluded: &Excluded) -> Vec<Found> {
    let code = without_tests(&code(source));
    let mut found = Vec::new();
    let module = |key: &str, path: &[&str], names: Option<&[&str]>, found: &mut Vec<Found>| {
        for (at, item) in items(&code, path) {
            let line = line_of(source, at);
            match item {
                Item::Named(name) if names.is_none_or(|names| names.contains(&name.as_str())) => {
                    found.push(Found {
                        line,
                        entry: key.to_string(),
                        name: Some(name),
                        cause: Cause::Used,
                    });
                }
                Item::Named(_) => {}
                Item::Glob(of) => {
                    // A glob of the module is the module's entry; one above it is an entry of its own
                    let joined = of.join("::");
                    let entry = if of.len() == path.len() {
                        key.to_string()
                    } else {
                        format!("{joined}::*")
                    };
                    found.push(Found {
                        line,
                        entry,
                        name: None,
                        cause: Cause::Glob(joined),
                    });
                }
            }
        }
    };
    for row in RUST {
        module(row.key, row.module, row.names, &mut found);
    }
    // A module that does I/O whatever is used of it is used by being imported, wherever a name of it is used after
    // (`use std::fs;`, `pub use std::fs;`, `use std::os::unix::{fs, net};`), as an import of one is in the other stacks
    for (at, segments) in paths(&code, &["std"]) {
        let module = segments
            .strip_suffix(&["self".to_string()])
            .unwrap_or(&segments);
        let module: Vec<&str> = module.iter().map(String::as_str).collect();
        let key = match module.as_slice() {
            ["std", "os", platform, name]
                if RUST_PLATFORMS.contains(platform) && RUST_OS_MODULES.contains(name) =>
            {
                format!("std::os::{name}")
            }
            _ => match RUST
                .iter()
                .find(|row| row.names.is_none() && row.module == module.as_slice())
            {
                Some(row) => row.key.to_string(),
                None => continue,
            },
        };
        found.push(Found {
            line: line_of(source, at),
            entry: key,
            name: None,
            cause: Cause::Used,
        });
    }
    let tokens = tokens(&code);
    // `std::os::<platform>::fs` is a long list of paths; a file that has no `os` in it names none of them
    if tokens
        .iter()
        .any(|(_, token)| matches!(token, Token::Word(word) if word == "os"))
    {
        for module_name in RUST_OS_MODULES {
            let key = format!("std::os::{module_name}");
            for platform in RUST_PLATFORMS {
                module(
                    &key,
                    &["std", "os", *platform, *module_name],
                    None,
                    &mut found,
                );
            }
        }
    }
    // The print macros: `std::println!` is a path of `std`, and a bare `println!` is the macro the prelude brings in
    for (at, item) in items(&code, &["std"]) {
        if let Item::Named(name) = item
            && RUST_MACROS.contains(&name.as_str())
        {
            found.push(Found {
                line: line_of(source, at),
                entry: RUST_IO_KEY.to_string(),
                name: Some(format!("{name}!")),
                cause: Cause::Used,
            });
        }
    }
    for (i, (at, token)) in tokens.iter().enumerate() {
        let Token::Word(name) = token else { continue };
        let bang = tokens.get(i + 1);
        let negation = tokens.get(i + 2);
        if !RUST_MACROS.contains(&name.as_str())
            || bang.map(|(_, token)| token) != Some(&Token::Punct('!'))
            // `a != b`, not a macro
            || negation.is_some_and(|(next, token)| {
                *token == Token::Punct('=') && bang.is_some_and(|(bang, _)| next == &(bang + 1))
            })
            // A macro named by a path is `std`'s (found above) or another crate's
            || (i > 0 && tokens[i - 1].1 == Token::PathSep)
        {
            continue;
        }
        found.push(Found {
            line: line_of(source, *at),
            entry: RUST_IO_KEY.to_string(),
            name: Some(format!("{name}!")),
            cause: Cause::Used,
        });
    }
    found.extend(rust::judge(source, &code, excluded));
    sorted(found)
}

fn sorted(mut found: Vec<Found>) -> Vec<Found> {
    found.sort();
    found.dedup();
    found
}

/// What to do about I/O in a layer of a project in `language`, the first thing a finding says. A screen's model in
/// TypeScript gets what the browser gives (`matchMedia`, `localStorage`, `document.title`) from a hook in `ui.atoms`
/// that its page uses, or through a use case.
fn remedy(layer: &str, language: Language) -> &'static str {
    match (layer, language) {
        ("presentation", Language::TypeScript) => {
            "a screen's model does no I/O: it gets what it needs from its page, through a hook in `ui.atoms`, or through a \
             use case of protocol.inbound it is given"
        }
        ("presentation", _) => {
            "a screen's model does no I/O: it gets what it needs from its page, or through a use case of protocol.inbound it \
             is given"
        }
        _ => {
            "move the I/O into infrastructure.outbound, behind a port in protocol.outbound that the use case is given"
        }
    }
}

/// What a finding of more than one line says after what it found, with every line in ascending order, the first
/// included: `, on lines 3, 8, 21`. Nothing for one line, which the finding names at its start.
fn on_lines(lines: &[usize]) -> String {
    if lines.len() < 2 {
        return String::new();
    }
    let lines: Vec<String> = lines.iter().map(usize::to_string).collect();
    format!(", on lines {}", lines.join(", "))
}

/// The findings for `found` in the file at `path`, whose text is `source` and whose layer is `layer`, in a project of
/// `language`: **one for each entry, with every line of the file that uses it**, as a [`Forbidden`] of the entry: the
/// key of an approval is the file and the entry, so a file that does one act on 13 lines (`print`) is one finding that
/// says the 13 lines, the source text of the first, and the names used under the entry. It says the line of the first
/// where a finding says one (`file:line:`), and the others after it. What the lines say differently stays apart: a
/// glob or a scheme is what its lines say, whatever else the same line has under the entry, and the lines that use an
/// entry by name are one finding apart from the lines of a glob of it. One that does the act of a group says what to
/// do about the act, with the example of the language.
pub fn findings(
    path: &str,
    source: &str,
    layer: &str,
    language: Language,
    found: &[Found],
) -> Vec<Forbidden> {
    let mut lines: BTreeMap<(usize, &str), (BTreeSet<&str>, &Cause)> = BTreeMap::new();
    for one in found {
        let slot = lines
            .entry((one.line, one.entry.as_str()))
            .or_insert_with(|| (BTreeSet::new(), &one.cause));
        if let Some(name) = &one.name {
            slot.0.insert(name);
        }
        // A glob or a scheme is what the finding says, whatever else is on the line under the entry
        if matches!(slot.1, Cause::Used) {
            slot.1 = &one.cause;
        }
    }
    // The lines that say the same of an entry are one finding; the lines come in ascending order
    type Grouped<'a> = BTreeMap<(&'a str, &'a Cause), (Vec<usize>, BTreeSet<&'a str>)>;
    let mut grouped = Grouped::new();
    for ((line, entry), (names, cause)) in lines {
        let slot = grouped.entry((entry, cause)).or_default();
        slot.0.push(line);
        slot.1.extend(names);
    }
    let mut grouped: Vec<_> = grouped.into_iter().collect();
    grouped.sort_by_key(|((entry, _), (lines, _))| (lines[0], *entry));
    grouped
        .into_iter()
        .map(|((entry, cause), (lines, names))| {
            let line = lines[0];
            let used = if names.is_empty() {
                String::new()
            } else {
                format!(" ({})", names.iter().copied().collect::<Vec<_>>().join(", "))
            };
            let what = match cause {
                Cause::Used if entry.starts_with("pathlib.") => format!(
                    "uses `{entry}`{used}, which does I/O: a path as a value is `PurePath` (`isinstance(x, PurePath)` \
                     narrows a `Path` too), and `Path` written in a type position alone is not judged"
                ),
                Cause::Used => format!("uses `{entry}`{used}, which does I/O"),
                Cause::Glob(of) => format!(
                    "a glob of `{of}`, which brings in names that do I/O: the names it brings in cannot be read, so name \
                     them (and a name of the table is I/O as well)"
                ),
                Cause::Scheme(specifier) => format!(
                    "imports `{specifier}`: a scheme other than node: names what cannot be judged, so it fails in an \
                     inner layer"
                ),
                Cause::Group(group) => {
                    format!("{}{used}, which does I/O (entry `{}`)", group.act(), group.key())
                }
                Cause::Module => format!(
                    "uses the module `{entry}` as a value (an assignment, an argument, `getattr`): what is done through \
                     it cannot be read, so use a name of it directly"
                ),
                Cause::Star(module) => format!(
                    "a star import of `{module}`, which brings in names that do I/O: the names it brings in cannot be \
                     read, so name them"
                ),
                Cause::Unread(what) => format!(
                    "uses `{what}`: what it runs or imports cannot be read, so it fails in an inner layer"
                ),
                Cause::Method => format!(
                    "calls `.{entry}(...)`, a method of a path that does I/O: a method is judged by its name on any \
                     receiver, as the type of what it is called on cannot be read"
                ),
                Cause::Extern => "declares an `extern` block: the functions of another language it brings in cannot be \
                                  read, so it fails in an inner layer"
                    .to_string(),
            };
            let first = match cause {
                Cause::Group(group) => group.remedy(layer, language),
                _ => remedy(layer, language).to_string(),
            };
            let text = source.lines().nth(line - 1).unwrap_or("").trim();
            let what = format!("{what}{}", on_lines(&lines));
            Forbidden {
                file: path.to_string(),
                import: entry.to_string(),
                detail: format!("{path}:{line}: {first}: {what}\n{text}"),
                rule: Rule::InnerIo {
                    names: names.iter().map(|name| name.to_string()).collect(),
                },
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::code::First;
    use crate::layers::UnitTag;

    fn python(source_imports: &[(usize, &str)]) -> Vec<(usize, String, Option<String>)> {
        let imports: Vec<(usize, Vec<String>)> = source_imports
            .iter()
            .map(|(line, name)| (*line, name.split('.').map(String::from).collect()))
            .collect();
        in_python(&imports, &[], &[], &Excluded::default())
            .into_iter()
            .map(|f| (f.line, f.entry, f.name))
            .collect()
    }

    fn called(line: usize, name: &str) -> Use {
        Use {
            line,
            name: name.into(),
            how: How::Called {
                positional: 0,
                keywords: Vec::new(),
                unpacked: false,
                first: First::Other,
            },
            in_type: false,
        }
    }

    fn typescript_use(line: usize, name: &str, how: How) -> Use {
        Use {
            line,
            name: name.into(),
            how,
            in_type: false,
        }
    }

    fn entry(line: usize, entry: &str, name: Option<&str>) -> (usize, String, Option<String>) {
        (line, entry.to_string(), name.map(String::from))
    }

    #[test]
    fn a_python_import_is_matched_by_its_dotted_prefix() {
        assert_eq!(
            python(&[
                (1, "shutil"),
                (2, "urllib.request"),
                (3, "urllib.request.urlopen"),
                (4, "urllib.parse"),
                (5, "urllib"),
                (6, "http.client"),
                (7, "http"),
                (8, "http.cookies"),
                (9, "socketserver"),
                (10, "sockets"),
                (11, "shutil.copyfile"),
            ]),
            [
                entry(1, "shutil", None),
                entry(2, "urllib.request", None),
                entry(3, "urllib.request", Some("urlopen")),
                entry(6, "http.client", None),
                entry(9, "socketserver", None),
                entry(11, "shutil", Some("copyfile")),
            ]
        );
    }

    #[test]
    fn what_python_does_wholly_is_in_the_table_and_what_it_does_partly_is_not() {
        for module in [
            "socket",
            "ssl",
            "sqlite3",
            "subprocess",
            "tempfile",
            "glob",
            "argparse",
            "platform",
            "locale",
            "importlib.resources",
            "importlib.metadata",
            "logging.handlers",
            "multiprocessing.shared_memory",
            "asyncio.streams",
            "asyncio.subprocess",
            "tkinter",
            "_socket",
            "_winapi",
            "_msi",
            "_posixshmem",
            "_pyrepl",
        ] {
            assert_eq!(python(&[(1, module)]).len(), 1, "{module}");
        }
        for module in [
            "io",
            "logging",
            "asyncio",
            "os",
            "os.path",
            "pathlib",
            "sys",
            "time",
            "datetime",
            "zipfile",
            "tarfile",
            "gzip",
            "bz2",
            "lzma",
            "mmap",
            "ctypes",
            "json",
            "csv",
            "importlib",
            "multiprocessing",
            "asyncio.tasks",
            "logging.config_x",
            "xmlrpc",
        ] {
            assert!(python(&[(1, module)]).is_empty(), "{module}");
        }
    }

    #[test]
    fn the_functions_of_asyncio_that_open_a_connection_or_a_process_are_entries_when_named() {
        assert_eq!(
            python(&[
                (1, "asyncio.create_subprocess_exec"),
                (2, "asyncio.open_connection"),
                (3, "asyncio.sleep"),
                (4, "asyncio"),
            ]),
            [
                entry(1, "asyncio.create_subprocess_exec", None),
                entry(2, "asyncio.open_connection", None),
            ]
        );
        // The attribute form, after `import asyncio`, is read from the names the file uses
        let found = in_python(
            &[(1, vec!["asyncio".to_string()])],
            &[
                called(3, "asyncio.open_connection"),
                called(4, "asyncio.sleep"),
                called(5, "asyncio.start_unix_server"),
            ],
            &[],
            &Excluded::default(),
        );
        assert_eq!(
            found
                .iter()
                .map(|f| (f.line, f.entry.as_str()))
                .collect::<Vec<_>>(),
            [
                (3, "asyncio.open_connection"),
                (5, "asyncio.start_unix_server")
            ]
        );
    }

    #[test]
    fn a_python_use_in_a_type_position_is_not_judged_by_the_table_and_the_import_is() {
        let mut typed = called(3, "argparse.ArgumentParser");
        typed.in_type = true;
        let mut value = typescript_use(4, "shutil.copyfile", How::Value);
        value.in_type = true;
        let found = in_python(
            &[(1, vec!["argparse".to_string()])],
            &[typed, value, called(5, "argparse.ArgumentParser")],
            &[],
            &Excluded::default(),
        );
        assert_eq!(
            found
                .iter()
                .map(|f| (f.line, f.entry.as_str()))
                .collect::<Vec<_>>(),
            [(1, "argparse"), (5, "argparse")]
        );
    }

    #[test]
    fn a_python_name_used_under_an_entry_is_said_and_a_star_import_is_the_import() {
        let found = in_python(
            &[
                (1, vec!["shutil".to_string()]),
                (2, vec!["urllib".to_string()]),
            ],
            &[
                called(3, "shutil.copyfile"),
                called(4, "shutil.rmtree"),
                called(5, "urllib.request.urlopen"),
                called(6, "urllib.parse.quote"),
                typescript_use(7, "socket", How::Star),
                typescript_use(8, "socket.socket", How::Value),
            ],
            &[],
            &Excluded::default(),
        );
        let said: Vec<(usize, &str, Option<&str>)> = found
            .iter()
            .map(|f| (f.line, f.entry.as_str(), f.name.as_deref()))
            .collect();
        assert_eq!(
            said,
            [
                (1, "shutil", None),
                (3, "shutil", Some("copyfile")),
                (4, "shutil", Some("rmtree")),
                (5, "urllib.request", Some("urlopen")),
                (8, "socket", Some("socket")),
            ]
        );
    }

    fn node(imports: &[&str]) -> Vec<(String, Cause)> {
        let imports: Vec<(usize, Import)> = imports
            .iter()
            .enumerate()
            .map(|(i, specifier)| {
                (
                    i + 1,
                    Import {
                        specifier: specifier.to_string(),
                        names: Vec::new(),
                    },
                )
            })
            .collect();
        in_typescript(&imports, &[])
            .into_iter()
            .map(|f| (f.entry, f.cause))
            .collect()
    }

    #[test]
    fn a_node_module_is_matched_with_or_without_the_prefix() {
        let used = |entry: &str| (entry.to_string(), Cause::Used);
        assert_eq!(
            node(&[
                "fs",
                "node:fs",
                "fs/promises",
                "node:fs/promises",
                "dns/promises",
                "node:readline/promises",
                "node:sqlite",
                "sqlite",
                "_http_agent",
                "node:_tls_wrap",
                "child_process",
                "process",
            ]),
            [
                used("fs"),
                used("fs"),
                used("fs"),
                used("fs"),
                used("dns"),
                used("readline"),
                used("sqlite"),
                used("_http_agent"),
                used("_tls_wrap"),
                used("child_process"),
                used("process"),
            ]
        );
    }

    #[test]
    fn what_node_does_partly_or_a_package_that_is_not_node_is_not_in_the_table() {
        assert!(
            node(&[
                "os",
                "node:os",
                "crypto",
                "stream",
                "path",
                "url",
                "util",
                "events",
                "buffer",
                "zlib",
                "worker_threads",
                "timers",
                "perf_hooks",
                "console",
                "test",
                "node:test",
                "fs-extra",
                "fs/extra",
                "./fs",
                "../fs",
                "@scope/fs",
                "react",
                "http-server",
                "lodash/fs",
            ])
            .is_empty()
        );
    }

    #[test]
    fn a_scheme_other_than_node_fails_by_its_scheme() {
        assert_eq!(
            node(&[
                "bun:sqlite",
                "npm:left-pad",
                "jsr:@std/path",
                "https://esm.sh/x",
                "data:text/javascript,1"
            ]),
            [
                ("bun:".to_string(), Cause::Scheme("bun:sqlite".into())),
                ("npm:".to_string(), Cause::Scheme("npm:left-pad".into())),
                ("jsr:".to_string(), Cause::Scheme("jsr:@std/path".into())),
                (
                    "https:".to_string(),
                    Cause::Scheme("https://esm.sh/x".into())
                ),
                (
                    "data:".to_string(),
                    Cause::Scheme("data:text/javascript,1".into())
                ),
            ]
        );
        // A colon that makes no scheme is no scheme
        assert!(node(&["./a:b", "@x:y/z", "1a:b", "_x:y", "/a:b"]).is_empty());
    }

    #[test]
    fn the_names_taken_and_used_under_a_node_module_are_said() {
        let imports = [(
            1,
            Import {
                specifier: "node:fs".into(),
                names: vec!["readFile".into(), "default".into()],
            },
        )];
        let uses = [
            typescript_use(1, "\"node:fs\".readFile", How::Imported),
            typescript_use(3, "\"fs\".default.readFileSync", How::Value),
            typescript_use(4, "\"fs\".promises.readFile", How::Value),
            typescript_use(5, "\"fs\"", How::Value),
            typescript_use(6, "\"react\".default.useState", How::Value),
            typescript_use(7, "Math.max", How::Value),
        ];
        let found = in_typescript(&imports, &uses);
        assert_eq!(
            found
                .iter()
                .map(|f| (f.line, f.entry.as_str(), f.name.as_deref()))
                .collect::<Vec<_>>(),
            [
                (1, "fs", Some("readFile")),
                (3, "fs", Some("readFileSync")),
                (4, "fs", Some("promises")),
                (5, "fs", None),
            ]
        );
    }

    #[test]
    fn the_operand_of_typeof_and_a_type_position_are_not_judged_by_the_table_and_the_import_is() {
        let imports = [(
            1,
            Import {
                specifier: "node:fs".into(),
                names: vec!["readFile".into()],
            },
        )];
        let mut typed = typescript_use(3, "\"node:fs\".readFile", How::Value);
        typed.in_type = true;
        let uses = [
            typescript_use(1, "\"node:fs\".readFile", How::Imported),
            typescript_use(2, "\"node:fs\".readFile", How::TypeOf),
            typed,
            typescript_use(4, "\"fs\".readFileSync", How::Value),
        ];
        let found = in_typescript(&imports, &uses);
        assert_eq!(
            found
                .iter()
                .map(|f| (f.line, f.entry.as_str(), f.name.as_deref()))
                .collect::<Vec<_>>(),
            [(1, "fs", Some("readFile")), (4, "fs", Some("readFileSync")),]
        );
    }

    #[test]
    fn a_global_is_judged_by_its_name_beside_the_table_and_what_cannot_be_read_is_found() {
        let imports = [(
            1,
            Import {
                specifier: "node:fs".into(),
                names: vec!["readFile".into()],
            },
        )];
        let uses = [
            typescript_use(1, "\"node:fs\".readFile", How::Imported),
            typescript_use(2, "localStorage.getItem", How::Value),
            typescript_use(3, "Date.now", How::Value),
            typescript_use(4, "Map", How::Value),
            typescript_use(5, "process.env.A", How::Value),
            typescript_use(6, "window", How::TypeOf),
        ];
        let found = in_typescript(&imports, &uses);
        assert_eq!(
            found
                .iter()
                .map(|f| (f.line, f.entry.as_str(), f.name.as_deref()))
                .collect::<Vec<_>>(),
            [
                (1, "fs", Some("readFile")),
                (2, "localStorage", Some("localStorage.getItem")),
                (3, "clock", Some("Date.now")),
            ]
        );
        let unread = unread(&[(7, "eval".into()), (8, "import()".into())]);
        assert_eq!(
            unread
                .iter()
                .map(|f| (f.line, f.entry.as_str(), f.cause.clone()))
                .collect::<Vec<_>>(),
            [
                (7, "eval", Cause::Unread("eval".into())),
                (8, "import()", Cause::Unread("import()".into())),
            ]
        );
    }

    #[test]
    fn a_screens_model_in_typescript_is_told_of_the_hook_and_the_other_layers_are_not() {
        let found = in_typescript(
            &[],
            &[typescript_use(1, "localStorage.getItem", How::Value)],
        );
        let said = |layer: &str| {
            findings(
                "src/x.ts",
                "const a = localStorage.getItem('k');\n",
                layer,
                Language::TypeScript,
                &found,
            )[0]
            .detail
            .clone()
        };
        assert!(
            said("presentation").contains(
                "it gets what it needs from its page, through a hook in `ui.atoms`, or through a use case of \
                 protocol.inbound it is given: uses `localStorage` (localStorage.getItem), which does I/O"
            ),
            "{}",
            said("presentation")
        );
        assert!(
            said("domain").contains("move the I/O into infrastructure.outbound, behind a port"),
            "{}",
            said("domain")
        );
        // Python has no hook
        assert!(
            !remedy("presentation", Language::Python).contains("hook"),
            "no hook in Python"
        );
    }

    fn rust(source: &str) -> Vec<(usize, String, Option<String>)> {
        in_rust(source, &Excluded::default())
            .into_iter()
            .map(|f| (f.line, f.entry, f.name))
            .collect()
    }

    #[test]
    fn a_rust_item_is_found_by_its_path_and_through_use() {
        let source = "\
fn a() { let _ = std::fs::read(\"a\"); }
use std::process::Command;
use std::net::{TcpStream, IpAddr};
fn b() { Command::new(\"x\"); let _ = std::process::exit; }
use std::io::{self, stdin, Read};
fn c() { let _ = io::stdout(); let _: Option<io::Result<()>> = None; }
use std::env;
fn d() { let _ = env::current_dir(); let _ = env::var(\"X\"); let _ = env::args(); }
";
        assert_eq!(
            rust(source),
            [
                entry(1, "std::fs", Some("read")),
                entry(2, "std::process", Some("Command")),
                entry(3, "std::net", Some("TcpStream")),
                entry(5, "std::io", Some("stdin")),
                entry(6, "std::io", Some("stdout")),
                // `env::args` is the arguments, a group; `env::var` is the environment check's
                entry(8, "argv", Some("std::env::args")),
                entry(8, "std::env", Some("current_dir")),
            ]
        );
        // A name a `use` took of an item is the item's: `Command::new` names nothing more than the `use` did
    }

    #[test]
    fn a_module_that_does_io_whatever_is_used_of_it_is_found_where_it_is_imported() {
        let source = "\
use std::fs;
pub use std::os::unix::{fs as ufs, net};
use std::fs::{self, read};
use std::io;
use std::net;
use std::process;
use std::os::unix::io::AsRawFd;
";
        assert_eq!(
            rust(source),
            [
                entry(1, "std::fs", None),
                entry(2, "std::os::fs", None),
                entry(2, "std::os::net", None),
                entry(3, "std::fs", None),
                entry(3, "std::fs", Some("read")),
            ]
        );
    }

    #[test]
    fn what_rust_does_partly_is_not_in_the_table() {
        let source = "\
use std::process::{ExitCode, ExitStatus, Output};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::io::{Read, Write, BufRead, Cursor, Result, ErrorKind, Error};
use std::io::prelude::*;
use std::env::{consts, var};
fn a() { std::process::exit(1); let _ = std::process::id(); let _ = std::env::consts::OS; }
use std::path::Path;
use std::collections::HashMap;
fn b() { let _ = format!(\"{}\", 1); let _ = write!(String::new(), \"x\"); }
";
        assert!(rust(source).is_empty(), "{:?}", rust(source));
    }

    #[test]
    fn a_rust_rename_of_the_module_or_of_one_above_it_is_followed() {
        let source = "\
use std::fs as f;
fn a() { f::read(\"a\"); }
use std as s;
fn b() { s::process::Command::new(\"x\"); s::net::TcpStream::connect(\"y\"); }
use std::{self as t, io as u};
fn c() { t::fs::write(\"a\", \"b\"); u::stderr(); }
use ::std::fs::File as F;
fn d() { let _ = ::std::env::current_exe(); }
";
        assert_eq!(
            rust(source),
            [
                entry(1, "std::fs", None),
                entry(2, "std::fs", Some("read")),
                entry(4, "std::net", Some("TcpStream")),
                entry(4, "std::process", Some("Command")),
                entry(6, "std::fs", Some("write")),
                entry(6, "std::io", Some("stderr")),
                entry(7, "std::fs", Some("File")),
                entry(8, "std::env", Some("current_exe")),
            ]
        );
    }

    #[test]
    fn a_glob_of_an_entry_or_of_a_module_above_it_fails_as_one_whose_names_cannot_be_read() {
        let found = in_rust(
            "use std::fs::*;\nuse std::io::*;\nuse std::*;\nuse std::os::unix::*;\nuse std::os::unix::fs::*;\nuse std::io::prelude::*;\nuse std::collections::*;\n",
            &Excluded::default(),
        );
        let said: Vec<(usize, &str, &Cause)> = found
            .iter()
            .map(|f| (f.line, f.entry.as_str(), &f.cause))
            .collect();
        let glob = |of: &str| Cause::Glob(of.to_string());
        assert!(said.contains(&(1, "std::fs", &glob("std::fs"))), "{said:?}");
        assert!(said.contains(&(2, "std::io", &glob("std::io"))));
        // Above the entries, a glob is an entry of its own, once for the line
        assert_eq!(
            said.iter().filter(|(line, _, _)| *line == 3).count(),
            1,
            "{said:?}"
        );
        assert!(said.contains(&(3, "std::*", &glob("std"))));
        assert!(said.contains(&(4, "std::os::unix::*", &glob("std::os::unix"))));
        assert!(said.contains(&(5, "std::os::fs", &glob("std::os::unix::fs"))));
        assert!(said.iter().all(|(line, _, _)| *line <= 5), "{said:?}");
    }

    #[test]
    fn the_os_modules_of_every_platform_are_one_entry_each() {
        let source = "\
use std::os::unix::fs::PermissionsExt;
use std::os::windows::fs::symlink_file;
use std::os::linux::net::TcpStreamExt;
use std::os::unix::process::CommandExt;
fn a() { std::os::wasi::fs::x(); }
use std::os::unix::io::AsRawFd;
use std::os::unix as u;
fn b() { u::fs::symlink(\"a\", \"b\"); }
";
        assert_eq!(
            rust(source),
            [
                entry(1, "std::os::fs", Some("PermissionsExt")),
                entry(2, "std::os::fs", Some("symlink_file")),
                entry(3, "std::os::net", Some("TcpStreamExt")),
                entry(4, "std::os::process", Some("CommandExt")),
                entry(5, "std::os::fs", Some("x")),
                entry(8, "std::os::fs", Some("symlink")),
            ]
        );
    }

    #[test]
    fn the_print_macros_and_the_terminal_items_are_one_entry_and_a_macro_of_another_path_is_not() {
        let source = "\
fn a() { println!(\"x\"); eprintln!(\"y\"); print!(\"p\"); eprint!(\"e\"); dbg!(1); }
fn b() { std::println!(\"x\"); ::std::dbg!(2); }
use std::println;
fn c() { log::println!(\"x\"); crate::print!(\"y\"); let _ = a != b; format_args!(\"{}\", 1); }
macro_rules! println { () => {}; }
fn d() { let _ = \"println!(x)\"; } // eprintln!(x)
";
        assert_eq!(
            rust(source),
            [
                entry(1, "std::io", Some("dbg!")),
                entry(1, "std::io", Some("eprint!")),
                entry(1, "std::io", Some("eprintln!")),
                entry(1, "std::io", Some("print!")),
                entry(1, "std::io", Some("println!")),
                entry(2, "std::io", Some("dbg!")),
                entry(2, "std::io", Some("println!")),
                entry(3, "std::io", Some("println!")),
            ]
        );
    }

    #[test]
    fn the_items_under_cfg_test_and_a_file_that_is_one_are_not_read() {
        let source = "\
pub fn a() -> u8 { 1 }
#[cfg(test)]
mod tests {
    use std::fs;
    #[test]
    fn t() { fs::read(\"x\"); println!(\"x\"); }
}
#[cfg(test)]
use std::process::Command;
#[cfg(test)]
fn helper() -> [u8; 2] { let _ = std::net::TcpStream::connect(\"x\"); [0; 2] }
fn b() { std::fs::read(\"b\"); }
";
        assert_eq!(rust(source), [entry(12, "std::fs", Some("read"))]);
        assert!(rust("#![cfg(test)]\nuse std::fs;\nfn t() { std::fs::read(\"x\"); }\n").is_empty());
        // A test of the other kinds of `cfg` is code
        assert_eq!(
            rust("#[cfg(not(test))]\nfn a() { std::fs::read(\"x\"); }\n").len(),
            1
        );
    }

    #[test]
    fn a_string_and_a_comment_name_nothing() {
        let source = "// std::fs::read\n/* println!(\"x\") */\nfn a() { let _ = \"std::process::Command\"; let _ = 'x'; }\n";
        assert!(rust(source).is_empty());
    }

    fn found(line: usize, entry: &str, name: Option<&str>, cause: Cause) -> Found {
        Found {
            line,
            entry: entry.into(),
            name: name.map(String::from),
            cause,
        }
    }

    #[test]
    fn a_finding_says_what_to_do_first_by_layer_and_names_what_is_used() {
        let source = "import shutil\nshutil.copyfile(a, b)\nshutil.rmtree(c)\n";
        let found = [
            found(1, "shutil", None, Cause::Used),
            found(2, "shutil", Some("copyfile"), Cause::Used),
            found(3, "shutil", Some("rmtree"), Cause::Used),
            found(3, "shutil", Some("rmtree"), Cause::Used),
        ];
        // One finding of the entry for the file, with the lines of all three
        let one = findings("domain/a.py", source, "domain", Language::Python, &found);
        assert_eq!(one.len(), 1);
        assert_eq!(
            one[0].detail,
            "domain/a.py:1: move the I/O into infrastructure.outbound, behind a port in protocol.outbound that the use \
             case is given: uses `shutil` (copyfile, rmtree), which does I/O, on lines 1, 2, 3\nimport shutil"
        );
        assert_eq!(one[0].file, "domain/a.py");
        assert_eq!(one[0].import, "shutil");
        assert_eq!(
            one[0].rule,
            Rule::InnerIo {
                names: vec!["copyfile".into(), "rmtree".into()]
            }
        );
        // A line alone says its line and no more
        let alone = findings(
            "domain/a.py",
            source,
            "domain",
            Language::Python,
            &found[1..2],
        );
        assert_eq!(
            alone[0].detail,
            "domain/a.py:2: move the I/O into infrastructure.outbound, behind a port in protocol.outbound that the use \
             case is given: uses `shutil` (copyfile), which does I/O\nshutil.copyfile(a, b)"
        );
        let model = super::findings(
            "ui/m.ts",
            source,
            "presentation",
            Language::TypeScript,
            &found[..1],
        );
        assert!(
            model[0].detail.starts_with(
                "ui/m.ts:1: a screen's model does no I/O: it gets what it needs from its page"
            ),
            "{}",
            model[0].detail
        );
        for layer in ["protocol", "application"] {
            let said = super::findings("x.py", source, layer, Language::Python, &found[..1]);
            assert!(
                said[0]
                    .detail
                    .contains(": move the I/O into infrastructure.outbound"),
                "{layer}"
            );
        }
    }

    #[test]
    fn the_lines_of_one_entry_are_one_finding_and_two_entries_are_two() {
        let source =
            "print('a')\nx = time.time()\nprint('b')\ny = time.time()\nprint('c')\nimport shutil\n";
        let terminal = |line| Found {
            line,
            entry: "terminal".into(),
            name: Some("print".into()),
            cause: Cause::Group(Group::Terminal),
        };
        let clock = |line| Found {
            line,
            entry: "clock".into(),
            name: Some("time.time".into()),
            cause: Cause::Group(Group::Clock),
        };
        let all = [
            clock(4),
            terminal(5),
            found(6, "shutil", None, Cause::Used),
            terminal(1),
            clock(2),
            terminal(3),
        ];
        let said = findings("domain/a.py", source, "domain", Language::Python, &all);
        // In the order of the first line of each, whatever the order of `found`
        let entries: Vec<&str> = said.iter().map(|one| one.import.as_str()).collect();
        assert_eq!(entries, ["terminal", "clock", "shutil"]);
        assert!(
            said[0].detail.starts_with("domain/a.py:1: ")
                && said[0].detail.ends_with(
                    "uses the terminal or opens a file (print), which does I/O (entry `terminal`), on lines 1, 3, 5\nprint('a')"
                ),
            "{}",
            said[0].detail
        );
        // The source text is the first line's, and the names are those of every line
        assert!(
            said[1].detail.ends_with(
                "reads the clock (time.time), which does I/O (entry `clock`), on lines 2, 4\nx = time.time()"
            ),
            "{}",
            said[1].detail
        );
        assert!(!said[2].detail.contains("on lines"), "{}", said[2].detail);
        assert_eq!(
            said[0].rule,
            Rule::InnerIo {
                names: vec!["print".into()]
            }
        );
        // What the lines say differently is not one finding: a glob of an entry and a use of it by name
        let said = findings(
            "a.rs",
            "use std::fs::*;\nfn a() { std::fs::read(\"a\"); }\nfn b() { std::fs::read(\"b\"); }\n",
            "domain",
            Language::Rust,
            &[
                found(1, "std::fs", None, Cause::Glob("std::fs".into())),
                found(2, "std::fs", Some("read"), Cause::Used),
                found(3, "std::fs", Some("read"), Cause::Used),
            ],
        );
        assert_eq!(said.len(), 2);
        assert!(
            said[0].detail.contains("a glob of `std::fs`") && !said[0].detail.contains("on lines")
        );
        assert!(
            said[1].detail.contains("on lines 2, 3"),
            "{}",
            said[1].detail
        );
        assert!(said.iter().all(|one| one.import == "std::fs"));
    }

    #[test]
    fn a_glob_and_a_scheme_are_said_as_what_they_are() {
        let source = "use std::fs::*;\nimport x from 'bun:sqlite';\n";
        let said = findings(
            "a.rs",
            source,
            "domain",
            Language::Rust,
            &[
                found(1, "std::fs", None, Cause::Glob("std::fs".into())),
                found(2, "bun:", Some("x"), Cause::Scheme("bun:sqlite".into())),
            ],
        );
        assert!(
            said[0].detail.contains("a glob of `std::fs`")
                && said[0].detail.contains("cannot be read")
        );
        assert!(
            said[1]
                .detail
                .contains("imports `bun:sqlite`: a scheme other than node:")
        );
        assert_eq!(said[1].import, "bun:");
    }

    #[test]
    fn four_layers_and_their_levels_are_judged_and_no_other() {
        let place = |name: &str, parent: Option<&str>| Place {
            name: name.into(),
            path: String::new(),
            parent: parent.map(String::from),
            role: String::new(),
            imports: Vec::new(),
            lacks: Vec::new(),
            below: Vec::new(),
            files: Vec::new(),
            unit: UnitTag::default(),
        };
        for judged in [
            place("domain", None),
            place("protocol", None),
            place("protocol.inbound", Some("protocol")),
            place("application", None),
            place("presentation", None),
            place("presentation.models", Some("presentation")),
        ] {
            assert!(is_judged(&judged), "{}", judged.name);
        }
        for free in [
            place("infrastructure", None),
            place("infrastructure.outbound", Some("infrastructure")),
            place("composition", None),
            place("ui", None),
            place("ui.pages", Some("ui")),
        ] {
            assert!(!is_judged(&free), "{}", free.name);
        }
    }

    #[test]
    fn the_guide_lists_the_table_of_its_stack() {
        for (language, names) in [
            (
                Language::Python,
                vec!["`shutil`", "`urllib.request`", "`_socket`"],
            ),
            (
                Language::TypeScript,
                vec!["`fs`", "`child_process`", "`sqlite`", "bun:"],
            ),
            (
                Language::Rust,
                vec!["`std::fs`", "`Command`", "`println!`", "`std::os::fs`"],
            ),
        ] {
            let text = guide(language);
            for name in names {
                assert!(text.contains(name), "{language:?}: {name}");
            }
            assert!(text.contains("rotproof approve"));
            // Every placeholder is filled
            for placeholder in [
                "tests", "hook", "table", "names", "passed", "keys", "allowed", "what", "default",
            ] {
                assert!(
                    !text.contains(&format!("{{{placeholder}}}")),
                    "{language:?}: {placeholder}"
                );
            }
            // The hook is a TypeScript screen's way to the browser
            assert_eq!(
                text.contains("gets what it needs from its page, through a hook in `ui.atoms`, or through a use case"),
                language == Language::TypeScript,
                "{language:?}"
            );
        }
    }

    #[test]
    fn the_guide_of_typescript_prints_the_allowlist_in_full() {
        let text = guide(Language::TypeScript);
        for name in typescript::GLOBALS_PURE {
            assert!(text.contains(&format!("`{name}`")), "{name}");
        }
        assert!(text.contains("`fetch`, `localStorage` or `process.cwd` for a global"));
        assert!(!text.contains("`os.getcwd`"));
    }

    #[test]
    fn the_guide_of_rust_prints_the_allowlist_the_net_and_the_alias_of_a_port() {
        let text = guide(Language::Rust);
        for name in rust::ENV_PURE
            .iter()
            .chain(&rust::NET_ALWAYS)
            .chain(&rust::NET_NO_ARGUMENT)
        {
            assert!(text.contains(&format!("`{name}`")), "{name}");
        }
        assert!(text.contains("`Box<dyn Fn() -> SystemTime>`, a generic `F: Fn() -> SystemTime`"));
        assert!(text.contains("`std::env` (`current_dir`) for a name of it that is I/O"));
        // `std::env` has one place: the allowlist, not a row of the table
        assert!(!table::RUST.iter().any(|row| row.key == "std::env"));
        assert!(!text.contains("`os.getcwd`") && !text.contains("`Date.now`"));
    }

    #[test]
    fn a_remedy_of_rust_names_the_callable_ports_of_the_language() {
        let said = Group::Clock.remedy("application", Language::Rust);
        assert!(
            said.ends_with("(a port may be a callable alias, `Box<dyn Fn() -> SystemTime>`, a generic `F: Fn() -> SystemTime`)"),
            "{said}"
        );
    }
}
