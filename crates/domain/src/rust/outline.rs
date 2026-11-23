//! The outline of a Rust file, read from its tokens with the scanner of `rust.rs`: every definition in it, for a
//! knowledge document that follows `src/layers.rs::Table.add` (`follows.rs` picks the pieces of a name from it).
//!
//! - **What counts:** `fn`, `struct`, `enum` and each of its variants, `union`, `trait` and each of its items, `type`,
//!   `const`, `static`, `macro_rules!` by its name, an inline `mod` with a body and the items in it, and the items of
//!   an `impl` block, exported or not. The names of a definition are the names above it: `["Table"]`,
//!   `["Table", "add"]`, `["Command", "Before"]`, `["tests", "helper"]`. A raw identifier is named without its `r#`
//!   (`pub fn r#match` is `match`, as `r#match` and `match` are one name to Rust), so a key never carries it.
//! - **A type covers its `impl` blocks:** an `impl` block is a definition of the type it is for, so the key of a type
//!   selects its definition and every `impl` block for it in the file (`impl Table`, `impl X for Table`,
//!   `impl<T> Table<T>`), and `Type.item` the item of that name in any of them, whether the block implements a trait
//!   or not. The name of an `impl` block is the last name of the type after `for` (or after `impl`), so a block for
//!   `Vec<Table>` is a piece of `Vec`.
//! - **A shape beyond the kind,** for the rule on `protocol.inbound`: an `enum` that is a vocabulary (no generic
//!   parameters, at least one variant, and no variant with fields, a discriminant allowed), a `trait`, and an `impl`
//!   block, which gives the paths it is written with: the type's (`crate::song::Kind`) and the trait's (`fmt::Display`
//!   for `impl fmt::Display for T`, `Sync` for `impl !Sync for T`), without generic arguments, nothing for the trait
//!   when the block is inherent, and whether the type is a parameter of the block (`impl<T: Bound> Rank for T`).
//!   References and pointers before the type are passed (`&'a mut m::Kind` is `m::Kind`). Everything else, the items
//!   and variants too, is plain. The outline reads one file: the rule that refuses an enum with an `impl` block
//!   reads the outline of every file of the crate, and resolves the paths there.
//! - **The `impl` blocks the outline does not see are given by [`nested_impls`],** for the rule on vocabularies alone:
//!   those in the body of a function or a method (in any block, closure or `if` inside it), and in the initializer of a
//!   `const` or a `static` (`const _: () = { ... };`), as that rule needs every block that puts a method on a type, and
//!   the traits declared there, which such a block may implement. The items there are read as items are: a body is
//!   walked token by token, and an `impl` in it starts an item, as the
//!   `impl Trait` of a type stands in a signature, which the reading of the function has passed. The blocks are named
//!   as those of the outline are, by the module they are in, but [`outline`] never holds them, so a key of `follows`
//!   selects the same text whether or not such a block exists. The arguments of a macro call are no code and are not
//!   walked; what a macro writes from pieces stays unseen, and a rule that needs to know of an `impl` in a call reads
//!   the call (`rust::calls_holding`).
//! - **A definition starts at its first outer attribute,** and `///` and `/** */` doc comments count as attributes,
//!   wherever they sit before the item and its attributes; the comments before that are not part of it. It ends at its
//!   closing brace or semicolon, the first one outside any group: the `< >` of generics are a group too, so the `{`
//!   of a const argument or default (`Table<{ 3 }>`, `<const N: usize = { 3 }>`) is not its body. In a head every
//!   `<` opens generics, as an expression there is only inside braces, and a `>` closes them unless it ends `->`.
//! - **Not seen:** what a macro defines, a `mod name;` without a body, an `extern` block, the fields of a struct, and
//!   whatever a function holds (an item inside a body).

use super::{Token, code, comments, tokens};
use crate::definitions::{Definition, Impl, Kind, Shape};

/// Every definition of a Rust file, a container before what it holds. A name defined more than once appears once for
/// each piece.
pub fn outline(source: &str) -> Vec<Definition> {
    read(source).0
}

/// The `impl` blocks of a Rust file that [`outline`] does not hold, each with the items in it, as the outline gives
/// them: those written in the body of a function or a method, and in the initializer of a `const` or a `static`, in
/// the order they are written, and the traits declared there, which such a block may implement. What a key of
/// `follows` selects never comes from here.
pub fn nested_impls(source: &str) -> Vec<Definition> {
    read(source).1
}

/// The outline of the file, and the `impl` blocks it leaves out.
fn read(source: &str) -> (Vec<Definition>, Vec<Definition>) {
    let code = code(source);
    let tokens = raw_identifiers(tokens(&code));
    let docs = comments(source)
        .into_iter()
        .filter(|(_, text)| is_outer_doc(text))
        .map(|(at, _)| at)
        .collect();
    let mut reader = Reader {
        tokens,
        docs,
        found: Vec::new(),
        associated: false,
        chain: Vec::new(),
        nested: Vec::new(),
    };
    let end = reader.tokens.len();
    reader.items(0, end, 0, &mut Vec::new());
    // The blocks were found where the walk met them, an inner body before the one around it
    reader.nested.sort_by_key(|definition| definition.start);
    (reader.found, reader.nested)
}

/// The tokens with each raw identifier (`r#match`: an `r`, a `#` and a word, written together) made one word that keeps
/// its `r#`, so that it is neither a keyword (`r#fn` starts no function) nor a word the scanner cut in three.
fn raw_identifiers(tokens: Vec<(usize, Token)>) -> Vec<(usize, Token)> {
    let mut out = Vec::with_capacity(tokens.len());
    let mut i = 0;
    while i < tokens.len() {
        if let [
            (at, Token::Word(r)),
            (hash, Token::Punct('#')),
            (word, Token::Word(name)),
        ] = &tokens[i..tokens.len().min(i + 3)]
            && r == "r"
            && at + 1 == *hash
            && hash + 1 == *word
        {
            out.push((*at, Token::Word(format!("r#{name}"))));
            i += 3;
        } else {
            out.push(tokens[i].clone());
            i += 1;
        }
    }
    out
}

/// A name as a key writes it: without the `r#` of a raw identifier, which is the same name (`r#match` is `match`).
fn plain(name: &str) -> &str {
    name.strip_prefix("r#").unwrap_or(name)
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
    /// Whether the items being read are those of an `impl` block or a trait, where a `type` is an associated type
    /// and no type of the program
    associated: bool,
    /// The names of the modules with a body that the reading is in, from the outermost: what the items found in a
    /// body are named under, as a body is no module
    chain: Vec<String>,
    /// The `impl` blocks found in bodies and initializers, with their items
    nested: Vec<Definition>,
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

    /// Whether the `>` at token `j` is the end of `->`, which closes no angle group.
    fn arrow(&self, j: usize) -> bool {
        j > 0 && self.punct(j - 1, '-') && self.tokens[j - 1].0 + 1 == self.tokens[j].0
    }

    /// The index after the item whose head ends at token `from`: past the first `;` outside any group, or, when
    /// `brace_ends`, past the first `{ }` group outside any other (a body ends the item; a braced value does not).
    /// The `< >` of generics are a group too: a `{` between them is a const argument or a default, not a body.
    fn item_end(&self, from: usize, hi: usize, brace_ends: bool) -> usize {
        let mut j = from;
        let mut angles = 0usize;
        while j < hi {
            match self.tokens[j].1 {
                Token::Punct('(' | '[') => j = self.skip_group(j),
                Token::Punct('<') => {
                    angles += 1;
                    j += 1;
                }
                Token::Punct('>') if !self.arrow(j) => {
                    angles = angles.saturating_sub(1);
                    j += 1;
                }
                Token::Punct('{') => {
                    j = self.skip_group(j);
                    if brace_ends && angles == 0 {
                        return j.min(hi);
                    }
                }
                Token::Punct(';') => return j + 1,
                _ => j += 1,
            }
        }
        hi
    }

    /// The index of the first `{` outside any group (the `< >` of generics included) from token `from`, before any
    /// `;`: `None` for an item without a body.
    fn body(&self, from: usize, hi: usize) -> Option<usize> {
        let mut j = from;
        let mut angles = 0usize;
        while j < hi {
            match self.tokens[j].1 {
                Token::Punct('(' | '[') => j = self.skip_group(j),
                Token::Punct('<') => {
                    angles += 1;
                    j += 1;
                }
                Token::Punct('>') if !self.arrow(j) => {
                    angles = angles.saturating_sub(1);
                    j += 1;
                }
                Token::Punct('{') if angles > 0 => j = self.skip_group(j),
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

    fn define(&mut self, scope: &[String], name: &str, kind: Kind, start: usize, end: usize) {
        let mut names = scope.to_vec();
        names.push(name.to_string());
        self.found.push(Definition {
            names,
            start,
            end,
            kind,
            shape: Shape::Plain,
        });
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
        let name = self.word(i + 1).map(|w| plain(w).to_string());
        match (keyword.as_str(), name) {
            ("fn", Some(name)) => {
                let end = self.item_end(i + 2, hi, true);
                // A signature in a trait has no body, and runs nothing
                let kind = if self.punct(end.saturating_sub(1), ';') {
                    Kind::Other
                } else {
                    Kind::Function
                };
                let open = self.body(i + 2, hi);
                let done = self.finish(scope, &name, kind, start, end);
                match open {
                    Some(open) if kind == Kind::Function => {
                        self.enter(open + 1, end.saturating_sub(1))
                    }
                    _ => {}
                }
                done
            }
            ("struct" | "type", Some(name)) => {
                let end = self.item_end(i + 2, hi, keyword == "struct");
                let kind = if keyword == "type" && self.associated {
                    Kind::Other
                } else {
                    Kind::Type
                };
                self.finish(scope, &name, kind, start, end)
            }
            ("const", Some(name)) => {
                let end = self.item_end(i + 2, hi, false);
                let done = self.finish(scope, &name, Kind::Other, start, end);
                self.enter(i + 2, end);
                done
            }
            ("static", Some(name)) => {
                // `static mut NAME`
                let (name, from) = match (self.word(i + 1), self.word(i + 2)) {
                    (Some("mut"), Some(real)) => (plain(real).to_string(), i + 3),
                    _ => (name, i + 2),
                };
                let end = self.item_end(from, hi, false);
                let done = self.finish(scope, &name, Kind::Other, start, end);
                self.enter(from, end);
                done
            }
            ("union", Some(name))
                if self.punct(i + 2, '{')
                    || self.punct(i + 2, '<')
                    || self.word(i + 2) == Some("where") =>
            {
                let end = self.item_end(i + 2, hi, true);
                self.finish(scope, &name, Kind::Type, start, end)
            }
            ("enum", Some(name)) => self.enumeration(i, hi, &name, start, scope),
            ("trait", Some(name)) => {
                // `block` defines the trait before anything in it, whether it has a body or not
                let at = self.found.len();
                let end = self.block(i, hi, &name, Kind::Type, start, scope);
                self.found[at].shape = Shape::Trait;
                end
            }
            ("mod", Some(name)) => match self.body(i + 2, hi) {
                Some(open) => {
                    self.chain.push(name.clone());
                    let end = self.block_at(open, hi, &name, Kind::Other, start, scope);
                    self.chain.pop();
                    end
                }
                None => self.item_end(i + 2, hi, false),
            },
            ("impl", _) => self.implementation(i, hi, start, scope),
            ("macro_rules", _) if self.punct(i + 1, '!') => {
                let Some(name) = self.word(i + 2).map(|w| plain(w).to_string()) else {
                    return i + 2;
                };
                let mut end = self.skip_group(i + 3).min(hi);
                // `macro_rules! m ( ... );`
                if !self.punct(i + 3, '{') && self.punct(end, ';') {
                    end += 1;
                }
                self.finish(scope, &name, Kind::Other, start, end)
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
    fn finish(
        &mut self,
        scope: &[String],
        name: &str,
        kind: Kind,
        start: usize,
        end: usize,
    ) -> usize {
        let end = end.max(1);
        let to = self.end(end - 1);
        self.define(scope, name, kind, start, to);
        end
    }

    /// A trait, or an inline module, whose head starts at token `i`: the definition, and the items in its body.
    fn block(
        &mut self,
        i: usize,
        hi: usize,
        name: &str,
        kind: Kind,
        start: usize,
        scope: &mut Vec<String>,
    ) -> usize {
        match self.body(i + 2, hi) {
            Some(open) => self.block_at(open, hi, name, kind, start, scope),
            None => {
                let end = self.item_end(i + 2, hi, false);
                self.finish(scope, name, kind, start, end)
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
        kind: Kind,
        start: usize,
        scope: &mut Vec<String>,
    ) -> usize {
        let after = self.skip_group(open).min(hi);
        let close = after.saturating_sub(1).max(open + 1);
        let end = self.finish(scope, name, kind, start, after);
        scope.push(name.to_string());
        let since = self.end(open);
        // The items of a trait are its associated items, as those of an `impl` block
        let outer = std::mem::replace(&mut self.associated, kind == Kind::Type);
        self.items(open + 1, close.min(after), since, scope);
        self.associated = outer;
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
            return self.finish(scope, name, Kind::Type, start, end);
        };
        let after = self.skip_group(open).min(hi);
        let close = after.saturating_sub(1).max(open + 1);
        let at = self.found.len();
        let end = self.finish(scope, name, Kind::Type, start, after);
        scope.push(name.to_string());
        // A vocabulary has no generics and no variant with fields: it is only a list of names
        let mut vocabulary = !self.punct(i + 2, '<');
        let mut variants = 0;
        let mut since = self.end(open);
        let mut j = open + 1;
        while j < close {
            let first = j;
            while self.punct(j, '#') && self.punct(j + 1, '[') {
                j = self.skip_group(j + 1);
            }
            let variant = self.word(j).map(|w| plain(w).to_string());
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
                self.define(scope, &variant, Kind::Other, begin, to);
                variants += 1;
                // A group right after the name holds fields; `= 1` is a discriminant, which holds none
                vocabulary &= !matches!(self.tokens.get(j + 1), Some((_, Token::Punct('(' | '{'))));
            }
            since = if k < close { self.end(k) } else { since };
            j = k + 1;
        }
        scope.pop();
        if vocabulary && variants > 0 {
            self.found[at].shape = Shape::Vocabulary;
        }
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
        let head = self.head(i + 1, open);
        if let Some(target) = head.target {
            let name = target.last().cloned().unwrap_or_default();
            let blanket = target.len() == 1 && head.params.contains(&name);
            let shape = Shape::Implementation(Impl {
                target,
                of: head.of,
                blanket,
            });
            let at = self.found.len();
            let end = self.finish(scope, &name, Kind::Other, start, after);
            self.found[at].shape = shape;
            scope.push(name);
            let since = self.end(open);
            let outer = std::mem::replace(&mut self.associated, true);
            self.items(open + 1, close.min(after), since, scope);
            self.associated = outer;
            scope.pop();
            return end;
        }
        after
    }

    /// What the head of an `impl` block says, from the tokens `from` (after its keyword) to `open` (its brace), read
    /// in one walk: the type parameters it declares (`T` and `U` of `<'a, T: Tr<A, B>, const N: usize, U>`, `->` and a
    /// group holding no angle of them), the path of the trait (the path before the first `for` outside groups and
    /// angles, and before `where`: `fmt::Display` for `fmt::Display`, `From` for `From<T>`, none for an inherent
    /// block; a second `for` would be that of a type, `for<'a> fn(&'a u8)`), and the path of the type (the path after
    /// the last such `for`, or after the generics when there is none), each without generic arguments. The type has
    /// no path for `[T]` and `(A, B)`.
    fn head(&self, from: usize, open: usize) -> Head {
        let mut params = Vec::new();
        let mut j = from;
        if self.punct(j, '<') {
            let mut depth = 0usize;
            while j < open {
                match &self.tokens[j].1 {
                    // A group holds no angle of the generics: `{ 1 < 2 }`
                    Token::Punct('(' | '[' | '{') => {
                        j = self.skip_group(j).min(open);
                        continue;
                    }
                    Token::Punct('<') => depth += 1,
                    Token::Punct('>') if !self.arrow(j) => {
                        depth = depth.saturating_sub(1);
                        if depth == 0 {
                            j += 1;
                            break;
                        }
                    }
                    Token::Word(w)
                        if depth == 1
                            && w != "const"
                            && (self.punct(j - 1, '<') || self.punct(j - 1, ',')) =>
                    {
                        params.push(plain(w).to_string());
                    }
                    _ => {}
                }
                j += 1;
            }
        }
        let after = j;
        let (mut first, mut last) = (None, None);
        let mut depth = 0usize;
        let mut k = after;
        while k < open {
            match &self.tokens[k].1 {
                Token::Word(w) if w == "where" && depth == 0 => break,
                Token::Word(w) if w == "for" && depth == 0 => {
                    first.get_or_insert(k);
                    last = Some(k);
                }
                Token::Punct('(' | '[' | '{') => {
                    k = self.skip_group(k).min(open);
                    continue;
                }
                Token::Punct('<') => depth += 1,
                Token::Punct('>') if !self.arrow(k) => depth = depth.saturating_sub(1),
                _ => {}
            }
            k += 1;
        }
        Head {
            params,
            of: first.and_then(|at| self.path_at(after, at, true)),
            // A leading `::` names another crate, and the outline has never held a block for such a type
            target: self.path_at(last.map_or(after, |at| at + 1), k, false),
        }
    }

    /// The path of the type that starts at token `j`, before token `end` ([`super::type_path`]), as a key writes its
    /// names: without the `r#` of a raw identifier.
    fn path_at(&self, j: usize, end: usize, leading: bool) -> Option<Vec<String>> {
        super::type_path(&self.tokens, j, end, leading)
            .map(|path| path.iter().map(|name| plain(name).to_string()).collect())
    }

    /// The items inside the tokens `lo` to `hi` of a body or an initializer, which the items of a module do not
    /// reach: their `impl` blocks and traits, with the items in them, go to `nested` (a trait declared there is what a
    /// block there may implement), and nothing else is kept, so what the outline holds does not change. They are named
    /// under the modules around them, not under a type or a function.
    fn enter(&mut self, lo: usize, hi: usize) {
        if lo >= hi {
            return;
        }
        let mark = self.found.len();
        let outer = std::mem::replace(&mut self.associated, false);
        let mut scope = self.chain.clone();
        self.walk(lo, hi.min(self.tokens.len()), &mut scope);
        self.associated = outer;
        let found = self.found.split_off(mark);
        let blocks: Vec<(usize, usize)> = found
            .iter()
            .filter(|d| matches!(d.shape, Shape::Implementation(_) | Shape::Trait))
            .map(|d| (d.start, d.end))
            .collect();
        self.nested.extend(found.into_iter().filter(|d| {
            matches!(d.shape, Shape::Implementation(_) | Shape::Trait)
                || blocks.iter().any(|(s, e)| *s <= d.start && d.end <= *e)
        }));
    }

    /// The tokens `lo` to `hi` of a body, read from the start to the end: every group is entered, and an item
    /// starts where a word that starts one stands, and is read as any item is (a function enters its own body). A
    /// macro call is passed whole, as its arguments cannot be read as code. Nothing is guessed from the token before
    /// an `impl`: in a body it only starts an item, since the `impl Trait` of a type stands in a signature, which the
    /// reading of the function has passed. A word that only looks like the start of an item (the `static` of
    /// `&'static str`, the `const` of `*const T`) makes a definition that is dropped, and its tokens are walked as
    /// the initializer of a `const` or a `static` is, so a block in them is found all the same.
    fn walk(&mut self, lo: usize, hi: usize, scope: &mut Vec<String>) {
        let mut i = lo;
        while i < hi {
            let Some(word) = self.word(i).map(str::to_string) else {
                i += 1;
                continue;
            };
            let call = self.punct(i + 1, '!')
                && matches!(
                    self.tokens.get(i + 2),
                    Some((_, Token::Punct('(' | '[' | '{')))
                );
            if call && word != "macro_rules" {
                i = self.skip_group(i + 2).min(hi);
                continue;
            }
            let starts = matches!(
                word.as_str(),
                "impl"
                    | "fn"
                    | "mod"
                    | "trait"
                    | "struct"
                    | "enum"
                    | "union"
                    | "type"
                    | "const"
                    | "static"
                    | "macro_rules"
            );
            if !starts {
                i += 1;
                continue;
            }
            // `unsafe impl` and `default impl` start at their qualifier
            let mut first = i;
            while word == "impl"
                && first > lo
                && matches!(self.word(first - 1), Some("unsafe" | "default"))
            {
                first -= 1;
            }
            let start = self.tokens[first].0;
            let done = self.item(i, hi, start, scope);
            i = done.max(i + 1).min(hi);
        }
    }
}

/// What the head of an `impl` block says ([`Reader::head`]).
struct Head {
    /// The names of the type parameters it declares
    params: Vec<String>,
    /// The path of the trait it implements, or none for an inherent block
    of: Option<Vec<String>>,
    /// The path of the type it is for, or none for a type with no path
    target: Option<Vec<String>>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::definitions::{pieces, selected};

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
    fn a_brace_inside_generics_is_a_const_argument_and_not_the_body() {
        let source = "pub struct Before<const N: usize = { 3 }> { pub items: [u8; N] }\n\
                      pub fn takes<const M: usize = { 2 + { 1 } }>() -> usize { M }\n\
                      impl Foo<{ 3 }> { pub fn inside() -> u8 { 1 } }\n\
                      pub fn ret() -> Foo<{ 2 }> { Foo }\n\
                      pub enum E<const N: usize = { 1 }> { A, B }\n\
                      pub trait T<const N: usize = { 1 }>: Sized { fn t(&self); }\n\
                      pub union U<const N: usize = { 1 }> { a: [u8; N] }\n\
                      mod m { pub fn after() {} }\n";
        let at = |name: &str| definition(source, name);
        assert_eq!(
            at("Before"),
            ["pub struct Before<const N: usize = { 3 }> { pub items: [u8; N] }"]
        );
        assert_eq!(
            at("takes"),
            ["pub fn takes<const M: usize = { 2 + { 1 } }>() -> usize { M }"]
        );
        assert_eq!(at("Foo").len(), 1);
        assert_eq!(at("Foo.inside"), ["pub fn inside() -> u8 { 1 }"]);
        assert_eq!(at("inside"), Vec::<String>::new());
        assert_eq!(at("ret"), ["pub fn ret() -> Foo<{ 2 }> { Foo }"]);
        assert_eq!(at("E")[0], "pub enum E<const N: usize = { 1 }> { A, B }");
        assert_eq!(at("E.B"), ["B"]);
        assert_eq!(at("T.t"), ["fn t(&self);"]);
        assert_eq!(
            at("U"),
            ["pub union U<const N: usize = { 1 }> { a: [u8; N] }"]
        );
        assert_eq!(at("m.after"), ["pub fn after() {}"]);
    }

    #[test]
    fn what_looks_like_an_angle_but_is_not_one_does_not_hide_the_body() {
        // `->` and `=>` close nothing, a comparison sits inside braces, and a head ends in `>` before its body
        let source = "fn a<F: Fn(u8) -> u8>(f: F) -> Vec<u8> { f(1); vec![] }\n\
                      fn b() -> impl Fn(u8) -> u8 { |x| x }\n\
                      fn c<const N: usize>() -> [u8; { N }] where [(); { N > 1 } as usize]: { [0; N] }\n\
                      fn d() where for<'a> Box<dyn Fn(&'a u8) -> &'a u8>: Sized { }\n\
                      fn e<T: Iterator<Item = Vec<u8>>>(t: T) -> Option<Box<dyn Fn() -> u8>> { None }\n\
                      impl<T: Fn() -> u8> Tr<{ 1 << 2 }> for W<T, { 1 < 2 }> where T: Copy { fn f() {} }\n\
                      impl<T> Q for m::Y<[u8; { 1 < 2 } as usize], T> { fn g() {} }\n\
                      fn turbo() { let _ = x::<{ 1 }>(); }\n\
                      struct P<const N: usize = 3>(u8);\n\
                      struct R<'a, T = [u8; { 1 }]> where T: 'a { r: &'a T }\n\
                      fn z() {}\n";
        let at = |name: &str| definition(source, name);
        assert_eq!(
            at("a"),
            ["fn a<F: Fn(u8) -> u8>(f: F) -> Vec<u8> { f(1); vec![] }"]
        );
        assert_eq!(at("b"), ["fn b() -> impl Fn(u8) -> u8 { |x| x }"]);
        assert!(at("c")[0].ends_with("{ [0; N] }"), "{:?}", at("c"));
        assert_eq!(
            at("d"),
            ["fn d() where for<'a> Box<dyn Fn(&'a u8) -> &'a u8>: Sized { }"]
        );
        assert!(at("e")[0].ends_with("None }"), "{:?}", at("e"));
        assert_eq!(at("W.f"), ["fn f() {}"]);
        assert_eq!(at("Y.g"), ["fn g() {}"]);
        assert_eq!(at("turbo"), ["fn turbo() { let _ = x::<{ 1 }>(); }"]);
        assert_eq!(at("P"), ["struct P<const N: usize = 3>(u8);"]);
        assert_eq!(
            at("R"),
            ["struct R<'a, T = [u8; { 1 }]> where T: 'a { r: &'a T }"]
        );
        assert_eq!(at("z"), ["fn z() {}"]);
    }

    #[test]
    fn a_function_and_a_method_are_functions_and_a_struct_enum_union_trait_or_alias_is_a_type() {
        let source = r#"
const C: u8 = 1;
static S: u8 = 2;
fn f() {}
struct A;
enum E { V }
union U { a: u8 }
type Alias = u8;
trait T { fn sig(&self); fn body(&self) {} type Assoc; const K: u8; }
impl A { fn m(&self) {} type Out = u8; const Z: u8 = 1; }
mod m { pub fn inner() {} }
macro_rules! mac { () => {} }
"#;
        let kinds: Vec<(String, Kind)> = outline(source)
            .into_iter()
            .map(|d| (d.names.join("."), d.kind))
            .collect();
        let kind = |name: &str, nth: usize| {
            kinds
                .iter()
                .filter(|(n, _)| n == name)
                .nth(nth)
                .unwrap_or_else(|| panic!("{name}: {kinds:?}"))
                .1
        };
        for function in ["f", "T.body", "A.m", "m.inner"] {
            assert_eq!(kind(function, 0), Kind::Function, "{function}");
        }
        for ty in ["A", "E", "U", "Alias", "T"] {
            assert_eq!(kind(ty, 0), Kind::Type, "{ty}");
        }
        // The `impl` block is a piece of `A` that is no type of its own, and an associated type is no type
        assert_eq!(kind("A", 1), Kind::Other);
        for other in [
            "C", "S", "E.V", "T.sig", "T.Assoc", "T.K", "A.Out", "A.Z", "m", "mac",
        ] {
            assert_eq!(kind(other, 0), Kind::Other, "{other}");
        }
    }

    #[test]
    fn a_raw_identifier_is_named_without_its_r_hash() {
        let source = "pub fn r#match() {}\npub struct r#type;\nimpl r#type { pub fn r#fn(&self) {} }\n\
                      impl Tr for m::r#type { fn y() {} }\npub enum Kind { r#loop, Plain(u8) }\n\
                      const r#in: u8 = 1;\nstatic mut r#mut: u8 = 0;\nmacro_rules! r#m { () => {} }\n\
                      union r#union { a: u8 }\nfn r() {}\nfn after() {}\n";
        let names: Vec<String> = outline(source).iter().map(|d| d.names.join(".")).collect();
        assert_eq!(
            names,
            [
                "match",
                "type",
                "type",
                "type.fn",
                "type",
                "type.y",
                "Kind",
                "Kind.loop",
                "Kind.Plain",
                "in",
                "mut",
                "m",
                "union",
                "r",
                "after"
            ]
        );
        let at = |name: &str| definition(source, name);
        assert_eq!(at("match"), ["pub fn r#match() {}"]);
        assert_eq!(at("type.fn"), ["pub fn r#fn(&self) {}"]);
        assert_eq!(at("Kind.loop"), ["r#loop"]);
        assert_eq!(at("mut"), ["static mut r#mut: u8 = 0;"]);
        // The `r` of `r#` is no name of its own, and the name with it is no key
        assert_eq!(at("r"), ["fn r() {}"]);
        assert_eq!(at("r#match"), Vec::<String>::new());
        // A raw keyword is not the keyword: it starts no item
        assert_eq!(
            definition("r#fn f() {}\nfn g() {}", "f"),
            Vec::<String>::new()
        );
        assert_eq!(definition("r#fn f() {}\nfn g() {}", "g"), ["fn g() {}"]);
        // `r # x` apart is not a raw identifier
        assert_eq!(definition("fn r # x() {}", "r"), ["fn r # x() {}"]);
    }

    /// The shape of the first definition of that name in a file.
    fn shape_of(source: &str, dotted: &str) -> Shape {
        outline(source)
            .into_iter()
            .find(|d| d.names.join(".") == dotted)
            .unwrap_or_else(|| panic!("{dotted} is not defined in {source}"))
            .shape
    }

    /// What each `impl` block of a type says of itself, in the order they appear: the path of its trait, joined with
    /// `::`, or `None` for an inherent block.
    fn implementations(source: &str, name: &str) -> Vec<Option<String>> {
        blocks(source, name)
            .into_iter()
            .map(|block| block.of.map(|path| path.join("::")))
            .collect()
    }

    /// The `impl` blocks of the type that is named `name` in the outline, in the order they appear.
    fn blocks(source: &str, name: &str) -> Vec<Impl> {
        outline(source)
            .into_iter()
            .filter(|d| d.names.last().map(String::as_str) == Some(name))
            .filter_map(|d| match d.shape {
                Shape::Implementation(block) => Some(block),
                _ => None,
            })
            .collect()
    }

    fn implements(path: &str) -> Option<String> {
        Some(path.to_string())
    }

    #[test]
    fn an_enum_of_unit_variants_is_a_vocabulary_whatever_its_attributes_docs_and_discriminants() {
        let source = "/// Docs.\n#[derive(Debug)]\npub enum Level {\n    /// Low.\n    #[default]\n    Low,\n    \
                      Mid = 1,\n    /* a block comment */\n    High = -2,\n    Top = { 3 },\n}\n\
                      enum Bare { A }\npub(crate) enum NoComma { A, B }\n\
                      mod inner { enum Deep { A, B, } pub enum Wide { X = 1 << 2 } }\n";
        for name in ["Level", "Bare", "NoComma", "inner.Deep", "inner.Wide"] {
            assert_eq!(shape_of(source, name), Shape::Vocabulary, "{name}");
        }
        // The variants stay plain
        for variant in [
            "Level.Low",
            "Level.Mid",
            "Level.High",
            "Level.Top",
            "Bare.A",
            "inner.Deep.B",
        ] {
            assert_eq!(shape_of(source, variant), Shape::Plain, "{variant}");
        }
        // The definition still ends at its closing brace
        assert!(definition(source, "Level")[0].ends_with("Top = { 3 },\n}"));
    }

    #[test]
    fn an_enum_with_fields_generics_or_no_variant_stays_plain() {
        let source = "enum Tuple { A, B(u8) }\nenum Record { A, B { x: u8 }, C }\n\
                      enum Both { #[allow(dead_code)] A(), B }\nenum Generic<T> { A, B }\n\
                      enum Lifetime<'a> { A, B }\nenum Constant<const N: usize = { 1 }> { A }\n\
                      enum Empty {}\nenum Later { A = 1, B(u8) = 2 }\n";
        for name in [
            "Tuple", "Record", "Both", "Generic", "Lifetime", "Constant", "Empty", "Later",
        ] {
            assert_eq!(shape_of(source, name), Shape::Plain, "{name}");
        }
        // Each variant is plain either way, and the variants after one with fields are found as before
        assert_eq!(shape_of(source, "Tuple.B"), Shape::Plain);
        assert_eq!(definition(source, "Record.C"), ["C"]);
    }

    #[test]
    fn a_trait_is_a_trait_whatever_its_form_and_its_items_are_plain() {
        let source = "trait Plain { fn f(&self); }\npub trait Public {}\npub unsafe trait Danger {}\n\
                      pub trait Generic<T: Clone>: Sized + Send where T: Copy { type Out; fn g(&self) -> T; }\n\
                      trait Const<const N: usize = { 1 }> { const K: u8; }\n\
                      mod m { pub(crate) trait Inner { fn h(&self) {} } }\n";
        for name in ["Plain", "Public", "Danger", "Generic", "Const", "m.Inner"] {
            assert_eq!(shape_of(source, name), Shape::Trait, "{name}");
        }
        for item in [
            "Plain.f",
            "Generic.Out",
            "Generic.g",
            "Const.K",
            "m.Inner.h",
        ] {
            assert_eq!(shape_of(source, item), Shape::Plain, "{item}");
        }
        // A module that holds a trait is no trait
        assert_eq!(shape_of(source, "m"), Shape::Plain);
    }

    #[test]
    fn an_impl_block_of_a_trait_gives_the_path_of_the_trait_without_its_arguments() {
        let source = "struct A;\nstruct X<T>(T);\n\
                      impl Clone for A { fn clone(&self) -> A { A } }\n\
                      impl fmt::Display for A { fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { Ok(()) } }\n\
                      impl<T> From<T> for X<T> where T: Into<u8> { fn from(t: T) -> Self { X(t) } }\n\
                      unsafe impl Send for A {}\nimpl !Sync for A {}\n\
                      impl<F: Fn() -> u8> ::std::ops::Add<Vec<m::B>> for A where F: Copy {}\n\
                      impl<'a, F: Fn(&'a u8) -> u8> crate::m::Tr<F, { 1 < 2 }> for &'a mut m::A {}\n\
                      impl<T> Tr<for<'a> fn(&'a T)> for A {}\n";
        assert_eq!(
            implementations(source, "A"),
            [
                implements("Clone"),
                implements("fmt::Display"),
                implements("Send"),
                implements("Sync"),
                implements("std::ops::Add"),
                implements("crate::m::Tr"),
                implements("Tr"),
            ]
        );
        assert_eq!(implementations(source, "X"), [implements("From")]);
        // The items inside and the struct keep their shape
        assert_eq!(shape_of(source, "A.clone"), Shape::Plain);
        assert_eq!(shape_of(source, "X.from"), Shape::Plain);
        assert_eq!(shape_of(source, "A"), Shape::Plain);
    }

    #[test]
    fn an_impl_block_gives_the_path_of_its_type_as_written_without_arguments_or_references() {
        let source = "impl A {}\nimpl crate::song::Kind {}\nimpl super::Kind {}\nimpl<'a> Tr for &'a mut m::Kind {}\n\
                      impl Tr for *const Kind {}\nimpl Tr for Box<Kind> {}\nimpl Tr for dyn p::Tr {}\n\
                      impl Tr for [Kind] {}\nimpl Tr for (Kind, u8) {}\n";
        let targets: Vec<Vec<String>> = outline(source)
            .into_iter()
            .filter_map(|d| match d.shape {
                Shape::Implementation(block) => Some(block.target),
                _ => None,
            })
            .collect();
        let path = |text: &str| text.split("::").map(String::from).collect::<Vec<_>>();
        assert_eq!(
            targets,
            [
                path("A"),
                path("crate::song::Kind"),
                path("super::Kind"),
                path("m::Kind"),
                path("Kind"),
                path("Box"),
                path("p::Tr"),
            ]
        );
        // The definition is named by the last name of the path, as before
        assert_eq!(definition(source, "Kind").len(), 4);
        assert_eq!(definition(source, "Box").len(), 1);
        // A type that starts with `::` names another crate, for which the outline has never held a block
        assert!(outline("impl ::alloc::Kind { fn f() {} }").is_empty());
        assert!(nested_impls("fn g() { impl ::alloc::Kind { fn f() {} } }").is_empty());
    }

    #[test]
    fn an_impl_block_for_a_type_parameter_is_blanket() {
        let source = "impl<T: Copy> Rank for T { fn r(&self) {} }\nimpl<'a, T> Rank for &'a T {}\n\
                      impl<const N: usize, U, T: Tr<U, T>> Rank for [T; N] {}\nimpl<T> Rank for Wrapper<T> {}\n\
                      impl<A, T: Tr<A, T>> Rank for T {}\nimpl Rank for T {}\nimpl<U> Rank for T {}\n";
        let blanket: Vec<bool> = blocks(source, "T")
            .into_iter()
            .map(|block| block.blanket)
            .collect();
        assert_eq!(blanket, [true, true, true, false, false]);
        // `[T; N]` is no path, and `Wrapper<T>` is for `Wrapper`
        assert!(blocks(source, "Wrapper").iter().all(|block| !block.blanket));
        assert_eq!(blocks(source, "N").len(), 0);
    }

    #[test]
    fn an_inherent_impl_block_has_no_trait() {
        let source = "struct A;\nstruct T<U>(U);\nimpl A { fn f() {} }\n\
                      impl<T> T<T> { const C: u8 = 1; }\nimpl<U: Fn() -> u8> T<U> where U: Copy { fn g() {} }\n\
                      impl<'a> m::A<'a> {}\nenum E { V }\nimpl E { fn name(&self) -> &str { \"v\" } }\n";
        assert_eq!(implementations(source, "A"), [None, None]);
        assert_eq!(implementations(source, "T"), [None, None]);
        assert_eq!(implementations(source, "E"), [None]);
        // An enum with an `impl` block is a vocabulary here: the rule reads the blocks of the whole crate
        assert_eq!(shape_of(source, "E"), Shape::Vocabulary);
        assert_eq!(shape_of(source, "T.C"), Shape::Plain);
    }

    #[test]
    fn an_impl_block_in_a_body_a_block_a_closure_a_const_or_a_static_is_nested_and_no_outline_of_it()
     {
        let source = "enum Kind { A }\n\
                      fn f() { struct Local; impl Kind { fn g(&self) {} } }\n\
                      const _: () = { impl Rank for Kind { fn r(&self) {} } };\n\
                      static S: u8 = { unsafe impl Send for Kind {} 1 };\n\
                      mod m { fn h() { impl crate::Kind { fn i() {} } } pub struct Q; impl Q { fn j() { impl Tr for Kind {} } } }\n\
                      fn c(x: bool) { if x { impl Kind { fn k() {} } } let f = |y: u8| { impl Kind { fn l() {} } y }; }\n\
                      fn d() { fn e() { trait T { fn t(&self) {} } impl T for Kind {} } m!(impl Kind { fn n() {} }); }\n";
        let blocks_of = |definitions: Vec<Definition>| -> Vec<(Vec<String>, Option<String>)> {
            definitions
                .into_iter()
                .filter_map(|d| match d.shape {
                    Shape::Implementation(block) => {
                        Some((d.names, block.of.map(|path| path.join("::"))))
                    }
                    _ => None,
                })
                .collect()
        };
        let names = |text: &str| text.split('.').map(String::from).collect::<Vec<_>>();
        // The outline holds the block of the module alone
        assert_eq!(blocks_of(outline(source)), [(names("m.Q"), None)]);
        assert_eq!(
            blocks_of(nested_impls(source)),
            [
                (names("Kind"), None),
                (names("Kind"), implements("Rank")),
                (names("Kind"), implements("Send")),
                (names("m.Kind"), None),
                (names("m.Kind"), implements("Tr")),
                (names("Kind"), None),
                (names("Kind"), None),
                (names("Kind"), implements("T")),
            ],
        );
        // A trait declared in a body is handed over with its items, as a block there may implement it
        let traits: Vec<Vec<String>> = nested_impls(source)
            .into_iter()
            .filter(|d| d.shape == Shape::Trait)
            .map(|d| d.names)
            .collect();
        assert_eq!(traits, [names("T")]);
        // The items of a nested block are under the type, as the items of any block are
        let nested = nested_impls(source);
        let items: Vec<String> = nested
            .iter()
            .filter(|d| d.kind == Kind::Function)
            .map(|d| source[d.start..d.end].to_string())
            .collect();
        assert_eq!(
            items,
            [
                "fn g(&self) {}",
                "fn r(&self) {}",
                "fn i() {}",
                "fn k() {}",
                "fn l() {}",
                "fn t(&self) {}"
            ]
        );
        // The arguments of a macro call are no code, so the block in `m!(..)` is not read
        assert!(!items.contains(&"fn n() {}".to_string()));
        // A body that holds no block, an `impl Trait` type in a signature, a body or a tuple, make none
        let types = "fn a(x: impl Fn()) -> impl Fn() { x }\nfn b() -> Box<impl Tr> { todo!() }\n\
                     fn c(x: &impl Tr, y: impl Tr + Send) where impl Tr: Sized {}\nstruct S { f: Box<dyn Fn() -> impl Tr> }\n\
                     fn d() { fn e() -> impl Iterator<Item = u8> { 0..1 } fn g() -> (impl Tr, u8) { todo!() } \
                     let p = &d as *const _; let s: &'static str = \"\"; const { 1 }; }\n";
        assert!(nested_impls(types).is_empty());
    }

    #[test]
    fn a_key_selects_the_same_text_whether_or_not_an_impl_block_of_the_type_is_written_in_a_body() {
        let plain = "enum Kind { A }\nimpl Kind { fn f(&self) {} }\nfn h() {}\n";
        let nested = "enum Kind { A }\nimpl Kind { fn f(&self) {} }\nfn h() { if true { let c = || { impl Kind { fn g(&self) {} } }; } }\n\
                      const _: () = { impl Kind { fn k() {} } };\nstatic S: u8 = { impl Rank for Kind { fn r() {} } 1 };\n";
        for key in ["Kind", "Kind.f", "h"] {
            let pieces = |source: &str| selected(source, &outline(source), key);
            let before = pieces(plain);
            let after = pieces(nested);
            if key == "h" {
                // The text of the function is its own, with the block it holds
                assert_eq!(
                    after.as_deref(),
                    Some("fn h() { if true { let c = || { impl Kind { fn g(&self) {} } }; } }")
                );
            } else {
                assert_eq!(before, after, "{key}");
            }
        }
        assert_eq!(
            selected(nested, &outline(nested), "Kind").as_deref(),
            Some("enum Kind { A }\nimpl Kind { fn f(&self) {} }")
        );
        // What is in a body is no key: its items are not selected from outside
        for key in ["Kind.g", "Kind.k", "Kind.r"] {
            assert_eq!(selected(nested, &outline(nested), key), None, "{key}");
        }
    }

    #[test]
    fn a_shape_changes_neither_the_names_nor_the_ranges_of_an_impl_block() {
        let source = "struct A;\nimpl<T> From<T> for A { fn from(t: T) -> Self { A } }\nimpl A { fn f() {} }\n";
        assert_eq!(
            definition(source, "A"),
            [
                "struct A;",
                "impl<T> From<T> for A { fn from(t: T) -> Self { A } }",
                "impl A { fn f() {} }"
            ]
        );
        // A block for a type with no name is no definition, so it has no shape either
        assert!(outline("impl Tr for [u8] { fn y() {} }").is_empty());
    }

    #[test]
    fn a_file_that_is_cut_short_is_read_as_far_as_it_goes() {
        assert_eq!(definition("fn a() {}\nfn b() {", "a"), ["fn a() {}"]);
        assert_eq!(definition("fn a() {}\nfn b() {", "b"), ["fn b() {"]);
        assert_eq!(definition("struct", "x"), Vec::<String>::new());
        assert_eq!(definition("", "x"), Vec::<String>::new());
    }
}
