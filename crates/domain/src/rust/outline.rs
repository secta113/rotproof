//! The outline of a Rust file, read from its tokens with the scanner of `rust.rs`: every definition in it, for a
//! knowledge document that follows `src/layers.rs::Table.add` (`follows.rs` picks the pieces of a name from it).
//!
//! - **What counts:** `fn`, `struct`, `enum` and each of its variants, `union`, `trait` and each of its items, `type`,
//!   `const`, `static`, `macro_rules!` by its name, an inline `mod` with a body and the items in it, and the items of
//!   an `impl` block, exported or not. The names of a definition are the names above it: `["Table"]`,
//!   `["Table", "add"]`, `["Command", "Before"]`, `["tests", "helper"]`.
//! - **A type covers its `impl` blocks:** an `impl` block is a definition of the type it is for, so the key of a type
//!   selects its definition and every `impl` block for it in the file (`impl Table`, `impl X for Table`,
//!   `impl<T> Table<T>`), and `Type.item` the item of that name in any of them, whether the block implements a trait
//!   or not. The name of an `impl` block is the last name of the type after `for` (or after `impl`), so a block for
//!   `Vec<Table>` is a piece of `Vec`.
//! - **A definition starts at its first outer attribute,** and `///` and `/** */` doc comments count as attributes,
//!   wherever they sit before the item and its attributes; the comments before that are not part of it. It ends at its
//!   closing brace or semicolon.
//! - **Not seen:** what a macro defines, a `mod name;` without a body, an `extern` block, the fields of a struct, and
//!   whatever a function holds (an item inside a body).

use super::{Token, code, comments, tokens};
use crate::definitions::Definition;

/// Every definition of a Rust file, a container before what it holds. A name defined more than once appears once for
/// each piece.
pub fn outline(source: &str) -> Vec<Definition> {
    let code = code(source);
    let tokens = tokens(&code);
    let docs = comments(source)
        .into_iter()
        .filter(|(_, text)| is_outer_doc(text))
        .map(|(at, _)| at)
        .collect();
    let mut reader = Reader {
        tokens,
        docs,
        found: Vec::new(),
    };
    let end = reader.tokens.len();
    reader.items(0, end, 0, &mut Vec::new());
    reader.found
}

/// Whether a comment is an outer doc comment: `/// text` and `/** text */`, not `////`, `/***` or `/**/`, and not an
/// inner one (`//!`).
fn is_outer_doc(text: &str) -> bool {
    (text.starts_with("///") && !text.starts_with("////"))
        || (text.starts_with("/**") && !text.starts_with("/***") && text != "/**/")
}

struct Reader {
    tokens: Vec<(usize, Token)>,
    /// Where each outer doc comment starts
    docs: Vec<usize>,
    found: Vec<Definition>,
}

impl Reader {
    fn word(&self, i: usize) -> Option<&str> {
        match self.tokens.get(i) {
            Some((_, Token::Word(w))) => Some(w),
            _ => None,
        }
    }

    fn punct(&self, i: usize, c: char) -> bool {
        matches!(self.tokens.get(i), Some((_, Token::Punct(p))) if *p == c)
    }

    /// Where the token `i` ends.
    fn end(&self, i: usize) -> usize {
        let (at, token) = &self.tokens[i];
        at + match token {
            Token::Word(w) => w.len(),
            Token::PathSep => 2,
            Token::Punct(_) => 1,
        }
    }

    /// The index after the group that opens at token `i` (`(`, `[` or `{`), or the end when it is not closed.
    fn skip_group(&self, i: usize) -> usize {
        let mut depth = 0;
        for j in i..self.tokens.len() {
            match self.tokens[j].1 {
                Token::Punct('(' | '[' | '{') => depth += 1,
                Token::Punct(')' | ']' | '}') => {
                    depth -= 1;
                    if depth <= 0 {
                        return j + 1;
                    }
                }
                _ => {}
            }
        }
        self.tokens.len()
    }

    /// The index after the item whose head ends at token `from`: past the first `;` outside any group, or, when
    /// `brace_ends`, past the first `{ }` group outside any other (a body ends the item; a braced value does not).
    fn item_end(&self, from: usize, hi: usize, brace_ends: bool) -> usize {
        let mut j = from;
        while j < hi {
            match self.tokens[j].1 {
                Token::Punct('(' | '[') => j = self.skip_group(j),
                Token::Punct('{') => {
                    j = self.skip_group(j);
                    if brace_ends {
                        return j.min(hi);
                    }
                }
                Token::Punct(';') => return j + 1,
                _ => j += 1,
            }
        }
        hi
    }

    /// The index of the first `{` outside any group from token `from`, before any `;`: `None` for an item without a
    /// body.
    fn body(&self, from: usize, hi: usize) -> Option<usize> {
        let mut j = from;
        while j < hi {
            match self.tokens[j].1 {
                Token::Punct('(' | '[') => j = self.skip_group(j),
                Token::Punct('{') => return Some(j),
                Token::Punct(';') => return None,
                _ => j += 1,
            }
        }
        None
    }

    /// Where a definition whose first token (an attribute or its head) is at `first` starts: there, or at the first
    /// outer doc comment after `since`, whichever comes first.
    fn start(&self, first: usize, since: usize) -> usize {
        let at = self.tokens[first].0;
        self.docs
            .iter()
            .copied()
            .filter(|&doc| doc >= since && doc < at)
            .min()
            .unwrap_or(at)
    }

    fn define(&mut self, scope: &[String], name: &str, start: usize, end: usize) {
        let mut names = scope.to_vec();
        names.push(name.to_string());
        self.found.push(Definition { names, start, end });
    }

    /// The items between tokens `lo` and `hi`, under the names in `scope`; `since` is where the block opened, which
    /// the doc comments of its first item are looked for after.
    fn items(&mut self, lo: usize, hi: usize, since: usize, scope: &mut Vec<String>) {
        let mut i = lo;
        let mut since = since;
        while i < hi {
            // An inner attribute belongs to the block, not to an item
            if self.punct(i, '#') && self.punct(i + 1, '!') && self.punct(i + 2, '[') {
                i = self.skip_group(i + 2);
                continue;
            }
            let first = i;
            while self.punct(i, '#') && self.punct(i + 1, '[') {
                i = self.skip_group(i + 1);
            }
            if i >= hi {
                break;
            }
            let start = self.start(first, since);
            let done = self.item(i, hi, start, scope);
            // Always on: a token that starts no item is passed
            i = done.max(i + 1).min(hi);
            since = self.end(i - 1);
        }
    }

    /// The item whose head (visibility, qualifiers and keyword) starts at token `i`, which began at `start` with its
    /// attributes: its definitions found, and the index after it.
    fn item(&mut self, mut i: usize, hi: usize, start: usize, scope: &mut Vec<String>) -> usize {
        if self.word(i) == Some("pub") {
            i += 1;
            if self.punct(i, '(') {
                i = self.skip_group(i);
            }
        }
        // Qualifiers, which `const` is only when a `fn` or another qualifier follows it
        loop {
            match self.word(i) {
                Some("async" | "unsafe" | "safe" | "default" | "auto") => i += 1,
                Some("const")
                    if matches!(self.word(i + 1), Some("fn" | "unsafe" | "async" | "extern")) =>
                {
                    i += 1
                }
                Some("extern") if self.word(i + 1) != Some("crate") => i += 1,
                _ => break,
            }
        }
        if i >= hi {
            return hi;
        }
        let Some(keyword) = self.word(i).map(str::to_string) else {
            // A group, or a token no item starts with
            return if matches!(self.tokens[i].1, Token::Punct('(' | '[' | '{')) {
                self.skip_group(i)
            } else {
                i + 1
            };
        };
        let name = self.word(i + 1).map(str::to_string);
        match (keyword.as_str(), name) {
            ("fn", Some(name)) => {
                let end = self.item_end(i + 2, hi, true);
                self.finish(scope, &name, start, end)
            }
            ("struct" | "type", Some(name)) => {
                let end = self.item_end(i + 2, hi, keyword == "struct");
                self.finish(scope, &name, start, end)
            }
            ("const", Some(name)) => {
                let end = self.item_end(i + 2, hi, false);
                self.finish(scope, &name, start, end)
            }
            ("static", Some(name)) => {
                // `static mut NAME`
                let (name, from) = match (name.as_str(), self.word(i + 2)) {
                    ("mut", Some(real)) => (real.to_string(), i + 3),
                    _ => (name, i + 2),
                };
                let end = self.item_end(from, hi, false);
                self.finish(scope, &name, start, end)
            }
            ("union", Some(name))
                if self.punct(i + 2, '{')
                    || self.punct(i + 2, '<')
                    || self.word(i + 2) == Some("where") =>
            {
                let end = self.item_end(i + 2, hi, true);
                self.finish(scope, &name, start, end)
            }
            ("enum", Some(name)) => self.enumeration(i, hi, &name, start, scope),
            ("trait", Some(name)) => self.block(i, hi, &name, start, scope),
            ("mod", Some(name)) => match self.body(i + 2, hi) {
                Some(open) => self.block_at(open, hi, &name, start, scope),
                None => self.item_end(i + 2, hi, false),
            },
            ("impl", _) => self.implementation(i, hi, start, scope),
            ("macro_rules", _) if self.punct(i + 1, '!') => {
                let Some(name) = self.word(i + 2).map(str::to_string) else {
                    return i + 2;
                };
                let mut end = self.skip_group(i + 3).min(hi);
                // `macro_rules! m ( ... );`
                if !self.punct(i + 3, '{') && self.punct(end, ';') {
                    end += 1;
                }
                self.finish(scope, &name, start, end)
            }
            ("use", _) | ("extern", _) => self.item_end(i + 1, hi, false),
            // A macro invocation: `name! { ... }`, `name!(...);`
            (_, _) if self.punct(i + 1, '!') => {
                let mut end = self.skip_group(i + 2).min(hi);
                if self.punct(end, ';') {
                    end += 1;
                }
                end
            }
            _ => i + 1,
        }
    }

    /// Define `name` from `start` to the end of the token before `end`, and give back `end`.
    fn finish(&mut self, scope: &[String], name: &str, start: usize, end: usize) -> usize {
        let end = end.max(1);
        let to = self.end(end - 1);
        self.define(scope, name, start, to);
        end
    }

    /// A trait, or an inline module, whose head starts at token `i`: the definition, and the items in its body.
    fn block(
        &mut self,
        i: usize,
        hi: usize,
        name: &str,
        start: usize,
        scope: &mut Vec<String>,
    ) -> usize {
        match self.body(i + 2, hi) {
            Some(open) => self.block_at(open, hi, name, start, scope),
            None => {
                let end = self.item_end(i + 2, hi, false);
                self.finish(scope, name, start, end)
            }
        }
    }

    /// The body that opens at token `open`: the definition `name` from `start` to its closing brace, and the items in
    /// the body under `name`.
    fn block_at(
        &mut self,
        open: usize,
        hi: usize,
        name: &str,
        start: usize,
        scope: &mut Vec<String>,
    ) -> usize {
        let after = self.skip_group(open).min(hi);
        let close = after.saturating_sub(1).max(open + 1);
        let end = self.finish(scope, name, start, after);
        scope.push(name.to_string());
        let since = self.end(open);
        self.items(open + 1, close.min(after), since, scope);
        scope.pop();
        end
    }

    /// An enum whose keyword is at token `i`: the definition, and each variant as a definition of its own.
    fn enumeration(
        &mut self,
        i: usize,
        hi: usize,
        name: &str,
        start: usize,
        scope: &mut Vec<String>,
    ) -> usize {
        let Some(open) = self.body(i + 2, hi) else {
            let end = self.item_end(i + 2, hi, false);
            return self.finish(scope, name, start, end);
        };
        let after = self.skip_group(open).min(hi);
        let close = after.saturating_sub(1).max(open + 1);
        let end = self.finish(scope, name, start, after);
        scope.push(name.to_string());
        let mut since = self.end(open);
        let mut j = open + 1;
        while j < close {
            let first = j;
            while self.punct(j, '#') && self.punct(j + 1, '[') {
                j = self.skip_group(j + 1);
            }
            let variant = self.word(j).map(str::to_string);
            let begin = self.start(first, since);
            // To the comma that separates the variants, outside any group
            let mut k = j;
            while k < close && !self.punct(k, ',') {
                k = match self.tokens[k].1 {
                    Token::Punct('(' | '[' | '{') => self.skip_group(k),
                    _ => k + 1,
                };
            }
            let k = k.min(close);
            if let (Some(variant), true) = (variant, k > j) {
                let to = self.end(k - 1);
                self.define(scope, &variant, begin, to);
            }
            since = if k < close { self.end(k) } else { since };
            j = k + 1;
        }
        scope.pop();
        end
    }

    /// An `impl` block whose keyword is at token `i`: a piece of the type it is for, and its items under that type.
    fn implementation(
        &mut self,
        i: usize,
        hi: usize,
        start: usize,
        scope: &mut Vec<String>,
    ) -> usize {
        let Some(open) = self.body(i + 1, hi) else {
            return self.item_end(i + 1, hi, false);
        };
        let after = self.skip_group(open).min(hi);
        let close = after.saturating_sub(1).max(open + 1);
        if let Some(name) = self.target(i + 1, open) {
            let end = self.finish(scope, &name, start, after);
            scope.push(name);
            let since = self.end(open);
            self.items(open + 1, close.min(after), since, scope);
            scope.pop();
            return end;
        }
        after
    }

    /// The name of the type an `impl` block is for, from the tokens `from` to `open` (its brace): the last name of the
    /// path after `for`, or after the generics when there is no `for`. `None` for a type with no name (`[T]`,
    /// `(A, B)`).
    fn target(&self, from: usize, open: usize) -> Option<String> {
        let mut j = from;
        // Generics of the impl: `<T: Fn() -> u8>`, where `->` does not close
        if self.punct(j, '<') {
            let mut depth = 0;
            while j < open {
                if self.punct(j, '<') {
                    depth += 1;
                } else if self.punct(j, '>') && !(j > 0 && self.punct(j - 1, '-')) {
                    depth -= 1;
                    if depth == 0 {
                        j += 1;
                        break;
                    }
                }
                j += 1;
            }
        }
        // The type is after the last `for` outside brackets and angles, before `where`
        let mut at = j;
        let mut depth = 0;
        let mut k = j;
        while k < open {
            match &self.tokens[k].1 {
                Token::Word(w) if w == "where" && depth == 0 => break,
                Token::Word(w) if w == "for" && depth == 0 => at = k + 1,
                Token::Punct('<' | '(' | '[') => depth += 1,
                Token::Punct('>') if !(k > 0 && self.punct(k - 1, '-')) => depth -= 1,
                Token::Punct(')' | ']') => depth -= 1,
                _ => {}
            }
            k += 1;
        }
        // `&'a mut dyn path::Name<T>`: past what comes before the path
        let mut name = None;
        let mut j = at;
        while j < k {
            match &self.tokens[j].1 {
                Token::Word(w) if matches!(w.as_str(), "mut" | "dyn" | "const" | "unsafe") => {
                    j += 1
                }
                Token::Punct('&' | '*' | '\'' | '!') => j += 1,
                // A lifetime is a `'` and a name
                Token::Word(_) if j > 0 && self.punct(j - 1, '\'') => j += 1,
                Token::Word(w) => {
                    name = Some(w.clone());
                    j += 1;
                    // `::Name`, until what ends the path
                    while matches!(self.tokens.get(j), Some((_, Token::PathSep))) {
                        if let Some(next) = self.word(j + 1) {
                            name = Some(next.to_string());
                        }
                        j += 2;
                    }
                    break;
                }
                _ => return None,
            }
        }
        name
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::definitions::pieces;

    /// The source of every definition of that name in a file, as a key hashes it.
    fn definition(source: &str, dotted: &str) -> Vec<String> {
        pieces(source, &outline(source), dotted)
            .into_iter()
            .map(str::to_string)
            .collect()
    }

    const SOURCE: &str = r##"//! Module docs.
use std::fmt;

// A plain comment.
/// Docs of the table.
#[derive(Debug, Clone)]
pub struct Table {
    rows: Vec<u8>,
}

impl Table {
    /// Adds a row.
    pub fn add(&mut self, row: u8) {
        self.rows.push(row);
        fn inner() {}
    }
    pub const LIMIT: usize = 3;
}

impl<T> From<T> for Table<T> where T: Into<u8> {
    fn from(t: T) -> Self { todo!() }
}

impl fmt::Display for Table {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { Ok(()) }
    fn add(&self) {}
}

pub enum Command {
    /// Before.
    #[allow(dead_code)]
    Before { args: Vec<String> },
    Stop,
    Go(u8, u8) = 3,
}

trait Shape: Sized {
    type Output;
    const SIDES: u8;
    fn area(&self) -> f64;
    fn name(&self) -> &str { "shape" }
}

pub(crate) type Rows = Vec<u8>;
static mut COUNT: u8 = 0;
const NAMES: [&str; 2] = ["a;", "b"];
union Bits { a: u8, b: i8 }
macro_rules! twice { ($e:expr) => { fn nope() {} }; }
#[cfg(test)]
mod tests {
    use super::*;
    fn helper() {}
    mod nested { pub fn deep() {} }
}
mod outside;
extern "C" { fn c_fn(); }
async fn later() {}
pub const unsafe fn dangerous() {}
"##;

    fn found(dotted: &str) -> Vec<String> {
        definition(SOURCE, dotted)
    }

    #[test]
    fn a_definition_starts_at_its_first_attribute_or_doc_comment() {
        let table = found("Table");
        // The struct, then each impl block for it, in the order they appear
        assert_eq!(table.len(), 4, "{table:#?}");
        assert!(
            table[0]
                .starts_with("/// Docs of the table.\n#[derive(Debug, Clone)]\npub struct Table {")
        );
        assert!(table[0].ends_with("rows: Vec<u8>,\n}"));
        assert!(table[1].starts_with("impl Table {"));
        assert!(table[2].starts_with("impl<T> From<T> for Table<T> where T: Into<u8> {"));
        assert!(table[3].starts_with("impl fmt::Display for Table {"));
        // The plain comment above the doc comment is not part of it
        assert!(!table[0].contains("A plain comment"));
        // A method from its doc comment
        assert_eq!(
            found("Table.add")[0],
            "/// Adds a row.\n    pub fn add(&mut self, row: u8) {\n        self.rows.push(row);\n        fn inner() {}\n    }"
        );
    }

    #[test]
    fn the_outline_names_each_definition_from_the_outermost() {
        let source = "struct A;\nimpl A { fn f() {} }\nmod m { enum E { V } }\n";
        let names: Vec<Vec<String>> = outline(source).into_iter().map(|d| d.names).collect();
        assert_eq!(
            names,
            [
                vec!["A"],
                // An `impl` block is a definition of its type, and its items are under the type
                vec!["A"],
                vec!["A", "f"],
                vec!["m"],
                vec!["m", "E"],
                vec!["m", "E", "V"],
            ]
        );
    }

    #[test]
    fn an_item_of_a_type_is_found_in_every_impl_block_of_it_trait_impls_too() {
        let add = found("Table.add");
        assert_eq!(add.len(), 2, "{add:#?}");
        assert!(add[1].starts_with("fn add(&self) {}"));
        assert_eq!(found("Table.fmt").len(), 1);
        assert_eq!(found("Table.from").len(), 1);
        assert_eq!(found("Table.LIMIT"), ["pub const LIMIT: usize = 3;"]);
        // What a function holds is not seen
        assert_eq!(found("Table.inner"), Vec::<String>::new());
        assert_eq!(found("inner"), Vec::<String>::new());
    }

    #[test]
    fn an_enum_variant_is_a_definition_with_its_attributes_and_docs() {
        assert_eq!(
            found("Command.Before"),
            ["/// Before.\n    #[allow(dead_code)]\n    Before { args: Vec<String> }"]
        );
        assert_eq!(found("Command.Stop"), ["Stop"]);
        assert_eq!(found("Command.Go"), ["Go(u8, u8) = 3"]);
        assert!(found("Command")[0].starts_with("pub enum Command {"));
        assert!(found("Command")[0].ends_with("Go(u8, u8) = 3,\n}"));
    }

    #[test]
    fn a_trait_holds_its_items_and_the_other_kinds_of_item_are_named() {
        assert!(found("Shape")[0].starts_with("trait Shape: Sized {"));
        assert_eq!(found("Shape.Output"), ["type Output;"]);
        assert_eq!(found("Shape.SIDES"), ["const SIDES: u8;"]);
        assert_eq!(found("Shape.area"), ["fn area(&self) -> f64;"]);
        assert_eq!(
            found("Shape.name"),
            ["fn name(&self) -> &str { \"shape\" }"]
        );
        assert_eq!(found("Rows"), ["pub(crate) type Rows = Vec<u8>;"]);
        assert_eq!(found("COUNT"), ["static mut COUNT: u8 = 0;"]);
        assert_eq!(
            found("NAMES"),
            ["const NAMES: [&str; 2] = [\"a;\", \"b\"];"]
        );
        assert_eq!(found("Bits"), ["union Bits { a: u8, b: i8 }"]);
        assert_eq!(found("later"), ["async fn later() {}"]);
        assert_eq!(found("dangerous"), ["pub const unsafe fn dangerous() {}"]);
    }

    #[test]
    fn a_macro_is_named_and_what_it_defines_is_not_seen() {
        assert_eq!(
            found("twice"),
            ["macro_rules! twice { ($e:expr) => { fn nope() {} }; }"]
        );
        assert_eq!(found("nope"), Vec::<String>::new());
    }

    #[test]
    fn an_inline_module_holds_its_items_under_its_name_and_a_bodiless_one_is_nothing() {
        let tests = found("tests");
        assert_eq!(tests.len(), 1);
        assert!(tests[0].starts_with("#[cfg(test)]\nmod tests {"));
        assert_eq!(found("tests.helper"), ["fn helper() {}"]);
        assert_eq!(found("tests.nested.deep"), ["pub fn deep() {}"]);
        assert_eq!(found("tests.nested").len(), 1);
        assert_eq!(found("helper"), Vec::<String>::new());
        assert_eq!(found("outside"), Vec::<String>::new());
        assert_eq!(found("c_fn"), Vec::<String>::new());
    }

    #[test]
    fn a_name_in_a_string_or_a_comment_is_not_a_definition() {
        let source = "// fn ghost() {}\nconst S: &str = \"fn ghost() {}\";\n/* struct Ghost; */\n\
                      const R: &str = r#\"fn ghost() {} \" struct Ghost;\"#;\nfn real() {}\n";
        assert_eq!(definition(source, "ghost"), Vec::<String>::new());
        assert_eq!(definition(source, "Ghost"), Vec::<String>::new());
        assert_eq!(definition(source, "real"), ["fn real() {}"]);
        assert_eq!(definition(source, "S").len(), 1);
    }

    #[test]
    fn an_impl_block_belongs_to_the_last_name_of_the_type_it_is_for() {
        let source = "struct A;\nimpl<'a, F: Fn() -> u8> Tr<F> for &'a mut m::A<F> where F: Copy { fn x() {} }\n\
                      impl Tr for [u8] { fn y() {} }\nimpl Tr for Vec<A> { fn z() {} }\n";
        assert_eq!(definition(source, "A.x").len(), 1);
        assert_eq!(definition(source, "A").len(), 2);
        assert_eq!(definition(source, "y"), Vec::<String>::new());
        assert_eq!(definition(source, "Vec.z"), ["fn z() {}"]);
    }

    #[test]
    fn doc_comments_between_attributes_and_block_ones_count() {
        let source = "fn a() {}\n/** Block docs. */\n#[inline]\n/// More.\nfn b() {}\n//// not docs\nfn c() {}\n";
        assert_eq!(
            definition(source, "b"),
            ["/** Block docs. */\n#[inline]\n/// More.\nfn b() {}"]
        );
        assert_eq!(definition(source, "c"), ["fn c() {}"]);
        assert_eq!(definition(source, "a"), ["fn a() {}"]);
    }

    #[test]
    fn a_file_that_is_cut_short_is_read_as_far_as_it_goes() {
        assert_eq!(definition("fn a() {}\nfn b() {", "a"), ["fn a() {}"]);
        assert_eq!(definition("fn a() {}\nfn b() {", "b"), ["fn b() {"]);
        assert_eq!(definition("struct", "x"), Vec::<String>::new());
        assert_eq!(definition("", "x"), Vec::<String>::new());
    }
}
