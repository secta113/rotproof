//! Reading TypeScript and JavaScript with oxc: what a file imports and its comments, the definitions it makes, and the
//! aliases of the project.
//!
//! - **What a file imports:** `import` (`import type` too), `export ... from`, `import("...")` and `require("...")`
//!   with a literal string, `import x = require("...")`, and `import("...")` in a type. A specifier built at run time
//!   is not seen.
//! - **The aliases an import resolves through** ([`aliases`]): `compilerOptions.paths` and `baseUrl` of the
//!   `tsconfig*.json` files at the root and the local files they extend. Where an import lands through them is the
//!   rule of `code.rs` ([`Aliases::resolve`]).
//! - **Its comments,** `//` and `/* */`. JSX text is not a comment.
//! - **Its reads of the environment** (`domain::environment`): `process.env` and `import.meta.env`.
//! - **Its outline** ([`outline`]), every definition with its names and its range, for a knowledge document that
//!   follows one.

use std::collections::BTreeSet;
use std::io;

use oxc_allocator::Allocator;
use oxc_ast::ast::{
    BindingPattern, CallExpression, Class, ClassElement, ComputedMemberExpression, Declaration,
    Decorator, ExportAllDeclaration, ExportDefaultDeclarationKind, ExportFromDeclaration,
    Expression, ImportDeclaration, ImportDeclarationSpecifier, ImportExpression, Program,
    PropertyKey, Statement, StaticMemberExpression, TSEnumMemberName, TSImportEqualsDeclaration,
    TSImportType, TSInterfaceDeclaration, TSModuleReference, TSNamespaceDeclaration,
    TSNamespaceDeclarationBody, TSSignature, VariableDeclarator,
};
use oxc_ast_visit::{Visit, walk};
use oxc_parser::{Parser, ParserReturn};
use oxc_span::{GetSpan, SourceType, Span};
use serde_json::Value;

use domain::code::{Alias, Aliases, Source};
use domain::definitions::Definition;
use domain::environment::VITE_OWN;
use domain::follows::is_name;
use domain::layers::Language;
use domain::paths::{join, parent};
use domain::text::line_of;
use domain::tree::with_lf;
use protocol::outbound::Tree;

/// Read one file, whose path from the root says how: `.tsx` with JSX, `.d.ts` as declarations.
pub fn read(source: &str, path: &str) -> Source<String> {
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, source, source_type(path)).parse();
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
    Source {
        imports: imports.found,
        comments,
        environment: environment(source, &parsed.program),
        error: first_error(source, &parsed),
    }
}

/// The modules whose default or namespace import is Node's `process`.
const PROCESS: [&str; 2] = ["process", "node:process"];

/// Every read or write of the environment in a file (`domain::environment`): `process.env` (the global `process`,
/// `globalThis.process`, or a name the file imports `process` as), `import.meta.env`, each as a member
/// (`process.env.X`, `process["env"]`) or taken apart (`const { env } = process`), and `env` imported from
/// `process` (`import { env } from "node:process"`). The line, and `process.env` or `import.meta.env`.
fn environment(source: &str, program: &Program) -> Vec<(usize, String)> {
    let mut process: BTreeSet<String> = BTreeSet::from(["process".to_string()]);
    let mut found = Vec::new();
    for statement in &program.body {
        let Statement::ImportDeclaration(import) = statement else {
            continue;
        };
        if !PROCESS.contains(&import.source.value.as_str()) {
            continue;
        }
        for specifier in import.specifiers.iter().flatten() {
            match specifier {
                ImportDeclarationSpecifier::ImportSpecifier(named) => {
                    if named.imported.name() == "env" {
                        found.push((
                            line_of(source, named.span.start as usize),
                            "process.env".to_string(),
                        ));
                    }
                }
                ImportDeclarationSpecifier::ImportDefaultSpecifier(default) => {
                    process.insert(default.local.name.to_string());
                }
                ImportDeclarationSpecifier::ImportNamespaceSpecifier(namespace) => {
                    process.insert(namespace.local.name.to_string());
                }
            }
        }
    }
    let mut reads = Reads {
        source,
        process: &process,
        found: &mut found,
    };
    reads.visit_program(program);
    found.sort();
    found
}

/// The reads of `env` from what holds the environment, as [`environment`] finds them
struct Reads<'b, 's> {
    source: &'s str,
    process: &'b BTreeSet<String>,
    found: &'b mut Vec<(usize, String)>,
}

impl Reads<'_, '_> {
    /// What `object` is, when its `env` is the environment: `process.env` or `import.meta.env`
    fn holder(&self, object: &Expression) -> Option<&'static str> {
        match object {
            Expression::Identifier(id) if self.process.contains(id.name.as_str()) => {
                Some("process.env")
            }
            Expression::ImportMeta(_) => Some("import.meta.env"),
            Expression::StaticMemberExpression(member)
                if member.property.name == "process"
                    && matches!(&member.object, Expression::Identifier(id) if id.name == "globalThis") =>
            {
                Some("process.env")
            }
            _ => None,
        }
    }

    fn add(&mut self, at: u32, what: &str) {
        self.found
            .push((line_of(self.source, at as usize), what.to_string()));
    }
}

/// Whether `expression` is `import.meta.env`, whose values Vite sets itself ([`VITE_OWN`]) pass
fn is_vite_env(expression: &Expression) -> bool {
    matches!(expression, Expression::StaticMemberExpression(member)
        if member.property.name == "env" && matches!(member.object, Expression::ImportMeta(_)))
}

impl<'a> Visit<'a> for Reads<'_, '_> {
    fn visit_static_member_expression(&mut self, it: &StaticMemberExpression<'a>) {
        // `import.meta.env.DEV` says how the code was built: nothing more is in it to read
        if VITE_OWN.contains(&it.property.name.as_str()) && is_vite_env(&it.object) {
            return;
        }
        if it.property.name == "env"
            && let Some(what) = self.holder(&it.object)
        {
            self.add(it.span.start, what);
        }
        walk::walk_static_member_expression(self, it);
    }

    fn visit_computed_member_expression(&mut self, it: &ComputedMemberExpression<'a>) {
        if literal(&it.expression) == Some("env")
            && let Some(what) = self.holder(&it.object)
        {
            self.add(it.span.start, what);
        }
        walk::walk_computed_member_expression(self, it);
    }

    fn visit_variable_declarator(&mut self, it: &VariableDeclarator<'a>) {
        // `const { DEV, MODE } = import.meta.env` takes only what Vite sets itself
        if let BindingPattern::ObjectPattern(pattern) = &it.id
            && pattern.rest.is_none()
            && it.init.as_ref().is_some_and(is_vite_env)
            && pattern.properties.iter().all(|property| {
                property
                    .key
                    .static_name()
                    .is_some_and(|name| VITE_OWN.contains(&name.as_ref()))
            })
        {
            self.visit_binding_pattern(&it.id);
            return;
        }
        if let BindingPattern::ObjectPattern(pattern) = &it.id
            && let Some(init) = &it.init
            && let Some(what) = self.holder(init)
            && pattern
                .properties
                .iter()
                .any(|property| property.key.static_name().as_deref() == Some("env"))
        {
            self.add(it.span.start, what);
        }
        walk::walk_variable_declarator(self, it);
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

/// The outline of a TypeScript or JavaScript file: every definition in it, each with its names from the outermost
/// (`f`, `Class.method`, `Namespace.f`, `Enum.Member`) and its range, from its first decorator, or the `export` or
/// `declare` before it, to its end, for a knowledge document that follows one (`follows.rs` in `domain`). Every name
/// counts, exported or not:
///
/// - **A statement** that declares a function (an overload signature too), class, interface, type, enum or namespace
///   (`module` too) by an identifier, and each variable it declares by one (`export const f = () => ...`): the range is
///   the whole statement, so `const a = 1, b = 2;` is the range of both `a` and `b`. An `export default` of a function,
///   a class or an interface is named by its own name, and `default` when it has none; any other `export default` is
///   `default`.
/// - **A member** of a class (a method, a getter or setter, a property, an `accessor`), of an interface (a property or
///   a method signature), of an enum, or a declaration inside a namespace, named after the one that holds it.
///
/// Left out: a comment before a definition (a JSDoc block too), the body of a function (what is declared inside it),
/// a member named by a computed key that is not a string, by a number or by `#name`, a call, construct or index
/// signature, a static block, `declare module "x"` and `declare global`, a variable declared by destructuring.
///
/// `Err` with the first error when oxc gives up on the file: it then gives an empty program, and reading that would
/// say every definition is gone, those above the error too. A file oxc recovers from is read as far as it recovers.
pub fn outline(source: &str, path: &str) -> Result<Vec<Definition>, (usize, String)> {
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, source, source_type(path)).parse();
    if parsed.fatal_error {
        return Err(first_error(source, &parsed)
            .unwrap_or_else(|| (1, "the parser gave up on the file".to_string())));
    }
    let mut outline = Outline { found: Vec::new() };
    outline.statements(&parsed.program.body, &mut Vec::new());
    Ok(outline.found)
}

/// The line of the first syntax error of a parse, and what the parser says of it.
fn first_error(source: &str, parsed: &ParserReturn) -> Option<(usize, String)> {
    parsed.diagnostics.errors().next().map(|e| {
        let at = e.labels.first().map_or(0, |label| label.offset() as usize);
        (line_of(source, at), e.message.to_string())
    })
}

/// What a file is parsed as, by its extension in any case: `.tsx` with JSX, `.d.ts` as declarations.
fn source_type(path: &str) -> SourceType {
    SourceType::from_path(path.to_ascii_lowercase()).unwrap_or_else(|_| SourceType::tsx())
}

/// The definitions of a program, as they are met.
struct Outline {
    found: Vec<Definition>,
}

impl Outline {
    /// `name` under the names in `scope` is a definition: record its range, from its first decorator.
    fn add(&mut self, scope: &[String], name: &str, span: Span, decorators: &[Decorator]) {
        let start = decorators
            .iter()
            .map(|d| d.span.start)
            .chain([span.start])
            .min()
            .unwrap_or(span.start);
        let mut names = scope.to_vec();
        names.push(name.to_string());
        self.found.push(Definition {
            names,
            start: start as usize,
            end: span.end as usize,
        });
    }

    fn statements(&mut self, body: &[Statement], scope: &mut Vec<String>) {
        for statement in body {
            let span = statement.span();
            match statement {
                Statement::ExportDeclaration(export) => {
                    self.declaration(&export.declaration, export.span, scope);
                }
                Statement::ExportDefaultDeclaration(export) => {
                    self.default_export(&export.declaration, export.span, scope);
                }
                other => {
                    if let Some(declaration) = other.as_declaration() {
                        self.declaration(declaration, span, scope);
                    }
                }
            }
        }
    }

    fn default_export(
        &mut self,
        declaration: &ExportDefaultDeclarationKind,
        span: Span,
        scope: &mut Vec<String>,
    ) {
        match declaration {
            ExportDefaultDeclarationKind::FunctionDeclaration(function) => {
                let name = function
                    .id
                    .as_ref()
                    .map_or("default", |id| id.name.as_str());
                self.add(scope, name, span, &[]);
            }
            ExportDefaultDeclarationKind::ClassDeclaration(class) => {
                let name = class.id.as_ref().map_or("default", |id| id.name.as_str());
                self.class(class, name, span, scope);
            }
            ExportDefaultDeclarationKind::TSInterfaceDeclaration(interface) => {
                self.interface(interface, interface.id.name.as_str(), span, scope);
            }
            _ => self.add(scope, "default", span, &[]),
        }
    }

    fn declaration(&mut self, declaration: &Declaration, span: Span, scope: &mut Vec<String>) {
        match declaration {
            Declaration::VariableDeclaration(variables) => {
                for declarator in &variables.declarations {
                    if let BindingPattern::BindingIdentifier(id) = &declarator.id {
                        self.add(scope, id.name.as_str(), span, &[]);
                    }
                }
            }
            Declaration::FunctionDeclaration(function) => {
                if let Some(id) = &function.id {
                    self.add(scope, id.name.as_str(), span, &[]);
                }
            }
            Declaration::ClassDeclaration(class) => {
                if let Some(id) = &class.id {
                    self.class(class, id.name.as_str(), span, scope);
                }
            }
            Declaration::TSTypeAliasDeclaration(alias) => {
                self.add(scope, alias.id.name.as_str(), span, &[]);
            }
            Declaration::TSInterfaceDeclaration(interface) => {
                self.interface(interface, interface.id.name.as_str(), span, scope);
            }
            Declaration::TSEnumDeclaration(declared) => {
                let name = declared.id.name.as_str();
                self.add(scope, name, span, &[]);
                scope.push(name.to_string());
                for member in &declared.body.members {
                    let name = match &member.id {
                        TSEnumMemberName::Identifier(id) => id.name.as_str(),
                        TSEnumMemberName::String(text) | TSEnumMemberName::ComputedString(text) => {
                            text.value.as_str()
                        }
                        TSEnumMemberName::ComputedTemplateString(_) => continue,
                    };
                    if is_name(Language::TypeScript, name) {
                        self.add(scope, name, member.span, &[]);
                    }
                }
                scope.pop();
            }
            Declaration::TSNamespaceDeclaration(namespace) => {
                self.namespace(namespace, span, scope);
            }
            _ => {}
        }
    }

    /// A namespace, or `module`, and what it holds. `namespace A.B` is `B` in `A`, and the span of each is its own.
    fn namespace(
        &mut self,
        namespace: &TSNamespaceDeclaration,
        span: Span,
        scope: &mut Vec<String>,
    ) {
        let name = namespace.id.name.as_str();
        self.add(scope, name, span, &[]);
        scope.push(name.to_string());
        match &namespace.body {
            TSNamespaceDeclarationBody::TSModuleBlock(block) => {
                self.statements(&block.body, scope);
            }
            TSNamespaceDeclarationBody::TSNamespaceDeclaration(inner) => {
                self.namespace(inner, inner.span, scope);
            }
        }
        scope.pop();
    }

    fn class(&mut self, class: &Class, name: &str, span: Span, scope: &mut Vec<String>) {
        self.add(scope, name, span, &class.decorators);
        scope.push(name.to_string());
        for element in &class.body.body {
            let (key, span, decorators) = match element {
                ClassElement::MethodDefinition(member) => {
                    (&member.key, member.span, &member.decorators)
                }
                ClassElement::PropertyDefinition(member) => {
                    (&member.key, member.span, &member.decorators)
                }
                ClassElement::AccessorProperty(member) => {
                    (&member.key, member.span, &member.decorators)
                }
                ClassElement::StaticBlock(_) | ClassElement::TSIndexSignature(_) => continue,
            };
            if let Some(name) = member_name(key) {
                self.add(scope, &name, span, decorators);
            }
        }
        scope.pop();
    }

    fn interface(
        &mut self,
        interface: &TSInterfaceDeclaration,
        name: &str,
        span: Span,
        scope: &mut Vec<String>,
    ) {
        self.add(scope, name, span, &[]);
        scope.push(name.to_string());
        for signature in &interface.body.body {
            let (key, span) = match signature {
                TSSignature::TSPropertySignature(member) => (&member.key, member.span),
                TSSignature::TSMethodSignature(member) => (&member.key, member.span),
                _ => continue,
            };
            if let Some(name) = member_name(key) {
                self.add(scope, &name, span, &[]);
            }
        }
        scope.pop();
    }
}

/// The name a member is addressed by: an identifier, or a string that is one (`'a'`). `None` for `#a`, for a number
/// and for what is computed.
fn member_name(key: &PropertyKey) -> Option<String> {
    let name = match key {
        PropertyKey::StaticIdentifier(id) => id.name.as_str(),
        PropertyKey::StringLiteral(text) => text.value.as_str(),
        _ => return None,
    };
    is_name(Language::TypeScript, name).then(|| name.to_string())
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
/// configs it extends, and a later one in `extends` over an earlier one. `None`, with a problem, when it cannot be
/// read.
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
    let text = match tree.read(name).map(|text| with_lf(&text)) {
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
    let dir = parent(name).to_string();
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
    use crate::outbound::disk::Disk;
    use domain::code::Landing;
    use domain::definitions::pieces;

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

    const DEFINED: &str = "\
import x from 'y';

/** The docs. */
@Injectable()
export class Router<T> extends Base implements I {
  static count = 0;
  #secret = 1;
  private routes: string[] = [];
  constructor(a: string) { super(); }
  add(a: string): void;
  add(a: number): void;
  add(a: any) { this.routes.push(a); }
  get size() { return 1; }
  set size(v) { }
  @bound
  remove() {}
  [Symbol.iterator]() {}
  'quoted-name'() {}
  'plain'() {}
  accessor mode = 1;
  static { init(); }
}
interface Router { extra(): void; readonly tag: string; }
export function f(a: string): void;
export function f(a: number): void;
export function f(a: any) { function inner() {} }
export const h = () => 1, i = 2;
let j = 1;
export default function () {}
enum Color { Red, 'Green', Blue = 3 }
export namespace A.B { export function k() {} const hidden = 1; }
declare module \"x\" { export function z(): void; }
type T = { a: number };
";

    /// The source of every definition of that name in a file the parser reads, as a key hashes it.
    fn found(source: &str, path: &str, dotted: &str) -> Vec<String> {
        let outline = outline(source, path).unwrap();
        pieces(source, &outline, dotted)
            .into_iter()
            .map(str::to_string)
            .collect()
    }

    fn defined(dotted: &str) -> Vec<String> {
        found(DEFINED, "src/api.ts", dotted)
    }

    #[test]
    fn a_name_defined_more_than_once_gives_every_piece_in_the_order_they_appear() {
        // The overloads and the implementation, each from its `export`
        assert_eq!(
            defined("f"),
            [
                "export function f(a: string): void;",
                "export function f(a: number): void;",
                "export function f(a: any) { function inner() {} }",
            ]
        );
        // The class from its decorator, which comes before `export`, and the interface merged with it
        let router = defined("Router");
        assert_eq!(router.len(), 2, "{router:?}");
        assert!(router[0].starts_with("@Injectable()\nexport class Router<T>"));
        assert!(router[0].ends_with("static { init(); }\n}"));
        assert_eq!(
            router[1],
            "interface Router { extra(): void; readonly tag: string; }"
        );
        // The overloads of a method, and a getter with its setter
        assert_eq!(
            defined("Router.add"),
            [
                "add(a: string): void;",
                "add(a: number): void;",
                "add(a: any) { this.routes.push(a); }"
            ]
        );
        assert_eq!(
            defined("Router.size"),
            ["get size() { return 1; }", "set size(v) { }"]
        );
    }

    #[test]
    fn a_member_starts_at_its_decorator_and_a_class_member_is_any_of_its_kinds() {
        assert_eq!(defined("Router.remove"), ["@bound\n  remove() {}"]);
        assert_eq!(
            defined("Router.constructor"),
            ["constructor(a: string) { super(); }"]
        );
        assert_eq!(defined("Router.count"), ["static count = 0;"]);
        assert_eq!(defined("Router.routes"), ["private routes: string[] = [];"]);
        assert_eq!(defined("Router.mode"), ["accessor mode = 1;"]);
        // A string key that is a name is that name
        assert_eq!(defined("Router.plain"), ["'plain'() {}"]);
        // A member of an interface, merged with the class's own
        assert_eq!(defined("Router.extra"), ["extra(): void;"]);
        assert_eq!(defined("Router.tag"), ["readonly tag: string;"]);
        // Not addressed: private names, computed keys, names no key can write, a static block
        for none in [
            "Router.secret",
            "Router.#secret",
            "Router.iterator",
            "Router.quoted-name",
            "Router.init",
        ] {
            assert_eq!(defined(none), Vec::<String>::new(), "{none}");
        }
    }

    #[test]
    fn a_variable_is_the_whole_statement_it_is_declared_in() {
        let whole = "export const h = () => 1, i = 2;";
        assert_eq!(defined("h"), [whole]);
        assert_eq!(defined("i"), [whole]);
        assert_eq!(defined("j"), ["let j = 1;"]);
        assert_eq!(defined("T"), ["type T = { a: number };"]);
    }

    #[test]
    fn an_export_default_is_named_by_its_name_or_default() {
        assert_eq!(defined("default"), ["export default function () {}"]);
        let named = "export default class Home { render() {} }\n";
        assert_eq!(
            found(named, "a.tsx", "Home"),
            ["export default class Home { render() {} }"]
        );
        assert_eq!(found(named, "a.tsx", "Home.render"), ["render() {}"]);
        assert_eq!(found(named, "a.tsx", "default"), Vec::<String>::new());
        assert_eq!(
            found("export default memo(Home);\n", "a.tsx", "default"),
            ["export default memo(Home);"]
        );
    }

    #[test]
    fn an_enum_and_a_namespace_hold_what_they_name() {
        assert_eq!(defined("Color.Red"), ["Red"]);
        assert_eq!(defined("Color.Green"), ["'Green'"]);
        assert_eq!(defined("Color.Blue"), ["Blue = 3"]);
        assert!(defined("Color")[0].starts_with("enum Color {"));
        assert_eq!(defined("A.B.k"), ["export function k() {}"]);
        assert_eq!(defined("A.B.hidden"), ["const hidden = 1;"]);
        // The inner one of `A.B` starts at its own name
        assert_eq!(
            defined("A.B"),
            ["B { export function k() {} const hidden = 1; }"]
        );
        assert!(defined("A")[0].starts_with("export namespace A.B"));
    }

    #[test]
    fn what_is_not_a_definition_is_not_found() {
        for none in [
            "inner",
            "f.inner",
            "x",
            "z",
            "x.z",
            "missing",
            "Router.missing",
        ] {
            assert_eq!(defined(none), Vec::<String>::new(), "{none}");
        }
        // A name in a comment or a string is not a definition
        let source =
            "// function ghost() {}\nconst s = \"function ghost() {}\";\n/* class Ghost {} */\n";
        assert_eq!(found(source, "a.ts", "ghost"), Vec::<String>::new());
        assert_eq!(found(source, "a.ts", "Ghost"), Vec::<String>::new());
    }

    #[test]
    fn the_extension_says_how_a_file_is_read() {
        let component =
            "export const App = () => <p>hi</p>;\nexport function Page() { return <App />; }\n";
        for path in ["a.tsx", "A.TSX", "a.jsx"] {
            assert_eq!(
                found(component, path, "App"),
                ["export const App = () => <p>hi</p>;"],
                "{path}"
            );
        }
        let plain = "function a() { return 1; }\nmodule.exports = { a };\n";
        assert_eq!(found(plain, "a.js", "a"), ["function a() { return 1; }"]);
        assert_eq!(
            found("declare function g(): void;\n", "a.d.ts", "g"),
            ["declare function g(): void;"]
        );
    }

    #[test]
    fn the_outline_names_each_definition_from_the_outermost() {
        let outline = outline(DEFINED, "src/api.ts").unwrap();
        let named = |names: &[&str]| {
            outline
                .iter()
                .any(|d| d.names.iter().map(String::as_str).eq(names.iter().copied()))
        };
        assert!(named(&["Router"]) && named(&["Router", "add"]) && named(&["Color", "Red"]));
        assert!(named(&["A", "B"]) && named(&["A", "B", "k"]) && named(&["default"]));
        // A name is one definition of the outline for each piece
        assert_eq!(
            outline.iter().filter(|d| d.names == ["f"]).count(),
            3,
            "{outline:?}"
        );
        // What is inside a function is not in it
        assert!(!outline.iter().any(|d| d.names.last().unwrap() == "inner"));
    }

    #[test]
    fn a_file_the_parser_gives_up_on_is_an_error_not_a_definition_gone() {
        // oxc gives an empty program for such a file, which would say that what is above the error is gone too
        for (source, line) in [
            ("export function before() {}\nconst = ;\n", 2),
            ("const = ;\nexport function after() {}\n", 1),
        ] {
            let (at, _) = outline(source, "a.ts").unwrap_err();
            assert_eq!(at, line, "{source:?}");
        }
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
        let at = |s: &str| aliases.resolve("src/ui/pages/home.tsx", s);
        let at_path = |path: &str| Landing::At(Some(path.into()));
        assert_eq!(at("@/domain/song"), at_path("src/domain/song"));
        assert_eq!(at("#theme"), at_path("src/ui/atoms/theme"));
        assert_eq!(at("~/application/play"), at_path("src/application/play"));
        // Through baseUrl when a module is there, which the caller asks the tree
        assert_eq!(
            at("domain/song"),
            Landing::UnderBaseUrl(vec!["src/domain/song".into()])
        );
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
