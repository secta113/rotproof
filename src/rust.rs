//! Reading Rust source with a small scanner of Rotproof's own: its comments.
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
    let b = source.as_bytes();
    let mut found = Vec::new();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'/' if b.get(i + 1) == Some(&b'/') => {
                let end = b[i..]
                    .iter()
                    .position(|&c| c == b'\n')
                    .map_or(b.len(), |n| i + n);
                found.push((i, source[i..end].to_string()));
                i = end;
            }
            b'/' if b.get(i + 1) == Some(&b'*') => {
                let end = block_comment_end(b, i);
                found.push((i, source[i..end].to_string()));
                i = end;
            }
            b'"' => i = string_end(b, i + 1),
            b'\'' => i = quote_end(b, i),
            c if is_identifier(c) => i = after_identifier(b, i),
            _ => i += 1,
        }
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
    fn rust_source_is_named_by_its_extension_in_any_case() {
        assert!(is_source("lib.rs") && is_source("LIB.RS"));
        assert!(!is_source("Cargo.toml") && !is_source("rs") && !is_source("x.rsx"));
    }
}
