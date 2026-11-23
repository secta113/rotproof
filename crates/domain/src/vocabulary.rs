//! The vocabularies of a unit's `domain` that its `protocol.inbound` may import: an input port names the values it
//! takes and gives back, and a list of names with no rule in it is what both sides may share.
//!
//! `protocol.inbound` imports from its own unit's `domain` the definitions whose shape is [`Shape::Vocabulary`],
//! **by name, from the file (in Rust, the module) of `domain` that defines them**, and nothing else from there. A name
//! a file only re-exports is not defined there, so it fails: re-exports are followed nowhere in Rotproof. An import
//! that names nothing (a whole module, a default export, a side effect, a glob) fails too: what it brings in cannot be
//! told. The rule judges what the importing language names, so each language hands over the names its import takes and
//! the outline of the file it lands in; it reads no file itself.
//!
//! - **Python and TypeScript:** a vocabulary is a definition at the top of the file whose reader gave it the shape
//!   (the readers' doc comments, on [`Shape`], say which). A TypeScript `enum` is no vocabulary, and the finding says
//!   to write an array `as const` in its place.
//! - **Rust:** an `enum` whose shape is a vocabulary is one only while it holds no rule, and its rules are in `impl`
//!   blocks that can be in any file of the crate, in any module, in a function's body too. The rule reads the crate
//!   whole ([`Crate`]) and resolves every path an `impl` block, a derive or a `type` alias is written with to the item
//!   it stands for, in the module it is written in ([`Names`]), so an enum is told from another of the same name in
//!   another module, and an `impl` through `use .. as K` or `type K = ..` is the enum's. An enum stops being a
//!   vocabulary when an `impl` block (a) is inherent for it and holds a function, or (b) implements for it a trait
//!   that resolves to a trait the crate defines, and either the block holds a function or the trait has a method with
//!   a body, which the type then has, or when (c) it derives such a trait. What a path cannot be resolved to (a name
//!   that a glob `use m::*;` may bring) is matched by its last name, so what falls to failure is never let through. An
//!   `impl` of a trait of the standard library or of another crate, and a derive of one, leave it a vocabulary, as
//!   does an `impl`, inherent or of a trait of the project, that brings constants or types alone (data, with no code to
//!   run). A reference or a pointer to the enum is the enum; a type that holds it (`Box<Kind>`) is another type.
//!   What hides the rules of every enum fails the whole crate, and the finding says which and where: a `macro_rules!`
//!   whose body holds `impl`, a call of any macro (`m!(..)`, `m![..]`, `m!{..}`) with `impl` in its arguments, which
//!   cannot be read as code (no macro name is exempt), an `include!`, and an `impl` of a trait of the project for a
//!   type parameter (`impl<T: Bound> Rank for T`, which may hold for the enum), when it holds a function or the trait
//!   has a method with a body. The `impl` blocks in the body of a function, in a closure or a block inside it, and in
//!   the initializer of a `const` or a `static` are read ([`nested_impls`]).

use std::collections::BTreeMap;

use crate::definitions::{Definition, Kind, Shape};
use crate::direction::{DOMAIN, ENUM_INSTEAD, Forbidden, PROTOCOL_INBOUND, Rule};
use crate::layers::Place;
use crate::rust::{
    Names, Resolved, Source, calls_holding, derives, inline_chains, invocations, join,
    macros_holding, module_at, nested_impls,
};
use crate::text::line_of;

/// The names an import takes that name no definition: the whole module (`import * as ns`, `from m import *`) and the
/// default export.
const WHOLE: [&str; 2] = ["*", "default"];

/// Whether an import from `from` that lands in `to` is the one this rule judges: a `protocol.inbound` importing the
/// `domain` of its own unit, which the layer table forbids. The `domain` of a shared unit another unit's
/// `protocol.inbound` reads whole is not this case: the table's own rule for it is kept.
pub fn applies(from: &Place, to: &Place) -> bool {
    from.name == PROTOCOL_INBOUND && to.name == DOMAIN && from.unit.name == to.unit.name
}

/// Why a Rust `enum` that would be a vocabulary is not one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Disqualified {
    /// An inherent `impl` block holds a function
    Methods,
    /// An `impl` block implements this trait of the project for it, with a function in the block or a method with a
    /// body in the trait
    Trait(String),
    /// It derives this trait of the project
    Derive(String),
    /// The crate holds what no reading of its code can follow, so no enum of it can be told a vocabulary: what it is
    /// and where, as the finding says it
    Unseen(String),
}

/// What a name defined at the top of a file is, for the rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Class {
    /// Every definition of the name is a vocabulary
    Vocabulary,
    /// A TypeScript `enum`
    Enum,
    /// A Rust `enum` of variants without fields that holds a rule: in an `impl` block anywhere in its crate, in a
    /// derive, or in what the crate hides
    Implemented(Disqualified),
    /// Anything else: a record, a class, a function, a type, a constant, a trait, a module
    Other,
}

/// The names a file defines at its top, each with what it is: the part of the landing file the rule reads. A name that
/// the file defines more than once is a vocabulary only when each of its definitions is (`export const X = [...] as
/// const` and `export type X = (typeof X)[number]` are one vocabulary), and is otherwise what its first definition
/// that is none is.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Defined {
    classes: BTreeMap<String, Class>,
}

impl Defined {
    /// What the file whose outline is `outline` defines at its top, for Python and TypeScript.
    pub fn new(outline: &[Definition]) -> Self {
        Self::of(outline, &[], &[], &BTreeMap::new(), None)
    }

    /// What the module `scope` of a Rust file, whose own module is `module`, defines: the outline of the file and the
    /// names of the inline modules down to it (`scope`, empty for the file's own top), the enums that the crate takes
    /// out (`disqualified`, by path from the crate's root), and, when the crate holds what cannot be seen, why every
    /// enum is out.
    fn of(
        outline: &[Definition],
        module: &[String],
        scope: &[String],
        disqualified: &BTreeMap<Vec<String>, Disqualified>,
        unseen: Option<&str>,
    ) -> Self {
        let mut classes: BTreeMap<String, Class> = BTreeMap::new();
        for definition in outline {
            let [parents @ .., name] = definition.names.as_slice() else {
                continue;
            };
            // An `impl` block defines no name of its own: the type it is for is defined where that type is
            if parents != scope || matches!(definition.shape, Shape::Implementation(_)) {
                continue;
            }
            let why = match unseen {
                Some(unseen) => Some(Disqualified::Unseen(unseen.to_string())),
                None => disqualified
                    .get(&join(&join(module, scope), std::slice::from_ref(name)))
                    .cloned(),
            };
            let class = match (&definition.shape, why) {
                (Shape::Vocabulary, None) => Class::Vocabulary,
                (Shape::Vocabulary, Some(why)) => Class::Implemented(why),
                (Shape::Enum, _) => Class::Enum,
                _ => Class::Other,
            };
            match classes.get(name) {
                Some(known) if *known != Class::Vocabulary => {}
                _ => {
                    classes.insert(name.clone(), class);
                }
            }
        }
        Defined { classes }
    }

    /// The names of the file that a `protocol.inbound` may import.
    pub fn vocabularies(&self) -> Vec<&str> {
        self.classes
            .iter()
            .filter(|(_, class)| **class == Class::Vocabulary)
            .map(|(name, _)| name.as_str())
            .collect()
    }
}

/// What a Rust crate defines in each of its modules, for the imports of its vocabularies by name: each module a file or
/// an inline `mod` makes, with the names it defines and what each is ([`Defined`]), read from every file of the crate
/// at once, since the rules of an enum are written anywhere in it.
#[derive(Debug, Clone, Default)]
pub struct Crate {
    modules: BTreeMap<Vec<String>, Defined>,
}

impl Crate {
    /// What the crate whose program files are `files` defines. An item under `#[cfg(test)]` is no part of the program,
    /// so the files hold their code with it blanked ([`crate::rust::without_tests`]).
    pub fn new(files: &[Source]) -> Self {
        // The blocks the outline of a file does not hold (those in a body, a const, a static or a macro call) put
        // methods on a type all the same, so they count here, and nowhere that reads the outline for a key
        let outlines: Vec<Vec<Definition>> = files
            .iter()
            .map(|file| {
                let nested = nested_impls(file.code);
                file.outline.iter().cloned().chain(nested).collect()
            })
            .collect();
        let sources: Vec<Source> = files
            .iter()
            .zip(&outlines)
            .map(|(file, outline)| Source { outline, ..*file })
            .collect();
        let names = Names::new(&sources);
        let found = disqualified(&sources, &names);
        let mut modules: BTreeMap<Vec<String>, Defined> = names
            .modules()
            .iter()
            .map(|module| (module.clone(), Defined::default()))
            .collect();
        for file in files {
            let scopes = std::iter::once(Vec::new()).chain(inline_chains(file.code));
            for scope in scopes {
                let defined = Defined::of(
                    file.outline,
                    file.module,
                    &scope,
                    &found.enums,
                    found.unseen.as_deref(),
                );
                modules
                    .entry(join(file.module, &scope))
                    .or_default()
                    .classes
                    .extend(defined.classes);
            }
        }
        Crate { modules }
    }

    /// What the module at `path` from the crate's root defines, or `None` when the crate has no such module.
    pub fn module(&self, path: &[String]) -> Option<&Defined> {
        self.modules.get(path)
    }

    /// The finding for the path `rest` from the crate's root (what follows the crate's name in `domain::x::Kind`),
    /// written by `file` on `line`, or `None` when it passes: it passes when it names, in the module it goes down to, a
    /// vocabulary that module defines. A path that stops at a module, ends in `self` or in a glob names no item, and an
    /// item a module only re-exports is not defined there: both fail, with the words of [`judged`]. What follows the
    /// item (`Kind::A`) is a variant or an associated item, which it takes with it.
    pub fn judged(
        &self,
        file: &str,
        line: usize,
        import: &str,
        what: &str,
        rest: &[String],
    ) -> Option<Forbidden> {
        let mut module: Vec<String> = Vec::new();
        let mut taken: Vec<String> = Vec::new();
        for (at, name) in rest.iter().enumerate() {
            let ends = name == "*" || (name == "self" && at + 1 == rest.len());
            let below = join(&module, std::slice::from_ref(name));
            if ends {
                break;
            }
            if self.modules.contains_key(&below) {
                module = below;
                continue;
            }
            taken.push(name.clone());
            break;
        }
        let nothing = Defined::default();
        let defined = self.modules.get(&module).unwrap_or(&nothing);
        judged(file, line, import, what, &taken, defined)
    }
}

/// What the `impl` blocks, derives and hidden code of a crate take out of its enums.
#[derive(Debug, Default)]
struct Found {
    /// The enums, by path from the crate's root, that a block or a derive of the crate takes out, and why
    enums: BTreeMap<Vec<String>, Disqualified>,
    /// Why no enum of the crate can be told a vocabulary, when it holds what cannot be seen
    unseen: Option<String>,
}

/// What the files of a crate take out of its enums: from every `impl` block, derive and macro of every file, with each
/// path resolved in the module it is written in.
fn disqualified(files: &[Source], names: &Names) -> Found {
    let mut traits: BTreeMap<Vec<String>, bool> = BTreeMap::new();
    for file in files {
        for definition in file.outline {
            if definition.shape == Shape::Trait {
                // Two traits of one path are one: a method with a body in either counts
                *traits
                    .entry(join(file.module, &definition.names))
                    .or_default() |= holds_function(file.outline, definition);
            }
        }
    }
    // Which of the project's traits a path written in `module` can be, each by its name and whether it has a method
    // with a body: the traits it resolves to, and every trait of its last name where it cannot be told
    let project = |module: &[String], path: &[String]| -> Vec<(String, bool)> {
        let mut found = Vec::new();
        for resolved in names.resolve(module, path) {
            match resolved {
                Resolved::Item(item) => {
                    if let (Some(bodied), Some(name)) = (traits.get(&item), item.last()) {
                        found.push((name.clone(), *bodied));
                    }
                }
                Resolved::Unknown => found.extend(
                    traits
                        .iter()
                        .filter(|(item, _)| item.last() == path.last())
                        .filter_map(|(item, bodied)| Some((item.last()?.clone(), *bodied))),
                ),
                Resolved::Module(_) | Resolved::External => {}
            }
        }
        found
    };
    let mut found = Found::default();
    let mut macros = None;
    let mut calls = None;
    let mut includes = None;
    let mut blanket = None;
    for file in files {
        let at = |offset: usize| format!("{}:{}", file.path, line_of(file.code, offset));
        if macros.is_none()
            && let Some((offset, name)) = macros_holding(file.code, "impl").into_iter().next()
        {
            macros = Some(format!(
                "the macro `{name}` holds `impl` blocks, which are not followed, at {}",
                at(offset)
            ));
        }
        if calls.is_none()
            && let Some((offset, name)) = calls_holding(file.code, "impl").into_iter().next()
        {
            calls = Some(format!(
                "the call `{name}!` has `impl` in its arguments, which cannot be read as code, at {}",
                at(offset)
            ));
        }
        if includes.is_none()
            && let Some(offset) = invocations(file.code, "include").into_iter().next()
        {
            includes = Some(format!(
                "`include!` takes in code that is not followed, at {}",
                at(offset)
            ));
        }
        for block in file.outline {
            let Shape::Implementation(implementation) = &block.shape else {
                continue;
            };
            let Some((_, scope)) = block.names.split_last() else {
                continue;
            };
            let module = join(file.module, scope);
            let holds = holds_function(file.outline, block);
            let why = match &implementation.of {
                None if holds => Disqualified::Methods,
                None => continue,
                Some(of) => {
                    let traits = project(&module, of);
                    let bodied = traits.iter().any(|(_, bodied)| *bodied);
                    let Some((name, _)) = traits.first() else {
                        continue;
                    };
                    if !(holds || bodied) {
                        continue;
                    }
                    Disqualified::Trait(name.clone())
                }
            };
            if implementation.blanket {
                // Every type that meets its bound has it, and which do cannot be told
                if blanket.is_none() {
                    let Disqualified::Trait(of) = &why else {
                        continue;
                    };
                    blanket = Some(format!(
                        "the trait {of} of the project is implemented for a type parameter, which stands for any \
                         type, at {}",
                        at(block.start)
                    ));
                }
                continue;
            }
            for resolved in names.resolve(&module, &implementation.target) {
                let items = match resolved {
                    Resolved::Item(item) => vec![item],
                    Resolved::Unknown => implementation
                        .target
                        .last()
                        .map(|name| names.items_named(name))
                        .unwrap_or_default(),
                    Resolved::Module(_) | Resolved::External => Vec::new(),
                };
                for item in items {
                    found.enums.entry(item).or_insert_with(|| why.clone());
                }
            }
        }
        for (offset, name, derived) in derives(file.code) {
            let module = join(file.module, &module_at(file.code, offset));
            for path in derived {
                if let Some((of, _)) = project(&module, &path).into_iter().next() {
                    let item = join(&module, std::slice::from_ref(&name));
                    found.enums.entry(item).or_insert(Disqualified::Derive(of));
                }
            }
        }
    }
    let unseen: Vec<String> = [macros, calls, includes, blanket]
        .into_iter()
        .flatten()
        .collect();
    found.unseen = (!unseen.is_empty()).then(|| unseen.join("; "));
    found
}

/// Whether the `impl` block or the trait `block` of `outline` holds a function: an item named under the block's own
/// names, inside its range. A constant, or an associated type, is data, and so is a method of a trait without a body,
/// which every `impl` of the trait writes.
fn holds_function(outline: &[Definition], block: &Definition) -> bool {
    outline.iter().any(|item| {
        item.kind == Kind::Function
            && item.names.len() == block.names.len() + 1
            && item.names.starts_with(&block.names)
            && block.start <= item.start
            && item.end <= block.end
    })
}

/// The finding for an import by `file`, on `line`, from a `protocol.inbound` of the `domain` of its own unit, or
/// `None` when it passes: `taken` are the names it takes (as the module exports them: `default` and `*` stand for the
/// default export and the whole module, and an import that takes none at all, such as a side effect, a Python
/// `import domain.x` or a glob, gives none), and `defined` what the file it lands in defines. `what` says what was
/// imported, as "imports x". The file and `import` are what an approval is keyed by, as for every other forbidden
/// import.
///
/// Every name that is not a vocabulary of the file is said once, with what to do instead; none of the ways says to
/// write a definition twice.
pub fn judged(
    file: &str,
    line: usize,
    import: &str,
    what: &str,
    taken: &[String],
    defined: &Defined,
) -> Option<Forbidden> {
    let mut whole = taken.is_empty();
    let mut enums = Vec::new();
    let mut implemented = Vec::new();
    let mut unseen: Vec<(&String, Vec<&String>)> = Vec::new();
    let mut others = Vec::new();
    let mut missing = Vec::new();
    for name in taken {
        if WHOLE.contains(&name.as_str()) {
            whole = true;
            continue;
        }
        let list = match defined.classes.get(name) {
            Some(Class::Vocabulary) => continue,
            Some(Class::Enum) => &mut enums,
            Some(Class::Implemented(Disqualified::Unseen(why))) => {
                match unseen.iter_mut().find(|(known, _)| known == &why) {
                    Some((_, names)) if !names.contains(&name) => names.push(name),
                    Some(_) => {}
                    None => unseen.push((why, vec![name])),
                }
                continue;
            }
            Some(Class::Implemented(why)) => {
                if !implemented.iter().any(|(known, _)| *known == name) {
                    implemented.push((name, why));
                }
                continue;
            }
            Some(Class::Other) => &mut others,
            None => &mut missing,
        };
        if !list.contains(&name) {
            list.push(name);
        }
    }
    let mut ways = Vec::new();
    if whole {
        ways.push(
            "this import takes no name, and what it brings in cannot be told: import each vocabulary by name"
                .to_string(),
        );
    }
    if !enums.is_empty() {
        ways.push(format!(
            "{} a TypeScript enum, which no layer holds: {ENUM_INSTEAD}, and import that",
            listed(&enums)
        ));
    }
    for (name, why) in implemented {
        let rule = match why {
            Disqualified::Methods => "has methods".to_string(),
            Disqualified::Trait(of) => format!("implements the trait {of} of the project"),
            Disqualified::Derive(of) => format!("derives the trait {of} of the project"),
            Disqualified::Unseen(_) => continue,
        };
        ways.push(format!(
            "`{name}` is an enum that {rule}, a rule of domain, so it is no vocabulary: move the methods into a \
             function of domain that takes the kind"
        ));
    }
    for (why, names) in unseen {
        ways.push(format!(
            "{} an enum of a crate that holds what hides the rules of its enums ({why}), so no enum of it can be \
             told a vocabulary: write out what hides them (the `impl` blocks in place of a macro, the module in \
             place of `include!`, the trait for the types that need it in place of one for a type parameter)",
            listed(&names)
        ));
    }
    if !others.is_empty() {
        ways.push(format!(
            "{} a record, class, function or type, no vocabulary: write the shape the port shows in \
             protocol.inbound, and convert in application",
            listed(&others)
        ));
    }
    if !missing.is_empty() {
        ways.push(format!(
            "{} not defined in this file (a name it only re-exports is not followed): import it from the file \
             that defines it",
            listed(&missing)
        ));
    }
    if ways.is_empty() {
        return None;
    }
    Some(Forbidden {
        file: file.to_string(),
        import: import.to_string(),
        detail: format!(
            "{file}:{line}: {what}, in domain; {PROTOCOL_INBOUND} may import only vocabularies, by name, from the file \
             of domain that defines them: {}",
            ways.join("; ")
        ),
        rule: Rule::Direction,
    })
}

/// The names written as code, joined, with the verb that agrees with them: "`A` is" and "`A`, `B` are each".
fn listed(names: &[&String]) -> String {
    let quoted: Vec<String> = names.iter().map(|name| format!("`{name}`")).collect();
    let verb = if names.len() == 1 { "is" } else { "are each" };
    format!("{} {verb}", quoted.join(", "))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::layers::{layout, table};
    use crate::rust::{code, outline};

    fn definition(name: &str, kind: Kind, shape: Shape) -> Definition {
        Definition {
            names: vec![name.to_string()],
            start: 0,
            end: 1,
            kind,
            shape,
        }
    }

    fn strings(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| name.to_string()).collect()
    }

    /// What a Python or TypeScript file defines: a vocabulary `Dir`, a TypeScript enum `Mode`, a record `Song`, and a
    /// function `play`.
    fn landing() -> Defined {
        Defined::new(&[
            definition("Dir", Kind::Other, Shape::Vocabulary),
            definition("Mode", Kind::Type, Shape::Enum),
            definition("Song", Kind::Type, Shape::Plain),
            definition("play", Kind::Function, Shape::Plain),
        ])
    }

    fn detail(taken: &[&str], defined: &Defined) -> Option<String> {
        judged(
            "billing/protocol/inbound/x.ts",
            3,
            "../../domain/x",
            "imports ../../domain/x",
            &strings(taken),
            defined,
        )
        .map(|forbidden| forbidden.detail)
    }

    #[test]
    fn only_the_inbound_level_of_a_unit_importing_its_own_domain_is_judged_here() {
        let places = layout("python", "atomic").unwrap().places(&table());
        let place = |name: &str| places.iter().find(|p| p.name == name).unwrap();
        assert!(applies(place("protocol.inbound"), place("domain")));
        // Not another place importing the domain, nor the outbound level, whose imports the table decides
        assert!(!applies(place("protocol.outbound"), place("domain")));
        assert!(!applies(place("protocol.inbound"), place("application")));
        assert!(!applies(place("infrastructure.inbound"), place("domain")));
        // Another unit's domain is read by the table and the rule of shared units
        let mut other = place("domain").clone();
        other.unit.name = "core".to_string();
        assert!(!applies(place("protocol.inbound"), &other));
    }

    #[test]
    fn an_import_of_vocabularies_alone_passes_by_name() {
        let defined = Defined::new(&[
            definition("Dir", Kind::Other, Shape::Vocabulary),
            definition("Level", Kind::Type, Shape::Vocabulary),
        ]);
        assert_eq!(detail(&["Dir"], &defined), None);
        assert_eq!(detail(&["Dir", "Level"], &defined), None);
        assert_eq!(defined.vocabularies(), ["Dir", "Level"]);
    }

    #[test]
    fn a_const_and_a_type_of_one_name_are_one_vocabulary_unless_one_of_them_is_not() {
        let both = Defined::new(&[
            definition("Dir", Kind::Other, Shape::Vocabulary),
            definition("Dir", Kind::Type, Shape::Vocabulary),
        ]);
        assert_eq!(detail(&["Dir"], &both), None);
        // An interface of the same name beside the const is a record too
        let merged = Defined::new(&[
            definition("Dir", Kind::Other, Shape::Vocabulary),
            definition("Dir", Kind::Type, Shape::Plain),
        ]);
        assert!(
            detail(&["Dir"], &merged)
                .unwrap()
                .contains("`Dir` is a record")
        );
        let reversed = Defined::new(&[
            definition("Dir", Kind::Type, Shape::Plain),
            definition("Dir", Kind::Other, Shape::Vocabulary),
        ]);
        assert!(
            detail(&["Dir"], &reversed)
                .unwrap()
                .contains("`Dir` is a record")
        );
    }

    #[test]
    fn a_typescript_enum_is_told_to_become_an_array_as_const() {
        let found = detail(&["Mode"], &landing()).unwrap();
        assert_eq!(
            found,
            "billing/protocol/inbound/x.ts:3: imports ../../domain/x, in domain; protocol.inbound may import only \
             vocabularies, by name, from the file of domain that defines them: `Mode` is a TypeScript enum, which \
             no layer holds: write it as an array `as const`, with a type of its elements, and import that"
        );
    }

    #[test]
    fn a_record_a_class_a_function_or_a_type_is_told_to_have_its_shape_written_in_the_port() {
        let way = "a record, class, function or type, no vocabulary: write the shape the port shows in \
                   protocol.inbound, and convert in application";
        let one = detail(&["Song"], &landing()).unwrap();
        assert!(one.ends_with(&format!("`Song` is {way}")), "{one}");
        let two = detail(&["Song", "play"], &landing()).unwrap();
        assert!(
            two.ends_with(&format!("`Song`, `play` are each {way}")),
            "{two}"
        );
    }

    #[test]
    fn a_name_the_file_does_not_define_is_told_to_come_from_the_file_that_does() {
        let found = detail(&["Dirr"], &landing()).unwrap();
        assert!(
            found.ends_with(
                "`Dirr` is not defined in this file (a name it only re-exports is not followed): import it from \
                 the file that defines it"
            ),
            "{found}"
        );
    }

    #[test]
    fn an_import_that_takes_no_name_is_told_to_import_the_vocabulary_by_name() {
        let by_name = "this import takes no name, and what it brings in cannot be told: import each vocabulary by \
                       name";
        // A side effect, `import domain.x` and a glob give none; `*` and `default` stand for the whole module and
        // the default export
        let forms: [&[&str]; 3] = [&[], &["*"], &["default"]];
        for taken in forms {
            let found = detail(taken, &landing()).unwrap();
            assert!(found.ends_with(by_name), "{taken:?}: {found}");
        }
        // The default beside a vocabulary taken by name fails for the default alone
        let found = detail(&["default", "Dir"], &landing()).unwrap();
        assert!(
            found.ends_with(by_name) && !found.contains("Dir"),
            "{found}"
        );
    }

    #[test]
    fn every_name_that_is_no_vocabulary_is_said_once_and_the_vocabularies_beside_them_are_not() {
        let found = detail(&["Dir", "Mode", "Song", "Song", "Dirr", "*"], &landing()).unwrap();
        assert_eq!(found.matches("`Song`").count(), 1, "{found}");
        assert!(!found.contains("`Dir`"), "{found}");
        for part in ["no name", "`Mode`", "`Song`", "`Dirr`"] {
            assert!(found.contains(part), "{part}: {found}");
        }
    }

    #[test]
    fn the_finding_is_a_direction_finding_keyed_by_the_file_and_the_import() {
        let forbidden = judged(
            "a/b.py",
            7,
            "domain.song.Dir",
            "imports domain.song.Dir",
            &strings(&["Dir"]),
            &Defined::new(&[]),
        )
        .unwrap();
        assert_eq!(forbidden.file, "a/b.py");
        assert_eq!(forbidden.import, "domain.song.Dir");
        assert_eq!(forbidden.rule, Rule::Direction);
        assert!(
            forbidden
                .detail
                .starts_with("a/b.py:7: imports domain.song.Dir, in domain; "),
            "{}",
            forbidden.detail
        );
    }

    #[test]
    fn no_way_says_to_copy_the_definition() {
        let defined = in_crate(
            "trait Own {}\nenum Kind { A }\nimpl Kind { fn f(&self) {} }\nenum Dir { A }\nimpl Own for Dir {}\n\
             struct Song;\n",
            &[],
        );
        let found = detail(&["Kind", "Dir", "Song", "Nowhere", "*"], &defined).unwrap();
        for word in ["copy", "twice", "duplicate", "again", "same"] {
            assert!(!found.contains(word), "{word}: {found}");
        }
    }

    /// The crate of these files, each a module (`""` for the root, `"song"`, `"song::kind"`) and its source.
    pub(crate) fn krate(files: &[(&str, &str)]) -> Crate {
        let programs: Vec<(String, Vec<String>, String, Vec<Definition>)> = files
            .iter()
            .map(|(module, source)| {
                let path: Vec<String> = module
                    .split("::")
                    .filter(|part| !part.is_empty())
                    .map(String::from)
                    .collect();
                let file = match module.is_empty() {
                    true => "crates/domain/src/lib.rs".to_string(),
                    false => format!("crates/domain/src/{}.rs", module.replace("::", "/")),
                };
                (file, path, code(source), outline(source))
            })
            .collect();
        let sources: Vec<Source> = programs
            .iter()
            .map(|(path, module, code, outline)| Source {
                path,
                module,
                code,
                outline,
            })
            .collect();
        Crate::new(&sources)
    }

    /// The `Defined` of the root module of a crate whose root is `source`, and `others` its other modules.
    fn in_crate(source: &str, others: &[(&str, &str)]) -> Defined {
        let files: Vec<(&str, &str)> = std::iter::once(("", source))
            .chain(others.iter().copied())
            .collect();
        krate(&files).module(&[]).unwrap().clone()
    }

    #[test]
    fn a_rust_enum_of_plain_variants_with_derives_and_std_impls_is_a_vocabulary() {
        let defined = in_crate(
            "#[derive(Debug, Clone, Copy)]\npub enum Dir { Up = 1, Down }\n\
             impl std::fmt::Display for Dir { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { Ok(()) } }\n\
             impl From<Dir> for u8 { fn from(d: Dir) -> u8 { 0 } }\n",
            &[],
        );
        assert_eq!(detail(&["Dir"], &defined), None);
    }

    #[test]
    fn a_rust_enum_with_an_inherent_method_anywhere_in_the_crate_is_no_vocabulary() {
        let here = in_crate(
            "pub enum Dir { Up }\nimpl Dir { pub fn flip(self) -> Dir { self } }\n",
            &[],
        );
        let there = in_crate(
            "pub enum Dir { Up }\n",
            &[(
                "rank",
                "impl crate::Dir { pub const fn flip(self) -> u8 { 0 } }\n",
            )],
        );
        for defined in [here, there] {
            let found = detail(&["Dir"], &defined).unwrap();
            assert!(
                found.ends_with(
                    "`Dir` is an enum that has methods, a rule of domain, so it is no vocabulary: move the methods \
                     into a function of domain that takes the kind"
                ),
                "{found}"
            );
        }
    }

    #[test]
    fn an_associated_function_with_no_self_is_a_method_too() {
        let defined = in_crate(
            "enum Dir { Up }\nimpl Dir { fn parse(s: &str) -> Option<Dir> { None } }\n",
            &[],
        );
        assert!(detail(&["Dir"], &defined).is_some());
    }

    #[test]
    fn an_inherent_impl_that_holds_constants_alone_leaves_the_enum_a_vocabulary() {
        let defined = in_crate(
            "enum Dir { Up, Down }\nimpl Dir { pub const ALL: [Dir; 2] = [Dir::Up, Dir::Down]; }\nimpl Dir {}\n",
            &[],
        );
        assert_eq!(detail(&["Dir"], &defined), None);
    }

    #[test]
    fn a_trait_of_the_project_that_brings_constants_or_types_alone_leaves_the_enum_a_vocabulary() {
        let defined = in_crate(
            "pub trait Rank { const R: u8; type T; }\npub trait Marker {}\nenum Dir { Up }\n\
             impl Rank for Dir { const R: u8 = 1; type T = u8; }\nimpl Marker for Dir {}\n",
            &[],
        );
        assert_eq!(detail(&["Dir"], &defined), None);
    }

    #[test]
    fn a_trait_of_the_project_with_a_method_with_a_body_disqualifies_an_empty_impl_of_it() {
        // The type has the method of the trait, though its `impl` block writes nothing
        let defined = in_crate(
            "use crate::rank::Rank;\nenum Dir { Up }\nimpl Rank for Dir {}\n",
            &[(
                "rank",
                "pub trait Rank { const R: u8 = 0; fn rank(&self) -> u8 { 0 } }\n",
            )],
        );
        let found = detail(&["Dir"], &defined).unwrap();
        assert!(
            found.contains("implements the trait Rank of the project"),
            "{found}"
        );
        // A method without a body is written by the `impl` block, which holds a function then
        let written = in_crate(
            "pub trait Rank { fn rank(&self) -> u8; }\nenum Dir { Up }\n\
             impl Rank for Dir { fn rank(&self) -> u8 { 0 } }\n",
            &[],
        );
        assert!(detail(&["Dir"], &written).is_some());
    }

    #[test]
    fn a_function_of_another_type_in_an_impl_beside_it_is_not_a_method_of_the_enum() {
        let defined = in_crate(
            "enum Dir { Up }\nstruct Song;\nimpl Song { fn f(&self) {} }\nimpl Dir {}\n",
            &[],
        );
        assert_eq!(detail(&["Dir"], &defined), None);
        assert!(detail(&["Song"], &defined).is_some());
    }

    #[test]
    fn a_rust_enum_that_implements_a_trait_of_the_project_is_no_vocabulary_wherever_the_trait_is() {
        let words = "`Dir` is an enum that implements the trait Rank of the project, a rule of domain, so it is no \
                     vocabulary: move the methods into a function of domain that takes the kind";
        let in_file = in_crate(
            "trait Rank { fn rank(&self) -> u8 { 0 } }\nenum Dir { Up }\nimpl Rank for Dir {}\n",
            &[],
        );
        // The trait in another file of the crate, written by a path
        let elsewhere = in_crate(
            "enum Dir { Up }\nimpl crate::rank::Rank for Dir { fn rank(&self) -> u8 { 0 } }\n",
            &[("rank", "pub trait Rank { fn rank(&self) -> u8; }\n")],
        );
        for defined in [in_file, elsewhere] {
            let found = detail(&["Dir"], &defined).unwrap();
            assert!(found.ends_with(words), "{found}");
        }
    }

    #[test]
    fn a_trait_of_the_std_is_no_trait_of_the_project_whatever_name_the_project_gives_its_own() {
        // The path says which `Display` is meant: the standard library's, written out or imported
        let written = in_crate(
            "enum Dir { Up }\nimpl std::fmt::Display for Dir { fn fmt(&self) {} }\n",
            &[("show", "pub trait Display { fn show(&self) {} }\n")],
        );
        let imported = in_crate(
            "use std::fmt::Display;\nenum Dir { Up }\nimpl Display for Dir { fn fmt(&self) {} }\n",
            &[("show", "pub trait Display { fn show(&self) {} }\n")],
        );
        let prelude = in_crate(
            "enum Dir { Up }\nimpl Default for Dir { fn default() -> Dir { Dir::Up } }\n",
            &[("show", "pub trait Default { fn show(&self) {} }\n")],
        );
        for defined in [written, imported, prelude] {
            assert_eq!(detail(&["Dir"], &defined), None);
        }
        // The project's own `Display`, by the path that names it, is the project's
        let own = in_crate(
            "enum Dir { Up }\nimpl show::Display for Dir {}\n",
            &[("show", "pub trait Display { fn show(&self) {} }\n")],
        );
        assert!(detail(&["Dir"], &own).is_some());
    }

    #[test]
    fn a_name_a_glob_may_bring_in_is_matched_by_its_last_name_and_falls_to_failure() {
        let globbed = krate(&[
            ("", "pub mod a;\npub mod b;\n"),
            ("a", "pub trait Display { fn show(&self) {} }\n"),
            (
                "b",
                "use super::a::*;\npub enum Dir { Up }\nimpl Display for Dir {}\n",
            ),
        ]);
        let found = detail(&["Dir"], globbed.module(&strings(&["b"])).unwrap()).unwrap();
        assert!(
            found.contains("implements the trait Display of the project"),
            "{found}"
        );
        // A name that no glob could bring is the prelude's
        let alone = krate(&[
            ("", "pub mod a;\npub mod b;\n"),
            ("a", "pub trait Display { fn show(&self) {} }\n"),
            ("b", "pub enum Dir { Up }\nimpl Display for Dir {}\n"),
        ]);
        let defined = alone.module(&strings(&["b"])).unwrap();
        assert_eq!(detail(&["Dir"], defined), None);
    }

    #[test]
    fn a_trait_implemented_with_arguments_for_a_reference_is_found_by_its_path() {
        let defined = in_crate(
            "enum Dir { Up }\nimpl<'a> serde::Serialize for &'a Dir {}\nimpl Tr<u8> for Dir {}\n\
             pub trait Tr<T> { fn get(&self) -> u8 { 0 } }\n",
            &[],
        );
        let found = detail(&["Dir"], &defined).unwrap();
        assert!(
            found.contains("implements the trait Tr of the project"),
            "{found}"
        );
    }

    #[test]
    fn a_rust_enum_with_a_field_in_a_variant_or_generics_is_no_vocabulary_and_a_struct_never_is() {
        let defined = in_crate(
            "enum A { X(u8) }\nenum B<T> { X }\nstruct C;\ntrait D {}\nfn e() {}\nconst F: u8 = 1;\nmod g {}\n",
            &[],
        );
        for name in ["A", "B", "C", "D", "e", "F", "g"] {
            let found = detail(&[name], &defined).unwrap();
            assert!(
                found.contains(&format!("`{name}` is a record")),
                "{name}: {found}"
            );
        }
    }

    #[test]
    fn a_variant_or_an_item_is_no_name_of_the_file() {
        let defined = in_crate(
            "enum Dir { Up }\nstruct S;\nimpl S { fn f(&self) {} }\n",
            &[],
        );
        for name in ["Up", "f"] {
            let found = detail(&[name], &defined).unwrap();
            assert!(
                found.contains(&format!("`{name}` is not defined in this file")),
                "{found}"
            );
        }
    }

    #[test]
    fn an_inline_module_is_a_module_of_its_own_whose_names_are_not_those_of_the_file() {
        let source = "pub enum Top { A }\npub mod inner { pub enum Deep { A } impl Deep { fn f(&self) {} } }\n";
        let krate = krate(&[("", source)]);
        let file = krate.module(&[]).unwrap();
        let inner = krate.module(&strings(&["inner"])).unwrap();
        assert_eq!(file.vocabularies(), ["Top"]);
        assert_eq!(
            detail(&["Deep"], file).map(|d| d.contains("not defined")),
            Some(true)
        );
        assert!(detail(&["Deep"], inner).unwrap().contains("has methods"));
        assert_eq!(inner.vocabularies(), Vec::<&str>::new());
    }

    #[test]
    fn an_impl_in_an_inline_module_counts_against_the_enum_of_the_same_name() {
        let defined = in_crate(
            "pub enum Dir { Up }\nmod inner { impl super::Dir { fn f(&self) {} } }\n",
            &[],
        );
        assert!(detail(&["Dir"], &defined).unwrap().contains("has methods"));
    }

    /// The words the finding of a crate holds for `name`, in the root module of a crate of `files` (the first is it).
    fn said(files: &[(&str, &str)], name: &str) -> Option<String> {
        detail(&[name], krate(files).module(&[]).unwrap())
    }

    #[test]
    fn two_enums_of_one_name_in_two_modules_are_told_apart() {
        let krate = krate(&[
            ("", "pub mod song;\npub mod user;\n"),
            ("song", "pub enum Kind { A }\n"),
            (
                "user",
                "pub enum Kind { B }\nimpl Kind { pub fn f(&self) {} }\n",
            ),
        ]);
        let song = krate.module(&strings(&["song"])).unwrap();
        let user = krate.module(&strings(&["user"])).unwrap();
        // The methods are the user's, and the enum without them is a vocabulary
        assert_eq!(detail(&["Kind"], song), None);
        assert!(detail(&["Kind"], user).unwrap().contains("has methods"));
        // A block that names the other module's enum, from a third, takes out that one alone
        let third = super::tests::krate(&[
            ("", "pub mod song;\npub mod user;\npub mod rest;\n"),
            ("song", "pub enum Kind { A }\n"),
            ("user", "pub enum Kind { B }\n"),
            ("rest", "impl crate::user::Kind { fn f(&self) {} }\n"),
        ]);
        assert_eq!(
            detail(&["Kind"], third.module(&strings(&["song"])).unwrap()),
            None
        );
        let found = detail(&["Kind"], third.module(&strings(&["user"])).unwrap());
        assert!(found.unwrap().contains("has methods"));
    }

    #[test]
    fn an_impl_through_an_alias_a_rename_or_a_reference_is_the_enums_and_through_a_box_is_not() {
        let rank = ("rank", "pub trait Rank { fn rank(&self) -> u8 { 0 } }\n");
        let song = ("song", "pub enum Kind { A }\n");
        // Whatever the name the type is written with, the enum of `song` is the one out
        let has_methods = |source: &str| {
            let krate = krate(&[("", source), song, rank]);
            detail(&["Kind"], krate.module(&strings(&["song"])).unwrap())
        };
        for source in [
            "pub mod song;\npub mod rank;\ntype K = song::Kind;\nimpl K { fn f() {} }\n",
            "pub mod song;\npub mod rank;\nuse crate::song::Kind as K;\nimpl K { fn f() {} }\n",
            "pub mod song;\npub mod rank;\nuse song::Kind as K;\nimpl K { const A: u8 = 1; fn f() {} }\n",
            "pub mod song;\npub mod rank;\ntype K = &'static song::Kind;\nimpl K { fn f() {} }\n",
            "pub mod song;\npub mod rank;\ntype J = song::Kind;\ntype K = J;\nimpl K { fn f() {} }\n",
            "pub mod song;\npub mod rank;\nimpl &song::Kind { fn f() {} }\n",
        ] {
            let found = has_methods(source).unwrap_or_else(|| panic!("passes: {source}"));
            assert!(found.contains("has methods"), "{source}: {found}");
        }
        // A trait of the project, renamed: the finding names it as the project does
        for source in [
            "pub mod song;\npub mod rank;\nuse rank::Rank as R;\nimpl R for song::Kind {}\n",
            "pub mod song;\npub mod rank;\ntype K = song::Kind;\nuse rank::Rank as R;\nimpl R for K {}\n",
            "pub mod song;\npub mod rank;\nimpl rank::Rank for &song::Kind {}\n",
        ] {
            let found = has_methods(source).unwrap_or_else(|| panic!("passes: {source}"));
            assert!(
                found.contains("implements the trait Rank of the project"),
                "{source}: {found}"
            );
        }
        // A box of the enum is a type of its own, which holds the methods, and so is a tuple of it
        for source in [
            "pub mod song;\npub mod rank;\nimpl rank::Rank for Box<song::Kind> {}\n",
            "pub mod song;\npub mod rank;\ntype K = Box<song::Kind>;\nimpl K { fn f() {} }\n",
            "pub mod song;\npub mod rank;\nimpl rank::Rank for (song::Kind, u8) { fn rank(&self) -> u8 { 1 } }\n",
            "pub mod song;\npub mod rank;\nimpl rank::Rank for Vec<song::Kind> { fn rank(&self) -> u8 { 1 } }\n",
        ] {
            assert_eq!(has_methods(source), None, "{source}");
        }
    }

    #[test]
    fn an_impl_in_a_body_a_block_a_closure_a_const_or_a_static_is_the_enums() {
        let rank = "pub trait Rank { fn rank(&self) -> u8 { 0 } }\n";
        for source in [
            "pub enum Kind { A }\nfn f() { impl Kind { fn g(&self) {} } }\n",
            "pub enum Kind { A }\nconst _: () = { impl Kind { fn g(&self) {} } };\n",
            "pub enum Kind { A }\nstatic S: u8 = { impl Kind { fn g(&self) {} } 1 };\n",
            "pub enum Kind { A }\nmod m { fn f() { impl crate::Kind { fn g(&self) {} } } }\n",
            "pub enum Kind { A }\nstruct S;\nimpl S { fn f() { impl Kind { fn g(&self) {} } } }\n",
            "pub enum Kind { A }\nfn f(c: bool) { if c { impl Kind { fn g(&self) {} } } }\n",
            "pub enum Kind { A }\nfn f() { let c = |x: u8| { impl Kind { fn g(&self) {} } x }; }\n",
            "pub enum Kind { A }\nfn f() { v.iter().map(|x| { impl Kind { fn g(&self) {} } x }); }\n",
            "pub enum Kind { A }\nfn f() { unsafe { loop { match x { _ => { impl Kind { fn g(&self) {} } } } } } }\n",
            "pub enum Kind { A }\nfn f() { fn inner() { impl Kind { fn g(&self) {} } } }\n",
            // The `const` of `*const T` and the name of a lifetime start no item that would pass the block over
            "pub enum Kind { A }\nfn f(x: &u8) { g(x as *const u8, |y| { impl Kind { fn g(&self) {} } y }); }\n",
            "pub enum Kind { A }\nfn f() { let c: &'static dyn Fn() = &|| { impl Kind { fn g(&self) {} } }; }\n",
            // An `impl` after a declaration of a type parameter or a signature in the same body
            "pub enum Kind { A }\nfn f() { fn r() -> impl Copy { 1 } impl Kind { fn g(&self) {} } }\n",
        ] {
            let found = said(&[("", source)], "Kind").unwrap_or_else(|| panic!("passes: {source}"));
            assert!(found.contains("has methods"), "{source}: {found}");
        }
        for source in [
            "pub enum Kind { A }\nfn f() { impl Rank for Kind {} }\n",
            "pub enum Kind { A }\nconst _: () = { impl Rank for Kind {} };\n",
            "pub enum Kind { A }\nstatic S: u8 = { unsafe impl Rank for Kind {} 1 };\n",
        ] {
            let found = said(&[("", source), ("rank", rank)], "Kind");
            assert!(
                found.is_none(),
                "an unknown name is the prelude's: {source}"
            );
            let with = format!("use crate::rank::Rank;\n{source}");
            let found = said(&[("", with.as_str()), ("rank", rank)], "Kind")
                .unwrap_or_else(|| panic!("passes: {with}"));
            assert!(
                found.contains("implements the trait Rank of the project"),
                "{with}: {found}"
            );
        }
        // A trait declared in the body is the project's too, where it is the one the block implements
        let local = "pub enum Kind { A }\nfn f() { trait Rank { fn rank(&self) -> u8 { 0 } } impl Rank for Kind {} }\n";
        let found = said(&[("", local)], "Kind").unwrap();
        assert!(
            found.contains("implements the trait Rank of the project"),
            "{found}"
        );
        // An `impl Trait` type is no block, in a signature, in a body, or in a tuple
        let defined = in_crate(
            "pub enum Kind { A }\nfn f(x: impl Fn()) -> impl Fn() { x }\nfn g(k: &impl Copy) {}\n\
             fn h() -> (impl Copy, u8) { todo!() }\n\
             fn i() { fn j() -> impl Iterator<Item = u8> { 0..1 } fn k() -> (impl Copy, u8) { todo!() } \
             let c = |x: u8| x; unsafe { } const { 1 }; let p = &c as *const _; let s: &'static str = \"\"; }\n",
            &[],
        );
        assert_eq!(detail(&["Kind"], &defined), None);
    }

    #[test]
    fn a_macro_call_with_impl_in_its_arguments_fails_every_enum_of_the_crate_whatever_its_delimiter()
     {
        for call in [
            "m!(impl Rank for Kind { fn f() {} });",
            "m![impl Rank for Kind { fn f() {} }];",
            "m! { impl Rank for Kind { fn f() {} } }",
            "fn f() { m!(impl Kind { fn g(&self) {} }); }",
            "fn f() { outer!(inner!(x, impl Kind {})); }",
        ] {
            let source = format!("pub mod song;\n{call}\n");
            let krate = krate(&[("", source.as_str()), ("song", "pub enum Kind { A }\n")]);
            let found = detail(&["Kind"], krate.module(&strings(&["song"])).unwrap())
                .unwrap_or_else(|| panic!("passes: {call}"));
            assert!(
                found.contains("has `impl` in its arguments, which cannot be read as code, at crates/domain/src/lib.rs:2)"),
                "{call}: {found}"
            );
        }
        // The call is named, as the finding names the macro
        let found = said(&[("", "pub enum Kind { A }\nm!(impl Kind {});\n")], "Kind").unwrap();
        assert!(found.contains("(the call `m!` has `impl`"), "{found}");
        // No impl in the arguments, a `!=`, a string and a comment hide nothing
        let calm = in_crate(
            "pub enum Kind { A }\nfn f(a: u8) { m!(a, 1); if a != 1 { n![1]; } p!(\"impl\"); q!{ /* impl */ } }\n\
             macro_rules! r { () => { 1 }; }\n",
            &[],
        );
        assert_eq!(detail(&["Kind"], &calm), None);
    }

    #[test]
    fn a_macro_that_writes_impl_blocks_fails_every_enum_of_the_crate_and_says_where() {
        let files = [
            (
                "",
                "pub mod song;\n\nmacro_rules! impls {\n    ($t:ty) => { impl $t { fn f(&self) {} } };\n}\n",
            ),
            ("song", "pub enum Kind { A }\n"),
        ];
        let krate = krate(&files);
        let found = detail(&["Kind"], krate.module(&strings(&["song"])).unwrap()).unwrap();
        assert!(
            found.ends_with(
                "`Kind` is an enum of a crate that holds what hides the rules of its enums (the macro `impls` holds \
                 `impl` blocks, which are not followed, at crates/domain/src/lib.rs:3), so no enum of it can be told \
                 a vocabulary: write out what hides them (the `impl` blocks in place of a macro, the module in place \
                 of `include!`, the trait for the types that need it in place of one for a type parameter)"
            ),
            "{found}"
        );
        // A macro with no `impl` in it hides nothing
        let plain = krate_of("macro_rules! a { () => { fn f() {} }; }\npub enum Kind { A }\n");
        assert_eq!(detail(&["Kind"], &plain), None);
    }

    /// The root module of a one-file crate.
    fn krate_of(source: &str) -> Defined {
        in_crate(source, &[])
    }

    #[test]
    fn an_include_fails_every_enum_of_the_crate_and_says_where() {
        let found = said(
            &[(
                "",
                "pub enum Kind { A }\n\ninclude!(concat!(env!(\"OUT_DIR\"), \"/x.rs\"));\n",
            )],
            "Kind",
        )
        .unwrap();
        assert!(
            found.contains(
                "(`include!` takes in code that is not followed, at crates/domain/src/lib.rs:3)"
            ),
            "{found}"
        );
        // `include_str!` takes in no code
        let text = krate_of("pub enum Kind { A }\nconst T: &str = include_str!(\"x.txt\");\n");
        assert_eq!(detail(&["Kind"], &text), None);
    }

    #[test]
    fn an_impl_of_a_trait_of_the_project_for_a_type_parameter_fails_every_enum_of_the_crate() {
        let rank = ("rank", "pub trait Rank { fn rank(&self) -> u8; }\n");
        let song = ("song", "pub enum Kind { A }\n");
        let blanket = "pub mod rank;\npub mod song;\n\nimpl<T: Copy> rank::Rank for T { fn rank(&self) -> u8 { 0 } }\n";
        let krate = krate(&[("", blanket), rank, song]);
        let found = detail(&["Kind"], krate.module(&strings(&["song"])).unwrap()).unwrap();
        assert!(
            found.contains(
                "(the trait Rank of the project is implemented for a type parameter, which stands for any type, at \
                 crates/domain/src/lib.rs:4)"
            ),
            "{found}"
        );
        // A trait with a method with a body, and an empty block: the type parameter has the method
        let bodied = krate_of_two(
            "pub mod rank;\npub mod song;\nimpl<T> rank::Rank for T {}\n",
            ("rank", "pub trait Rank { fn rank(&self) -> u8 { 0 } }\n"),
            song,
        );
        assert!(bodied.is_some());
        // No method anywhere is data alone, which leaves the enum a vocabulary
        let marker = krate_of_two(
            "pub mod rank;\npub mod song;\nimpl<T> rank::Rank for T {}\n",
            ("rank", "pub trait Rank { const R: u8 = 0; }\n"),
            song,
        );
        assert_eq!(marker, None);
        // A type parameter of a trait of the standard library is no matter of the crate's
        let std = krate_of_two(
            "pub mod rank;\npub mod song;\nimpl<T: Copy> From<T> for Wrapper<T> { fn from(t: T) -> Self { todo!() } }\n\
             impl<T: Copy> std::fmt::Display for T { fn fmt(&self) {} }\n",
            rank,
            song,
        );
        assert_eq!(std, None);
        // A parameter that is not the type the block is for (`Wrapper<T>`, `Vec<T>`) makes no blanket
        let wrapped = krate_of_two(
            "pub mod rank;\npub mod song;\nimpl<T> rank::Rank for Wrapper<T> { fn rank(&self) -> u8 { 0 } }\n",
            rank,
            song,
        );
        assert_eq!(wrapped, None);
    }

    /// What is said of `Kind` in the module `song` of the crate whose root is `root`, with two more modules.
    fn krate_of_two(root: &str, a: (&str, &str), b: (&str, &str)) -> Option<String> {
        let krate = krate(&[("", root), a, b]);
        detail(&["Kind"], krate.module(&strings(&["song"])).unwrap())
    }

    #[test]
    fn a_derive_of_a_trait_of_the_project_takes_the_enum_out_and_one_of_the_std_does_not() {
        let rank = ("rank", "pub trait Rank { const R: u8; }\n");
        let said_of = |source: &str| {
            let krate = krate(&[("", source), rank]);
            detail(&["Kind"], krate.module(&[]).unwrap())
        };
        for source in [
            "use crate::rank::Rank;\n#[derive(Rank)]\npub enum Kind { A }\n",
            "#[derive(Debug, crate::rank::Rank, Clone)]\npub enum Kind { A }\n",
            "use crate::rank::Rank;\n#[derive(Debug)]\n#[derive(Rank)]\npub enum Kind { A }\n",
            "use crate::rank::Rank as R;\n#[cfg_attr(feature = \"x\", derive(R))]\npub(crate) enum Kind { A }\n",
        ] {
            let found = said_of(source).unwrap_or_else(|| panic!("passes: {source}"));
            assert!(
                found.contains("`Kind` is an enum that derives the trait Rank of the project"),
                "{source}: {found}"
            );
        }
        for source in [
            "#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize)]\npub enum Kind { A }\n",
            "use serde::Serialize;\n#[derive(Serialize)]\npub enum Kind { A }\n",
            "#[derive(Rank)]\npub enum Kind { A }\n",
        ] {
            assert_eq!(said_of(source), None, "{source}");
        }
        // The derive of another enum takes out that one
        let other = said_of(
            "use crate::rank::Rank;\n#[derive(Rank)]\nenum Other { A }\npub enum Kind { A }\n",
        );
        assert_eq!(other, None);
    }

    #[test]
    fn a_name_a_glob_brings_in_is_the_enum_of_that_name_wherever_it_is_defined() {
        let krate = krate(&[
            ("", "pub mod a;\npub mod b;\npub mod c;\n"),
            ("a", "pub enum Kind { A }\n"),
            ("b", "pub enum Kind { B }\n"),
            ("c", "use super::a::*;\nimpl Kind { fn f(&self) {} }\n"),
        ]);
        // The glob may bring the `Kind` of `a` or of `b`, which cannot be told: both fail
        for module in ["a", "b"] {
            let defined = krate.module(&strings(&[module])).unwrap();
            let found = detail(&["Kind"], defined).unwrap();
            assert!(found.contains("has methods"), "{module}: {found}");
        }
        // An enum of another name is not taken out
        let apart = super::tests::krate(&[
            ("", "pub mod a;\npub mod c;\n"),
            ("a", "pub enum Kind { A }\npub enum Other { A }\n"),
            ("c", "use super::a::*;\nimpl Kind { fn f(&self) {} }\n"),
        ]);
        assert_eq!(
            detail(&["Other"], apart.module(&strings(&["a"])).unwrap()),
            None
        );
    }

    #[test]
    fn a_path_names_a_module_and_an_item_from_the_root_of_the_crate() {
        let krate = krate(&[
            ("", "pub mod song;\npub enum Top { A }\n"),
            (
                "song",
                "pub mod deep;\npub enum Kind { A }\nstruct Record;\n",
            ),
            ("song::deep", "pub enum Inner { A }\n"),
        ]);
        let judged = |rest: &[&str]| {
            krate
                .judged(
                    "p/inbound/mod.rs",
                    3,
                    "domain::x",
                    "uses domain::x",
                    &strings(rest),
                )
                .map(|forbidden| forbidden.detail)
        };
        // A vocabulary of the module that defines it, a variant after it too
        for rest in [
            &["song", "Kind"][..],
            &["song", "Kind", "A"],
            &["song", "deep", "Inner"],
            &["Top"],
            &["Top", "A"],
        ] {
            assert_eq!(judged(rest), None, "{rest:?}");
        }
        // A module and no item, a glob, `self`, and nothing at all take no name
        for rest in [
            &["song"][..],
            &["song", "deep"],
            &["song", "*"],
            &["song", "self"],
            &["*"],
            &[],
        ] {
            let found = judged(rest).unwrap_or_else(|| panic!("passes: {rest:?}"));
            assert!(found.ends_with("this import takes no name, and what it brings in cannot be told: import each vocabulary by name"), "{rest:?}: {found}");
        }
        // Not a vocabulary, and not defined in the module the path goes down to
        assert!(
            judged(&["song", "Record"])
                .unwrap()
                .contains("`Record` is a record")
        );
        assert!(
            judged(&["song", "Inner"])
                .unwrap()
                .contains("`Inner` is not defined in this file")
        );
        assert!(
            judged(&["song", "Nowhere", "Kind"])
                .unwrap()
                .contains("`Nowhere` is not defined")
        );
        assert!(judged(&["Kind"]).unwrap().contains("`Kind` is not defined"));
        // The file, the line and the import are those the caller gave
        let forbidden = krate
            .judged(
                "p/inbound/mod.rs",
                3,
                "domain::x",
                "uses domain::x",
                &strings(&["Nope"]),
            )
            .unwrap();
        assert_eq!(
            (forbidden.file.as_str(), forbidden.import.as_str()),
            ("p/inbound/mod.rs", "domain::x")
        );
        assert!(
            forbidden
                .detail
                .starts_with("p/inbound/mod.rs:3: uses domain::x, in domain; ")
        );
    }

    #[test]
    fn a_name_a_module_only_re_exports_is_not_defined_there() {
        let krate = krate(&[
            ("", "pub mod song;\npub mod prelude;\n"),
            ("song", "pub enum Kind { A }\n"),
            ("prelude", "pub use crate::song::Kind;\n"),
        ]);
        let found = krate
            .judged("f", 1, "i", "uses i", &strings(&["prelude", "Kind"]))
            .unwrap()
            .detail;
        assert!(found.contains(
            "`Kind` is not defined in this file (a name it only re-exports is not followed)"
        ));
        assert!(
            krate
                .judged("f", 1, "i", "uses i", &strings(&["song", "Kind"]))
                .is_none()
        );
    }

    #[test]
    fn the_names_that_hide_the_rules_are_said_once_for_the_enums_they_take_out() {
        let defined = in_crate(
            "pub enum A { X }\npub enum B { X }\ninclude!(\"x.rs\");\n",
            &[],
        );
        let found = detail(&["A", "B", "A"], &defined).unwrap();
        assert_eq!(found.matches("`A`").count(), 1, "{found}");
        assert!(
            found.contains("`A`, `B` are each an enum of a crate that holds what hides the rules"),
            "{found}"
        );
        for word in ["copy", "twice", "duplicate", "again", "same"] {
            assert!(!found.contains(word), "{word}: {found}");
        }
    }
}
