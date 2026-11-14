//! Reading TypeScript and JavaScript with oxc. [`read`] gives what a file imports, its comments and its reads of the
//! environment; the outline and the aliases are read apart, by a module each in `typescript/`.
//!
//! - **What a file imports** (here): `import` (`import type` too), `export ... from`, `import("...")` and
//!   `require("...")` with a literal string, `import x = require("...")`, and `import("...")` in a type. A specifier
//!   built at run time is not seen.
//! - **Its comments** (here), `//` and `/* */`. JSX text is not a comment.
//! - **Its reads of the environment** (`environment.rs`, for `domain::environment`): `process.env` and
//!   `import.meta.env`, given by [`read`] with the rest.
//! - **Its outline** ([`outline`], in `outline.rs`), every definition with its names and its range, for a knowledge
//!   document that follows one.
//! - **The aliases an import resolves through** ([`aliases`], in `aliases.rs`): `compilerOptions.paths` and `baseUrl`
//!   of the `tsconfig*.json` files at the root and the local files they extend. Where an import lands through them is
//!   the rule of `code.rs` ([`Aliases::resolve`](domain::code::Aliases::resolve)).
//!
//! The imports and the comments stay here because they are what [`read`] returns: a module for them would hold the
//! visitor and leave `read` calling it. Here too is what more than one reader needs: how a file is parsed
//! ([`source_type`], [`first_error`]) and [`literal`], the text of a literal string, which the imports and the reads of
//! the environment both look for.

mod aliases;
mod environment;
mod outline;

pub use aliases::aliases;
pub use outline::outline;

use oxc_allocator::Allocator;
use oxc_ast::ast::{
    CallExpression, ExportAllDeclaration, ExportFromDeclaration, Expression, ImportDeclaration,
    ImportExpression, TSImportEqualsDeclaration, TSImportType, TSModuleReference,
};
use oxc_ast_visit::{Visit, walk};
use oxc_parser::{Parser, ParserReturn};
use oxc_span::SourceType;

use domain::code::Source;
use domain::text::line_of;

use environment::environment;

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
}
