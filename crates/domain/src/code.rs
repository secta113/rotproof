//! What Rotproof reads from code, as plain values. The port to the readers that give them is `protocol::outbound`'s
//! `Parsers`.
//!
//! The readers wrap parsers (Ruff for Python, oxc for TypeScript, toml_edit for a `Cargo.toml`) and give what a file
//! imports, its comments and its first syntax error; the checks judge those values and never see a parser. The
//! rules on the values are here too: the module a Python file is, which files are TypeScript source, the alias an
//! entry of `compilerOptions.paths` makes, where a TypeScript import lands through the project's aliases, and where
//! one lands through the `package.json` of a unit's package.
//!
//! Where a TypeScript import lands is a path from the root, found without asking whether a file is there: the place of
//! an import is its directory. A relative specifier resolves against the file's directory, and one that starts with
//! `/` against the root, as Vite reads it. Another resolves through the aliases (`compilerOptions.paths`), then
//! through a `baseUrl` when a module is there, then through the name of a unit's package ([`Aliases::package`]);
//! otherwise it names a package that is no unit's, which no layer is. A query (`./logo.svg?url`) is not part of the
//! path. Whether a module is under a `baseUrl` is the one question asked of the tree: [`Aliases::resolve`] gives the
//! paths to ask about, and the caller asks.
//!
//! A unit's package is read from the `package.json` at the unit's `root`: `name` is what an import starts with, and
//! the rest of the specifier is a subpath of the package. With `exports`, only a subpath it lists is the package's,
//! each key an exact subpath or a pattern with one `*`, the longest prefix winning as in Node, and a target a path
//! from the package's root, or conditions among which the first of `types`, `import` and `default` that leads to a
//! path is taken. An import of a subpath it does not list names no file Node or tsc would find, so it is a finding of
//! its own, never a path ([`Reach::Unlisted`]); so does one it lists whose target holds no file, which the caller that
//! asks the tree finds ([`Reach::Missing`]). Without `exports`, a subpath is a path in the package, and the
//! package itself is its `types`, or its `main`, or its root. Not read: a `tsconfig` of the unit, `imports`
//! (`#name`), and a condition other than the three.

use std::collections::{BTreeMap, BTreeSet};

use crate::paths::{join, normalize, parent};

/// What one file of code holds: what it imports, as `I`, its comments and its first syntax error.
#[derive(Debug, Clone)]
pub struct Source<I> {
    /// Everything the file imports, each with the line (from 1) it is imported on
    pub imports: Vec<(usize, I)>,
    /// Every comment: its byte offset in the source, and its text with its delimiters. A reader reads comments past a
    /// syntax error too
    pub comments: Vec<(usize, String)>,
    /// Every use of a name that resolves to something outside the file's own code, in the order of the lines: what
    /// the checks of a language judge (`domain::environment::in_python` and `in_typescript`). See [`Use`]
    pub uses: Vec<Use>,
    /// What the reader met and could not read: the line (from 1), and what it is. In Python, the function a file calls
    /// to import a module whose name it does not write as a string (`importlib.import_module(name)`, a relative
    /// `".x"`), which is built at run time, and `eval` and `exec` (`builtins.eval`), whether called or taken as a
    /// value; in TypeScript, `import` and `require` of what is no literal string (`import()`, `require`), `eval` and
    /// `Function`
    pub unread: Vec<(usize, String)>,
    /// Every call of an attribute, whatever it is the attribute of (`a.b()`, `f().g()`, `self.x.y()`), in the order of
    /// the lines: what a rule judges by the method's name alone, when the type of the receiver cannot be known.
    /// Python's reader gives them; TypeScript's does not
    pub methods: Vec<Method>,
    /// Every method a class defines, with how many parameters it takes: what a rule that judges a method by its name
    /// leaves out when the project has one of its own. Python's reader gives them; TypeScript's does not
    pub defined: Vec<Defined>,
    /// The first syntax error, if any: its line and what the parser says. The parser recovers, but what follows may be
    /// misread
    pub error: Option<(usize, String)>,
}

impl<I> Default for Source<I> {
    fn default() -> Self {
        Source {
            imports: Vec::new(),
            comments: Vec::new(),
            uses: Vec::new(),
            unread: Vec::new(),
            methods: Vec::new(),
            defined: Vec::new(),
            error: None,
        }
    }
}

/// What the first positional argument of a call is, when it is a literal a rule asks about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum First {
    /// Anything else, and a call that has none
    #[default]
    Other,
    /// The literal `None` (Python): `time.gmtime(None)` reads the clock as `time.gmtime()` does
    Null,
    /// A string literal (Python's `p.open("w")`), with any quotes; an f-string is not one
    Text,
}

/// One call of an attribute in Python, whatever it is the attribute of: `p.read_text()`, `Path("a").exists()`,
/// `self.repo.mkdir(name)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Method {
    /// The line (from 1) of the attribute's name
    pub line: usize,
    /// The attribute called, without what it is the attribute of
    pub name: String,
    /// What the chain of names the call starts at resolves to, when its first name is an import or a builtin: the whole
    /// callee, as a [`Use`] names it (`os.path.exists` for `os.path.exists(p)`); empty when it starts at anything else
    /// (a variable, a parameter, a call, a subscript)
    pub resolved: Vec<String>,
    /// How many positional arguments, a `*args` not counted
    pub positional: usize,
    /// The names of the keyword arguments, in the order written
    pub keywords: Vec<String>,
    /// Whether the call has a `*args` or a `**kwargs`, which hides how many arguments there are
    pub unpacked: bool,
    /// What the first positional argument is
    pub first: First,
}

/// One method a class defines (a `def` directly in a class body), with how many parameters it takes, not counting
/// `self` (nor the `cls` of a `@classmethod`); a `@staticmethod` has no such parameter and counts them all. A `*args`
/// and a `**kwargs` count one each.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Defined {
    pub name: String,
    pub parameters: usize,
}

/// One use of a name in a Python or TypeScript file that resolves to something outside the file's own code, as the
/// scopes of the language resolve it (in Python: module, function and lambda, class body, comprehension, where a name
/// is the one of the scope it is bound in at all, wherever in the scope it is bound; in TypeScript: the block, the
/// function, the module, with what `var` hoists and the names `declare` makes).
///
/// A name is outside the file's own code when an `import` binds it (in Python `import a.b as c`, then `c.x` is
/// `a.b.x`; `from a import b as c`, then `c` is `a.b`; `import os`, then `os.path.exists` is `os.path.exists`; in
/// TypeScript `import * as fs from "fs"`, then `fs.readFile` is `"fs".readFile`), or when no scope binds it and it is a
/// builtin in Python (`open` is `builtins.open`) or a global in TypeScript (`fetch`). A name the file defines, a
/// parameter, a name assigned and, in Python, one no scope binds that is no builtin are not uses. A chain of names is
/// followed as far as it is static: the use is the whole of it (`os.environ.get`, `process.env.X`), and what comes
/// after a call or a subscript that is no literal is not part of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Use {
    /// The line (from 1) the use starts on
    pub line: usize,
    /// The name it resolves to, whole.
    ///
    /// **In Python** it is dotted: `os.path.exists`, `datetime.datetime.now`, `builtins.open`. The module `from m
    /// import *` brings the names of, for [`How::Star`].
    ///
    /// **In TypeScript** a global is as written, dotted, with what the global object (`window`, `globalThis`, `self`,
    /// `global`) holds taken as a global: `fetch`, `Date.now`, `localStorage.getItem`, `process.env.X`, and
    /// `import.meta.env.X` for `import.meta`. A name taken from a module starts with the specifier as written, in
    /// double quotes (it has dots and slashes in it, and `#` at times; a `"` or a `\` in it is escaped with a `\`),
    /// then the export it takes and the properties that follow, after dots: `"node:fs".readFile`,
    /// `"fs".promises.readFile` for `import * as fs` then `fs.promises.readFile`, `"react".default.useState` for
    /// `import React from "react"` then `React.useState`, and `"fs"` for the namespace itself. A name a declarator
    /// binds from `require("fs")` or `await import("fs")` with a literal is taken as an import's (`const fs =
    /// require("fs")` is the namespace, `const { readFile } = require("fs")` takes `readFile`). A property that is no
    /// identifier is written `["a-b"]`, after no dot. [`typescript_name`] writes one and [`split_typescript_name`]
    /// reads one back
    pub name: String,
    /// How the name is used
    pub how: How,
    /// Whether the use sits in a type position: an annotation, a bound or default of a type parameter, or the value
    /// of a `type` statement, and in it a name, an attribute or a subscript (`Path`, `os.PathLike`, `list[Path]`). A
    /// call in it, with its callee and its arguments, is not: it runs when the annotation is evaluated, so
    /// `Annotated[datetime, Field(default_factory=datetime.now)]` has `datetime` in a type position and `Field` and
    /// `datetime.now` out of it. A rule on what the code does at run time does not judge a use in a type position.
    ///
    /// TypeScript's reader leaves a type position out (a type reference is no value reference, and `import type` takes
    /// nothing a program runs), so it is `false` there
    pub in_type: bool,
}

/// How a name is used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum How {
    /// The name is taken at an import, `from m import n` (`m.n`, resolved against the package for a relative `m`),
    /// whether the file uses it or not. `import m` is no use: it is in [`Source::imports`]. In TypeScript, a named or
    /// a default import (`import { readFile } from "node:fs"` is `"node:fs".readFile`, `import React from "react"` is
    /// `"react".default`), at the line of the specifier; `import * as fs` takes no name, and `import type` is no use
    Imported,
    /// `from m import *`: the names it brings in cannot be read. The use is of the module `m`; the names the file then
    /// uses without binding them are not uses
    Star,
    /// The name is called: `os.getenv("X")`, `datetime.now()`, `open(path)`
    Called {
        /// How many positional arguments, a `*args` not counted
        positional: usize,
        /// The names of the keyword arguments, in the order written (`**kwargs` is not one)
        keywords: Vec<String>,
        /// Whether the call has a `*args` or a `**kwargs`, which hides how many arguments there are
        unpacked: bool,
        /// What the first positional argument is: Python's reader tells `None` and a string literal, TypeScript's
        /// leaves it [`First::Other`]
        first: First,
    },
    /// The name is used as a value, and called by no one here: `default_factory=datetime.now`, `f = time.time`,
    /// `os.environ["A"]`, `getattr(os, "environ")` (the `os`), `o = os`, `@functools.cache`; in TypeScript
    /// `useState(Date.now)`, `const f = fetch`, `process.env.X`, `document.title`. When the name is a module (`os`),
    /// the module object itself is used: the reader does not know which names are modules, a rule asks its own table
    /// of them
    Value,
    /// TypeScript's `new`: `new Date()` is `Date` constructed with none, `new Date(x)` with one. Python's reader gives
    /// none: a class is called there
    Constructed {
        /// How many arguments, a spread not counted
        positional: usize,
        /// Whether one is a spread (`new Date(...args)`), which hides how many there are
        unpacked: bool,
    },
    /// The name is the operand of `typeof` (TypeScript): `typeof window`, `typeof process.env`. Nothing of it runs but
    /// the test of what it is, which does not fail for a name nothing binds, so a rule on what the code reads may skip
    /// it. Python's reader gives none
    TypeOf,
}

/// The name of a TypeScript [`Use`] taken from `module` (as written, `None` for a global) with the `parts` that follow
/// it: `"node:fs".readFile`, `localStorage.getItem`, `import.meta.env`, `["a-b"]` for a part that is no identifier.
pub fn typescript_name(module: Option<&str>, parts: &[String]) -> String {
    let escape = |text: &str| text.replace('\\', "\\\\").replace('"', "\\\"");
    let mut name = String::new();
    if let Some(module) = module {
        name.push('"');
        name.push_str(&escape(module));
        name.push('"');
    }
    for part in parts {
        let mut chars = part.chars();
        let identifier = chars
            .next()
            .is_some_and(|c| c.is_alphabetic() || c == '_' || c == '$')
            && chars.all(|c| c.is_alphanumeric() || c == '_' || c == '$');
        if identifier {
            if !name.is_empty() {
                name.push('.');
            }
            name.push_str(part);
        } else {
            name.push_str("[\"");
            name.push_str(&escape(part));
            name.push_str("\"]");
        }
    }
    name
}

/// The module a name of TypeScript's [`Use`] is taken from, unquoted (`None` for a global), and its names, each
/// without the dot or the brackets: `"node:fs".readFile` is `Some("node:fs")` and `["readFile"]`, `fetch` is `None` and
/// `["fetch"]`, `"react".default.useState` is `Some("react")` and `["default", "useState"]`, and `"fs"` alone has none
/// (the module itself).
pub fn split_typescript_name(name: &str) -> (Option<String>, Vec<String>) {
    let mut chars = name.chars().peekable();
    let module = (chars.peek() == Some(&'"')).then(|| quoted(&mut chars));
    let mut parts = Vec::new();
    while let Some(&next) = chars.peek() {
        match next {
            '.' => {
                chars.next();
            }
            '[' => {
                chars.next();
                parts.push(quoted(&mut chars));
                // The closing bracket
                chars.next();
            }
            _ => {
                let mut part = String::new();
                while let Some(&c) = chars.peek() {
                    if c == '.' || c == '[' {
                        break;
                    }
                    part.push(c);
                    chars.next();
                }
                parts.push(part);
            }
        }
    }
    (module, parts)
}

/// What a string in double quotes holds, the quotes taken: a `\` stands for the character after it.
fn quoted(chars: &mut std::iter::Peekable<std::str::Chars>) -> String {
    let mut text = String::new();
    chars.next();
    while let Some(c) = chars.next() {
        match c {
            '"' => break,
            '\\' => text.extend(chars.next()),
            _ => text.push(c),
        }
    }
    text
}

/// What one import of a TypeScript or JavaScript file names: the module, and the names it takes from it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Import {
    /// The specifier, as written
    pub specifier: String,
    /// The names it takes, as the module exports them: `default`, `*` for the whole module (`import * as ns`,
    /// `export * from`), and each name of `import { a as b }` or `export { a as b } from` as `a`. Empty for an import
    /// that takes none (`import "x"`, `import("x")`, `import.meta.resolve("x")`, `require("x")`,
    /// `import x = require("x")`, `import("x")` in a type), which is read as the module
    pub names: Vec<String>,
}

/// The module a file is, as parts: `ui/pages/home.py` is `ui.pages.home`, and `ui/pages/__init__.py` is `ui.pages`.
pub fn module_parts(path: &str) -> Vec<String> {
    let mut parts: Vec<String> = path.split('/').map(String::from).collect();
    let last = parts.pop().unwrap_or_default();
    // `.PY` too: Windows runs it with Python
    let stem = &last[..last.len() - ".py".len()];
    if !stem.eq_ignore_ascii_case("__init__") {
        parts.push(stem.to_string());
    }
    parts
}

/// The extensions read as source, in lower case
pub const EXTENSIONS: [&str; 8] = ["ts", "tsx", "mts", "cts", "js", "jsx", "mjs", "cjs"];

/// Whether a file is TypeScript or JavaScript source, by its extension in any case.
pub fn is_source(path: &str) -> bool {
    path.rsplit_once('.')
        .is_some_and(|(_, extension)| EXTENSIONS.contains(&extension.to_ascii_lowercase().as_str()))
}

/// `path` without the extension that makes it source (`a/b.ts` is `a/b`, `a/b.d.ts` is `a/b`), which an import leaves
/// off as often as not: the name of the module an import lands in.
pub fn without_extension(path: &str) -> &str {
    if !is_source(path) {
        return path;
    }
    let stem = path.rsplit_once('.').map_or(path, |(stem, _)| stem);
    stem.strip_suffix(".d").unwrap_or(stem)
}

/// One entry of `compilerOptions.paths`: a pattern with at most one `*`, and the first of its targets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Alias {
    /// Before the `*`, or the whole pattern
    pub prefix: String,
    /// After the `*`; `None` for a pattern without one, which matches only itself
    pub suffix: Option<String>,
    /// The target, with `*` where the matched text goes, from the root
    pub target: String,
    /// The config that maps it, for the message
    pub config: String,
}

/// How a specifier that is not relative resolves in this project, from the `tsconfig*.json` files at the root.
#[derive(Debug, Default)]
pub struct Aliases {
    pub paths: Vec<Alias>,
    /// The `baseUrl` directories, from the root
    pub base_urls: BTreeSet<String>,
    /// The package of each unit that has a `package.json` with a name, once for each root
    pub packages: Vec<Package>,
    /// What could not be read, each with the config it is in: the aliases it would give are not known
    pub problems: Vec<String>,
}

/// Where a specifier lands, as far as the aliases say.
#[derive(Debug, PartialEq, Eq)]
pub enum Landing {
    /// A path from the root, or `None` for a package, or a path above the root
    At(Option<String>),
    /// The first of these paths (from the root, under each `baseUrl` in turn) where a module is, by
    /// [`module_files`]; a package when there is none
    UnderBaseUrl(Vec<String>),
}

/// What an `exports` entry leads to, as `package.json` writes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// A path from the package's root, written `./...`, with a `*` where a pattern's match goes
    Path(String),
    /// Conditions, each with what it leads to, in the order written
    Conditions(Vec<(String, Target)>),
    /// A list, of which the first that leads to a path is taken
    Any(Vec<Target>),
    /// `null`: the subpath is not exported
    Nothing,
}

/// The conditions Rotproof takes, in the order it tries them: `types` is what tsc reads, `import` what an import
/// takes, and `default` what is left. Any other (`node`, `require`, `browser`) is passed over.
const CONDITIONS: [&str; 3] = ["types", "import", "default"];

impl Target {
    /// The path this leads to, `star` put where the pattern's `*` was, from the package's root; `None` when it leads
    /// to none: `null`, a path that is not written `./...`, or none of the three conditions.
    fn path(&self, star: &str) -> Option<String> {
        match self {
            Target::Path(path) => path.strip_prefix("./").map(|rest| rest.replace('*', star)),
            Target::Conditions(conditions) => CONDITIONS.iter().find_map(|wanted| {
                conditions
                    .iter()
                    .find(|(name, _)| name == wanted)
                    .and_then(|(_, target)| target.path(star))
            }),
            Target::Any(targets) => targets.iter().find_map(|target| target.path(star)),
            Target::Nothing => None,
        }
    }
}

/// The package of a unit, from its `package.json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Package {
    /// `name`: what an import of the package starts with
    pub name: String,
    /// The directory the `package.json` is in, from the root
    pub root: String,
    /// The `package.json`, from the root, for the message
    pub config: String,
    /// `exports` as its entries, each a key (`.` or `./subpath`, with at most one `*`) and what it leads to: a string
    /// or a map of conditions alone is the entry `.`. `None` when the package has no `exports`
    pub exports: Option<Vec<(String, Target)>>,
    /// `types` (or `typings`) and `main`, as written, from the root of the package
    pub types: Option<String>,
    pub main: Option<String>,
}

/// Where an import through a unit's package lands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reach {
    /// A path from the root, or `None` for one above the root
    At(Option<String>),
    /// The package has `exports` and the subpath is not among them (or is `null`, or leads to no path by the three
    /// conditions): Node and tsc fail it, and where it would land is not known
    Unlisted {
        /// The `package.json`, from the root
        config: String,
    },
    /// The package exports the subpath, but the path it leads to holds no file: a pattern such as
    /// `"./*": "./src/*/index.ts"` takes `domain/invoice.ts` to `src/domain/invoice.ts/index.ts`. The package never
    /// gives this itself, as it does not look at the tree: the caller that does (`lands`) turns an `At` into it
    Missing {
        /// The `package.json`, from the root
        config: String,
        /// Where the export leads, from the root, where no file is
        path: String,
    },
}

impl Package {
    /// Where the subpath `sub` of this package (`protocol/inbound`; "" for the package itself) lands.
    pub fn reach(&self, sub: &str) -> Reach {
        let at = |path: &str| Reach::At(normalize(&join(&self.root, path)));
        let Some(exports) = &self.exports else {
            let main = self.types.as_deref().or(self.main.as_deref()).unwrap_or("");
            return at(if sub.is_empty() { main } else { sub });
        };
        let key = if sub.is_empty() {
            ".".to_string()
        } else {
            format!("./{sub}")
        };
        match export_for(exports, &key).and_then(|(target, star)| target.path(&star)) {
            Some(path) => at(&path),
            None => Reach::Unlisted {
                config: self.config.clone(),
            },
        }
    }
}

/// The entry of `exports` that `key` is, and the text a `*` of its key matched: an exact key first, then the pattern
/// with the longest part before its `*`, as Node chooses.
fn export_for<'a>(exports: &'a [(String, Target)], key: &str) -> Option<(&'a Target, String)> {
    if let Some((_, target)) = exports.iter().find(|(name, _)| name == key)
        && !key.contains('*')
    {
        return Some((target, String::new()));
    }
    exports
        .iter()
        .filter(|(name, _)| name.matches('*').count() == 1)
        .filter_map(|(name, target)| {
            let (before, after) = name.split_once('*')?;
            let star = key.strip_prefix(before)?.strip_suffix(after)?;
            // `*` matches at least one character
            (!star.is_empty()).then(|| (before.len(), name.len(), target, star.to_string()))
        })
        .max_by_key(|(before, length, _, _)| (*before, *length))
        .map(|(_, _, target, star)| (target, star))
}

impl Alias {
    /// The alias one entry of `paths`, `pattern` mapped to its first `target`, makes, the target resolved against the
    /// directory `base` (from the root) and the entry kept as the one of `config`. `None` when the entry cannot be
    /// read: a pattern with more than one `*`, or a target with more `*` than the pattern, since there would be no
    /// text to put in the extra one.
    pub fn from_entry(pattern: &str, target: &str, base: &str, config: &str) -> Option<Alias> {
        let stars = pattern.matches('*').count();
        if stars > 1 || target.matches('*').count() > stars {
            return None;
        }
        let (prefix, suffix) = match pattern.split_once('*') {
            Some((prefix, suffix)) => (prefix.to_string(), Some(suffix.to_string())),
            None => (pattern.to_string(), None),
        };
        Some(Alias {
            prefix,
            suffix,
            target: join(base, target),
            config: config.to_string(),
        })
    }

    /// The target `specifier` maps to, with what the `*` matched in place of the target's `*`; `None` when the
    /// pattern does not match it.
    pub fn mapped(&self, specifier: &str) -> Option<String> {
        let rest = specifier.strip_prefix(&self.prefix)?;
        match &self.suffix {
            None => rest.is_empty().then(|| self.target.clone()),
            Some(suffix) => rest
                .strip_suffix(suffix.as_str())
                .map(|star| self.target.replacen('*', star, 1)),
        }
    }
}

impl Aliases {
    /// Where `specifier`, imported by the file at `file` (from the root), lands, in the order TypeScript tries: a
    /// relative path, a path from the root, `paths`, then each `baseUrl`.
    pub fn resolve(&self, file: &str, specifier: &str) -> Landing {
        let specifier = specifier.split('?').next().unwrap_or(specifier);
        if is_relative(specifier) {
            return Landing::At(normalize(&join(parent(file), specifier)));
        }
        if let Some(from_root) = specifier.strip_prefix('/') {
            return Landing::At(normalize(from_root));
        }
        if let Some(target) = self.mapped(specifier) {
            return Landing::At(normalize(&target));
        }
        Landing::UnderBaseUrl(
            self.base_urls
                .iter()
                .filter_map(|base| normalize(&join(base, specifier)))
                .collect(),
        )
    }

    /// Where `specifier` lands through the package of a unit whose name it starts with (the longest, when two do), or
    /// `None` when it names no such package. It is tried after `paths` and `baseUrl`, as tsc does.
    pub fn package(&self, specifier: &str) -> Option<Reach> {
        self.package_of(specifier)
            .map(|(package, sub)| package.reach(sub))
    }

    /// The package of a unit whose name `specifier` starts with (the longest, when two do), and the subpath that is
    /// left of it (`protocol/inbound`; "" for the package itself).
    pub fn package_of<'a>(&'a self, specifier: &'a str) -> Option<(&'a Package, &'a str)> {
        let specifier = specifier.split('?').next().unwrap_or(specifier);
        self.packages
            .iter()
            .filter_map(|package| {
                let rest = specifier.strip_prefix(&package.name)?;
                let sub = if rest.is_empty() {
                    rest
                } else {
                    rest.strip_prefix('/')?
                };
                Some((package, sub))
            })
            .max_by_key(|(package, _)| package.name.len())
    }

    /// The target of the alias in `paths` that maps `specifier`: the one with the longest prefix, as in TypeScript.
    fn mapped(&self, specifier: &str) -> Option<String> {
        self.paths
            .iter()
            .filter_map(|alias| Some((alias.prefix.len(), alias.mapped(specifier)?)))
            .max_by_key(|(length, _)| *length)
            .map(|(_, target)| target)
    }
}

/// Whether `specifier` is relative to the file that imports it: `.`, `..`, or a path that starts with either.
fn is_relative(specifier: &str) -> bool {
    specifier == "."
        || specifier == ".."
        || specifier.starts_with("./")
        || specifier.starts_with("../")
}

/// The files that make a module at `path` (from the root), any one of them enough: the file itself, with one of
/// [`EXTENSIONS`], or an `index` in it.
pub fn module_files(path: &str) -> Vec<String> {
    let mut files = vec![path.to_string()];
    for extension in EXTENSIONS {
        files.push(format!("{path}.{extension}"));
        files.push(format!("{path}/index.{extension}"));
    }
    files
}

/// Where a dependency comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Origin {
    /// A path, from the manifest's directory, as written
    Path(String),
    /// The entry of the same name in its workspace's `[workspace.dependencies]`
    Workspace,
}

/// A dependency that can name a path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dependency {
    /// The line (from 1) of its name
    pub line: usize,
    /// Its name in the manifest (with `package`, the name the crate is used by)
    pub name: String,
    pub origin: Origin,
}

/// What one manifest holds.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Manifest {
    /// Every dependency read that comes from a path or from the workspace, in the order written
    pub dependencies: Vec<Dependency>,
    /// Whether the manifest declares a package (`[package]`): a manifest that declares only `[workspace]` builds no
    /// crate
    pub is_package: bool,
    /// `[package] workspace`: the workspace's directory, from the manifest's directory
    pub workspace: Option<String>,
    /// Whether the manifest declares a workspace
    pub is_workspace: bool,
    /// `[workspace.dependencies]`: name -> its path from the manifest's directory, or `None` for one from a registry
    /// or git
    pub workspace_dependencies: BTreeMap<String, Option<String>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_alias_with_the_longest_prefix_maps_and_a_suffix_must_match() {
        let alias = |pattern: &str, target: &str| {
            Alias::from_entry(pattern, target, "", "tsconfig.json").unwrap()
        };
        let aliases = Aliases {
            paths: vec![
                alias("@/*", "src/*"),
                alias("@/ui/*", "lib/ui/*"),
                alias("#styles/*.css", "src/styles/*.css"),
                alias("#theme", "src/theme"),
            ],
            ..Aliases::default()
        };
        let at = |s: &str| match aliases.resolve("src/a.ts", s) {
            Landing::At(path) => path,
            other => panic!("{s}: {other:?}"),
        };
        assert_eq!(at("@/domain/song"), Some("src/domain/song".into()));
        assert_eq!(at("@/ui/button"), Some("lib/ui/button".into()));
        assert_eq!(at("#styles/main.css"), Some("src/styles/main.css".into()));
        assert_eq!(at("#theme"), Some("src/theme".into()));
        // A pattern without `*` matches only itself, and one with a suffix only what ends in it
        assert_eq!(
            aliases.resolve("src/a.ts", "#theme/dark"),
            Landing::UnderBaseUrl(Vec::new())
        );
        assert_eq!(
            aliases.resolve("src/a.ts", "#styles/main.scss"),
            Landing::UnderBaseUrl(Vec::new())
        );
    }

    #[test]
    fn an_entry_of_paths_with_one_star_at_most_on_both_sides_is_an_alias() {
        let alias = Alias::from_entry("@/*", "*", "src", "tsconfig.app.json").unwrap();
        assert_eq!(alias.prefix, "@/");
        assert_eq!(alias.suffix.as_deref(), Some(""));
        assert_eq!(alias.target, "src/*");
        assert_eq!(alias.config, "tsconfig.app.json");
        // A pattern without `*` is itself, and so is its target, from the root when the base is
        let theme = Alias::from_entry("#theme", "ui/theme", "", "tsconfig.json").unwrap();
        assert_eq!((theme.prefix.as_str(), theme.suffix), ("#theme", None));
        assert_eq!(theme.target, "ui/theme");
        // A suffix is what follows the `*`
        let css = Alias::from_entry("#styles/*.css", "s/*.css", "src", "t.json").unwrap();
        assert_eq!(
            (css.prefix.as_str(), css.suffix.as_deref()),
            ("#styles/", Some(".css"))
        );
        // Two `*` in the pattern, or more in the target than in the pattern, cannot be read
        for (pattern, target) in [("*/*", "src/*"), ("a", "src/*"), ("a/*", "*/*")] {
            assert_eq!(
                Alias::from_entry(pattern, target, "", "t.json"),
                None,
                "{pattern} {target}"
            );
        }
    }

    #[test]
    fn a_relative_or_rooted_specifier_resolves_against_the_file() {
        let aliases = Aliases::default();
        let at = |s: &str| match aliases.resolve("src/ui/atoms/button.tsx", s) {
            Landing::At(path) => path,
            other => panic!("{s}: {other:?}"),
        };
        assert_eq!(at("../molecules/row"), Some("src/ui/molecules/row".into()));
        assert_eq!(
            at("./theme.css?inline"),
            Some("src/ui/atoms/theme.css".into())
        );
        assert_eq!(at("../../domain"), Some("src/domain".into()));
        assert_eq!(at("/src/utils/x"), Some("src/utils/x".into()));
        assert_eq!(at("../../../../../out"), None);
        // Without a baseUrl, nothing to ask about: a package
        assert_eq!(
            aliases.resolve("src/a.ts", "react"),
            Landing::UnderBaseUrl(Vec::new())
        );
    }

    #[test]
    fn under_a_base_url_a_specifier_is_each_path_it_names_in_turn() {
        let aliases = Aliases {
            base_urls: ["src".to_string(), "lib".to_string()].into(),
            ..Aliases::default()
        };
        assert_eq!(
            aliases.resolve("src/a.ts", "domain/song?raw"),
            Landing::UnderBaseUrl(vec!["lib/domain/song".into(), "src/domain/song".into()])
        );
        let files = module_files("src/domain/song");
        assert_eq!(files.len(), 1 + 2 * EXTENSIONS.len());
        assert!(files.contains(&"src/domain/song".into()));
        assert!(files.contains(&"src/domain/song.tsx".into()));
        assert!(files.contains(&"src/domain/song/index.mjs".into()));
    }

    #[test]
    fn a_python_file_is_the_module_its_path_names() {
        let parts = |path: &str| module_parts(path).join(".");
        assert_eq!(parts("ui/pages/home.py"), "ui.pages.home");
        assert_eq!(parts("ui/pages/__init__.py"), "ui.pages");
        assert_eq!(parts("domain/Model.PY"), "domain.Model");
    }

    #[test]
    fn the_name_of_a_typescript_use_is_written_and_read_back() {
        let parts = |names: &[&str]| names.iter().map(|n| n.to_string()).collect::<Vec<_>>();
        for (module, names, written) in [
            (None, &["fetch"][..], "fetch"),
            (None, &["process", "env", "X"], "process.env.X"),
            (None, &["import", "meta", "env"], "import.meta.env"),
            (Some("node:fs"), &["readFile"], "\"node:fs\".readFile"),
            (
                Some("react"),
                &["default", "useState"],
                "\"react\".default.useState",
            ),
            (Some("fs"), &[], "\"fs\""),
            (Some("#internal/x.js"), &["a"], "\"#internal/x.js\".a"),
            (None, &["window", "a-b", "c"], "window[\"a-b\"].c"),
            (Some("a\"b\\c"), &["d.e"], "\"a\\\"b\\\\c\"[\"d.e\"]"),
        ] {
            let names = parts(names);
            assert_eq!(typescript_name(module, &names), written);
            assert_eq!(
                split_typescript_name(written),
                (module.map(String::from), names),
                "{written}"
            );
        }
    }

    #[test]
    fn typescript_source_is_named_by_its_extension_in_any_case() {
        assert!(is_source("src/a.ts") && is_source("src/A.TSX") && is_source("x.mjs"));
        assert!(!is_source("src/a.css") && !is_source("src/a.d") && !is_source("ts"));
    }
}
