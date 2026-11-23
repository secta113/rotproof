//! The levels inside a Rust crate: a level of a layer is a module of the layer's crate (`crates/infrastructure/src/
//! inbound/`), so the crate's manifest cannot tell the levels apart, and the crate's code is read instead.
//!
//! - **A file of a level names another level only as the table allows.** Every path from `crate`, `super` or `self`
//!   is read, in a `use`, in an expression or a type, and in the tokens of a macro alike, and resolved to a module
//!   path from the crate's root, through the modules with a body it sits in (`mod tests { use super::*; }`). A path
//!   that lands in another level is judged as an import of that level; one that lands at the root (`crate::*`, an item
//!   of the root) is an import of the layer beside its levels.
//! - **A file of a level names another layer only as its level may.** The manifest says what the whole crate may
//!   depend on, which is more than each level may: `infrastructure` depends on `protocol` and `domain`, and its
//!   `inbound` may use `protocol::inbound` alone. So a path from the name of a dependency on another layer is judged
//!   too, as an import of that layer, or of its level when the path names one (`protocol::inbound::Commands`). A path
//!   that starts with `::` names the crate as the bare path does (edition 2018 and later), and is judged as it.
//! - **A name bound with `as` is the path it renames.** `use protocol as p;`, `use protocol::{self as p};` and `extern
//!   crate protocol as p;` bind `p` to the dependency, `use p::outbound as o;` binds `o` to a level of it, and `use
//!   crate as c;` binds `c` to the crate: a path from such a name (`p::outbound::Port`) is judged as the path it stands
//!   for, and its message names the alias. The file is one scope, so a name bound twice stands for both, and a binding
//!   counts wherever in the file it sits. The binding alone is judged as the path it is (`use protocol::outbound as
//!   o;` is an import of `protocol.outbound`), and `use protocol as p;` as none, since it takes nothing in.
//! - **The crate's root (`src/lib.rs`) only declares the modules of its levels** (`pub mod inbound;`), with its inner
//!   attributes (`#![...]`). Anything else there, a re-export (`pub use outbound::*`) above all, would let a level
//!   reach another by a name of the root, and sits beside the levels.
//! - **Every other `.rs` file in `src/` sits in a level:** under `src/<level>/`, or `src/<level>.rs`. A crate's tests,
//!   benches and build script are outside `src/`, and not judged here.
//! - **`#[path]` is forbidden in such a crate:** it would make a file of one level the module of another.
//!
//! Not judged: a path a macro builds from pieces, which no reading of tokens sees, and a name bound in one file and
//! used through another (`pub use protocol as p;` in a level's `mod.rs`, then `super::p::outbound::Port`).

use crate::direction::{Forbidden, judged};
use crate::layers::Place;
use crate::rust::{Token, attributes, code, inline_modules, paths, renames, tokens};
use crate::text::line_of;
use crate::vocabulary::{Crate, applies};
use std::collections::BTreeMap;

/// What the reading of one crate with levels found: what sits beside the levels or cannot be judged, and every path
/// the table forbids.
#[derive(Debug, Default)]
pub struct Found {
    pub problems: Vec<String>,
    pub forbidden: Vec<Forbidden>,
}

/// A dependency of a crate on another layer: the name its code uses for it (the key in `Cargo.toml`, with `-` as `_`),
/// the layer it lands in, and that layer's levels.
pub struct Dependency<'a> {
    pub name: String,
    pub layer: &'a Place,
    pub levels: Vec<&'a Place>,
    /// What the crate defines, when it is the `domain` of the unit that the `protocol` crate belongs to, which the
    /// imports of `protocol.inbound` from it are judged by (the vocabularies, by name, from the module that defines
    /// them). `None` for any other layer, and for a `domain` that could not be read, whose every import is then judged
    /// by the table, which forbids it.
    pub vocabularies: Option<Crate>,
}

/// Every finding in the `.rs` files of the crate of `layer` (path from the root, and source) that sit in its `src/`,
/// whose levels are `levels`, and whose dependencies on other layers are `dependencies`.
pub fn judged_crate(
    files: &[(String, String)],
    layer: &Place,
    levels: &[&Place],
    dependencies: &[Dependency],
) -> Found {
    let mut found = Found::default();
    let mut roots = vec!["crate", "super", "self"];
    roots.extend(dependencies.iter().map(|d| d.name.as_str()));
    let src = format!("{}/src/", layer.path);
    let names: Vec<(&str, &Place)> = levels
        .iter()
        .map(|level| (directory(level), *level))
        .collect();
    let paths_of_levels: Vec<String> = levels.iter().map(|l| format!("{}/", l.path)).collect();
    for (path, source) in files {
        let Some(rel) = path.strip_prefix(&src) else {
            continue;
        };
        let code = code(source);
        for at in attributes(&code, "path") {
            found.problems.push(format!(
                "{path}:{}: #[path] is not allowed in a crate with levels: it can make a file of one level the module \
                 of another",
                line_of(source, at)
            ));
        }
        if rel == "lib.rs" {
            if let Some(at) = beside_in_root(&code, &names) {
                found.problems.push(format!(
                    "{path}:{}: the root of a crate with levels only declares their modules ({}), and holds nothing \
                     beside them",
                    line_of(source, at),
                    names
                        .iter()
                        .map(|(name, _)| format!("pub mod {name};"))
                        .collect::<Vec<_>>()
                        .join(" ")
                ));
            }
            continue;
        }
        let module = module_of(rel);
        let Some(from) = names
            .iter()
            .find(|(name, _)| module.first().map(String::as_str) == Some(*name))
            .map(|(_, place)| *place)
        else {
            found.problems.push(format!(
                "code in {} outside its levels: {path} (move it into a level: {})",
                layer.name,
                paths_of_levels.join(" or ")
            ));
            continue;
        };
        let bodies = inline_modules(&code);
        let aliases = aliases(&code, &module, &bodies, dependencies);
        let mut starts = roots.clone();
        starts.extend(aliases.keys().map(String::as_str));
        for (at, written_from) in paths(&code, &starts) {
            for (segments, through) in followed(&aliases, written_from) {
                let mut vocabularies = None;
                let to = match dependencies.iter().find(|d| d.name == segments[0]) {
                    // Another layer's crate: one of its levels when the path names one, or the layer
                    Some(dependency) => {
                        let to = dependency
                            .levels
                            .iter()
                            .copied()
                            .find(|level| {
                                segments.get(1).map(String::as_str) == Some(directory(level))
                            })
                            .unwrap_or(dependency.layer);
                        // The `domain` of the own unit, read by `protocol.inbound`: by name, not whole
                        if applies(from, to) {
                            vocabularies = dependency.vocabularies.as_ref();
                        }
                        to
                    }
                    None => {
                        // The module the path is written in: the file's, then each module with a body around it
                        let base = base_at(&module, &bodies, at);
                        let Some(target) = resolved(&base, &segments) else {
                            continue;
                        };
                        names
                            .iter()
                            .find(|(name, _)| target.first().map(String::as_str) == Some(*name))
                            .map_or(layer, |(_, place)| *place)
                    }
                };
                let written = segments.join("::");
                let what = match &through {
                    Some(alias) => format!("uses {written} through `{alias}`"),
                    None => format!("uses {written}"),
                };
                let line = line_of(source, at);
                found.forbidden.extend(match vocabularies {
                    Some(domain) => domain.judged(path, line, &written, &what, &segments[1..]),
                    None => judged(path, line, &written, &what, from, to),
                });
            }
        }
    }
    found
}

/// The module a path written at offset `at` is in: the file's, then each module with a body around it.
fn base_at(module: &[String], bodies: &[(String, usize, usize)], at: usize) -> Vec<String> {
    let mut base = module.to_vec();
    base.extend(
        bodies
            .iter()
            .filter(|(_, open, close)| *open < at && at < *close)
            .map(|(name, _, _)| name.clone()),
    );
    base
}

/// The names a file binds with `as` to a path that starts in the crate or in a dependency on another layer, each with
/// the full paths it can stand for, written from `crate` or from the name of the dependency: `use protocol as p;`,
/// `use protocol::{self as p}` and `extern crate protocol as p;` make `p` stand for `protocol`, `use p::outbound as o;`
/// makes `o` stand for `protocol::outbound`, and `use crate as c;` makes `c` stand for `crate`. A name bound twice
/// stands for both, whatever module each is in: the file is read as one scope, which is the safe side.
fn aliases(
    code: &str,
    module: &[String],
    bodies: &[(String, usize, usize)],
    dependencies: &[Dependency],
) -> BTreeMap<String, Vec<Vec<String>>> {
    let bindings = renames(code);
    let is_root = |name: &str| {
        matches!(name, "crate" | "super" | "self") || dependencies.iter().any(|d| d.name == name)
    };
    let mut aliases: BTreeMap<String, Vec<Vec<String>>> = BTreeMap::new();
    // A chain of renames (`use p::o as q;`) is followed one link a round
    for _ in 0..=bindings.len() {
        let mut changed = false;
        for (at, name, target) in &bindings {
            let first = target[0].as_str();
            if is_root(name) {
                continue;
            }
            let rooted: Vec<Vec<String>> = if matches!(first, "crate" | "super" | "self") {
                resolved(&base_at(module, bodies, *at), target)
                    .map(|rest| std::iter::once("crate".to_string()).chain(rest).collect())
                    .into_iter()
                    .collect()
            } else if is_root(first) {
                vec![target.clone()]
            } else if let Some(stands_for) = aliases.get(first) {
                stands_for
                    .iter()
                    .map(|path| path.iter().chain(&target[1..]).cloned().collect())
                    .collect()
            } else {
                continue;
            };
            let entry = aliases.entry(name.clone()).or_default();
            for path in rooted {
                if !entry.contains(&path) {
                    entry.push(path);
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }
    aliases
}

/// A path as the paths it stands for, each with the alias it goes through: itself, with none, unless it starts with
/// a name `aliases` binds.
fn followed(
    aliases: &BTreeMap<String, Vec<Vec<String>>>,
    segments: Vec<String>,
) -> Vec<(Vec<String>, Option<String>)> {
    match aliases.get(&segments[0]) {
        Some(stands_for) => stands_for
            .iter()
            .map(|path| {
                let full = path.iter().chain(&segments[1..]).cloned().collect();
                (full, Some(segments[0].clone()))
            })
            .collect(),
        None => vec![(segments, None)],
    }
}

/// The name of a level's directory, the module it is: the last part of its path.
fn directory(level: &Place) -> &str {
    level.path.rsplit('/').next().unwrap_or(&level.path)
}

/// The module a file in `src/` is, from the crate's root: `inbound/cli.rs` is `inbound::cli`, and `inbound/mod.rs`
/// and `inbound.rs` are `inbound`.
fn module_of(rel: &str) -> Vec<String> {
    let mut parts: Vec<String> = rel
        .trim_end_matches(".rs")
        .split('/')
        .map(String::from)
        .collect();
    if parts.last().map(String::as_str) == Some("mod") {
        parts.pop();
    }
    parts
}

/// The module a file in `src/` is, from the crate's root, when it is read as a crate's names are: the root file
/// (`lib.rs`, `main.rs`) is the root itself, `[]`, and any other is as [`module_of`] has it.
pub fn crate_module(rel: &str) -> Vec<String> {
    match rel {
        "lib.rs" | "main.rs" => Vec::new(),
        _ => module_of(rel),
    }
}

/// A path as a module path from the crate's root, written in the module `base`: `None` when it climbs above the root,
/// which Rust rejects. A `self` at its end names what comes before it.
fn resolved(base: &[String], segments: &[String]) -> Option<Vec<String>> {
    let (first, rest) = segments.split_first()?;
    let mut out = match first.as_str() {
        "crate" => rest.to_vec(),
        "self" => base.iter().chain(rest).cloned().collect(),
        "super" => {
            let supers = 1 + rest.iter().take_while(|s| *s == "super").count();
            if supers > base.len() {
                return None;
            }
            base[..base.len() - supers]
                .iter()
                .chain(&segments[supers..])
                .cloned()
                .collect()
        }
        _ => return None,
    };
    if out.last().map(String::as_str) == Some("self") {
        out.pop();
    }
    Some(out)
}

/// Where the root of a crate with levels holds something other than its inner attributes and the declarations of its
/// levels' modules (`pub mod inbound;`, or with `pub(crate)` or no `pub`): the offset of the first such token.
fn beside_in_root(code: &str, levels: &[(&str, &Place)]) -> Option<usize> {
    let tokens = tokens(code);
    let is = |i: usize, token: &Token| tokens.get(i).map(|(_, t)| t) == Some(token);
    let word = |i: usize| match tokens.get(i).map(|(_, t)| t) {
        Some(Token::Word(w)) => Some(w.as_str()),
        _ => None,
    };
    let mut i = 0;
    while i < tokens.len() {
        // An inner attribute, to its closing bracket
        if is(i, &Token::Punct('#'))
            && is(i + 1, &Token::Punct('!'))
            && is(i + 2, &Token::Punct('['))
        {
            let mut depth = 0;
            let mut j = i + 2;
            while j < tokens.len() {
                match &tokens[j].1 {
                    Token::Punct('[') => depth += 1,
                    Token::Punct(']') => {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    _ => {}
                }
                j += 1;
            }
            i = j + 1;
            continue;
        }
        let mut j = i;
        if word(j) == Some("pub") {
            j += 1;
            if is(j, &Token::Punct('('))
                && word(j + 1) == Some("crate")
                && is(j + 2, &Token::Punct(')'))
            {
                j += 3;
            }
        }
        let declares = word(j) == Some("mod")
            && word(j + 1).is_some_and(|name| levels.iter().any(|(level, _)| *level == name))
            && is(j + 2, &Token::Punct(';'));
        if !declares {
            return Some(tokens[i].0);
        }
        i = j + 3;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layers::{layout, table};

    /// What the Rust layout makes of `infrastructure`, and its two levels.
    fn infrastructure() -> (Place, Vec<Place>) {
        let places = layout("rust", "none").unwrap().places(&table());
        let layer = places
            .iter()
            .find(|p| p.name == "infrastructure")
            .unwrap()
            .clone();
        let levels = places
            .into_iter()
            .filter(|p| p.parent.as_deref() == Some("infrastructure"))
            .collect();
        (layer, levels)
    }

    fn found(files: &[(&str, &str)]) -> Found {
        let (layer, levels) = infrastructure();
        let levels: Vec<&Place> = levels.iter().collect();
        let files: Vec<(String, String)> = files
            .iter()
            .map(|(p, s)| (format!("crates/infrastructure/{p}"), s.to_string()))
            .collect();
        // The crate depends on the two layers it may, one of them under another name
        let places = layout("rust", "none").unwrap().places(&table());
        let place = |name: &str| places.iter().find(|p| p.name == name).unwrap();
        let dependencies = [
            Dependency {
                name: "protocol".into(),
                layer: place("protocol"),
                levels: vec![place("protocol.inbound"), place("protocol.outbound")],
                vocabularies: None,
            },
            Dependency {
                name: "core".into(),
                layer: place("domain"),
                levels: Vec::new(),
                vocabularies: None,
            },
        ];
        judged_crate(&files, &layer, &levels, &dependencies)
    }

    #[test]
    fn a_level_names_another_layer_only_as_the_level_may() {
        let said = details(&found(&[
            (
                "src/inbound/mod.rs",
                "use protocol::inbound::Commands;\nuse protocol::outbound::Tree;\nuse core::Layer;\n\
                 fn f() { protocol::Version::new(); }\nuse std::io;\n",
            ),
            (
                "src/outbound/mod.rs",
                "use protocol::outbound::Tree;\nuse core::Layer;\nuse protocol::{inbound::Commands, outbound::Writer};\n",
            ),
        ]));
        for line in [
            "crates/infrastructure/src/inbound/mod.rs:2: uses protocol::outbound::Tree, in protocol.outbound; \
             infrastructure.inbound may import `protocol.inbound`",
            "crates/infrastructure/src/inbound/mod.rs:3: uses core::Layer, in domain; ",
            "crates/infrastructure/src/inbound/mod.rs:4: uses protocol::Version::new, in protocol; ",
            "crates/infrastructure/src/outbound/mod.rs:3: uses protocol::inbound::Commands, in protocol.inbound; \
             infrastructure.outbound may import `protocol.outbound` and `domain`",
        ] {
            assert!(
                said.iter().any(|s| s.starts_with(line)),
                "{line}:\n{said:#?}"
            );
        }
        assert_eq!(said.len(), 4, "{said:#?}");
    }

    fn details(found: &Found) -> Vec<String> {
        found
            .forbidden
            .iter()
            .map(|f| f.detail.clone())
            .chain(found.problems.iter().cloned())
            .collect()
    }

    #[test]
    fn a_level_naming_the_other_level_fails_however_it_names_it() {
        let found = found(&[
            (
                "src/lib.rs",
                "//! Doc\n#![forbid(unsafe_code)]\n\npub mod inbound;\npub mod outbound;\n",
            ),
            (
                "src/inbound/mod.rs",
                "use crate::outbound::db::Db;\nuse super::outbound;\nuse crate::{inbound::cli, outbound::Http};\n\
                 pub mod cli;\nfn f() { m!(crate::outbound::x); }\nuse crate::*;\n",
            ),
            (
                "src/inbound/cli.rs",
                "use super::super::outbound::Db;\nuse super::args;\nuse self::inner::x;\n",
            ),
            // Inside a module with a body, `super` is the file's own module
            (
                "src/outbound.rs",
                "pub struct Db;\n#[cfg(test)]\nmod tests {\n    use super::*;\n    use super::super::inbound;\n}\n",
            ),
            // Outside src/: not judged here
            ("tests/all.rs", "use infrastructure::outbound::Db;\n"),
        ]);
        let said = details(&found);
        for line in [
            "crates/infrastructure/src/inbound/mod.rs:1: uses crate::outbound::db::Db, in infrastructure.outbound; \
             infrastructure.inbound may import `protocol.inbound`",
            "crates/infrastructure/src/inbound/mod.rs:2: uses super::outbound, in infrastructure.outbound; ",
            "crates/infrastructure/src/inbound/mod.rs:3: uses crate::outbound::Http, in infrastructure.outbound; ",
            "crates/infrastructure/src/inbound/mod.rs:5: uses crate::outbound::x, in infrastructure.outbound; ",
            "crates/infrastructure/src/inbound/mod.rs:6: uses crate::*, in infrastructure outside its levels; ",
            "crates/infrastructure/src/inbound/cli.rs:1: uses super::super::outbound::Db, in infrastructure.outbound; ",
            "crates/infrastructure/src/outbound.rs:5: uses super::super::inbound, in infrastructure.inbound; ",
        ] {
            assert!(
                said.iter().any(|s| s.starts_with(line)),
                "{line}:\n{said:#?}"
            );
        }
        // Nothing else: what stays in the file's own level passes, `super::*` in the module with a body included
        assert_eq!(said.len(), 7, "{said:#?}");
    }

    #[test]
    fn a_name_bound_with_as_is_judged_as_the_path_it_stands_for() {
        let said = details(&found(&[
            (
                "src/inbound/mod.rs",
                "use protocol as p;\nuse p as q;\nextern crate protocol as e;\nuse crate as c;\nuse {protocol as r};\n\
                 fn f(_: p::outbound::Port, _: q::outbound::Port, _: e::outbound::Port) {}\n\
                 fn g(_: r::outbound::Port) { c::outbound::Db::new(); }\n\
                 fn ok(_: p::inbound::Commands, _: c::inbound::Cli) {}\n\
                 // use protocol as z;\nfn h(_: z::outbound::Port) { y::outbound::Port; }\n",
            ),
            (
                "src/outbound/mod.rs",
                "use protocol as p;\nuse p::inbound as i;\nuse protocol::outbound as o;\nuse p::{outbound as w};\n\
                 use p::*;\nfn f(_: i::Commands, _: o::Port, _: w::Port) {}\n\
                 mod tests { use super::super as s; fn g() { s::inbound::x(); } }\n",
            ),
        ]));
        for line in [
            "crates/infrastructure/src/inbound/mod.rs:6: uses protocol::outbound::Port through `p`, in protocol.outbound; \
             infrastructure.inbound may import `protocol.inbound`",
            "crates/infrastructure/src/inbound/mod.rs:6: uses protocol::outbound::Port through `q`, ",
            "crates/infrastructure/src/inbound/mod.rs:6: uses protocol::outbound::Port through `e`, ",
            "crates/infrastructure/src/inbound/mod.rs:7: uses protocol::outbound::Port through `r`, ",
            "crates/infrastructure/src/inbound/mod.rs:7: uses crate::outbound::Db::new through `c`, in \
             infrastructure.outbound; ",
            "crates/infrastructure/src/outbound/mod.rs:2: uses protocol::inbound through `p`, in protocol.inbound; \
             infrastructure.outbound may import `protocol.outbound` and `domain`",
            "crates/infrastructure/src/outbound/mod.rs:5: uses protocol::* through `p`, in protocol; ",
            "crates/infrastructure/src/outbound/mod.rs:6: uses protocol::inbound::Commands through `i`, ",
            "crates/infrastructure/src/outbound/mod.rs:7: uses super::super, in infrastructure outside its levels; ",
            "crates/infrastructure/src/outbound/mod.rs:7: uses crate::inbound::x through `s`, in infrastructure.inbound; ",
        ] {
            assert!(
                said.iter().any(|s| s.starts_with(line)),
                "{line}:\n{said:#?}"
            );
        }
        // Nothing else: the paths through `p::inbound`, `o` and `w` stay in what the level may import, and a name
        // bound in a comment, or not bound at all, stands for nothing
        assert_eq!(said.len(), 10, "{said:#?}");
    }

    #[test]
    fn a_path_from_a_leading_colons_is_judged_as_the_path_without_them() {
        let said = details(&found(&[(
            "src/inbound/mod.rs",
            "pub fn rooted(_: ::protocol::outbound::Port) {}\nuse ::protocol::outbound::Tree;\n\
             use ::{protocol::outbound as o};\n\
             fn g(_: o::Q) -> ::protocol::inbound::Commands { <T as ::protocol::outbound::X>::f(); ::std::io::x(); }\n",
        )]));
        for line in [
            "crates/infrastructure/src/inbound/mod.rs:1: uses protocol::outbound::Port, in protocol.outbound; \
             infrastructure.inbound may import `protocol.inbound`",
            "crates/infrastructure/src/inbound/mod.rs:2: uses protocol::outbound::Tree, in protocol.outbound; ",
            "crates/infrastructure/src/inbound/mod.rs:3: uses protocol::outbound, in protocol.outbound; ",
            "crates/infrastructure/src/inbound/mod.rs:4: uses protocol::outbound::Q through `o`, in protocol.outbound; ",
            "crates/infrastructure/src/inbound/mod.rs:4: uses protocol::outbound::X, in protocol.outbound; ",
        ] {
            assert!(
                said.iter().any(|s| s.starts_with(line)),
                "{line}:\n{said:#?}"
            );
        }
        // Nothing else: `::protocol::inbound` is the level's own, and `::std` is no layer
        assert_eq!(said.len(), 5, "{said:#?}");
    }

    #[test]
    fn the_root_holds_only_the_modules_of_the_levels_and_every_file_sits_in_one() {
        let found = found(&[
            (
                "src/lib.rs",
                "pub mod inbound;\npub mod outbound;\npub use outbound::*;\n",
            ),
            ("src/shared.rs", "pub fn f() {}\n"),
            (
                "src/inbound/mod.rs",
                "#[path = \"../outbound/db.rs\"]\nmod db;\n",
            ),
        ]);
        let said = details(&found);
        assert_eq!(said.len(), 3, "{said:#?}");
        assert!(said.iter().any(|s| s.starts_with(
            "crates/infrastructure/src/lib.rs:3: the root of a crate with levels only declares their modules \
             (pub mod inbound; pub mod outbound;)"
        )), "{said:#?}");
        assert!(said.contains(
            &"code in infrastructure outside its levels: crates/infrastructure/src/shared.rs (move it into a level: \
              crates/infrastructure/src/inbound/ or crates/infrastructure/src/outbound/)"
                .to_string()
        ), "{said:#?}");
        assert!(said.iter().any(|s| s.starts_with(
            "crates/infrastructure/src/inbound/mod.rs:1: #[path] is not allowed in a crate with levels"
        )), "{said:#?}");
        // `pub(crate) mod` and no `pub` declare a level too; a module that is no level does not
        assert!(
            details(&super::tests::found(&[(
                "src/lib.rs",
                "pub(crate) mod inbound;\nmod outbound;\n"
            )]))
            .is_empty()
        );
        assert_eq!(
            details(&super::tests::found(&[(
                "src/lib.rs",
                "pub mod inbound;\npub mod helpers;\n"
            )]))
            .len(),
            1
        );
    }

    /// What the Rust layout makes of `protocol` read as a crate whose `src/` holds `files`, with a dependency on the
    /// `domain` of its own unit, which defines `domain` (`None`: a `domain` that could not be read).
    fn protocol(files: &[(&str, &str)], domain: Option<&[(&str, &str)]>) -> Found {
        let places = layout("rust", "none").unwrap().places(&table());
        let place = |name: &str| places.iter().find(|p| p.name == name).unwrap();
        let levels = [place("protocol.inbound"), place("protocol.outbound")];
        let dependencies = [Dependency {
            name: "domain".into(),
            layer: place("domain"),
            levels: Vec::new(),
            vocabularies: domain.map(crate::vocabulary::tests::krate),
        }];
        let files: Vec<(String, String)> = files
            .iter()
            .map(|(p, s)| (format!("crates/protocol/{p}"), s.to_string()))
            .collect();
        judged_crate(&files, place("protocol"), &levels, &dependencies)
    }

    /// A `domain` of two modules: `song`, with a vocabulary `Kind`, an enum with methods `Rank`, and a record
    /// `Record`, and `prelude`, which only re-exports `Kind`.
    const DOMAIN: &[(&str, &str)] = &[
        ("", "pub mod song;\npub mod prelude;\n"),
        (
            "song",
            "pub enum Kind { A, B }\npub enum Rank { A }\nimpl Rank { pub fn up(&self) {} }\npub struct Record;\n",
        ),
        ("prelude", "pub use crate::song::Kind;\n"),
    ];

    #[test]
    fn the_inbound_level_imports_the_vocabularies_of_the_domain_of_its_unit_by_name() {
        let said = details(&protocol(
            &[(
                "src/inbound/mod.rs",
                "use domain::song::Kind;\nuse domain::song::{Kind as K, Rank, Record};\nfn f(_: domain::song::Kind::A) {}\n\
                 use domain::song;\nuse domain::song::*;\nuse domain::prelude::Kind;\nuse domain::Kind;\n\
                 use domain as d;\nfn g(_: d::song::Kind, _: d::song::Rank) {}\nuse domain::song::{self};\n",
            )],
            Some(DOMAIN),
        ));
        let name = "crates/protocol/src/inbound/mod.rs";
        let by_name = "protocol.inbound may import only vocabularies, by name, from the file of domain that defines them";
        let takes_none = "this import takes no name, and what it brings in cannot be told: import each vocabulary by name";
        for line in [
            format!(
                "{name}:2: uses domain::song::Rank, in domain; {by_name}: `Rank` is an enum that has methods, a rule of \
                 domain, so it is no vocabulary: move the methods into a function of domain that takes the kind"
            ),
            format!(
                "{name}:2: uses domain::song::Record, in domain; {by_name}: `Record` is a record, class, function or \
                 type, no vocabulary: write the shape the port shows in protocol.inbound, and convert in application"
            ),
            format!("{name}:4: uses domain::song, in domain; {by_name}: {takes_none}"),
            format!("{name}:5: uses domain::song::*, in domain; {by_name}: {takes_none}"),
            format!(
                "{name}:6: uses domain::prelude::Kind, in domain; {by_name}: `Kind` is not defined in this file (a \
                 name it only re-exports is not followed): import it from the file that defines it"
            ),
            format!(
                "{name}:7: uses domain::Kind, in domain; {by_name}: `Kind` is not defined in this file (a name it \
                 only re-exports is not followed): import it from the file that defines it"
            ),
            format!(
                "{name}:9: uses domain::song::Rank through `d`, in domain; {by_name}: `Rank` is an enum that has methods, \
                 a rule of domain, so it is no vocabulary: move the methods into a function of domain that takes the kind"
            ),
            format!("{name}:10: uses domain::song::self, in domain; {by_name}: {takes_none}"),
        ] {
            assert!(said.contains(&line), "{line}:\n{said:#?}");
        }
        // A vocabulary by its name, renamed, with a variant after it, or through a rename of the crate: no finding
        assert_eq!(said.len(), 8, "{said:#?}");
    }

    #[test]
    fn the_finding_of_a_vocabulary_import_is_keyed_by_the_file_and_the_path_as_a_table_finding_is()
    {
        let found = protocol(
            &[("src/inbound/mod.rs", "use domain::song::Rank;\n")],
            Some(DOMAIN),
        );
        let [forbidden] = found.forbidden.as_slice() else {
            panic!("{:#?}", found.forbidden);
        };
        assert_eq!(forbidden.file, "crates/protocol/src/inbound/mod.rs");
        assert_eq!(forbidden.import, "domain::song::Rank");
        assert_eq!(forbidden.rule, crate::direction::Rule::Direction);
        assert!(found.problems.is_empty(), "{:#?}", found.problems);
        // The table's finding of the same import has the same key
        let table = protocol(&[("src/inbound/mod.rs", "use domain::song::Rank;\n")], None);
        assert_eq!(table.forbidden[0].import, forbidden.import);
        assert_eq!(table.forbidden[0].file, forbidden.file);
    }

    #[test]
    fn the_outbound_level_and_a_domain_that_could_not_be_read_are_judged_as_before() {
        // The outbound level reads the whole `domain`, and the table says so
        let outbound = details(&protocol(
            &[(
                "src/outbound/mod.rs",
                "use domain::song::Rank;\nuse domain::prelude::Kind;\n",
            )],
            Some(DOMAIN),
        ));
        assert!(outbound.is_empty(), "{outbound:#?}");
        // With no read of the `domain`, the table forbids what it forbade: any import of it from the inbound level
        let unread = details(&protocol(
            &[("src/inbound/mod.rs", "use domain::song::Kind;\n")],
            None,
        ));
        assert_eq!(
            unread,
            [
                "crates/protocol/src/inbound/mod.rs:1: uses domain::song::Kind, in domain; protocol.inbound imports \
                 no other layer"
            ]
        );
    }

    #[test]
    fn a_vocabulary_is_taken_by_the_module_that_defines_it_whatever_the_file_it_is_written_in() {
        let domain: &[(&str, &str)] = &[
            ("", "pub mod song;\n"),
            ("song", "pub mod kind;\npub enum Top { A }\n"),
            (
                "song::kind",
                "pub enum Kind { A }\nmod inline { pub enum Deep { A } }\n",
            ),
        ];
        let said = details(&protocol(
            &[(
                "src/inbound/cli.rs",
                "use domain::song::kind::Kind;\nuse domain::song::Top;\nuse domain::song::kind::inline::Deep;\n\
                 use domain::song::Kind;\n",
            )],
            Some(domain),
        ));
        // The file is a module of the inbound level, and the third names an inline module, which is no module of the
        // crate's files but is one all the same; the last names what the module `song` does not define
        assert_eq!(said.len(), 1, "{said:#?}");
        assert!(
            said[0].contains("`Kind` is not defined in this file"),
            "{said:#?}"
        );
    }

    #[test]
    fn the_files_of_a_crate_are_the_modules_the_names_of_the_crate_read_them_as() {
        for (file, module) in [
            ("lib.rs", ""),
            ("main.rs", ""),
            ("song.rs", "song"),
            ("song/mod.rs", "song"),
            ("song/kind.rs", "song::kind"),
        ] {
            assert_eq!(crate_module(file).join("::"), module, "{file}");
        }
        // The levels' files keep the names they had: `lib.rs` is none of them
        assert_eq!(module_of("lib.rs"), ["lib"]);
    }
}
