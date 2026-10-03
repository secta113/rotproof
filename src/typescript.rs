//! Reading TypeScript and JavaScript with oxc: what a file imports and where each import lands, and its comments.
//!
//! - **What a file imports:** `import` (`import type` too), `export ... from`, `import("...")` and `require("...")`
//!   with a literal string, `import x = require("...")`, and `import("...")` in a type. A specifier built at run time
//!   is not seen.
//! - **Where an import lands,** as a path from the root, without asking whether a file is there: the place of an
//!   import is its directory. A relative specifier resolves against the file's directory, and one that starts with
//!   `/` against the root, as Vite reads it. Another resolves through `compilerOptions.paths` of the `tsconfig*.json`
//!   files at the root and the local files they extend, then through their `baseUrl` when a module is there; otherwise
//!   it names a package, which no layer is. A query (`./logo.svg?url`) is not part of the path.
//! - **Its comments,** `//` and `/* */`. JSX text is not a comment.

use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::path::Path;

use oxc_allocator::Allocator;
use oxc_ast::ast::{
    CallExpression, ExportAllDeclaration, ExportFromDeclaration, Expression, ImportDeclaration,
    ImportExpression, TSImportEqualsDeclaration, TSImportType, TSModuleReference,
};
use oxc_ast_visit::{Visit, walk};
use oxc_parser::Parser;
use oxc_span::SourceType;
use serde_json::Value;

use crate::source::{line_of, read_source};

/// The extensions read as source, in lower case
pub const EXTENSIONS: [&str; 8] = ["ts", "tsx", "mts", "cts", "js", "jsx", "mjs", "cjs"];

/// Whether a file is TypeScript or JavaScript source, by its extension in any case.
pub fn is_source(path: &str) -> bool {
    path.rsplit_once('.')
        .is_some_and(|(_, extension)| EXTENSIONS.contains(&extension.to_ascii_lowercase().as_str()))
}

/// What one file holds.
#[derive(Debug, Default)]
pub struct Source {
    /// Every specifier the file imports, with its line (from 1)
    pub imports: Vec<(usize, String)>,
    /// Every comment: its byte offset in the source, and its text with its delimiters
    pub comments: Vec<(usize, String)>,
    /// The first syntax error, if any: its line and what the parser says. The parser recovers, but what follows may be
    /// misread
    pub error: Option<(usize, String)>,
}

/// Read one file, whose path from the root says how: `.tsx` with JSX, `.d.ts` as declarations.
pub fn read(source: &str, path: &str) -> Source {
    let allocator = Allocator::default();
    // The extension in lower case: Windows reads `App.TSX` as the same file
    let source_type =
        SourceType::from_path(path.to_ascii_lowercase()).unwrap_or_else(|_| SourceType::tsx());
    let parsed = Parser::new(&allocator, source, source_type).parse();
    let mut imports = Imports {
        source,
        found: Vec::new(),
    };
    imports.visit_program(&parsed.program);
    let comments = parsed
        .program
        .comments
        .iter()
        .map(|comment| {
            let (start, end) = (comment.span.start as usize, comment.span.end as usize);
            (start, source[start..end].to_string())
        })
        .collect();
    let error = parsed.diagnostics.errors().next().map(|e| {
        let at = e.labels.first().map_or(0, |label| label.offset() as usize);
        (line_of(source, at), e.message.to_string())
    });
    Source {
        imports: imports.found,
        comments,
        error,
    }
}

struct Imports<'s> {
    source: &'s str,
    found: Vec<(usize, String)>,
}

impl Imports<'_> {
    fn add(&mut self, at: u32, specifier: &str) {
        self.found
            .push((line_of(self.source, at as usize), specifier.to_string()));
    }
}

impl<'a> Visit<'a> for Imports<'_> {
    fn visit_import_declaration(&mut self, it: &ImportDeclaration<'a>) {
        self.add(it.source.span.start, &it.source.value);
        walk::walk_import_declaration(self, it);
    }

    fn visit_export_all_declaration(&mut self, it: &ExportAllDeclaration<'a>) {
        self.add(it.source.span.start, &it.source.value);
        walk::walk_export_all_declaration(self, it);
    }

    fn visit_export_from_declaration(&mut self, it: &ExportFromDeclaration<'a>) {
        self.add(it.source.span.start, &it.source.value);
        walk::walk_export_from_declaration(self, it);
    }

    fn visit_import_expression(&mut self, it: &ImportExpression<'a>) {
        if let Some(specifier) = literal(&it.source) {
            self.add(it.span.start, specifier);
        }
        walk::walk_import_expression(self, it);
    }

    fn visit_call_expression(&mut self, it: &CallExpression<'a>) {
        let is_require = matches!(&it.callee, Expression::Identifier(id) if id.name == "require");
        if is_require
            && it.arguments.len() == 1
            && let Some(specifier) = it.arguments[0].as_expression().and_then(literal)
        {
            self.add(it.span.start, specifier);
        }
        walk::walk_call_expression(self, it);
    }

    fn visit_ts_import_equals_declaration(&mut self, it: &TSImportEqualsDeclaration<'a>) {
        if let TSModuleReference::ExternalModuleReference(reference) = &it.module_reference {
            self.add(reference.expression.span.start, &reference.expression.value);
        }
        walk::walk_ts_import_equals_declaration(self, it);
    }

    fn visit_ts_import_type(&mut self, it: &TSImportType<'a>) {
        self.add(it.source.span.start, &it.source.value);
        walk::walk_ts_import_type(self, it);
    }
}

/// The text of a literal string: `'x'`, or a template without expressions. `None` for anything built at run time.
fn literal<'b>(expression: &'b Expression) -> Option<&'b str> {
    match expression {
        Expression::StringLiteral(string) => Some(string.value.as_str()),
        Expression::TemplateLiteral(template) if template.expressions.is_empty() => template
            .quasis
            .first()
            .and_then(|quasi| quasi.value.cooked.as_ref())
            .map(|cooked| cooked.as_str()),
        _ => None,
    }
}

/// One entry of `compilerOptions.paths`: a pattern with at most one `*`, and the first of its targets.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Alias {
    /// Before the `*`, or the whole pattern
    prefix: String,
    /// After the `*`; `None` for a pattern without one, which matches only itself
    suffix: Option<String>,
    /// The target, with `*` where the matched text goes, from the root
    target: String,
    /// The config that maps it, for the message
    config: String,
}

/// How a specifier that is not relative resolves in this project, from the `tsconfig*.json` files at the root.
#[derive(Debug, Default)]
pub struct Aliases {
    paths: Vec<Alias>,
    /// The `baseUrl` directories, from the root
    base_urls: BTreeSet<String>,
    /// What could not be read, each with the config it is in: the aliases it would give are not known
    pub problems: Vec<String>,
}

/// What one config says, its `extends` followed.
#[derive(Debug, Default, Clone)]
struct Options {
    /// From the root
    base_url: Option<String>,
    /// Pattern -> target, and the directory (from the root) the targets resolve against when there is no `baseUrl`
    paths: Option<(Vec<(String, String)>, String)>,
}

/// The aliases of the project at `root`. An error is a config that cannot be read at all from disk.
pub fn aliases(root: &Path) -> io::Result<Aliases> {
    let mut names: Vec<String> = Vec::new();
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with("tsconfig") && name.ends_with(".json") && entry.file_type()?.is_file() {
            names.push(name);
        }
    }
    names.sort();
    let mut aliases = Aliases::default();
    for name in &names {
        let mut seen = Vec::new();
        let Some(options) = load(root, name, &mut seen, &mut aliases.problems)? else {
            continue;
        };
        if let Some(base) = &options.base_url {
            aliases.base_urls.insert(base.clone());
        }
        let Some((paths, dir)) = options.paths else {
            continue;
        };
        let base = options.base_url.clone().unwrap_or(dir);
        for (pattern, target) in paths {
            let stars = pattern.matches('*').count();
            if stars > 1 || target.matches('*').count() > stars {
                aliases.problems.push(format!(
                    "{name}: paths maps {pattern:?} to {target:?}, which Rotproof cannot read: one * at most, on both \
                     sides"
                ));
                continue;
            }
            let (prefix, suffix) = match pattern.split_once('*') {
                Some((prefix, suffix)) => (prefix.to_string(), Some(suffix.to_string())),
                None => (pattern.clone(), None),
            };
            let target = join(&base, &target);
            let alias = Alias {
                prefix,
                suffix,
                target,
                config: name.clone(),
            };
            match aliases
                .paths
                .iter()
                .find(|a| a.prefix == alias.prefix && a.suffix == alias.suffix)
            {
                Some(other) if other.target != alias.target => aliases.problems.push(format!(
                    "{} and {name} map {pattern:?} to different paths, so Rotproof does not know which a file uses",
                    other.config
                )),
                Some(_) => {}
                None => aliases.paths.push(alias),
            }
        }
    }
    Ok(aliases)
}

/// The options of the config at `name` (from the root), its local `extends` followed: a field it sets wins over the
/// configs it extends, and a later one in `extends` over an earlier one. `None`, with a problem, when it cannot be read.
fn load(
    root: &Path,
    name: &str,
    seen: &mut Vec<String>,
    problems: &mut Vec<String>,
) -> io::Result<Option<Options>> {
    if seen.contains(&name.to_string()) {
        problems.push(format!(
            "{name}: extends itself through {}",
            seen.join(", ")
        ));
        return Ok(None);
    }
    seen.push(name.to_string());
    let options = load_one(root, name, seen, problems);
    seen.pop();
    options
}

/// [`load`] for one config, while `seen` holds it and the configs that extend it.
fn load_one(
    root: &Path,
    name: &str,
    seen: &mut Vec<String>,
    problems: &mut Vec<String>,
) -> io::Result<Option<Options>> {
    let text = match read_source(&root.join(name)) {
        Ok(text) => text,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            problems.push(format!("{name}: extended by {}, and not there", seen[0]));
            return Ok(None);
        }
        Err(e) => return Err(io::Error::new(e.kind(), format!("{name}: {e}"))),
    };
    let json: Value = match serde_json::from_str(&strip_jsonc(&text)) {
        Ok(json) => json,
        Err(e) => {
            problems.push(format!(
                "{name}: cannot be read as JSON with comments ({e}), so its paths are not known"
            ));
            return Ok(None);
        }
    };
    let dir = parent(name);
    let mut options = Options::default();
    let extends: Vec<&str> = match json.get("extends") {
        Some(Value::String(one)) => vec![one.as_str()],
        Some(Value::Array(many)) => many.iter().filter_map(Value::as_str).collect(),
        _ => Vec::new(),
    };
    // A package's config (`@tsconfig/strictest`) resolves its paths inside the package: no layer is there
    for extended in extends.iter().filter(|e| e.starts_with('.')) {
        let mut path = join(&dir, extended);
        if !path.ends_with(".json") {
            path.push_str(".json");
        }
        if let Some(parent) = load(root, &path, seen, problems)? {
            options.base_url = parent.base_url.or(options.base_url);
            options.paths = parent.paths.or(options.paths);
        }
    }
    let compiler = json.get("compilerOptions");
    if let Some(base) = compiler.and_then(|c| c.get("baseUrl")) {
        match base.as_str() {
            Some(base) => options.base_url = Some(join(&dir, base)),
            None => problems.push(format!("{name}: baseUrl is not a string")),
        }
    }
    if let Some(paths) = compiler.and_then(|c| c.get("paths")) {
        let Some(map) = paths.as_object() else {
            problems.push(format!("{name}: paths is not an object"));
            return Ok(Some(options));
        };
        let mut entries = Vec::new();
        for (pattern, targets) in map {
            match targets
                .as_array()
                .and_then(|t| t.first())
                .and_then(Value::as_str)
            {
                Some(target) => entries.push((pattern.clone(), target.to_string())),
                None => problems.push(format!(
                    "{name}: paths maps {pattern:?} to {targets}, not to a list of paths"
                )),
            }
        }
        options.paths = Some((entries, dir));
    }
    Ok(Some(options))
}

impl Aliases {
    /// Where `specifier`, imported by the file at `file` (from the root), lands: a path from the root, or `None` for a
    /// package, or a path above the root.
    pub fn resolve(&self, root: &Path, file: &str, specifier: &str) -> Option<String> {
        let specifier = specifier.split('?').next().unwrap_or(specifier);
        if specifier == "."
            || specifier == ".."
            || specifier.starts_with("./")
            || specifier.starts_with("../")
        {
            return normalize(&format!("{}/{specifier}", parent(file)));
        }
        if let Some(from_root) = specifier.strip_prefix('/') {
            return normalize(from_root);
        }
        let matched = self
            .paths
            .iter()
            .filter_map(|alias| {
                let rest = specifier.strip_prefix(&alias.prefix)?;
                match &alias.suffix {
                    None => rest.is_empty().then(|| alias.target.clone()),
                    Some(suffix) => rest
                        .strip_suffix(suffix.as_str())
                        .map(|star| alias.target.replacen('*', star, 1)),
                }
                .map(|target| (alias.prefix.len(), target))
            })
            // The longest prefix wins, as in TypeScript
            .max_by_key(|(length, _)| *length);
        if let Some((_, target)) = matched {
            return normalize(&target);
        }
        self.base_urls.iter().find_map(|base| {
            let path = normalize(&join(base, specifier))?;
            is_module(root, &path).then_some(path)
        })
    }
}

/// Whether a module is at `path` (from the root): the file itself, with one of [`EXTENSIONS`], or an `index` in it.
fn is_module(root: &Path, path: &str) -> bool {
    root.join(path).is_file()
        || EXTENSIONS.iter().any(|extension| {
            root.join(format!("{path}.{extension}")).is_file()
                || root.join(format!("{path}/index.{extension}")).is_file()
        })
}

/// The directory of a path from the root: "" for a file at the root.
fn parent(path: &str) -> String {
    path.rsplit_once('/')
        .map_or(String::new(), |(dir, _)| dir.to_string())
}

/// `path` from the directory `dir` (from the root), not yet normalized.
fn join(dir: &str, path: &str) -> String {
    if dir.is_empty() {
        path.to_string()
    } else {
        format!("{dir}/{path}")
    }
}

/// A path from the root without `.`, `..` or empty parts. `None` when it climbs above the root.
fn normalize(path: &str) -> Option<String> {
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            part => parts.push(part),
        }
    }
    Some(parts.join("/"))
}

/// JSON with comments, as tsconfig files are written, made into JSON: `//` and `/* */` comments and the commas before
/// a closing bracket go, and strings are kept as they are.
fn strip_jsonc(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '"' {
            out.push(c);
            i += 1;
            while i < chars.len() {
                out.push(chars[i]);
                if chars[i] == '\\' && i + 1 < chars.len() {
                    out.push(chars[i + 1]);
                    i += 2;
                    continue;
                }
                i += 1;
                if chars[i - 1] == '"' {
                    break;
                }
            }
        } else if c == '/' && chars.get(i + 1) == Some(&'/') {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
        } else if c == '/' && chars.get(i + 1) == Some(&'*') {
            i += 2;
            while i < chars.len() && !(chars[i] == '*' && chars.get(i + 1) == Some(&'/')) {
                i += 1;
            }
            i += 2;
        } else if c == ',' {
            if !matches!(next_token(&chars, i + 1), Some('}' | ']')) {
                out.push(c);
            }
            i += 1;
        } else {
            out.push(c);
            i += 1;
        }
    }
    out
}

/// The first character from `i` on that is neither white space nor in a comment.
fn next_token(chars: &[char], mut i: usize) -> Option<char> {
    while i < chars.len() {
        match (chars[i], chars.get(i + 1)) {
            (c, _) if c.is_whitespace() => i += 1,
            ('/', Some('/')) => {
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
            }
            ('/', Some('*')) => {
                i += 2;
                while i < chars.len() && !(chars[i] == '*' && chars.get(i + 1) == Some(&'/')) {
                    i += 1;
                }
                i += 2;
            }
            (c, _) => return Some(c),
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_form_of_import_is_read() {
        let source = "\
import a from '../domain/a';
import type { B } from \"../application/b\";
export * from './c';
export { d } from './d';
const e = await import('../infrastructure/e');
const f = require(`../utils/f`);
import g = require('../handler/g');
type H = typeof import('../ui/h');
const i = import(name);
const j = require('x' + y);
export const k = 1;
";
        let read = read(source, "src/ui/atoms/x.ts");
        assert_eq!(read.error, None);
        let found: Vec<(usize, &str)> =
            read.imports.iter().map(|(l, s)| (*l, s.as_str())).collect();
        assert_eq!(
            found,
            [
                (1, "../domain/a"),
                (2, "../application/b"),
                (3, "./c"),
                (4, "./d"),
                (5, "../infrastructure/e"),
                (6, "../utils/f"),
                (7, "../handler/g"),
                (8, "../ui/h"),
            ]
        );
    }

    #[test]
    fn jsx_text_is_not_a_comment_and_comments_are_read() {
        let source = "// one\nconst x = <p>// TODO not a comment</p>;\n/* two\n three */\n";
        let read = read(source, "src/a.tsx");
        assert_eq!(read.error, None);
        let texts: Vec<&str> = read.comments.iter().map(|(_, t)| t.as_str()).collect();
        assert_eq!(texts, ["// one", "/* two\n three */"]);
        assert_eq!(read.comments[1].0, source.find("/*").unwrap());
    }

    #[test]
    fn a_syntax_error_is_named_with_its_line() {
        let read = read("import a from './a';\nconst = ;\n", "src/x.ts");
        assert_eq!(read.error.map(|(line, _)| line), Some(2));
    }

    #[test]
    fn a_relative_or_rooted_specifier_resolves_against_the_file() {
        let aliases = Aliases::default();
        let root = Path::new(".");
        let at = |s: &str| aliases.resolve(root, "src/ui/atoms/button.tsx", s);
        assert_eq!(at("../molecules/row"), Some("src/ui/molecules/row".into()));
        assert_eq!(
            at("./theme.css?inline"),
            Some("src/ui/atoms/theme.css".into())
        );
        assert_eq!(at("../../domain"), Some("src/domain".into()));
        assert_eq!(at("/src/utils/x"), Some("src/utils/x".into()));
        assert_eq!(at("../../../../../out"), None);
        assert_eq!(at("react"), None);
    }

    fn project(files: &[(&str, &str)]) -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        for (path, text) in files {
            let full = root.path().join(path);
            fs::create_dir_all(full.parent().unwrap()).unwrap();
            fs::write(full, text).unwrap();
        }
        root
    }

    #[test]
    fn paths_and_base_url_resolve_through_every_config_and_what_they_extend() {
        let root = project(&[
            (
                "tsconfig.json",
                "{\n  // the root config\n  \"files\": [],\n  \"extends\": \"./configs/base.json\",\n}\n",
            ),
            (
                "configs/base.json",
                "{ \"compilerOptions\": { /* from here */ \"baseUrl\": \"../src\", \"paths\": { \"@/*\": [\"./*\"], \"#theme\": [\"ui/atoms/theme\"], } } }",
            ),
            (
                "tsconfig.app.json",
                "{ \"compilerOptions\": { \"paths\": { \"~/*\": [\"src/*\"] } } }",
            ),
            ("src/domain/song.ts", ""),
        ]);
        let aliases = aliases(root.path()).unwrap();
        assert_eq!(aliases.problems, Vec::<String>::new());
        let at = |s: &str| aliases.resolve(root.path(), "src/ui/pages/home.tsx", s);
        assert_eq!(at("@/domain/song"), Some("src/domain/song".into()));
        assert_eq!(at("#theme"), Some("src/ui/atoms/theme".into()));
        assert_eq!(
            at("~/application/play"),
            Some("src/application/play".into())
        );
        // Through baseUrl only when a module is there: a package of the same name otherwise
        assert_eq!(at("domain/song"), Some("src/domain/song".into()));
        assert_eq!(at("domain/missing"), None);
        assert_eq!(at("react"), None);
    }

    #[test]
    fn what_cannot_be_read_is_a_problem() {
        let root = project(&[
            (
                "tsconfig.json",
                "{ \"compilerOptions\": { \"paths\": { \"*/*\": [\"src/*\"], \"a\": \"src/a\" } } }",
            ),
            ("tsconfig.node.json", "{ not json"),
            ("tsconfig.app.json", "{ \"extends\": \"./missing.json\" }"),
        ]);
        let problems = aliases(root.path()).unwrap().problems;
        assert_eq!(problems.len(), 4, "{problems:#?}");
        assert!(problems.iter().any(|p| p.contains("\"*/*\"")));
        assert!(
            problems
                .iter()
                .any(|p| p.contains("not to a list of paths"))
        );
        assert!(
            problems
                .iter()
                .any(|p| p.starts_with("tsconfig.node.json: cannot be read"))
        );
        assert!(
            problems
                .iter()
                .any(|p| p.contains("missing.json: extended by tsconfig.app.json"))
        );
    }

    #[test]
    fn jsonc_loses_its_comments_and_trailing_commas_and_keeps_its_strings() {
        let text =
            "{ \"a\": \"// not a comment, /* nor this */\", // gone\n \"b\": [1, 2,], /* gone */ }";
        let json: Value = serde_json::from_str(&strip_jsonc(text)).unwrap();
        assert_eq!(json["a"], "// not a comment, /* nor this */");
        assert_eq!(json["b"], serde_json::json!([1, 2]));
    }
}
