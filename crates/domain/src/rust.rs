//! Reading Rust source with a small scanner of Rotproof's own: its comments, and the code alone.
//!
//! - **Its comments:** `//` to the end of the line, and `/* */`, which nest. Doc comments (`///`, `//!`, `/** */`) are
//!   comments too: Rust writes them as comments, and the text in them is read as text.
//! - **What is not a comment:** what sits in a string (`"..."`, `b"..."`, `c"..."`, and the raw forms `r#"..."#`,
//!   `br"..."`, `cr"..."`) or a character literal (`'/'`, `b'"'`, `'\''`), which the scanner skips. A `'` that starts
//!   no character literal starts a lifetime or a label (`'a`, `'static`).
//!
//! A string or a comment left open runs to the end of the file. Rust itself fails on such a file, so it is not reported
//! here.
//!
//! [`outline`] reads the definitions of a file from its tokens, for a knowledge document that follows one.

mod outline;

pub use outline::outline;

/// Whether a file name is Rust source. Case does not count: Windows compiles `LIB.RS` for `mod lib`.
pub fn is_source(name: &str) -> bool {
    name.to_ascii_lowercase().ends_with(".rs")
}

/// Every comment of a source: its byte offset, and its text with its delimiters.
pub fn comments(source: &str) -> Vec<(usize, String)> {
    spans(source)
        .into_iter()
        .filter(|(kind, _, _)| *kind == Kind::Comment)
        .map(|(_, start, end)| (start, source[start..end].to_string()))
        .collect()
}

/// The source with every comment and every string or character literal blanked with spaces, its line breaks kept: the
/// code alone, at the same byte offsets and on the same lines.
pub fn code(source: &str) -> String {
    let mut bytes = source.as_bytes().to_vec();
    for (_, start, end) in spans(source) {
        for byte in &mut bytes[start..end] {
            if *byte != b'\n' {
                *byte = b' ';
            }
        }
    }
    String::from_utf8(bytes).expect("only whole characters are blanked, each byte with a space")
}

/// One token of the code alone, as [`code`] leaves it: an identifier or a keyword, `::`, or any other byte that is not
/// space.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token {
    Word(String),
    PathSep,
    Punct(char),
}

/// The tokens of the code alone ([`code`]), each with its byte offset.
pub fn tokens(code: &str) -> Vec<(usize, Token)> {
    let b = code.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c.is_ascii_whitespace() {
            i += 1;
        } else if is_identifier(c) {
            let end = i + b[i..].iter().take_while(|&&c| is_identifier(c)).count();
            out.push((i, Token::Word(code[i..end].to_string())));
            i = end;
        } else if c == b':' && b.get(i + 1) == Some(&b':') {
            out.push((i, Token::PathSep));
            i += 2;
        } else {
            out.push((i, Token::Punct(c as char)));
            i += 1;
        }
    }
    out
}

/// Every path the code alone ([`code`]) names from one of the words `starts` (such as `crate`, `super`, `self` or the
/// name of a dependency), as its segments, with the offset where it starts: in `use`, in an expression or a type, and
/// in the tokens of a macro alike. A `::` before the first word (`::protocol::x`) names the crate as the bare path
/// does, and is read as it, so `::std::env::var` is `std::env::var`. A `use` group is spread into a path per item
/// (`crate::{a, b::c}` is `crate::a` and `crate::b::c`), `self` in a group stays a segment of its own, a glob ends in
/// `*`, and a rename (`as x`) is dropped (what it binds is read by [`renames`]).
pub fn paths(code: &str, starts: &[&str]) -> Vec<(usize, Vec<String>)> {
    let tokens = tokens(code);
    let mut out = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        let starts = matches!(&tokens[i].1, Token::Word(w) if starts.contains(&w.as_str()))
            && (i == 0 || tokens[i - 1].1 != Token::PathSep || is_leading(&tokens, i - 1))
            && tokens.get(i + 1).map(|(_, t)| t) == Some(&Token::PathSep);
        if !starts {
            i += 1;
            continue;
        }
        let at = tokens[i].0;
        let mut found = Vec::new();
        i = path_tree(&tokens, i, Vec::new(), &mut found);
        out.extend(found.into_iter().map(|(segments, _)| (at, segments)));
    }
    out
}

/// Whether the `::` at token `at` starts a path (`::protocol::x`) and is not one inside a path. It is inside one when
/// a name stands before it (`a::b`, `crate::b`, `Self::B`), or the `>` that closes generics (`Vec<u8>::new`,
/// `<T as Tr>::X`). Anything else before it makes it a start: a keyword (`as`, `impl`, `dyn`, `return`, `in`), the
/// name of a lifetime (`&'a ::x::T`), a `->` or `=>`, or a token that ends no path (`(`, `,`, `=`, `<`, `;`, `}`).
fn is_leading(tokens: &[(usize, Token)], at: usize) -> bool {
    let Some(before) = at.checked_sub(1) else {
        return true;
    };
    let (offset, token) = &tokens[before];
    match token {
        Token::Word(word) => {
            let lifetime = before
                .checked_sub(1)
                .is_some_and(|b| tokens[b].1 == Token::Punct('\'') && tokens[b].0 + 1 == *offset);
            // Only a word that names a place can be followed by `::` inside a path
            lifetime || is_keyword(word)
        }
        // `->` and `=>` are no generics closed; any other `>` is
        Token::Punct('>') => before.checked_sub(1).is_some_and(|b| {
            matches!(tokens[b].1, Token::Punct('-' | '=')) && tokens[b].0 + 1 == *offset
        }),
        _ => true,
    }
}

/// Whether a word is a keyword of Rust (2024, strict and reserved) that names no place in a path: `crate`, `self`,
/// `super` and `Self` do, and are not in it.
fn is_keyword(word: &str) -> bool {
    matches!(
        word,
        "as" | "async"
            | "await"
            | "break"
            | "const"
            | "continue"
            | "dyn"
            | "else"
            | "enum"
            | "extern"
            | "false"
            | "fn"
            | "for"
            | "gen"
            | "if"
            | "impl"
            | "in"
            | "let"
            | "loop"
            | "match"
            | "mod"
            | "move"
            | "mut"
            | "pub"
            | "ref"
            | "return"
            | "static"
            | "struct"
            | "trait"
            | "true"
            | "type"
            | "unsafe"
            | "use"
            | "where"
            | "while"
            | "abstract"
            | "become"
            | "box"
            | "do"
            | "final"
            | "macro"
            | "override"
            | "priv"
            | "try"
            | "typeof"
            | "unsized"
            | "virtual"
            | "yield"
    )
}

/// Every name the code alone ([`code`]) binds to a path with `as`, in a `use` (`use protocol as p;`, `use a::{b as c,
/// self as d};`) or an `extern crate` (`extern crate protocol as p;`): the offset of the statement, the name, and the
/// path it renames, with a `self` that ends it dropped (`use a::{self as d}` renames `a`). `as _` binds no name. A path
/// that starts with `::` is read as if it did not.
pub fn renames(code: &str) -> Vec<(usize, String, Vec<String>)> {
    let tokens = tokens(code);
    let word = |i: usize, w: &str| matches!(tokens.get(i), Some((_, Token::Word(x))) if x == w);
    let mut out = Vec::new();
    for (i, (at, _)) in tokens.iter().enumerate() {
        let mut start = if word(i, "use") {
            i + 1
        } else if word(i, "extern") && word(i + 1, "crate") {
            i + 2
        } else {
            continue;
        };
        if tokens.get(start).map(|(_, t)| t) == Some(&Token::PathSep) {
            start += 1;
        }
        let mut found = Vec::new();
        path_tree(&tokens, start, Vec::new(), &mut found);
        for (mut segments, alias) in found {
            if segments.len() > 1 && segments.last().map(String::as_str) == Some("self") {
                segments.pop();
            }
            if let Some(name) = alias {
                out.push((*at, name, segments));
            }
        }
    }
    out
}

/// The paths of one path, or of one item of a `use` group, that starts at token `i` after `prefix`, into `out`, each
/// with the name a rename (`as x`) gives it. Where the tokens after it start.
fn path_tree(
    t: &[(usize, Token)],
    mut i: usize,
    prefix: Vec<String>,
    out: &mut Vec<(Vec<String>, Option<String>)>,
) -> usize {
    let mut segments = prefix;
    loop {
        match t.get(i).map(|(_, token)| token) {
            Some(Token::Word(word)) => {
                segments.push(word.clone());
                i += 1;
                if t.get(i).map(|(_, token)| token) != Some(&Token::PathSep) {
                    // A rename names nothing more, and binds a name unless it is `_`
                    let mut alias = None;
                    if matches!(t.get(i).map(|(_, token)| token), Some(Token::Word(w)) if w == "as")
                    {
                        if let Some((_, Token::Word(name))) = t.get(i + 1) {
                            alias = (name != "_").then(|| name.clone());
                        }
                        i += 2;
                    }
                    out.push((segments, alias));
                    return i;
                }
                i += 1;
            }
            Some(Token::Punct('*')) => {
                segments.push("*".into());
                out.push((segments, None));
                return i + 1;
            }
            Some(Token::Punct('{')) => {
                i += 1;
                loop {
                    match t.get(i).map(|(_, token)| token) {
                        None => return i,
                        Some(Token::Punct('}')) => return i + 1,
                        Some(Token::Punct(',')) => i += 1,
                        Some(_) => {
                            let before = i;
                            i = path_tree(t, i, segments.clone(), out);
                            // A token no item starts with is passed, so a group never stops the reading
                            if i == before {
                                i += 1;
                            }
                        }
                    }
                }
            }
            // `crate::` before a generic or nothing names what is read so far
            _ => {
                if !segments.is_empty() {
                    out.push((segments, None));
                }
                return i;
            }
        }
    }
}

/// Every item of the module at `module` (`["std", "env"]`) the code alone ([`code`]) names, with the offset where the
/// path starts, in order: by its whole path (`std::env::var`), in a `use` (`use std::env::var`, `use std::env::{self,
/// var}`), and through the module's last name once a `use` binds it (`use std::env;` or `use std::{env, fs}`, then
/// `env::var`). A rename (`use std::env as e`) and a path from `::` are not followed, as [`paths`] drops them.
pub fn items(code: &str, module: &[&str]) -> Vec<(usize, String)> {
    let (Some(first), Some(last)) = (module.first(), module.last()) else {
        return Vec::new();
    };
    let item = |segments: &[String], prefix: &[&str]| match segments.split_last() {
        Some((name, rest)) if rest.iter().map(String::as_str).eq(prefix.iter().copied()) => {
            Some(name.clone())
        }
        _ => None,
    };
    let mut found = Vec::new();
    let mut binds = false;
    for (at, segments) in paths(code, &[first]) {
        if segments
            .iter()
            .map(String::as_str)
            .eq(module.iter().copied())
        {
            binds = true;
        } else if let Some(name) = item(&segments, module) {
            if name == "self" {
                binds = true;
            } else {
                found.push((at, name));
            }
        }
    }
    if binds {
        found.extend(
            paths(code, &[last])
                .into_iter()
                .filter_map(|(at, segments)| item(&segments, &[last]).map(|name| (at, name))),
        );
    }
    found.sort();
    found
}

/// Every module the code alone ([`code`]) declares with a body (`mod tests { ... }`): its name, and the offsets of its
/// opening and closing braces. One left open runs to the end.
pub fn inline_modules(code: &str) -> Vec<(String, usize, usize)> {
    let tokens = tokens(code);
    let mut out = Vec::new();
    for (i, window) in tokens.windows(3).enumerate() {
        let [
            (_, Token::Word(keyword)),
            (_, Token::Word(name)),
            (open, Token::Punct('{')),
        ] = window
        else {
            continue;
        };
        if keyword != "mod" {
            continue;
        }
        let mut depth = 0;
        let mut close = code.len();
        for (at, token) in &tokens[i + 2..] {
            match token {
                Token::Punct('{') => depth += 1,
                Token::Punct('}') => {
                    depth -= 1;
                    if depth == 0 {
                        close = *at;
                        break;
                    }
                }
                _ => {}
            }
        }
        out.push((name.clone(), *open, close));
    }
    out
}

/// The offsets of every attribute named `name` in the code alone ([`code`]), outer (`#[name]`) or inner (`#![name]`).
pub fn attributes(code: &str, name: &str) -> Vec<usize> {
    let tokens = tokens(code);
    let mut out = Vec::new();
    for (i, (at, token)) in tokens.iter().enumerate() {
        if token != &Token::Punct('#') {
            continue;
        }
        let mut j = i + 1;
        if tokens.get(j).map(|(_, t)| t) == Some(&Token::Punct('!')) {
            j += 1;
        }
        if tokens.get(j).map(|(_, t)| t) == Some(&Token::Punct('['))
            && matches!(tokens.get(j + 1).map(|(_, t)| t), Some(Token::Word(w)) if w == name)
        {
            out.push(*at);
        }
    }
    out
}

/// What the scanner sets apart from the code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Comment,
    /// A string or a character literal, prefixed forms and raw strings included
    Literal,
}

/// Every comment and literal of a source, in order: its kind and its byte range.
fn spans(source: &str) -> Vec<(Kind, usize, usize)> {
    let b = source.as_bytes();
    let mut found = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let (kind, end) = match b[i] {
            b'/' if b.get(i + 1) == Some(&b'/') => {
                let end = b[i..]
                    .iter()
                    .position(|&c| c == b'\n')
                    .map_or(b.len(), |n| i + n);
                (Some(Kind::Comment), end)
            }
            b'/' if b.get(i + 1) == Some(&b'*') => (Some(Kind::Comment), block_comment_end(b, i)),
            b'"' => (Some(Kind::Literal), string_end(b, i + 1)),
            // Only the `'` itself when it starts a lifetime or a label
            b'\'' => {
                let end = quote_end(b, i);
                ((end > i + 1).then_some(Kind::Literal), end)
            }
            // An identifier, or the prefix of the literal it runs into
            c if is_identifier(c) => {
                let end = after_identifier(b, i);
                let literal = b[i..end].iter().any(|&c| c == b'"' || c == b'\'');
                (literal.then_some(Kind::Literal), end)
            }
            _ => (None, i + 1),
        };
        if let Some(kind) = kind {
            found.push((kind, i, end));
        }
        i = end;
    }
    found
}

/// Whether a byte belongs to an identifier: ASCII letters, digits, `_`, and every byte of a character beyond ASCII.
fn is_identifier(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_' || c >= 0x80
}

/// The end of the block comment that starts at `start`, nested ones inside it included.
fn block_comment_end(b: &[u8], start: usize) -> usize {
    let mut depth = 0;
    let mut i = start;
    while i + 1 < b.len() {
        match (b[i], b[i + 1]) {
            (b'/', b'*') => {
                depth += 1;
                i += 2;
            }
            (b'*', b'/') => {
                depth -= 1;
                i += 2;
                if depth == 0 {
                    return i;
                }
            }
            _ => i += 1,
        }
    }
    b.len()
}

/// The end of a string whose text starts at `i`, after its opening `"`: `\` escapes the byte after it.
fn string_end(b: &[u8], mut i: usize) -> usize {
    while i < b.len() {
        match b[i] {
            b'\\' => i += 2,
            b'"' => return i + 1,
            _ => i += 1,
        }
    }
    b.len()
}

/// The end of what a `'` at `start` opens: a character literal (`'a'`, `'é'`, `'\''`, `'\u{1F600}'`), or only the
/// `'` of a lifetime or a label.
fn quote_end(b: &[u8], start: usize) -> usize {
    let Some(&first) = b.get(start + 1) else {
        return b.len();
    };
    if first == b'\\' {
        // The escaped byte, then up to the closing quote
        let after = start + 3;
        return b[after.min(b.len())..]
            .iter()
            .position(|&c| c == b'\'')
            .map_or(b.len(), |n| after + n + 1);
    }
    // One character, whose length its first byte gives in UTF-8
    let length = match first {
        0x00..=0x7f => 1,
        0xc0..=0xdf => 2,
        0xe0..=0xef => 3,
        _ => 4,
    };
    if b.get(start + 1 + length) == Some(&b'\'') {
        start + 2 + length
    } else {
        start + 1
    }
}

/// Past the identifier that starts at `start`, or past the string or character literal it prefixes (`b"`, `r#"`,
/// `br"`, `c"`, `b'`). `r#type` is a raw identifier, not a string.
fn after_identifier(b: &[u8], start: usize) -> usize {
    let mut i = start;
    if matches!(b[i], b'b' | b'c') {
        i += 1;
    }
    if b.get(i) == Some(&b'r') {
        let hashes = b[i + 1..].iter().take_while(|&&c| c == b'#').count();
        let open = i + 1 + hashes;
        if b.get(open) == Some(&b'"') {
            return raw_string_end(b, open + 1, hashes);
        }
    } else if i > start {
        match b.get(i) {
            Some(b'"') => return string_end(b, i + 1),
            Some(b'\'') if b[start] == b'b' => return quote_end(b, i),
            _ => {}
        }
    }
    start
        + b[start..]
            .iter()
            .take_while(|&&c| is_identifier(c))
            .count()
            .max(1)
}

/// The end of a raw string whose text starts at `i`: the first `"` followed by `hashes` of `#`.
fn raw_string_end(b: &[u8], mut i: usize, hashes: usize) -> usize {
    while i < b.len() {
        if b[i] == b'"'
            && b[i + 1..]
                .iter()
                .take(hashes)
                .filter(|&&c| c == b'#')
                .count()
                == hashes
        {
            return i + 1 + hashes;
        }
        i += 1;
    }
    b.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_items_of_a_module_are_named_by_path_and_through_use() {
        let named = code(
            "\
fn a() { std::env::var(\"A\"); }
use std::env;
fn b() { env::args(); }
use std::env::{self as _, vars};
mod m { pub fn x() {} }
fn c() { m::x(); std::fs::read(\"f\"); }
",
        );
        let names: Vec<String> = items(&named, &["std", "env"])
            .into_iter()
            .map(|(_, name)| name)
            .collect();
        assert_eq!(names, ["var", "args", "vars"]);
        // Without a `use` of the module, its last name is the file's own
        assert!(
            items(
                &code("mod env { pub fn var() {} }\nfn a() { env::var(); }\n"),
                &["std", "env"]
            )
            .is_empty()
        );
    }

    fn texts(source: &str) -> Vec<String> {
        comments(source).into_iter().map(|(_, text)| text).collect()
    }

    #[test]
    fn every_comment_is_read_with_where_it_starts() {
        let source =
            "fn f() {} // one\n/// two\n//! three\n/* four /* nested */ still four */ let x = 1;\n";
        assert_eq!(
            comments(source),
            vec![
                (10, "// one".to_string()),
                (17, "/// two".to_string()),
                (25, "//! three".to_string()),
                (35, "/* four /* nested */ still four */".to_string()),
            ]
        );
    }

    #[test]
    fn what_sits_in_a_string_or_a_character_is_not_a_comment() {
        let source = r####"
let a = "// not";
let b = "a \" // not";
let c = r#"" // not "#;
let d = r##"a "# // not"##;
let e = br"// not";
let f = b"\" // not";
let g = c"// not";
let h = '"'; // one
let i = '\''; // two
let j = b'"'; // three
let k = '/'; let l = 'é'; let m = '\u{2F}'; // four
"####;
        assert_eq!(texts(source), ["// one", "// two", "// three", "// four"]);
    }

    #[test]
    fn a_lifetime_or_a_label_is_not_a_character() {
        let source = "fn f<'a>(x: &'a str) -> &'static str { 'outer: loop {} } // one\nstruct S<'b>; // two\n";
        assert_eq!(texts(source), ["// one", "// two"]);
    }

    #[test]
    fn a_prefix_inside_an_identifier_starts_no_string() {
        // `for"..."` is the keyword and a string, not a raw string `r"..."` whose escape would end it early
        let source =
            "let r#type = 1; // one\nfor\"x\\\" // not\" in y {} // two\nlet abc = 1; // three\n";
        assert_eq!(texts(source), ["// one", "// two", "// three"]);
    }

    #[test]
    fn what_is_left_open_runs_to_the_end() {
        assert_eq!(
            texts("/* open /* nested */ // not\n"),
            ["/* open /* nested */ // not\n"]
        );
        assert_eq!(texts("let s = \"open // not\n"), Vec::<String>::new());
        assert_eq!(texts("let c = '\\"), Vec::<String>::new());
    }

    #[test]
    fn the_code_alone_keeps_its_offsets_and_lines() {
        let source = "let s = \"{x} é\"; // a {\nlet c = '}'; let l: &'a str = r#\"}\"#; /* b\n */ f(b'{');\n";
        let code = code(source);
        assert_eq!(code.len(), source.len());
        assert_eq!(
            code,
            "let s =         ;       \nlet c =    ; let l: &'a str =       ;     \n    f(    );\n"
        );
    }

    fn named(source: &str) -> Vec<String> {
        paths(&code(source), &["crate", "super", "self"])
            .into_iter()
            .map(|(_, segments)| segments.join("::"))
            .collect()
    }

    #[test]
    fn every_path_from_the_crate_is_read_wherever_it_is_written() {
        let source = "use crate::outbound::db::Db;\nuse super::{self, inner::{a, b as c}, *};\n\
                      fn f() { let x = crate::outbound::Db::new(); let y: self::T = todo!(); }\n\
                      pub(crate) fn g() {}\nm!(crate::outbound::x);\nuse std::io;\n\
                      let s = \"crate::outbound\"; // crate::outbound\nlet v = crate::Repo::<u8>::new();\n";
        assert_eq!(
            named(source),
            [
                "crate::outbound::db::Db",
                "super::self",
                "super::inner::a",
                "super::inner::b",
                "super::*",
                "crate::outbound::Db::new",
                "self::T",
                "crate::outbound::x",
                "crate::Repo",
            ]
        );
        // Each with where it starts
        assert_eq!(paths(&code("use crate::a;"), &["crate"])[0].0, 4);
        // From the name of a dependency, and not from one inside a path
        assert_eq!(
            paths(
                &code("use domain::x;\nlet y = a::domain::z;\n"),
                &["domain"]
            ),
            [(4, vec!["domain".to_string(), "x".to_string()])]
        );
    }

    #[test]
    fn a_path_from_a_leading_colons_is_read_as_the_path_without_them() {
        let source = "use ::protocol::outbound::Port;\nuse ::{protocol::outbound, other::x};\n\
                      fn f(_: ::protocol::a::A) -> ::protocol::b::B { ::protocol::c(); }\n\
                      let x: &'a ::protocol::d::D = <T as ::protocol::E>::f();\n\
                      impl ::protocol::F for X { fn g() { match v { _ => ::protocol::h() } } }\n\
                      let y = Vec::<u8>::new(); let z = a::protocol::i::I; let w = <T as Tr>::protocol::j();\n\
                      let s = \"::protocol::k\"; // ::protocol::l\n";
        let read: Vec<String> = paths(&code(source), &["protocol"])
            .into_iter()
            .map(|(_, segments)| segments.join("::"))
            .collect();
        assert_eq!(
            read,
            [
                "protocol::outbound::Port",
                "protocol::outbound",
                "protocol::a::A",
                "protocol::b::B",
                "protocol::c",
                "protocol::d::D",
                "protocol::E",
                "protocol::F",
                "protocol::h",
            ]
        );
        // Where it starts is the word, not the colons
        assert_eq!(paths(&code("f(::a::b);"), &["a"])[0].0, 4);
    }

    #[test]
    fn a_name_bound_with_as_is_read_from_a_use_or_an_extern_crate_alone() {
        let source = "use a as b;\npub use ::c::{self as d, e::f as _, g::{h as i}};\nextern crate j as k;\n\
                      use {l::m as n};\nlet x = o::P as u8;\nuse q::r;\n// use s as t;\n";
        let bound: Vec<(String, String)> = renames(&code(source))
            .into_iter()
            .map(|(_, name, target)| (name, target.join("::")))
            .collect();
        let expected = [
            ("b", "a"),
            ("d", "c"),
            ("i", "c::g::h"),
            ("k", "j"),
            ("n", "l::m"),
        ];
        assert_eq!(
            bound,
            expected.map(|(name, target)| (name.to_string(), target.to_string()))
        );
    }

    #[test]
    fn a_module_with_a_body_is_read_with_its_braces() {
        let source = "mod a;\nmod tests {\n    fn f() { {} }\n}\nfn g() {}\n";
        let modules = inline_modules(&code(source));
        assert_eq!(modules.len(), 1);
        let (name, open, close) = &modules[0];
        assert_eq!(name, "tests");
        assert_eq!(&source[*open..=*open], "{");
        assert_eq!(*close, source.find("}\nfn").unwrap());
    }

    #[test]
    fn an_attribute_is_found_by_its_name_outer_or_inner() {
        let source = "#[path = \"x.rs\"]\nmod a;\n#![path = \"y\"]\n#[cfg(test)]\n// #[path]\n";
        assert_eq!(
            attributes(&code(source), "path"),
            [0, source.find("#![").unwrap()]
        );
    }

    #[test]
    fn rust_source_is_named_by_its_extension_in_any_case() {
        assert!(is_source("lib.rs") && is_source("LIB.RS"));
        assert!(!is_source("Cargo.toml") && !is_source("rs") && !is_source("x.rsx"));
    }
}
