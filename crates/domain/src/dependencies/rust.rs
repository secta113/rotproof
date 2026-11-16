//! The names of Rust's crates: a crate is named with `-` in its manifest and `_` in the code, and the two are one
//! name; and which of them the code of a crate names.

use crate::rust::{Token, paths, renames, tokens};

/// A crate's name as the code writes it: `serde-json` is `serde_json`.
pub fn normalize(name: &str) -> String {
    name.replace('-', "_")
}

/// Every place the code alone ([`crate::rust::code`]) names one of the crates `starts`, by the offset and the crate: a
/// path that starts with it (`serde::Serialize`, in a `use`, an expression, an attribute or a macro), a name it is
/// bound to with `as`, and the forms with no path after it, `extern crate serde;` and `use serde;`. A path from `::`
/// counts as the bare one. What a name is bound to is read, so `use serde as s;` names `serde`, and `s::x` is the same
/// crate.
pub fn names(code: &str, starts: &[&str]) -> Vec<(usize, String)> {
    let mut found: Vec<(usize, String)> = paths(code, starts)
        .into_iter()
        .map(|(at, path)| (at, path[0].clone()))
        .collect();
    for (at, _, path) in renames(code) {
        if let Some(first) = path
            .first()
            .filter(|first| starts.contains(&first.as_str()))
        {
            found.push((at, first.clone()));
        }
    }
    let tokens = tokens(code);
    for (i, (at, token)) in tokens.iter().enumerate() {
        let after = match token {
            Token::Word(word) if word == "use" => i + 1,
            Token::Word(word) if word == "extern" => match tokens.get(i + 1) {
                Some((_, Token::Word(next))) if next == "crate" => i + 2,
                _ => continue,
            },
            _ => continue,
        };
        let after = match tokens.get(after) {
            Some((_, Token::PathSep)) => after + 1,
            _ => after,
        };
        if let Some((_, Token::Word(name))) = tokens.get(after)
            && starts.contains(&name.as_str())
        {
            found.push((*at, name.clone()));
        }
    }
    found.sort();
    found.dedup();
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn named(source: &str, starts: &[&str]) -> Vec<String> {
        let mut found: Vec<String> = names(&crate::rust::code(source), starts)
            .into_iter()
            .map(|(_, name)| name)
            .collect();
        found.dedup();
        found
    }

    #[test]
    fn a_crate_is_named_by_a_path_a_use_a_rename_or_an_extern_crate() {
        let starts = ["serde", "serde_json", "tokio", "log", "rand", "unused"];
        assert_eq!(named("use serde::Serialize;", &starts), ["serde"]);
        assert_eq!(
            named("fn f() { serde_json::json!({}); }", &starts),
            ["serde_json"]
        );
        assert_eq!(
            named("#[tokio::main]\nasync fn main() {}", &starts),
            ["tokio"]
        );
        assert_eq!(named("use log as l;", &starts), ["log"]);
        assert_eq!(named("extern crate rand;", &starts), ["rand"]);
        assert_eq!(
            named("use ::serde;\nuse serde::{self, de};", &starts),
            ["serde"]
        );
        // A word that is no path start, a string and a comment name nothing
        assert!(
            named(
                "let serde = 1; // tokio::spawn\nlet s = \"log::info\";",
                &starts
            )
            .is_empty()
        );
        assert!(named("mod unused_things; fn serde_like() {}", &starts).is_empty());
    }

    #[test]
    fn the_hyphen_and_the_underscore_are_one_name() {
        assert_eq!(normalize("serde-json"), "serde_json");
        assert_eq!(normalize("serde_json"), "serde_json");
        assert_eq!(normalize("tokio"), "tokio");
    }
}
