//! What a TypeScript or JavaScript file uses of what is not its own code, with the scopes of the language: the one
//! reader of names that the checks of TypeScript share (`domain::code::Use`), built on oxc's semantic analysis
//! (`oxc_semantic`: symbols, scopes and references), as Python's is built on its own (`python/uses.rs`).
//!
//! - **A name resolves** the way the language resolves it: from the scope of its use outwards, to the first scope that
//!   binds it (a parameter, a `var` hoisted to its function, a `let` or `const` of its block, a `catch` binding, a
//!   function or class name, an import, a name `declare`d in the file). A name that binds to an import is the module
//!   it is imported from; a name no scope binds is a global; a name that binds to anything else is the file's own
//!   and no use. `function f(process) { process.env }` is no read of the global.
//! - **An import** gives the specifier as written and the name it takes: `import { readFile } from "node:fs"` makes
//!   `readFile` the name `"node:fs".readFile`, `import * as fs from "fs"` makes `fs.promises.readFile` the name
//!   `"fs".promises.readFile`, `import react from "react"` makes `react.useState` the name `"react".default.useState`
//!   (the default import is the export named `default`), and the namespace itself (`const f = fs`) is `"fs"`. The
//!   specifier is quoted, since a specifier has dots and slashes in it; it is not resolved (`"./x"`, `"@/x"`, as
//!   written). Each name an import takes (a named or default import, not a namespace one) is also a use, how
//!   [`How::Imported`], at its line. `import type` and a `type` specifier take nothing a program runs, and are no use.
//! - **`require` and `import()` with a literal bind as an import does:** a `const`, `let` or `var` whose initializer is
//!   exactly `require("m")` or `import("m")`, optionally awaited (with or without parentheses), cast (`as`,
//!   `satisfies`, `!`) and followed by `.default`, binds its name to the module. `const fs = require("fs")` is the
//!   namespace, so `fs.readFileSync` is `"fs".readFileSync`; `const { readFile } = await import("node:fs")` takes
//!   `readFile`, as `import { readFile } from "node:fs"` does, and is also a use [`How::Imported`] at the key;
//!   `.default` is a name taken first (`"m".default`). It is followed wherever the declaration is, in any scope, and
//!   only when the name is the global `require`. Not followed: a name assigned afterwards (`let fs = require("fs");
//!   fs = other`, which stays the module), a call wrapped in another (`wrap(require("fs"))`), a member taken without a
//!   declaration (`require("fs").readFileSync`), a rest element, and an array pattern. A `require` or `import()` of
//!   what is no literal binds nothing and is in `unread`.
//! - **A global** is the name as written (`fetch`, `Date.now`, `localStorage.getItem`, `document.title`), and what the
//!   global object holds is the global: `window.fetch`, `globalThis.fetch`, `self.fetch`, `global.fetch` and `fetch`
//!   are the name `fetch`, and `window.window.fetch` too. The global object alone is the name it is written as
//!   (`window`). `undefined` and `arguments` are no use. `import.meta` is a name of its own, `import.meta.env.X`.
//! - **A chain** is as far as the names are static: a name, then `.property`, and `["property"]` or `[\`property\`]`
//!   with a literal, through parentheses, optional chaining (`a?.b`), `!`, `as`, `satisfies`, `<T>` and an
//!   instantiation; so `(window as any).localStorage!.getItem` is `localStorage.getItem`. What comes after a call, a
//!   subscript that is no literal, a `#private` field or a `this` is not part of it: `a.b[c].d` is the use `a.b`. A
//!   segment that is no identifier is written in brackets and quotes (`window["a-b"]` is `window["a-b"]`), after no
//!   dot.
//! - **How it is used:** [`How::Called`] when the chain is the callee of a call (with the number of arguments written,
//!   a spread counted as none and making `unpacked`; `keywords` is always empty: JavaScript has none), also a tagged
//!   template (the strings and each expression count); [`How::Constructed`] for `new`; [`How::TypeOf`] when the chain
//!   is the operand of `typeof`; and [`How::Value`] for the rest (`useState(Date.now)`, `const f = fetch`, `fetch.call`
//!   is the call of `fetch.call`).
//! - **A destructuring** of a chain (`const { fetch } = globalThis`, `const { env } = process`, `const { env: { A } } =
//!   process`, with a default) is a use of each name it takes, as a value, at the line of the key; a rest element, a
//!   key that is no literal, and a pattern with no key use the chain itself. The binding it makes is the file's own: a
//!   later `fetch(x)` through it is no use.
//! - **A type position is no use:** oxc's type references (`let x: Date`, `type T = typeof process.env`) are not
//!   value references and are left out, as `import type` is. `in_type` is always `false`.
//!
//! The reader also gives, in `unread`, what it cannot read: `import(x)` of anything but a literal, `require(x)`
//! likewise, `eval(...)`, and `Function(...)` or `new Function(...)`, when the name is a global (the file binds none
//! of that name): the line, and `import()`, `require`, `eval` or `Function`.
//!
//! What it does not resolve: a name through an assignment (`const f = fetch`, then `f()`; `const { process } =
//! globalThis`, then `process.env`) or through a `require` or `import()` that is not the initializer of a declarator
//! (above), an object's member (`this.clock.now()`), a name a `declare global` or an ambient file brings in beyond
//! being a global, the properties of `window` that are the window itself (`window.top`, `window.parent`,
//! `window.frames`), a destructuring that is an assignment (`({ env } = process)`) or a parameter's
//! (`function f({ env } = process)`), and `export * from "m"`, whose names are no use.

use std::collections::HashMap;

use oxc_ast::ast::{
    Argument, BindingPattern, CallExpression, ChainElement, ComputedMemberExpression, Expression,
    IdentifierReference, ImportDeclarationSpecifier, ImportExpression, JSXMemberExpression,
    JSXMemberExpressionObject, NewExpression, ObjectPattern, Program, Statement,
    StaticMemberExpression, TSModuleReference, TaggedTemplateExpression, UnaryExpression,
    UnaryOperator, VariableDeclarator,
};
use oxc_ast_visit::{Visit, walk};
use oxc_semantic::{ReferenceId, Scoping, SymbolId};

use domain::code::{How, Use, typescript_name};
use domain::text::line_of;

use super::literal;

/// The names of the global object: what it holds is a global, so `window.fetch` is `fetch`.
const GLOBAL_OBJECTS: [&str; 4] = ["globalThis", "window", "self", "global"];

/// The globals that are no use: a value, not something the environment gives.
const NOT_USES: [&str; 2] = ["undefined", "arguments"];

/// What the reader gives of one file.
pub struct Found {
    /// Every use, in the order of the lines
    pub uses: Vec<Use>,
    /// What cannot be read: the line, and what it is (`import()`, `require`, `eval`, `Function`)
    pub unread: Vec<(usize, String)>,
}

/// Read a file whose syntax tree is `program` and whose scopes are `scoping`.
pub fn read(source: &str, program: &Program, scoping: &Scoping) -> Found {
    let mut reader = Reader {
        source,
        scoping,
        imports: HashMap::new(),
        uses: Vec::new(),
        unread: Vec::new(),
    };
    reader.bind_imports(program);
    // What a `require` or an `import()` binds, wherever the declaration is: before any use is resolved
    Requires {
        reader: &mut reader,
    }
    .visit_program(program);
    reader.visit_program(program);
    // In the order of the lines, the order the file is read in among those of one line
    reader.uses.sort_by_key(|used| used.line);
    reader.unread.sort_by_key(|(line, _)| *line);
    Found {
        uses: reader.uses,
        unread: reader.unread,
    }
}

/// What an import binds a name to: the module, and the names it takes from it (none for the whole namespace).
struct Bound {
    module: String,
    path: Vec<String>,
}

/// The module and the names after it that `expression` is, when it is `require("m")` or `import("m")` with a literal
/// string, optionally awaited, in parentheses, cast and followed by `.default`.
fn required(scoping: &Scoping, expression: &Expression) -> Option<(String, Vec<String>)> {
    match expression {
        Expression::AwaitExpression(inner) => required(scoping, &inner.argument),
        Expression::ParenthesizedExpression(inner) => required(scoping, &inner.expression),
        Expression::TSAsExpression(inner) => required(scoping, &inner.expression),
        Expression::TSSatisfiesExpression(inner) => required(scoping, &inner.expression),
        Expression::TSNonNullExpression(inner) => required(scoping, &inner.expression),
        Expression::StaticMemberExpression(member) if member.property.name == "default" => {
            let (module, mut path) = required(scoping, &member.object)?;
            path.push("default".to_string());
            Some((module, path))
        }
        Expression::CallExpression(call) => {
            let Expression::Identifier(callee) = &call.callee else {
                return None;
            };
            // The global `require`, not a name the file binds
            let global = callee.name == "require"
                && callee
                    .reference_id
                    .get()
                    .is_some_and(|id| scoping.get_reference(id).symbol_id().is_none());
            let [argument] = call.arguments.as_slice() else {
                return None;
            };
            let module = argument.as_expression().and_then(literal)?;
            global.then(|| (module.to_string(), Vec::new()))
        }
        Expression::ImportExpression(import) => {
            literal(&import.source).map(|module| (module.to_string(), Vec::new()))
        }
        _ => None,
    }
}

/// The pass that finds what `require` and `import()` bind: every declarator of the file, in any scope.
struct Requires<'r, 's> {
    reader: &'r mut Reader<'s>,
}

impl<'a> Visit<'a> for Requires<'_, '_> {
    fn visit_variable_declarator(&mut self, it: &VariableDeclarator<'a>) {
        self.reader.bind_required(it);
        walk::walk_variable_declarator(self, it);
    }
}

/// Where a chain starts.
#[derive(Clone)]
enum Root {
    /// A name, and the reference semantic analysis resolved it with
    Name {
        name: String,
        reference: Option<ReferenceId>,
    },
    /// `import.meta`
    ImportMeta,
}

/// A chain of static names: where it starts, the properties after it, and the offset it starts at.
#[derive(Clone)]
struct Chain {
    root: Root,
    keys: Vec<String>,
    at: u32,
}

impl Chain {
    fn with(&self, key: String) -> Chain {
        let mut chain = self.clone();
        chain.keys.push(key);
        chain
    }
}

/// A chain that resolves outside the file's own code.
struct Resolved {
    /// The module it is taken from, as written, for the name of an import
    module: Option<String>,
    /// The names after the module, or all of the name of a global
    parts: Vec<String>,
    /// Whether no scope binds the root of the name
    global: bool,
}

struct Reader<'s> {
    source: &'s str,
    scoping: &'s Scoping,
    /// The symbols the file's imports bind
    imports: HashMap<SymbolId, Bound>,
    uses: Vec<Use>,
    unread: Vec<(usize, String)>,
}

impl Reader<'_> {
    /// The symbols the imports of the file bind, and each name an import takes as a use.
    fn bind_imports(&mut self, program: &Program) {
        for statement in &program.body {
            match statement {
                Statement::ImportDeclaration(import) if import.import_kind.is_value() => {
                    let module = import.source.value.as_str();
                    for specifier in import.specifiers.iter().flatten() {
                        match specifier {
                            ImportDeclarationSpecifier::ImportSpecifier(named)
                                if named.import_kind.is_value() =>
                            {
                                let export = vec![named.imported.name().to_string()];
                                self.bind(&named.local.symbol_id.get(), module, &export);
                                self.imported(named.span.start, module, &export);
                            }
                            ImportDeclarationSpecifier::ImportSpecifier(_) => {}
                            ImportDeclarationSpecifier::ImportDefaultSpecifier(default) => {
                                self.bind(
                                    &default.local.symbol_id.get(),
                                    module,
                                    &["default".to_string()],
                                );
                                self.imported(default.span.start, module, &["default".to_string()]);
                            }
                            ImportDeclarationSpecifier::ImportNamespaceSpecifier(namespace) => {
                                self.bind(&namespace.local.symbol_id.get(), module, &[]);
                            }
                        }
                    }
                }
                Statement::TSImportEqualsDeclaration(import) if import.import_kind.is_value() => {
                    if let TSModuleReference::ExternalModuleReference(reference) =
                        &import.module_reference
                    {
                        self.bind(
                            &import.id.symbol_id.get(),
                            reference.expression.value.as_str(),
                            &[],
                        );
                    }
                }
                _ => {}
            }
        }
    }

    fn bind(&mut self, symbol: &Option<SymbolId>, module: &str, path: &[String]) {
        if let Some(symbol) = symbol {
            self.imports.insert(
                *symbol,
                Bound {
                    module: module.to_string(),
                    path: path.to_vec(),
                },
            );
        }
    }

    /// The names a `const`, `let` or `var` binds from a `require("m")` or an `import("m")` with a literal, as an import
    /// binds them: `const fs = require("fs")` is the namespace, `const { readFile } = require("fs")` takes `readFile`.
    fn bind_required(&mut self, declarator: &VariableDeclarator) {
        let Some(init) = &declarator.init else {
            return;
        };
        let Some((module, path)) = required(self.scoping, init) else {
            return;
        };
        self.bind_pattern(&declarator.id, &module, path, declarator.span.start, true);
    }

    /// What `pattern` binds, of what is at `path` in `module`: each name a destructuring takes is also a use at `at`.
    fn bind_pattern(
        &mut self,
        pattern: &BindingPattern,
        module: &str,
        path: Vec<String>,
        at: u32,
        whole: bool,
    ) {
        match pattern {
            BindingPattern::BindingIdentifier(id) => {
                self.bind(&id.symbol_id.get(), module, &path);
                if !whole {
                    self.imported(at, module, &path);
                }
            }
            BindingPattern::AssignmentPattern(with_default) => {
                self.bind_pattern(&with_default.left, module, path, at, whole);
            }
            BindingPattern::ObjectPattern(object) => {
                for property in &object.properties {
                    if let Some(key) = property.key.static_name() {
                        let mut inner = path.clone();
                        inner.push(key.to_string());
                        self.bind_pattern(
                            &property.value,
                            module,
                            inner,
                            property.span.start,
                            false,
                        );
                    }
                }
            }
            BindingPattern::ArrayPattern(_) => {}
        }
    }

    /// The name an import takes, as a use at `at`.
    fn imported(&mut self, at: u32, module: &str, path: &[String]) {
        self.uses.push(Use {
            line: line_of(self.source, at as usize),
            name: typescript_name(Some(module), path),
            how: How::Imported,
            in_type: false,
        });
    }

    /// What `chain` resolves to, `None` when it is the file's own code or sits in a type position.
    fn resolve(&self, chain: &Chain) -> Option<Resolved> {
        let (name, reference) = match &chain.root {
            Root::ImportMeta => {
                let mut parts = vec!["import".to_string(), "meta".to_string()];
                parts.extend(chain.keys.iter().cloned());
                return Some(Resolved {
                    module: None,
                    parts,
                    global: false,
                });
            }
            Root::Name { name, reference } => (name, (*reference)?),
        };
        let reference = self.scoping.get_reference(reference);
        let flags = reference.flags();
        // `typeof x` in a type and `let x: T` are no use, and a name only written to has no value used
        if flags.is_type() || flags.is_value_as_type() || flags.is_write_only() {
            return None;
        }
        match reference.symbol_id() {
            Some(symbol) => {
                let bound = self.imports.get(&symbol)?;
                let mut parts = bound.path.clone();
                parts.extend(chain.keys.iter().cloned());
                Some(Resolved {
                    module: Some(bound.module.clone()),
                    parts,
                    global: false,
                })
            }
            None => {
                if NOT_USES.contains(&name.as_str()) {
                    return None;
                }
                let mut parts = vec![name.clone()];
                parts.extend(chain.keys.iter().cloned());
                // What the global object holds is a global, whichever name the object has
                while parts.len() >= 2 && GLOBAL_OBJECTS.contains(&parts[0].as_str()) {
                    parts.remove(0);
                }
                Some(Resolved {
                    module: None,
                    parts,
                    global: true,
                })
            }
        }
    }

    /// Record the use of `chain`, which starts the use at the offset `at`, and give what it resolved to.
    fn record(&mut self, chain: &Chain, how: How, at: u32) -> Option<Resolved> {
        let resolved = self.resolve(chain)?;
        self.uses.push(Use {
            line: line_of(self.source, at as usize),
            name: typescript_name(resolved.module.as_deref(), &resolved.parts),
            how,
            in_type: false,
        });
        Some(resolved)
    }

    /// A call, or a construction, of what `callee` names: the use, and the arguments, which are read as ever.
    fn called(&mut self, callee: &Chain, how: How) -> Option<Resolved> {
        self.record(callee, how, callee.at)
    }

    /// The names an object pattern takes from `chain`.
    fn taken_apart(&mut self, pattern: &ObjectPattern, chain: &Chain) {
        // A rest element, a key that is no literal and a pattern with no key take the chain itself
        let mut whole = pattern.rest.is_some() || pattern.properties.is_empty();
        for property in &pattern.properties {
            let Some(key) = property.key.static_name() else {
                whole = true;
                continue;
            };
            let inner = chain.with(key.to_string());
            let value = match &property.value {
                BindingPattern::AssignmentPattern(with_default) => &with_default.left,
                other => other,
            };
            match value {
                BindingPattern::ObjectPattern(nested) => self.taken_apart(nested, &inner),
                _ => {
                    self.record(&inner, How::Value, property.span.start);
                }
            }
        }
        if whole {
            self.record(chain, How::Value, chain.at);
        }
    }

    /// What a call or a construction at `at` of a global says it cannot be read for.
    fn unread_call(&mut self, at: u32, resolved: &Resolved, arguments: &[Argument]) {
        if !resolved.global || resolved.parts.len() != 1 {
            return;
        }
        let name = resolved.parts[0].as_str();
        let read = match name {
            "eval" | "Function" => false,
            "require" => {
                arguments.len() == 1 && arguments[0].as_expression().and_then(literal).is_some()
            }
            _ => return,
        };
        if !read {
            self.unread
                .push((line_of(self.source, at as usize), name.to_string()));
        }
    }
}

/// The chain `expression` is, if it is one.
fn chain_of(expression: &Expression) -> Option<Chain> {
    match expression {
        Expression::Identifier(id) => Some(name_of(id)),
        Expression::ImportMeta(meta) => Some(Chain {
            root: Root::ImportMeta,
            keys: Vec::new(),
            at: meta.span.start,
        }),
        Expression::ParenthesizedExpression(inner) => chain_of(&inner.expression),
        Expression::TSAsExpression(inner) => chain_of(&inner.expression),
        Expression::TSSatisfiesExpression(inner) => chain_of(&inner.expression),
        Expression::TSTypeAssertion(inner) => chain_of(&inner.expression),
        Expression::TSNonNullExpression(inner) => chain_of(&inner.expression),
        Expression::TSInstantiationExpression(inner) => chain_of(&inner.expression),
        Expression::ChainExpression(chain) => match &chain.expression {
            ChainElement::TSNonNullExpression(inner) => chain_of(&inner.expression),
            ChainElement::StaticMemberExpression(member) => static_chain(member),
            ChainElement::ComputedMemberExpression(member) => computed_chain(member),
            _ => None,
        },
        Expression::StaticMemberExpression(member) => static_chain(member),
        Expression::ComputedMemberExpression(member) => computed_chain(member),
        _ => None,
    }
}

fn name_of(id: &IdentifierReference) -> Chain {
    Chain {
        root: Root::Name {
            name: id.name.to_string(),
            reference: id.reference_id.get(),
        },
        keys: Vec::new(),
        at: id.span.start,
    }
}

fn static_chain(member: &StaticMemberExpression) -> Option<Chain> {
    Some(chain_of(&member.object)?.with(member.property.name.to_string()))
}

fn computed_chain(member: &ComputedMemberExpression) -> Option<Chain> {
    let key = literal(&member.expression)?;
    Some(chain_of(&member.object)?.with(key.to_string()))
}

/// The chain `<A.B.C />` names: `None` when it starts at `this`.
fn jsx_chain(member: &JSXMemberExpression) -> Option<Chain> {
    let object = match &member.object {
        JSXMemberExpressionObject::IdentifierReference(id) => name_of(id),
        JSXMemberExpressionObject::MemberExpression(inner) => jsx_chain(inner)?,
        JSXMemberExpressionObject::ThisExpression(_) => return None,
    };
    Some(object.with(member.property.name.to_string()))
}

/// How many arguments a call writes, a spread not counted, and whether one is a spread.
fn counted(arguments: &[Argument]) -> (usize, bool) {
    let unpacked = arguments
        .iter()
        .filter(|argument| matches!(argument, Argument::SpreadElement(_)))
        .count();
    (arguments.len() - unpacked, unpacked > 0)
}

impl<'a> Visit<'a> for Reader<'_> {
    fn visit_identifier_reference(&mut self, it: &IdentifierReference<'a>) {
        let chain = name_of(it);
        self.record(&chain, How::Value, chain.at);
    }

    fn visit_static_member_expression(&mut self, it: &StaticMemberExpression<'a>) {
        match static_chain(it) {
            Some(chain) => {
                self.record(&chain, How::Value, chain.at);
            }
            None => walk::walk_static_member_expression(self, it),
        }
    }

    fn visit_computed_member_expression(&mut self, it: &ComputedMemberExpression<'a>) {
        match computed_chain(it) {
            Some(chain) => {
                self.record(&chain, How::Value, chain.at);
            }
            None => walk::walk_computed_member_expression(self, it),
        }
    }

    fn visit_jsx_member_expression(&mut self, it: &JSXMemberExpression<'a>) {
        match jsx_chain(it) {
            Some(chain) => {
                self.record(&chain, How::Value, chain.at);
            }
            None => walk::walk_jsx_member_expression(self, it),
        }
    }

    fn visit_call_expression(&mut self, it: &CallExpression<'a>) {
        let Some(callee) = chain_of(&it.callee) else {
            walk::walk_call_expression(self, it);
            return;
        };
        let (positional, unpacked) = counted(&it.arguments);
        let how = How::Called {
            positional,
            keywords: Vec::new(),
            unpacked,
        };
        if let Some(resolved) = self.called(&callee, how) {
            self.unread_call(it.span.start, &resolved, &it.arguments);
        }
        self.visit_arguments(&it.arguments);
    }

    fn visit_new_expression(&mut self, it: &NewExpression<'a>) {
        let Some(callee) = chain_of(&it.callee) else {
            walk::walk_new_expression(self, it);
            return;
        };
        let (positional, unpacked) = counted(&it.arguments);
        if let Some(resolved) = self.called(
            &callee,
            How::Constructed {
                positional,
                unpacked,
            },
        ) && resolved.global
            && resolved.parts == ["Function"]
        {
            self.unread_call(it.span.start, &resolved, &it.arguments);
        }
        self.visit_arguments(&it.arguments);
    }

    fn visit_tagged_template_expression(&mut self, it: &TaggedTemplateExpression<'a>) {
        let Some(tag) = chain_of(&it.tag) else {
            walk::walk_tagged_template_expression(self, it);
            return;
        };
        let how = How::Called {
            positional: 1 + it.quasi.expressions.len(),
            keywords: Vec::new(),
            unpacked: false,
        };
        self.called(&tag, how);
        self.visit_template_literal(&it.quasi);
    }

    fn visit_unary_expression(&mut self, it: &UnaryExpression<'a>) {
        if it.operator == UnaryOperator::Typeof
            && let Some(operand) = chain_of(&it.argument)
        {
            self.record(&operand, How::TypeOf, operand.at);
            return;
        }
        walk::walk_unary_expression(self, it);
    }

    fn visit_import_expression(&mut self, it: &ImportExpression<'a>) {
        if literal(&it.source).is_none() {
            self.unread.push((
                line_of(self.source, it.span.start as usize),
                "import()".into(),
            ));
        }
        walk::walk_import_expression(self, it);
    }

    fn visit_variable_declarator(&mut self, it: &VariableDeclarator<'a>) {
        if let BindingPattern::ObjectPattern(pattern) = &it.id
            && let Some(init) = &it.init
            && let Some(chain) = chain_of(init)
        {
            self.taken_apart(pattern, &chain);
            // What the pattern holds besides the names it takes: a default, a computed key
            self.visit_binding_pattern(&it.id);
            return;
        }
        walk::walk_variable_declarator(self, it);
    }
}

#[cfg(test)]
mod tests {
    use crate::outbound::typescript::read;
    use domain::code::How;

    /// Every use of a file, as `line:name:how`, and the unread as `line:what`
    fn used(source: &str) -> Vec<String> {
        let read = read(source, "src/domain/a.ts");
        assert_eq!(read.error, None);
        read.uses
            .iter()
            .map(|used| {
                let how = match &used.how {
                    How::Imported => "imported".to_string(),
                    How::Star => "star".to_string(),
                    How::Value => "value".to_string(),
                    How::TypeOf => "typeof".to_string(),
                    How::Called {
                        positional,
                        keywords,
                        unpacked,
                    } => {
                        assert!(keywords.is_empty());
                        format!("call {positional}{}", if *unpacked { " *" } else { "" })
                    }
                    How::Constructed {
                        positional,
                        unpacked,
                    } => format!("new {positional}{}", if *unpacked { " *" } else { "" }),
                };
                assert!(!used.in_type);
                format!("{}:{}:{how}", used.line, used.name)
            })
            .collect()
    }

    fn unread(source: &str) -> Vec<String> {
        read(source, "src/domain/a.ts")
            .unread
            .iter()
            .map(|(line, what)| format!("{line}:{what}"))
            .collect()
    }

    #[test]
    fn a_name_an_import_takes_is_the_specifier_and_the_name() {
        let source = "\
import { readFile, writeFile as write } from 'node:fs';
import * as fs from \"fs\";
import path from 'path';
import React, { useState } from 'react';
import './side-effect';
import type { Stats } from 'fs';
import { type Dirent, mkdir } from 'fs';
readFile('a', 1);
write('b');
fs.promises.readFile('c');
path.join('a', 'b');
const f = fs;
React.createElement('div');
useState(0);
";
        assert_eq!(
            used(source),
            [
                "1:\"node:fs\".readFile:imported",
                "1:\"node:fs\".writeFile:imported",
                "3:\"path\".default:imported",
                "4:\"react\".default:imported",
                "4:\"react\".useState:imported",
                "7:\"fs\".mkdir:imported",
                "8:\"node:fs\".readFile:call 2",
                "9:\"node:fs\".writeFile:call 1",
                "10:\"fs\".promises.readFile:call 1",
                "11:\"path\".default.join:call 2",
                "12:\"fs\":value",
                "13:\"react\".default.createElement:call 1",
                "14:\"react\".useState:call 1",
            ]
        );
    }

    #[test]
    fn the_specifier_is_written_as_it_is_and_quoted_since_it_has_dots_and_slashes() {
        let source = "\
import { a } from './x.js';
import * as b from '#internal/y';
import c = require('@scope/z');
a();
b.f();
c.g();
";
        assert_eq!(
            used(source),
            [
                "1:\"./x.js\".a:imported",
                "4:\"./x.js\".a:call 0",
                "5:\"#internal/y\".f:call 0",
                "6:\"@scope/z\".g:call 0",
            ]
        );
    }

    #[test]
    fn the_global_object_holds_the_globals_whichever_name_it_has() {
        let source = "\
fetch('a');
window.fetch('b');
globalThis.fetch('c');
self.fetch('d');
global.fetch('e');
window.window.fetch('f');
const w = window;
const g = globalThis.self;
window.localStorage.getItem('k');
document.title;
process.env.X;
Date.now();
";
        assert_eq!(
            used(source),
            [
                "1:fetch:call 1",
                "2:fetch:call 1",
                "3:fetch:call 1",
                "4:fetch:call 1",
                "5:fetch:call 1",
                "6:fetch:call 1",
                "7:window:value",
                "8:self:value",
                "9:localStorage.getItem:call 1",
                "10:document.title:value",
                "11:process.env.X:value",
                "12:Date.now:call 0",
            ]
        );
    }

    #[test]
    fn a_member_with_a_literal_is_a_static_member_and_a_chain_goes_through_what_types_add() {
        let source = "\
window['document'];
globalThis[\"process\"][`env`].A;
(window as any).localStorage!.getItem('k');
(<any>window).fetch('x');
window?.localStorage?.getItem('k');
(globalThis satisfies object).process.env;
window['a-b'].c;
window[name].d;
a.b[c].d;
";
        assert_eq!(
            used(source),
            [
                "1:document:value",
                "2:process.env.A:value",
                "3:localStorage.getItem:call 1",
                "4:fetch:call 1",
                "5:localStorage.getItem:call 1",
                "6:process.env:value",
                "7:[\"a-b\"].c:value",
                "8:window:value",
                "8:name:value",
                "9:a.b:value",
                "9:c:value",
            ]
        );
    }

    #[test]
    fn a_destructuring_of_a_global_uses_each_name_it_takes() {
        let source = "\
const { fetch } = globalThis;
const { env } = process;
const { env: { A, B: b } } = process;
const { argv, ...rest } = process;
const { [key]: k } = process;
const { DEV, VITE_KEY = 1 } = import.meta.env;
const { x = window.y } = z;
const {} = process;
";
        assert_eq!(
            used(source),
            [
                "1:fetch:value",
                "2:process.env:value",
                "3:process.env.A:value",
                "3:process.env.B:value",
                "4:process.argv:value",
                "4:process:value",
                "5:process:value",
                "5:key:value",
                "6:import.meta.env.DEV:value",
                "6:import.meta.env.VITE_KEY:value",
                "7:z.x:value",
                "7:y:value",
                "8:process:value",
            ]
        );
    }

    #[test]
    fn a_name_bound_in_a_scope_is_not_the_global_outside_it() {
        let source = "\
function a(process) { return process.env; }
function b() { const process = 1; return process.env; }
function c() { let fetch = null; return fetch; }
try { a(); } catch (process) { process.env; }
{ const process = {}; process.env; }
function e() { var process; return process.env; }
function f() { { var process = 1; } return process.env; }
function g() { return process.env; var process; }
const h = (process) => process.env;
class C { m(process) { return process.env; } }
const i = function process() { return process.env; };
for (const process of []) { process.env; }
";
        assert_eq!(used(source), Vec::<String>::new());
    }

    #[test]
    fn a_name_the_module_binds_is_its_own_in_every_function_below_it() {
        let source = "\
function d() { return process.env; }
function process() {}
";
        assert_eq!(used(source), Vec::<String>::new());
        let source = "\
import process from 'node:process';
function d() { return process.env.A; }
function e(process) { return process.env.B; }
";
        assert_eq!(
            used(source),
            [
                "1:\"node:process\".default:imported",
                "2:\"node:process\".default.env.A:value"
            ]
        );
    }

    #[test]
    fn a_name_bound_only_in_an_inner_scope_leaves_the_outer_use_global() {
        let source = "\
function a(process) { return process.env; }
const x = process.env.A;
{ const process = {}; process.env; }
const y = process.env.B;
";
        assert_eq!(
            used(source),
            ["2:process.env.A:value", "4:process.env.B:value"]
        );
    }

    #[test]
    fn a_call_a_construction_and_a_reference_are_told_apart_with_their_arguments() {
        let source = "\
new Date();
new Date(x);
new Date(...a);
new Date;
Date.now();
fetch(u, o, ...rest);
useState(Date.now);
const f = fetch;
tag`a${1}b${2}`;
";
        assert_eq!(
            used(source),
            [
                "1:Date:new 0",
                "2:Date:new 1",
                "2:x:value",
                "3:Date:new 0 *",
                "3:a:value",
                "4:Date:new 0",
                "5:Date.now:call 0",
                "6:fetch:call 2 *",
                "6:u:value",
                "6:o:value",
                "6:rest:value",
                "7:useState:call 1",
                "7:Date.now:value",
                "8:fetch:value",
                "9:tag:call 3",
            ]
        );
    }

    #[test]
    fn the_operand_of_typeof_is_recorded_as_one() {
        let source = "\
if (typeof window !== 'undefined') {}
const a = typeof process.env;
const b = typeof localStorage.getItem;
const c = typeof (globalThis as any).fetch;
const d = typeof (1 + x);
";
        assert_eq!(
            used(source),
            [
                "1:window:typeof",
                "2:process.env:typeof",
                "3:localStorage.getItem:typeof",
                "4:fetch:typeof",
                "5:x:value",
            ]
        );
    }

    #[test]
    fn a_type_position_is_no_use() {
        let source = "\
import type { Stats } from 'fs';
import { type Dirent } from 'fs';
import { Buffer } from 'node:buffer';
let a: Date;
let b: Buffer;
type C = typeof process.env;
type D = typeof Buffer;
function f(x: Promise<Stats>): Dirent { return x as any; }
interface I extends Array<Date> {}
const e = <Date>{};
const g = 1 as Date;
";
        // What is left is the import of `Buffer`, which is taken whether the file uses it or not
        assert_eq!(used(source), ["3:\"node:buffer\".Buffer:imported"]);
    }

    #[test]
    fn a_jsx_component_is_a_use_of_what_it_is_bound_to() {
        let source = "\
import * as UI from 'ui';
import { Button } from 'ui';
const a = <Button><UI.Card.Body /><div /></Button>;
";
        let read = read(source, "src/ui/a.tsx");
        let names: Vec<String> = read
            .uses
            .iter()
            .map(|used| format!("{}:{}", used.line, used.name))
            .collect();
        assert_eq!(
            names,
            [
                "2:\"ui\".Button",
                "3:\"ui\".Button",
                "3:\"ui\".Card.Body",
                "3:\"ui\".Button",
            ]
        );
    }

    #[test]
    fn import_meta_is_a_name_of_its_own() {
        let source = "\
const a = import.meta.env.VITE_URL;
const b = import.meta.env;
const c = import.meta['env'].DEV;
const d = import.meta.resolve('./x');
";
        assert_eq!(
            used(source),
            [
                "1:import.meta.env.VITE_URL:value",
                "2:import.meta.env:value",
                "3:import.meta.env.DEV:value",
                "4:import.meta.resolve:call 1",
            ]
        );
    }

    #[test]
    fn what_a_file_binds_is_its_own_whatever_it_is_called() {
        let source = "\
declare const __VERSION__: string;
const config = { env: 'dev' };
const a = config.env;
const { env } = config;
class C { x = this.y; }
export function f(self: any) { return self.fetch; }
console.log(__VERSION__, undefined);
";
        assert_eq!(used(source), ["7:console.log:call 2"]);
    }

    #[test]
    fn a_name_that_is_only_written_to_is_no_use() {
        let source = "\
let x;
y = 1;
document.title = 'a';
process.env.A = 'b';
delete window.foo;
";
        assert_eq!(
            used(source),
            [
                "3:document.title:value",
                "4:process.env.A:value",
                "5:foo:value",
            ]
        );
    }

    #[test]
    fn what_cannot_be_read_is_unread() {
        let source = "\
import('./a');
import(name);
import(`./b`);
require('x');
require(name);
require('x' + y);
eval(code);
window.eval(code);
new Function('return 1');
Function('a', 'b');
const require2 = require;
function f(eval) { eval(1); }
";
        assert_eq!(
            unread(source),
            [
                "2:import()",
                "5:require",
                "6:require",
                "7:eval",
                "8:eval",
                "9:Function",
                "10:Function"
            ]
        );
    }

    #[test]
    fn a_name_require_or_a_dynamic_import_binds_is_the_module_as_an_import_binds_it() {
        let source = "\
const fs = require('fs');
const { readFile, promises: { stat } } = require('node:fs');
const m = await import('m');
const { a } = await import('n');
const c = (await import('c')).default;
const d = require('d').default;
let e = require('e') as any;
fs.readFileSync('x');
readFile('y');
stat('z');
m.f();
a();
c();
d.g();
e.h;
function g() { const h = require('h'); return h.i(); }
";
        let used = used(source);
        let has = |line: &str| assert!(used.contains(&line.to_string()), "{line} in {used:#?}");
        // The names a destructuring takes are uses at the key, as the names of an import are
        has("2:\"node:fs\".readFile:imported");
        has("2:\"node:fs\".promises.stat:imported");
        has("4:\"n\".a:imported");
        // A namespace takes no name
        assert!(!used.iter().any(|line| line.starts_with("1:\"fs\"")));
        has("8:\"fs\".readFileSync:call 1");
        has("9:\"node:fs\".readFile:call 1");
        has("10:\"node:fs\".promises.stat:call 1");
        has("11:\"m\".f:call 0");
        has("12:\"n\".a:call 0");
        has("13:\"c\".default:call 0");
        has("14:\"d\".default.g:call 0");
        has("15:\"e\".h:value");
        has("16:\"h\".i:call 0");
        // The `require` itself is read as ever, a use of the global
        has("1:require:call 1");
    }

    #[test]
    fn what_require_or_import_binds_is_followed_only_when_it_is_the_whole_initializer_and_the_global()
     {
        let source = "\
const a = wrap(require('a'));
const b = require(name);
const c = await import(name);
const d = require('d')();
const { ...e } = require('e');
const [f] = require('f');
function g(require) { const h = require('h'); return h.i(); }
a.x();
b.x();
c.x();
d.x();
e.x();
f.x();
";
        let names: Vec<String> = used(source)
            .into_iter()
            .filter(|line| line.contains(".x:"))
            .collect();
        // None of them is a module's: each is the file's own name
        assert_eq!(names, Vec::<String>::new());
        assert_eq!(unread(source), ["2:require", "3:import()"]);
    }

    #[test]
    fn what_a_file_binds_as_require_or_eval_is_its_own() {
        let source = "\
import { require } from 'mod';
require(name);
function g(Function) { return new Function(x); }
";
        assert!(unread(source).is_empty());
    }

    #[test]
    fn the_uses_are_in_the_order_of_the_lines() {
        let source = "\
a();
import { b } from 'b';
c();
";
        assert_eq!(
            used(source),
            ["1:a:call 0", "2:\"b\".b:imported", "3:c:call 0"]
        );
    }
}
