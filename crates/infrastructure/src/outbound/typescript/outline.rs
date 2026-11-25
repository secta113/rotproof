//! The outline of a TypeScript or JavaScript file, read from oxc's syntax tree: every definition in it with its names
//! and its range, for a knowledge document that follows one (`follows.rs` in `domain` picks the pieces of a name from
//! it).
//!
//! It parses the file itself and, unlike [`read`](super::read), gives up on a file oxc gives up on: a program that is
//! empty because of an error would say every definition is gone.

use std::collections::HashSet;

use oxc_allocator::Allocator;
use oxc_ast::ast::{
    BindingPattern, Class, ClassElement, Declaration, Decorator, ExportDefaultDeclarationKind,
    Expression, PropertyKey, Statement, TSEnumMemberName, TSInterfaceDeclaration,
    TSNamespaceDeclaration, TSNamespaceDeclarationBody, TSSignature, TSType, TSTypeName,
    TSTypeQueryExprName, UnaryOperator, VariableDeclarationKind,
};
use oxc_parser::Parser;
use oxc_span::{GetSpan, Span};

use domain::definitions::{Definition, Kind, Shape};
use domain::follows::is_name;
use domain::layers::Language;

use super::{first_error, source_type};

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
/// Each definition also has a [`Shape`], for the vocabularies of `protocol.inbound`; every one not named here is
/// `Plain`:
///
/// - **`Vocabulary`**: a `const` at the top of the file (not in a namespace) bound to an array of string or number
///   literals `as const` (a `satisfies` after it too), with no spread, hole or template and at least one element; and
///   a type alias without type parameters that takes the elements of such a `const` of the same file, before or after
///   it (`typeof X[number]`).
/// - **`Enum`**: every `enum` (`const` and `declare` too), wherever it is declared. Its members stay `Plain`.
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
    let mut outline = Outline {
        found: Vec::new(),
        vocabularies: vocabulary_consts(&parsed.program.body),
    };
    outline.statements(&parsed.program.body, &mut Vec::new());
    Ok(outline.found)
}

/// The definitions of a program, as they are met.
struct Outline {
    found: Vec<Definition>,
    /// The names of the `const`s at the top of the file that are vocabularies, found before the walk: an alias may
    /// come before the `const` it takes the elements of
    vocabularies: HashSet<String>,
}

impl Outline {
    /// `name` under the names in `scope` is a definition: record its range, from its first decorator.
    fn add(
        &mut self,
        scope: &[String],
        name: &str,
        kind: Kind,
        span: Span,
        decorators: &[Decorator],
    ) {
        self.add_shaped(scope, name, kind, span, decorators, Shape::Plain);
    }

    /// [`Self::add`] for a definition that is more than `Plain`.
    fn add_shaped(
        &mut self,
        scope: &[String],
        name: &str,
        kind: Kind,
        span: Span,
        decorators: &[Decorator],
        shape: Shape,
    ) {
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
            kind,
            shape,
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
                self.add(scope, name, Kind::Function, span, &[]);
            }
            ExportDefaultDeclarationKind::ClassDeclaration(class) => {
                let name = class.id.as_ref().map_or("default", |id| id.name.as_str());
                self.class(class, name, span, scope);
            }
            ExportDefaultDeclarationKind::TSInterfaceDeclaration(interface) => {
                self.interface(interface, interface.id.name.as_str(), span, scope);
            }
            ExportDefaultDeclarationKind::ArrowFunctionExpression(_)
            | ExportDefaultDeclarationKind::FunctionExpression(_) => {
                self.add(scope, "default", Kind::Function, span, &[]);
            }
            _ => self.add(scope, "default", Kind::Other, span, &[]),
        }
    }

    fn declaration(&mut self, declaration: &Declaration, span: Span, scope: &mut Vec<String>) {
        match declaration {
            Declaration::VariableDeclaration(variables) => {
                for declarator in &variables.declarations {
                    if let BindingPattern::BindingIdentifier(id) = &declarator.id {
                        let kind = if is_function(declarator.init.as_ref()) {
                            Kind::Function
                        } else {
                            Kind::Other
                        };
                        // Only a `const` at the top: the rule that reads a vocabulary does not look into a namespace
                        let shape = if scope.is_empty()
                            && variables.kind == VariableDeclarationKind::Const
                            && is_vocabulary(declarator.init.as_ref())
                        {
                            Shape::Vocabulary
                        } else {
                            Shape::Plain
                        };
                        self.add_shaped(scope, id.name.as_str(), kind, span, &[], shape);
                    }
                }
            }
            Declaration::FunctionDeclaration(function) => {
                if let Some(id) = &function.id {
                    self.add(scope, id.name.as_str(), Kind::Function, span, &[]);
                }
            }
            Declaration::ClassDeclaration(class) => {
                if let Some(id) = &class.id {
                    self.class(class, id.name.as_str(), span, scope);
                }
            }
            Declaration::TSTypeAliasDeclaration(alias) => {
                let shape = if scope.is_empty()
                    && alias.type_parameters.is_none()
                    && takes_elements_of(&alias.type_annotation, &self.vocabularies)
                {
                    Shape::Vocabulary
                } else {
                    Shape::Plain
                };
                self.add_shaped(scope, alias.id.name.as_str(), Kind::Type, span, &[], shape);
            }
            Declaration::TSInterfaceDeclaration(interface) => {
                self.interface(interface, interface.id.name.as_str(), span, scope);
            }
            Declaration::TSEnumDeclaration(declared) => {
                let name = declared.id.name.as_str();
                self.add_shaped(scope, name, Kind::Type, span, &[], Shape::Enum);
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
                        self.add(scope, name, Kind::Other, member.span, &[]);
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
        self.add(scope, name, Kind::Other, span, &[]);
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
        self.add(scope, name, Kind::Type, span, &class.decorators);
        scope.push(name.to_string());
        for element in &class.body.body {
            let (key, span, decorators, kind) = match element {
                ClassElement::MethodDefinition(member) => {
                    (&member.key, member.span, &member.decorators, Kind::Function)
                }
                ClassElement::PropertyDefinition(member) => {
                    let kind = if is_function(member.value.as_ref()) {
                        Kind::Function
                    } else {
                        Kind::Other
                    };
                    (&member.key, member.span, &member.decorators, kind)
                }
                ClassElement::AccessorProperty(member) => {
                    (&member.key, member.span, &member.decorators, Kind::Other)
                }
                ClassElement::StaticBlock(_) | ClassElement::TSIndexSignature(_) => continue,
            };
            if let Some(name) = member_name(key) {
                self.add(scope, &name, kind, span, decorators);
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
        self.add(scope, name, Kind::Type, span, &[]);
        scope.push(name.to_string());
        for signature in &interface.body.body {
            let (key, span) = match signature {
                TSSignature::TSPropertySignature(member) => (&member.key, member.span),
                TSSignature::TSMethodSignature(member) => (&member.key, member.span),
                _ => continue,
            };
            if let Some(name) = member_name(key) {
                self.add(scope, &name, Kind::Other, span, &[]);
            }
        }
        scope.pop();
    }
}

/// The names of the `const`s at the top of a program (exported or not) that are vocabularies.
fn vocabulary_consts(body: &[Statement]) -> HashSet<String> {
    let mut names = HashSet::new();
    for statement in body {
        let declaration = match statement {
            Statement::ExportDeclaration(export) => Some(&export.declaration),
            other => other.as_declaration(),
        };
        let Some(Declaration::VariableDeclaration(variables)) = declaration else {
            continue;
        };
        if variables.kind != VariableDeclarationKind::Const {
            continue;
        }
        for declarator in &variables.declarations {
            if let BindingPattern::BindingIdentifier(id) = &declarator.id
                && is_vocabulary(declarator.init.as_ref())
            {
                names.insert(id.name.to_string());
            }
        }
    }
    names
}

/// The expression inside any parentheses around it.
fn unparenthesized<'a, 'b>(mut value: &'b Expression<'a>) -> &'b Expression<'a> {
    while let Expression::ParenthesizedExpression(inner) = value {
        value = &inner.expression;
    }
    value
}

/// Whether a value is the initializer of a vocabulary: an array of string or number literals `as const`, with a
/// `satisfies` after it once at most. A spread, a hole, a template or any other element, and an empty array, say that
/// the values are not all written there.
fn is_vocabulary(value: Option<&Expression>) -> bool {
    let Some(value) = value else {
        return false;
    };
    let mut value = unparenthesized(value);
    if let Expression::TSSatisfiesExpression(satisfies) = value {
        value = unparenthesized(&satisfies.expression);
    }
    let Expression::TSAsExpression(cast) = value else {
        return false;
    };
    let is_const = matches!(
        &cast.type_annotation,
        TSType::TSTypeReference(reference)
            if reference.type_arguments.is_none()
                && matches!(
                    &reference.type_name,
                    TSTypeName::IdentifierReference(id) if id.name == "const"
                )
    );
    let Expression::ArrayExpression(array) = unparenthesized(&cast.expression) else {
        return false;
    };
    is_const
        && !array.elements.is_empty()
        && array
            .elements
            .iter()
            .all(|element| element.as_expression().is_some_and(is_literal_value))
}

/// Whether an element is a string, a number, or a number after a minus.
fn is_literal_value(element: &Expression) -> bool {
    match element {
        Expression::StringLiteral(_) | Expression::NumericLiteral(_) => true,
        Expression::UnaryExpression(unary) => {
            unary.operator == UnaryOperator::UnaryNegation
                && matches!(unary.argument, Expression::NumericLiteral(_))
        }
        _ => false,
    }
}

/// Whether a type is `typeof X[number]` (parentheses round `typeof X` too) where `X` is in `vocabularies`.
fn takes_elements_of(ty: &TSType, vocabularies: &HashSet<String>) -> bool {
    let TSType::TSIndexedAccessType(indexed) = ty else {
        return false;
    };
    if !matches!(indexed.index_type, TSType::TSNumberKeyword(_)) {
        return false;
    }
    let mut object = &indexed.object_type;
    while let TSType::TSParenthesizedType(inner) = object {
        object = &inner.type_annotation;
    }
    matches!(
        object,
        TSType::TSTypeQuery(query)
            if query.type_arguments.is_none()
                && matches!(
                    &query.expr_name,
                    TSTypeQueryExprName::IdentifierReference(id)
                        if vocabularies.contains(id.name.as_str())
                )
    )
}

/// Whether a value is a function: an arrow function or a function expression, which a name is bound to.
fn is_function(value: Option<&Expression>) -> bool {
    matches!(
        value,
        Some(Expression::ArrowFunctionExpression(_) | Expression::FunctionExpression(_))
    )
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

#[cfg(test)]
mod tests {
    use super::*;
    use domain::definitions::pieces;

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
    fn a_function_a_method_and_a_name_bound_to_a_function_are_functions_and_a_class_or_a_type_is_a_type()
     {
        let source = r#"
export function f() {}
export const g = () => 1, h = 2;
const k = function () {};
export default () => 1;
class C {
  m() {}
  p = 1;
  q = () => 2;
  accessor a = 1;
}
interface I { x: number; y(): void; }
type T = { a: number };
enum E { A, B }
namespace N { export function n() {} }
"#;
        let kinds: Vec<(String, Kind)> = outline(source, "a.ts")
            .unwrap()
            .into_iter()
            .map(|d| (d.names.join("."), d.kind))
            .collect();
        let kind = |name: &str| kinds.iter().find(|(n, _)| n == name).unwrap().1;
        for function in ["f", "g", "k", "default", "C.m", "C.q", "N.n"] {
            assert_eq!(kind(function), Kind::Function, "{function}");
        }
        for ty in ["C", "I", "T", "E"] {
            assert_eq!(kind(ty), Kind::Type, "{ty}");
        }
        for other in ["h", "C.p", "C.a", "I.x", "I.y", "E.A", "N"] {
            assert_eq!(kind(other), Kind::Other, "{other}");
        }
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

    /// The shape of every definition of a name in a file, as the outline gives it.
    fn shapes(source: &str, dotted: &str) -> Vec<Shape> {
        outline(source, "a.ts")
            .unwrap()
            .into_iter()
            .filter(|d| d.names.join(".") == dotted)
            .map(|d| d.shape)
            .collect()
    }

    fn is_vocabulary_of(source: &str, name: &str) -> bool {
        shapes(source, name) == [Shape::Vocabulary]
    }

    #[test]
    fn a_const_array_of_literals_as_const_is_a_vocabulary_exported_or_not() {
        for source in [
            "export const A = ['a', 'b'] as const;",
            "const A = ['a', 'b'] as const;",
            "const A = [1, 2.5, -3] as const;",
            "const A = ['a', 1, -1] as const;",
            "const A = ['a'] as const satisfies readonly string[];",
            "const A = (['a'] as const) satisfies readonly string[];",
            "const A = (['a', 'b'] as const);",
            "const A = (((['a'] as const)));",
            "const A = ['a',] as const;",
            "const A = [\"a\"] as const;",
        ] {
            assert!(is_vocabulary_of(source, "A"), "{source}");
        }
    }

    #[test]
    fn a_const_that_is_not_an_array_of_literals_as_const_stays_plain() {
        for source in [
            "let A = ['a'] as const;",
            "var A = ['a'] as const;",
            "const A = ['a'];",
            "const A = ['a'] satisfies readonly string[];",
            "const A = ['a'] as string[];",
            "const A = ['a'] as const as const;",
            "const A = ['a'] as const satisfies readonly string[] satisfies unknown;",
            "const A = [] as const;",
            "const A = [...B] as const;",
            "const A = ['a', ...B] as const;",
            "const A = ['a', , 'b'] as const;",
            "const A = [`a`] as const;",
            "const A = [`a${B}`] as const;",
            "const A = ['a', B] as const;",
            "const A = ['a', +1] as const;",
            "const A = ['a', -B] as const;",
            "const A = ['a', null] as const;",
            "const A = [['a']] as const;",
            "const A = { a: 1 } as const;",
            "const A = 'a' as const;",
            "declare const A: readonly ['a'];",
        ] {
            assert_eq!(shapes(source, "A"), [Shape::Plain], "{source}");
        }
    }

    #[test]
    fn one_statement_can_declare_a_vocabulary_beside_plain_values() {
        let source = "const A = 1, B = ['a'] as const, C = ['c'];";
        assert_eq!(shapes(source, "A"), [Shape::Plain]);
        assert_eq!(shapes(source, "B"), [Shape::Vocabulary]);
        assert_eq!(shapes(source, "C"), [Shape::Plain]);
    }

    #[test]
    fn a_const_inside_a_namespace_or_a_function_is_not_a_vocabulary() {
        let source = "namespace N { export const A = ['a'] as const; } \
            function f() { const B = ['b'] as const; } \
            if (true) { const C = ['c'] as const; }";
        let outline = outline(source, "a.ts").unwrap();
        assert!(
            outline.iter().all(|d| d.shape == Shape::Plain),
            "{outline:?}"
        );
        assert!(outline.iter().any(|d| d.names == ["N", "A"]));
    }

    #[test]
    fn a_type_alias_over_the_elements_of_a_vocabulary_is_a_vocabulary() {
        let declared = "const A = ['a', 'b'] as const;";
        for alias in [
            "type T = typeof A[number];",
            "type T = (typeof A)[number];",
            "export type T = (typeof A)[number];",
            "type T = ((typeof A))[number];",
        ] {
            // Before the const, after it, and with the const exported
            for source in [
                format!("{alias} {declared}"),
                format!("{declared} {alias}"),
                format!("export {declared} {alias}"),
            ] {
                assert!(is_vocabulary_of(&source, "T"), "{source}");
                assert!(is_vocabulary_of(&source, "A"), "{source}");
            }
        }
    }

    #[test]
    fn a_type_alias_that_does_not_take_the_elements_of_a_vocabulary_stays_plain() {
        let declared = "const A = ['a', 'b'] as const;";
        for alias in [
            // Type parameters, even unused
            "type T<U> = (typeof A)[number];",
            // Not `number` as the index, or not `typeof` of a plain name as the object
            "type T = (typeof A)[0];",
            "type T = (typeof A)['length'];",
            "type T = (typeof A)[string];",
            "type T = (typeof A)[keyof typeof A];",
            "type T = typeof A;",
            "type T = A[number];",
            "type T = (typeof A.b)[number];",
            "type T = (typeof B)[number];",
            "type T = (typeof A)[number] | 'c';",
            "type T = readonly (typeof A)[number][];",
            "type T = (typeof A)[number][];",
            "type T = ((typeof A)[number]);",
            "type T = (typeof A<string>)[number];",
        ] {
            let source = format!("{declared} {alias}");
            assert_eq!(shapes(&source, "T"), [Shape::Plain], "{source}");
        }
        // A const that is no vocabulary, a const of another name, and a nested one
        for (const_source, alias) in [
            (
                "const A = ['a', 1n] as const;",
                "type T = (typeof A)[number];",
            ),
            ("const A = ['a'];", "type T = (typeof A)[number];"),
            ("let A = ['a'] as const;", "type T = (typeof A)[number];"),
            ("const B = ['a'] as const;", "type T = (typeof A)[number];"),
            (
                "function f() { const A = ['a'] as const; }",
                "type T = (typeof A)[number];",
            ),
            (
                "namespace N { export const A = ['a'] as const; }",
                "type T = (typeof A)[number];",
            ),
            ("", "type T = (typeof A)[number];"),
        ] {
            let source = format!("{const_source} {alias}");
            assert_eq!(shapes(&source, "T"), [Shape::Plain], "{source}");
        }
        // An alias inside a namespace is not read, even over a vocabulary at the top
        let source =
            "const A = ['a'] as const; namespace N { export type T = (typeof A)[number]; }";
        assert_eq!(shapes(source, "N.T"), [Shape::Plain]);
    }

    #[test]
    fn every_enum_is_an_enum_and_its_members_stay_plain() {
        let source = "enum A { X, Y } export enum B { X = 1 } const enum C { X } declare enum D { X } \
            declare const enum E { X } namespace N { export enum F { X } }";
        for name in ["A", "B", "C", "D", "E", "N.F"] {
            assert_eq!(shapes(source, name), [Shape::Enum], "{name}");
        }
        for member in ["A.X", "B.X", "C.X", "D.X", "E.X", "N.F.X"] {
            assert_eq!(shapes(source, member), [Shape::Plain], "{member}");
        }
        assert_eq!(shapes(source, "N"), [Shape::Plain]);
    }

    #[test]
    fn what_is_neither_a_vocabulary_nor_an_enum_is_plain() {
        let outline = outline(DEFINED, "src/api.ts").unwrap();
        let shape = |name: &str| {
            outline
                .iter()
                .find(|d| d.names == [name])
                .unwrap()
                .shape
                .clone()
        };
        for name in ["Router", "f", "h", "i", "j", "default", "T", "A"] {
            assert_eq!(shape(name), Shape::Plain, "{name}");
        }
        assert_eq!(shape("Color"), Shape::Enum);
    }
}
