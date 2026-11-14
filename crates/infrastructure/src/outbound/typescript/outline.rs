//! The outline of a TypeScript or JavaScript file, read from oxc's syntax tree: every definition in it with its names
//! and its range, for a knowledge document that follows one (`follows.rs` in `domain` picks the pieces of a name from
//! it).
//!
//! It parses the file itself and, unlike [`read`](super::read), gives up on a file oxc gives up on: a program that is
//! empty because of an error would say every definition is gone.

use oxc_allocator::Allocator;
use oxc_ast::ast::{
    BindingPattern, Class, ClassElement, Declaration, Decorator, ExportDefaultDeclarationKind,
    PropertyKey, Statement, TSEnumMemberName, TSInterfaceDeclaration, TSNamespaceDeclaration,
    TSNamespaceDeclarationBody, TSSignature,
};
use oxc_parser::Parser;
use oxc_span::{GetSpan, Span};

use domain::definitions::Definition;
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
}
