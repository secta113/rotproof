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
/// in the tokens of a macro alike. A `use` group is spread into a path per item (`crate::{a, b::c}` is `crate::a` and
/// `crate::b::c`), `self` in a group stays a segment of its own, a glob ends in `*`, and a rename (`as x`) is dropped.
pub fn paths(code: &str, starts: &[&str]) -> Vec<(usize, Vec<String>)> {
    let tokens = tokens(code);
    let mut out = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        let starts = matches!(&tokens[i].1, Token::Word(w) if starts.contains(&w.as_str()))
            && (i == 0 || tokens[i - 1].1 != Token::PathSep)
            && tokens.get(i + 1).map(|(_, t)| t) == Some(&Token::PathSep);
        if !starts {
            i += 1;
            continue;
        }
        let at = tokens[i].0;
        let mut found = Vec::new();
        i = path_tree(&tokens, i, Vec::new(), &mut found);
        out.extend(found.into_iter().map(|segments| (at, segments)));
    }
    out
}

/// The paths of one path, or of one item of a `use` group, that starts at token `i` after `prefix`, into `out`.
/// Where the tokens after it start.
fn path_tree(
    t: &[(usize, Token)],
    mut i: usize,
    prefix: Vec<String>,
    out: &mut Vec<Vec<String>>,
) -> usize {
    let mut segments = prefix;
    loop {
        match t.get(i).map(|(_, token)| token) {
            Some(Token::Word(word)) => {
                segments.push(word.clone());
                i += 1;
                if t.get(i).map(|(_, token)| token) != Some(&Token::PathSep) {
                    out.push(segments);
                    // A rename names nothing more
                    if matches!(t.get(i).map(|(_, token)| token), Some(Token::Word(w)) if w == "as")
                    {
                        i += 2;
                    }
                    return i;
                }
                i += 1;
            }
            Some(Token::Punct('*')) => {
                segments.push("*".into());
                out.push(segments);
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
                    out.push(segments);
                }
                return i;
            }
        }
    }
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
