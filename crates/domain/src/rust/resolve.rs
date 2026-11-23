//! What a path written in a Rust crate names: the modules of the crate, the names each defines and binds (`use`, a
//! `type` alias), and the item, module or crate a path stands for where it is written.
//!
//! - **A path is read the way the compiler follows it,** in the module it is written in: `crate`, `self` and `super`
//!   start from the root, the module and its parent; a name that is a module declared in the module (a file `x.rs`,
//!   `x/mod.rs`, or an inline `mod x`) goes down into it; a name the module defines is that item; a name it binds with
//!   `use` (`as` followed, groups spread) or with `type` is what it is bound to, read in the same module, as far as
//!   [`DEPTH`] links.
//! - **A name that stands for nothing the crate has** is a crate or the prelude ([`Resolved::External`]) when it starts
//!   a path and no glob (`use m::*;`) of the module could bring it in. With a glob, and for a name looked up below a
//!   module where it is not, what it stands for cannot be told ([`Resolved::Unknown`]), and a rule that needs the
//!   item fails on every item of that last name, which is the side that never lets a rule through.
//! - **The names are read from the code of each file as one scope per module:** a `use` in the body of a function binds
//!   its name in the module around it, and a name bound twice stands for both, which only makes a rule fail more.
//!   What is not followed: a name a macro defines, a path a macro builds, and a `#[path]` module.

use std::collections::{BTreeMap, BTreeSet};

use super::{aliases, bindings, inline_modules};
use crate::definitions::{Definition, Shape};

/// The most links of `use` and `type` a path is followed through, which ends a cycle.
const DEPTH: usize = 16;

/// A file of a crate, as the rules that read the crate's names take it: where it is, which module it is, its code
/// alone ([`super::code`], the items under `#[cfg(test)]` blanked), and its outline.
#[derive(Debug, Clone, Copy)]
pub struct Source<'a> {
    pub path: &'a str,
    /// The module the file is, from the crate's root: `[]` for `lib.rs`, `["song"]` for `song.rs` and `song/mod.rs`
    pub module: &'a [String],
    pub code: &'a str,
    pub outline: &'a [Definition],
}

/// What a path stands for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolved {
    /// An item the crate defines, by its path from the root (its module, and its name)
    Item(Vec<String>),
    /// A module of the crate, by its path from the root
    Module(Vec<String>),
    /// A crate of another package, or the prelude: nothing the crate defines
    External,
    /// What cannot be told: the name may come from a glob, or the crate does not define what the path says
    Unknown,
}

/// The names of a crate: its modules, what each defines, and what each binds.
#[derive(Debug, Clone, Default)]
pub struct Names {
    modules: BTreeSet<Vec<String>>,
    defined: BTreeMap<Vec<String>, BTreeSet<String>>,
    bound: BTreeMap<Vec<String>, BTreeMap<String, Vec<Vec<String>>>>,
    globbed: BTreeSet<Vec<String>>,
}

impl Names {
    /// The names of the crate that `files` are the program files of.
    pub fn new(files: &[Source]) -> Self {
        let mut names = Names::default();
        for file in files {
            names.modules.insert(file.module.to_vec());
            for chain in inline_chains(file.code) {
                names.modules.insert(join(file.module, &chain));
            }
        }
        for file in files {
            let bodies = inline_modules(file.code);
            for definition in file.outline {
                let [parents @ .., name] = definition.names.as_slice() else {
                    continue;
                };
                // An `impl` block defines no name of its own, and what is under a type or a function is no item of
                // a module
                let module = join(file.module, parents);
                if !matches!(definition.shape, Shape::Implementation(_))
                    && names.modules.contains(&module)
                {
                    names
                        .defined
                        .entry(module)
                        .or_default()
                        .insert(name.clone());
                }
            }
            let uses = bindings(file.code)
                .into_iter()
                .map(|(at, name, path)| (at, name, path, false));
            let types = aliases(file.code)
                .into_iter()
                .map(|(at, name, path)| (at, Some(name), path, true));
            for (at, name, path, alias) in uses.chain(types) {
                let module = join(file.module, &in_modules(&bodies, at));
                let Some(name) = name else {
                    names.globbed.insert(module);
                    continue;
                };
                // An alias is the name it gives, and not an item of its own that something else could be
                if alias && let Some(defined) = names.defined.get_mut(&module) {
                    defined.remove(&name);
                }
                names
                    .bound
                    .entry(module)
                    .or_default()
                    .entry(name)
                    .or_default()
                    .push(path);
            }
        }
        names
    }

    /// Every module of the crate, by its path from the root.
    pub fn modules(&self) -> &BTreeSet<Vec<String>> {
        &self.modules
    }

    /// Whether `path` is a module of the crate.
    pub fn is_module(&self, path: &[String]) -> bool {
        self.modules.contains(path)
    }

    /// The path of every item of the crate, in any module, that is named `name`.
    pub fn items_named(&self, name: &str) -> Vec<Vec<String>> {
        self.defined
            .iter()
            .filter(|(_, names)| names.contains(name))
            .map(|(module, _)| join(module, &[name.to_string()]))
            .collect()
    }

    /// What `path`, written in the module `module`, stands for: every item it can be, as a name bound twice stands for
    /// both. Never empty.
    pub fn resolve(&self, module: &[String], path: &[String]) -> Vec<Resolved> {
        let mut found: Vec<Resolved> = Vec::new();
        for one in self.follow(module, path, 0) {
            if !found.contains(&one) {
                found.push(one);
            }
        }
        found
    }

    fn follow(&self, module: &[String], path: &[String], depth: usize) -> Vec<Resolved> {
        let Some((first, rest)) = path.split_first() else {
            return vec![Resolved::Module(module.to_vec())];
        };
        if depth > DEPTH {
            return vec![Resolved::Unknown];
        }
        match first.as_str() {
            "crate" => self.below(&[], rest, depth),
            "self" => self.below(module, rest, depth),
            "super" => {
                let mut up = module.to_vec();
                let mut rest = path;
                while rest.first().map(String::as_str) == Some("super") {
                    if up.pop().is_none() {
                        return vec![Resolved::Unknown];
                    }
                    rest = &rest[1..];
                }
                self.below(&up, rest, depth)
            }
            _ => self.named(module, first, rest, depth, true),
        }
    }

    /// The path `rest`, written from the module `module`: a module or an item of it.
    fn below(&self, module: &[String], rest: &[String], depth: usize) -> Vec<Resolved> {
        match rest.split_first() {
            None => vec![Resolved::Module(module.to_vec())],
            Some((first, tail)) => self.named(module, first, tail, depth, false),
        }
    }

    /// `first` and then `tail`, where `first` is looked up in `module`: a module below it, an item it defines, or a
    /// name it binds. `bare` says that `first` starts a path, so what the module has no name of may be a crate or the
    /// prelude.
    fn named(
        &self,
        module: &[String],
        first: &str,
        tail: &[String],
        depth: usize,
        bare: bool,
    ) -> Vec<Resolved> {
        let child = join(module, &[first.to_string()]);
        if self.modules.contains(&child) {
            return self.below(&child, tail, depth);
        }
        let mut found = Vec::new();
        if self
            .defined
            .get(module)
            .is_some_and(|names| names.contains(first))
        {
            // The rest of the path, if any, names a variant or an associated item of it
            found.push(Resolved::Item(child));
        }
        for bound in self
            .bound
            .get(module)
            .and_then(|names| names.get(first))
            .into_iter()
            .flatten()
        {
            let full: Vec<String> = bound.iter().chain(tail).cloned().collect();
            found.extend(self.follow(module, &full, depth + 1));
        }
        if found.is_empty() {
            found.push(if self.globbed.contains(module) || !bare {
                Resolved::Unknown
            } else {
                Resolved::External
            });
        }
        found
    }
}

/// `module` and then `names`.
pub(crate) fn join(module: &[String], names: &[String]) -> Vec<String> {
    module.iter().chain(names).cloned().collect()
}

/// The modules with a body that the offset `at` is in, from the outermost: the modules a file declares inline, read
/// with [`inline_modules`].
fn in_modules(bodies: &[(String, usize, usize)], at: usize) -> Vec<String> {
    bodies
        .iter()
        .filter(|(_, open, close)| *open <= at && at <= *close)
        .map(|(name, _, _)| name.clone())
        .collect()
}

/// The path, from the file's own module, of every module with a body the code declares (`mod a { mod b { } }` is `a`
/// and `a::b`).
pub fn inline_chains(code: &str) -> Vec<Vec<String>> {
    let bodies = inline_modules(code);
    bodies
        .iter()
        .map(|(_, open, _)| in_modules(&bodies, *open))
        .collect()
}

/// The module that the offset `at` of the code is in, from the file's own module: the names of the modules with a body
/// around it.
pub fn module_at(code: &str, at: usize) -> Vec<String> {
    in_modules(&inline_modules(code), at)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rust::{code, outline};

    fn path(text: &str) -> Vec<String> {
        text.split("::")
            .filter(|s| !s.is_empty())
            .map(String::from)
            .collect()
    }

    fn item(text: &str) -> Resolved {
        Resolved::Item(path(text))
    }

    /// The names of a crate of these files, each given its module and its source.
    fn names_of(files: &[(&str, &str)]) -> Names {
        let programs: Vec<(Vec<String>, String, Vec<Definition>)> = files
            .iter()
            .map(|(module, source)| (path(module), code(source), outline(source)))
            .collect();
        let sources: Vec<Source> = programs
            .iter()
            .map(|(module, code, outline)| Source {
                path: "x.rs",
                module,
                code,
                outline,
            })
            .collect();
        Names::new(&sources)
    }

    fn resolved(names: &Names, module: &str, written: &str) -> Vec<Resolved> {
        names.resolve(&path(module), &path(written))
    }

    #[test]
    fn a_bare_name_is_the_item_of_its_module_and_a_path_goes_down_modules() {
        let names = names_of(&[
            (
                "",
                "pub mod song;\npub enum Top { A }\nmod inner { pub struct Deep; }\n",
            ),
            ("song", "pub enum Kind { A }\n"),
        ]);
        assert_eq!(resolved(&names, "song", "Kind"), [item("song::Kind")]);
        assert_eq!(resolved(&names, "", "Top"), [item("Top")]);
        assert_eq!(resolved(&names, "", "song::Kind"), [item("song::Kind")]);
        assert_eq!(resolved(&names, "song", "crate::Top"), [item("Top")]);
        assert_eq!(resolved(&names, "song", "super::Top"), [item("Top")]);
        assert_eq!(resolved(&names, "song", "self::Kind"), [item("song::Kind")]);
        assert_eq!(resolved(&names, "", "inner::Deep"), [item("inner::Deep")]);
        // A variant is an item of the type, which is what it names
        assert_eq!(resolved(&names, "", "song::Kind::A"), [item("song::Kind")]);
        assert_eq!(
            resolved(&names, "", "song"),
            [Resolved::Module(path("song"))]
        );
        assert!(names.is_module(&path("inner")) && !names.is_module(&path("Top")));
    }

    #[test]
    fn a_name_bound_with_use_or_type_is_what_it_is_bound_to_as_far_as_it_goes() {
        let names = names_of(&[
            (
                "",
                "pub mod song; pub mod rank;\nuse crate::song::Kind as K;\nuse self::rank::{Order, self as r};\n\
                 type Alias = K;\ntype Boxed = Box<song::Kind>;\ntype Refd<'a> = &'a mut song::Kind;\n",
            ),
            ("song", "pub enum Kind { A }\n"),
            ("rank", "pub trait Order {}\n"),
        ]);
        assert_eq!(resolved(&names, "", "K"), [item("song::Kind")]);
        assert_eq!(resolved(&names, "", "Alias"), [item("song::Kind")]);
        assert_eq!(resolved(&names, "", "Order"), [item("rank::Order")]);
        assert_eq!(resolved(&names, "", "r::Order"), [item("rank::Order")]);
        assert_eq!(resolved(&names, "", "Refd"), [item("song::Kind")]);
        // A box of the type is the box, which names a crate or the prelude
        assert_eq!(resolved(&names, "", "Boxed"), [Resolved::External]);
    }

    #[test]
    fn a_name_re_exported_by_another_module_is_followed_to_where_it_is_defined() {
        let names = names_of(&[
            (
                "",
                "pub mod song; pub mod prelude { pub use crate::song::Kind; pub use crate::song::Kind as Genre; }\n",
            ),
            ("song", "pub enum Kind { A }\n"),
        ]);
        assert_eq!(resolved(&names, "", "prelude::Kind"), [item("song::Kind")]);
        assert_eq!(resolved(&names, "", "prelude::Genre"), [item("song::Kind")]);
    }

    #[test]
    fn a_name_that_stands_for_nothing_is_external_unless_a_glob_could_bring_it() {
        let names = names_of(&[
            ("", "pub mod a; pub mod b;\nuse std::fmt::Display;\n"),
            ("a", "use super::*;\nimpl Missing {}\n"),
            ("b", "impl Missing {}\n"),
        ]);
        // A crate, or the prelude
        assert_eq!(resolved(&names, "b", "Missing"), [Resolved::External]);
        assert_eq!(
            resolved(&names, "b", "std::fmt::Display"),
            [Resolved::External]
        );
        assert_eq!(resolved(&names, "", "Display"), [Resolved::External]);
        // The glob of module `a` may bring any name in
        assert_eq!(resolved(&names, "a", "Missing"), [Resolved::Unknown]);
        // Below a module, a name it does not have is nothing that can be told
        assert_eq!(resolved(&names, "b", "crate::a::Nope"), [Resolved::Unknown]);
        assert_eq!(resolved(&names, "", "super::x"), [Resolved::Unknown]);
    }

    #[test]
    fn a_name_bound_twice_stands_for_both_and_a_cycle_ends() {
        let names = names_of(&[
            (
                "",
                "pub mod a; pub mod b;\nfn f() { use a::Kind as K; }\nfn g() { use b::Kind as K; }\n",
            ),
            ("a", "pub enum Kind { A }\n"),
            (
                "b",
                "pub enum Kind { B }\nuse self::X as Y;\nuse self::Y as X;\n",
            ),
        ]);
        assert_eq!(
            resolved(&names, "", "K"),
            [item("a::Kind"), item("b::Kind")]
        );
        assert_eq!(resolved(&names, "b", "X"), [Resolved::Unknown]);
    }

    #[test]
    fn the_names_of_inline_modules_are_modules_of_their_own() {
        let names = names_of(&[(
            "song",
            "pub mod inner { pub enum Deep { A } mod more { pub struct E; } }\nuse inner::Deep as D;\n\
             enum Top { A }\nimpl Top {}\n",
        )]);
        assert!(names.is_module(&path("song::inner::more")));
        assert_eq!(resolved(&names, "song", "D"), [item("song::inner::Deep")]);
        assert_eq!(
            resolved(&names, "song::inner", "more::E"),
            [item("song::inner::more::E")]
        );
        assert_eq!(names.items_named("Deep"), [path("song::inner::Deep")]);
        // A variant and a method are no item of a module
        assert!(names.items_named("A").is_empty());
    }
}
