//! The levels inside a Rust crate: a level of a layer is a module of the layer's crate (`crates/infrastructure/src/
//! inbound/`), so the crate's manifest cannot tell the levels apart, and the crate's code is read instead.
//!
//! - **A file of a level names another level only as the table allows.** Every path from `crate`, `super` or `self`
//!   is read, in a `use`, in an expression or a type, and in the tokens of a macro alike, and resolved to a module
//!   path from the crate's root, through the modules with a body it sits in (`mod tests { use super::*; }`). A path
//!   that lands in another level is judged as an import of that level; one that lands at the root (`crate::*`, an item
//!   of the root) is an import of the layer beside its levels.
//! - **The crate's root (`src/lib.rs`) only declares the modules of its levels** (`pub mod inbound;`), with its inner
//!   attributes (`#![...]`). Anything else there, a re-export (`pub use outbound::*`) above all, would let a level
//!   reach another by a name of the root, and sits beside the levels.
//! - **Every other `.rs` file in `src/` sits in a level:** under `src/<level>/`, or `src/<level>.rs`. A crate's tests,
//!   benches and build script are outside `src/`, and not judged here.
//! - **`#[path]` is forbidden in such a crate:** it would make a file of one level the module of another.
//!
//! Not judged: a path a macro builds from pieces, which no reading of tokens sees.

use crate::direction::{Forbidden, judged};
use crate::layers::Place;
use utils::rust::{Token, attributes, code, inline_modules, paths, tokens};
use utils::text::line_of;

/// What the reading of one crate with levels found: what sits beside the levels or cannot be judged, and every path
/// the table forbids.
#[derive(Debug, Default)]
pub struct Found {
    pub problems: Vec<String>,
    pub forbidden: Vec<Forbidden>,
}

/// Every finding in the `.rs` files of the crate of `layer` (path from the root, and source) that sit in its `src/`,
/// whose levels are `levels`.
pub fn judged_crate(files: &[(String, String)], layer: &Place, levels: &[&Place]) -> Found {
    let mut found = Found::default();
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
        for (at, segments) in paths(&code) {
            // The module the path is written in: the file's, then each module with a body around it
            let mut base = module.clone();
            base.extend(
                bodies
                    .iter()
                    .filter(|(_, open, close)| *open < at && at < *close)
                    .map(|(name, _, _)| name.clone()),
            );
            let Some(target) = resolved(&base, &segments) else {
                continue;
            };
            let to = names
                .iter()
                .find(|(name, _)| target.first().map(String::as_str) == Some(*name))
                .map_or(layer, |(_, place)| *place);
            let written = segments.join("::");
            found.forbidden.extend(judged(
                path,
                line_of(source, at),
                &written,
                &format!("uses {written}"),
                from,
                to,
            ));
        }
    }
    found
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
        let places = layout("rust").unwrap().places(&table());
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
        judged_crate(&files, &layer, &levels)
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
}
