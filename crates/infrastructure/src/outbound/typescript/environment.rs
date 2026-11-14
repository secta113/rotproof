//! The reads of the environment in a TypeScript or JavaScript file (`domain::environment`): `process.env` and
//! `import.meta.env`.
//!
//! The visitor is a reader of its own: what counts as the environment is `process` however the file names it, and
//! `import.meta.env` except the values Vite sets itself ([`VITE_OWN`]). It shares with the other readers of
//! `typescript.rs` only the parse and [`literal`].

use std::collections::BTreeSet;

use oxc_ast::ast::{
    BindingPattern, ComputedMemberExpression, Expression, ImportDeclarationSpecifier, Program,
    Statement, StaticMemberExpression, VariableDeclarator,
};
use oxc_ast_visit::{Visit, walk};

use domain::environment::VITE_OWN;
use domain::text::line_of;

use super::literal;

/// The modules whose default or namespace import is Node's `process`.
const PROCESS: [&str; 2] = ["process", "node:process"];

/// Every read or write of the environment in a file (`domain::environment`): `process.env` (the global `process`,
/// `globalThis.process`, or a name the file imports `process` as), `import.meta.env`, each as a member
/// (`process.env.X`, `process["env"]`) or taken apart (`const { env } = process`), and `env` imported from
/// `process` (`import { env } from "node:process"`). The line, and `process.env` or `import.meta.env`.
pub(super) fn environment(source: &str, program: &Program) -> Vec<(usize, String)> {
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

#[cfg(test)]
mod tests {
    use crate::outbound::typescript::read;

    #[test]
    fn every_way_to_reach_the_environment_is_read_and_what_vite_sets_itself_is_not() {
        let source = "\
import { env } from 'node:process';
import proc from 'process';
const a = process.env.A;
const b = proc.env['B'];
const c = import.meta.env.VITE_URL;
const d = import.meta.env.DEV;
const { env: e } = globalThis.process;
const { MODE, PROD } = import.meta.env;
const { VITE_KEY } = import.meta.env;
const f = other.env.F;
";
        let read = read(source, "src/infrastructure/x.ts");
        assert_eq!(read.error, None);
        let found: Vec<(usize, &str)> = read
            .environment
            .iter()
            .map(|(line, what)| (*line, what.as_str()))
            .collect();
        assert_eq!(
            found,
            [
                (1, "process.env"),
                (3, "process.env"),
                (4, "process.env"),
                (5, "import.meta.env"),
                (7, "process.env"),
                (9, "import.meta.env"),
            ]
        );
    }
}
