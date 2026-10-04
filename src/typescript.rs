//! Reading TypeScript and JavaScript with oxc: what a file imports and its comments, and the aliases of the project.
//!
//! - **What a file imports:** `import` (`import type` too), `export ... from`, `import("...")` and `require("...")`
//!   with a literal string, `import x = require("...")`, and `import("...")` in a type. A specifier built at run time
//!   is not seen.
//! - **The aliases an import resolves through** ([`aliases`]): `compilerOptions.paths` and `baseUrl` of the
//!   `tsconfig*.json` files at the root and the local files they extend. Where an import lands through them is the
//!   rule of `code.rs` ([`Aliases::resolve`]).
//! - **Its comments,** `//` and `/* */`. JSX text is not a comment.

use std::io;

use oxc_allocator::Allocator;
use oxc_ast::ast::{
    CallExpression, ExportAllDeclaration, ExportFromDeclaration, Expression, ImportDeclaration,
    ImportExpression, TSImportEqualsDeclaration, TSImportType, TSModuleReference,
};
use oxc_ast_visit::{Visit, walk};
use oxc_parser::Parser;
use oxc_span::SourceType;
use serde_json::Value;

use crate::code::{Alias, Aliases, Source, join, parent};
use crate::source::line_of;
use crate::tree::{Tree, read_text};

/// Read one file, whose path from the root says how: `.tsx` with JSX, `.d.ts` as declarations.
pub fn read(source: &str, path: &str) -> Source<String> {
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

/// What one config says, its `extends` followed.
#[derive(Debug, Default, Clone)]
struct Options {
    /// From the root
    base_url: Option<String>,
    /// Pattern -> target, and the directory (from the root) the targets resolve against when there is no `baseUrl`
    paths: Option<(Vec<(String, String)>, String)>,
}

/// The aliases of the project in `tree`. An error is a config that cannot be read at all.
pub fn aliases(tree: &dyn Tree) -> io::Result<Aliases> {
    let mut names: Vec<String> = Vec::new();
    for (name, is_dir) in tree.entries("")? {
        if name.starts_with("tsconfig") && name.ends_with(".json") && !is_dir {
            names.push(name);
        }
    }
    names.sort();
    let mut aliases = Aliases::default();
    for name in &names {
        let mut seen = Vec::new();
        let Some(options) = load(tree, name, &mut seen, &mut aliases.problems)? else {
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
    tree: &dyn Tree,
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
    let options = load_one(tree, name, seen, problems);
    seen.pop();
    options
}

/// [`load`] for one config, while `seen` holds it and the configs that extend it.
fn load_one(
    tree: &dyn Tree,
    name: &str,
    seen: &mut Vec<String>,
    problems: &mut Vec<String>,
) -> io::Result<Option<Options>> {
    let text = match read_text(tree, name) {
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
        if let Some(parent) = load(tree, &path, seen, problems)? {
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
    use std::fs;

    use super::*;
    use crate::disk::Disk;

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
        let tree = Disk::new(root.path());
        let aliases = aliases(&tree).unwrap();
        assert_eq!(aliases.problems, Vec::<String>::new());
        let at = |s: &str| aliases.resolve(&tree, "src/ui/pages/home.tsx", s);
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
        let problems = aliases(&Disk::new(root.path())).unwrap().problems;
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
