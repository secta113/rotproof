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
//!   blocks that can be in any file of the crate. So it stops being a vocabulary when an `impl` block anywhere in the
//!   crate (a) is inherent and holds a function, or (b) implements, for that type, a trait the crate defines, and
//!   either the block holds a function or the trait has a method with a body, which the type then has. What an
//!   outline cannot tell is decided against the enum: the type an `impl` block is for and a trait are matched by their
//!   last name, so a trait of the project that shares its name with a trait of the standard library, or two types of
//!   one name in two modules, disqualify, and what falls to failure is never let through. An `impl` of a trait of the
//!   standard library or of another crate, and a derive, leave it a vocabulary, as does an `impl`, inherent or of a
//!   trait of the project, that brings constants or types alone (data, with no code to run).

use std::collections::BTreeMap;

use crate::definitions::{Definition, Kind, Shape};
use crate::direction::{DOMAIN, Forbidden, PROTOCOL_INBOUND, Rule};
use crate::layers::Place;

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
}

/// What a name defined at the top of a file is, for the rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Class {
    /// Every definition of the name is a vocabulary
    Vocabulary,
    /// A TypeScript `enum`
    Enum,
    /// A Rust `enum` of variants without fields that holds a rule in an `impl` block, in this file or another
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
        Self::in_scope(outline, &[], &BTreeMap::new())
    }

    /// What a Rust module defines: the outline of its file, and the names of the inline modules down to it (`scope`,
    /// empty for the file's own top), with the enums that the `impl` blocks of the crate take out
    /// ([`disqualified_by_impls`]).
    pub fn in_scope(
        outline: &[Definition],
        scope: &[String],
        disqualified: &BTreeMap<String, Disqualified>,
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
            let class = match (&definition.shape, disqualified.get(name)) {
                (Shape::Vocabulary, None) => Class::Vocabulary,
                (Shape::Vocabulary, Some(why)) => Class::Implemented(why.clone()),
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

/// The Rust types, by name, that no `impl` block of the crate leaves a vocabulary, from the outline of every file of
/// the crate: one that an inherent `impl` block holds a function of, or for which a trait the crate defines (a `trait`
/// of that name in any outline) is implemented by a block that holds a function, or is a trait with a method with a
/// body, which the type has without the block writing it. A block that brings constants or types alone leaves the
/// type a vocabulary. Which of them are enums that would be vocabularies is the business of [`Defined::in_scope`].
pub fn disqualified_by_impls(files: &[&[Definition]]) -> BTreeMap<String, Disqualified> {
    let mut traits: BTreeMap<&str, bool> = BTreeMap::new();
    for outline in files {
        for definition in outline.iter() {
            if definition.shape == Shape::Trait
                && let Some(name) = definition.names.last()
            {
                // Two traits of one name are one: a method with a body in either counts
                let bodied = traits.entry(name.as_str()).or_default();
                *bodied |= holds_function(outline, definition);
            }
        }
    }
    let mut found = BTreeMap::new();
    for outline in files {
        for block in outline.iter() {
            let (Shape::Implementation(implemented), Some(name)) =
                (&block.shape, block.names.last())
            else {
                continue;
            };
            let why = match implemented {
                None if holds_function(outline, block) => Disqualified::Methods,
                Some(name)
                    if traits
                        .get(name.as_str())
                        .is_some_and(|bodied| *bodied || holds_function(outline, block)) =>
                {
                    Disqualified::Trait(name.clone())
                }
                _ => continue,
            };
            found.entry(name.clone()).or_insert(why);
        }
    }
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
            "{} a TypeScript enum, which no layer holds: write it as an array `as const`, with a type of its \
             elements, and import that",
            listed(&enums)
        ));
    }
    for (name, why) in implemented {
        let rule = match why {
            Disqualified::Methods => "has methods".to_string(),
            Disqualified::Trait(of) => format!("implements the trait {of} of the project"),
        };
        ways.push(format!(
            "`{name}` is an enum that {rule}, a rule of domain, so it is no vocabulary: move the methods into a \
             function of domain that takes the kind"
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
mod tests {
    use super::*;
    use crate::layers::{layout, table};
    use crate::rust::outline;

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

    /// The `Defined` of the module `source` for a crate of these files.
    fn in_crate(source: &str, others: &[&str]) -> Defined {
        let files: Vec<Vec<Definition>> = std::iter::once(source)
            .chain(others.iter().copied())
            .map(outline)
            .collect();
        let refs: Vec<&[Definition]> = files.iter().map(Vec::as_slice).collect();
        Defined::in_scope(&files[0], &[], &disqualified_by_impls(&refs))
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
            &["impl crate::Dir { pub const fn flip(self) -> u8 { 0 } }\n"],
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
            "enum Dir { Up }\nimpl Rank for Dir {}\n",
            &["pub trait Rank { const R: u8 = 0; fn rank(&self) -> u8 { 0 } }\n"],
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
            &["pub trait Rank { fn rank(&self) -> u8; }\n"],
        );
        for defined in [in_file, elsewhere] {
            let found = detail(&["Dir"], &defined).unwrap();
            assert!(found.ends_with(words), "{found}");
        }
    }

    #[test]
    fn a_trait_of_the_project_with_the_name_of_a_std_trait_disqualifies_the_enum() {
        // Which `Display` is meant cannot be told from the name, so it falls to failure
        let defined = in_crate(
            "enum Dir { Up }\nimpl std::fmt::Display for Dir { fn fmt(&self) {} }\n",
            &["pub trait Display { fn show(&self); }\n"],
        );
        assert!(detail(&["Dir"], &defined).is_some());
    }

    #[test]
    fn a_trait_implemented_with_arguments_for_a_reference_is_found_by_its_last_name() {
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
        let files = [outline(source)];
        let disqualified = disqualified_by_impls(&[&files[0]]);
        let file = Defined::in_scope(&files[0], &[], &disqualified);
        let inner = Defined::in_scope(&files[0], &strings(&["inner"]), &disqualified);
        assert_eq!(file.vocabularies(), ["Top"]);
        assert_eq!(
            detail(&["Deep"], &file).map(|d| d.contains("not defined")),
            Some(true)
        );
        assert!(detail(&["Deep"], &inner).unwrap().contains("has methods"));
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
}
