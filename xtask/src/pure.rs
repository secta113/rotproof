//! `domain` calls no port and does no I/O: a rule takes the values a use case in `application` has read (rust-stack
//! spec, step 5: strict, so a closure that reads is not passed in either). Splitting the layers into crates does not
//! enforce it: the ports are traits in `domain`, so the compiler lets `domain` call them.
//!
//! - **Anywhere:** a port taken (every trait `domain` declares, so a new one is a port from the start), and I/O.
//! - **What `application` can reach:** whether a function passed in reads cannot be told from its type, so what
//!   could carry a reader in is not taken where `application` can pass it: a function or a closure, or an iterator, in
//!   the signature of a `pub` function, a `pub` field, or a `pub` struct, enum or type alias. A private or
//!   `pub(crate)` function is called by `domain` alone, with what `domain` made. A type parameter calls nothing without
//!   a bound, and a bound is read as the forms above. Returning a closure or an iterator is a value going out.
//!
//! Read in the code alone (comments and literals blanked), outside the items under `#[cfg(test)]` and the blocks of the
//! traits themselves, whose methods take ports and return `io::Result` by design. A form is read line by line, as
//! rustfmt writes it; a parser would also see a signature split inside a type, and would still not see what a value
//! does once taken.

use std::collections::BTreeSet;
use std::sync::LazyLock;

use regex::Regex;

/// The ports Rotproof has: each has to be found as a trait, so the check cannot pass with the traits moved away
pub const KNOWN: [&str; 4] = ["Tree", "Writer", "Parsers", "Changes"];

/// Where the rules are
pub const DOMAIN: &str = "crates/domain/src";

/// A form, what it says, whether it passes right after `->` (what a function gives back), and whether it counts only
/// where `application` can reach
struct Form {
    pattern: Regex,
    what: &'static str,
    returned_passes: bool,
    reached_only: bool,
}

fn form(pattern: &str, what: &'static str, returned_passes: bool, reached_only: bool) -> Form {
    Form {
        pattern: Regex::new(pattern).expect("a valid pattern"),
        what,
        returned_passes,
        reached_only,
    }
}

/// The forms that take a port, for the traits `domain` declares
fn port_forms(ports: &BTreeSet<String>) -> Vec<Form> {
    if ports.is_empty() {
        return Vec::new();
    }
    let names: Vec<&str> = ports.iter().map(String::as_str).collect();
    // A port by its name, or by a path to it (`crate::tree::Tree`)
    let port = format!(r"(?:[A-Za-z_][A-Za-z0-9_]*::)*(?:{})\b", names.join("|"));
    vec![
        form(
            &format!(r"\b(?:dyn|impl)\s+{port}"),
            "takes a port",
            false,
            false,
        ),
        // A bound, `T: Tree`: one `:`, not the `::` of a path that `use` names a port by
        form(
            &format!(r"(?:^|[^:]):\s*{port}"),
            "takes a port",
            false,
            false,
        ),
        form(&format!(r"\+\s*{port}"), "takes a port", false, false),
    ]
}

/// The forms that could carry a reader in, or do I/O
static FORMS: LazyLock<Vec<Form>> = LazyLock::new(|| {
    let function = r"(?:FnOnce|FnMut|Fn)";
    let iterator = r"(?:Iterator|IntoIterator|DoubleEndedIterator|ExactSizeIterator)";
    let reader = r"(?:io::)?(?:Read|BufRead|Seek)";
    vec![
        form(
            &format!(r"\b(?:dyn|impl)\s+{function}\b"),
            "takes a function",
            true,
            true,
        ),
        form(
            &format!(r"(?:^|[^:]):\s*{function}\s*\("),
            "takes a function",
            false,
            true,
        ),
        form(
            &format!(r"\+\s*{function}\s*\("),
            "takes a function",
            false,
            true,
        ),
        // A function pointer: a definition is `fn name(`
        form(r"\bfn\s*\(", "takes a function", true, true),
        form(
            &format!(r"\b(?:dyn|impl)\s+{iterator}\b"),
            "takes an iterator",
            true,
            true,
        ),
        form(
            &format!(r"(?:^|[^:]):\s*{iterator}\b"),
            "takes an iterator",
            false,
            true,
        ),
        form(
            &format!(r"\+\s*{iterator}\b"),
            "takes an iterator",
            false,
            true,
        ),
        form(
            &format!(r"\b(?:dyn|impl)\s+{reader}\b"),
            "does I/O",
            false,
            false,
        ),
        form(
            &format!(r"(?:^|[^:]):\s*{reader}\b"),
            "does I/O",
            false,
            false,
        ),
        form(
            r"\bio::(?:Result|Error|ErrorKind|Read|BufRead|Write|Seek|stdin|stdout|stderr)\b",
            "does I/O",
            false,
            false,
        ),
        form(r"\bstd::(?:fs|process|net|env)\b", "does I/O", false, false),
        form(
            r"\bstd::\{[^}]*\b(?:fs|process|net|env)\b",
            "does I/O",
            false,
            false,
        ),
        form(
            r"\b(?:print|println|eprint|eprintln|dbg)!",
            "does I/O",
            false,
            false,
        ),
    ]
});

static TRAIT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\btrait\s+([A-Za-z_][A-Za-z0-9_]*)").expect("a valid pattern"));

/// A `pub` item: a function (its signature), or a struct, an enum, a union or a type alias (all of it). `pub(crate)`
/// and the like are seen by `domain` alone, and do not match
static PUBLIC_ITEM: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?m)^\s*pub\s+(?:(?:const|async|unsafe|extern)\s+)*(fn|struct|enum|union|type)\b")
        .expect("a valid pattern")
});

/// A `pub` field, of a struct or of a variant
static PUBLIC_FIELD: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?m)^\s*pub\s+[A-Za-z_][A-Za-z0-9_]*\s*:.*$").expect("a valid pattern")
});

/// What the forms are read in, in one file of `domain`
struct Read {
    /// The code, with the tests and the trait blocks blanked too
    code: String,
    /// The traits the file declares
    traits: BTreeSet<String>,
    /// The byte ranges `application` can reach
    reached: Vec<(usize, usize)>,
}

fn read(source: &str) -> Read {
    let code = utils::rust::code(source);
    let mut skipped: Vec<(usize, usize)> = Vec::new();
    let mut traits = BTreeSet::new();
    for (at, _) in code.match_indices("#[cfg(test)]") {
        skipped.push((at, item_end(&code, at)));
    }
    for found in TRAIT.captures_iter(&code) {
        let whole = found.get(0).expect("a match");
        traits.insert(found[1].to_string());
        skipped.push((whole.start(), item_end(&code, whole.start())));
    }
    let mut bytes = code.into_bytes();
    for (start, end) in skipped {
        for byte in &mut bytes[start..end] {
            if *byte != b'\n' {
                *byte = b' ';
            }
        }
    }
    let code = String::from_utf8(bytes).expect("only ASCII spaces were written");
    let mut reached = Vec::new();
    for found in PUBLIC_ITEM.captures_iter(&code) {
        let start = found.get(0).expect("a match").start();
        let end = if &found[1] == "fn" {
            signature_end(&code, start)
        } else {
            item_end(&code, start)
        };
        reached.push((start, end));
    }
    reached.extend(PUBLIC_FIELD.find_iter(&code).map(|m| (m.start(), m.end())));
    Read {
        code,
        traits,
        reached,
    }
}

/// The findings in one file of `domain` at `path`: on each line, the first form found.
fn in_file(path: &str, source: &str, read: &Read, ports: &[Form]) -> Vec<String> {
    let mut problems = Vec::new();
    let mut at = 0;
    for (number, line) in read.code.split('\n').enumerate() {
        let start = at;
        at += line.len() + 1;
        let reached = |m: &regex::Match| {
            read.reached
                .iter()
                .any(|&(from, to)| start + m.start() < to && from < start + m.end())
        };
        let found = ports.iter().chain(FORMS.iter()).find_map(|form| {
            form.pattern
                .find_iter(line)
                .find(|m| {
                    !(form.returned_passes && line[..m.start()].trim_end().ends_with("->"))
                        && (!form.reached_only || reached(m))
                })
                .map(|m| (form.what, m.as_str().trim()))
        });
        if let Some((what, text)) = found {
            let original = source.lines().nth(number).unwrap_or_default().trim();
            problems.push(format!(
                "{path}:{}: {what} (`{text}`): a rule in domain takes the values application has read\n    {original}",
                number + 1
            ));
        }
    }
    problems
}

/// The end of the item that starts at `start`: past the `}` that closes its first `{`, or past a `;` before any `{`.
fn item_end(code: &str, start: usize) -> usize {
    let b = code.as_bytes();
    let mut depth = 0;
    for (i, &c) in b.iter().enumerate().skip(start) {
        match c {
            b';' if depth == 0 => return i + 1,
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return i + 1;
                }
            }
            _ => {}
        }
    }
    b.len()
}

/// The end of the signature of the function that starts at `start`: its body's `{`, or the `;` of a declaration.
fn signature_end(code: &str, start: usize) -> usize {
    code[start..]
        .find(['{', ';'])
        .map_or(code.len(), |n| start + n)
}

/// Every finding in `files` (path, text), with the floor: at least one file read, and every port Rotproof has found
/// as a trait.
pub fn problems(files: &[(String, String)]) -> Vec<String> {
    let read: Vec<Read> = files.iter().map(|(_, source)| read(source)).collect();
    let traits: BTreeSet<String> = read.iter().flat_map(|r| r.traits.clone()).collect();
    let ports = port_forms(&traits);
    let mut found = Vec::new();
    for ((path, source), read) in files.iter().zip(&read) {
        found.extend(in_file(path, source, read, &ports));
    }
    if files.is_empty() {
        found.push(format!("no .rs file of {DOMAIN}/ was read"));
    }
    for port in KNOWN {
        if !traits.contains(port) {
            found.push(format!(
                "no trait {port} in {DOMAIN}/: a port renamed or moved would escape this check; name it in KNOWN \
                 (xtask/src/pure.rs)"
            ));
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    const PORT_TRAITS: &str = "pub trait Tree {\n    fn read(&self, path: &str) -> io::Result<String>;\n}\n\
        pub trait Writer {\n    fn write(&self, path: &str) -> io::Result<()>;\n}\n\
        pub trait Parsers {\n    fn aliases(&self, tree: &dyn Tree) -> io::Result<()>;\n}\n\
        pub trait Changes {\n    fn changed(&self) -> bool;\n}\n";

    /// The findings beside the port traits, each as `line: what`
    fn named(source: &str) -> Vec<String> {
        let files = [
            (
                "crates/domain/src/ports.rs".to_string(),
                PORT_TRAITS.to_string(),
            ),
            ("crates/domain/src/x.rs".to_string(), source.to_string()),
        ];
        problems(&files)
            .iter()
            .map(|why| {
                why.trim_start_matches("crates/domain/src/x.rs:")
                    .split(" (")
                    .next()
                    .unwrap()
                    .to_string()
            })
            .collect()
    }

    #[test]
    fn a_port_or_io_anywhere_outside_the_traits_and_the_tests_is_found_by_its_line() {
        let source = "use crate::tree::Tree;\nuse std::io;\n\
            pub fn a(tree: &dyn Tree) {}\n\
            fn b(tree: &impl Writer) {}\n\
            pub(crate) fn c<T: Parsers>(t: T) {}\n\
            pub fn d<T: Clone + Changes>(t: T) {}\n\
            fn e() -> io::Result<()> { Ok(()) }\n\
            fn f() { let _ = std::fs::read(\"x\"); }\n\
            use std::{collections, process};\n\
            fn g() { println!(\"x\"); }\n\
            fn i(tree: &dyn crate::tree::Tree) {}\n\
            pub fn j<T>(t: T) where T: crate::hook::Changes {}\n\
            use crate::tree::{Tree, Writer};\n\
            fn k(input: &mut dyn Read) {}\n\
            fn l<R: io::BufRead>(input: R) {}\n";
        assert_eq!(
            named(source),
            [
                "3: takes a port",
                "4: takes a port",
                "5: takes a port",
                "6: takes a port",
                "7: does I/O",
                "8: does I/O",
                "9: does I/O",
                "10: does I/O",
                "11: takes a port",
                "12: takes a port",
                "14: does I/O",
                "15: does I/O",
            ]
        );
    }

    #[test]
    fn what_could_carry_a_reader_in_is_not_taken_where_application_reaches() {
        let source = "pub trait Clock {\n    fn now(&self) -> u64;\n}\n\
            pub fn a(clock: &dyn Clock) {}\n\
            pub fn b(read: &dyn Fn(&str) -> Option<String>) {}\n\
            pub fn c(\n    path: &str,\n    read: impl FnMut(&str) -> bool,\n) {}\n\
            pub fn d(read: fn(&str) -> bool) {}\n\
            pub fn e(lines: impl Iterator<Item = String>) {}\n\
            pub fn f<F: Fn(&str) -> bool>(read: F) {}\n\
            pub fn g<I>(lines: I) where I: Iterator<Item = String> {}\n\
            pub type Reader = Box<dyn Fn(&str) -> String>;\n\
            pub struct Holder {\n    pub read: Box<dyn Fn(&str) -> String>,\n}\n\
            struct Private {\n    pub read: Box<dyn Fn(&str) -> String>,\n}\n\
            impl Thing {\n    pub fn h(&self, read: &dyn Fn(&str) -> String) {}\n}\n";
        assert_eq!(
            named(source),
            [
                "4: takes a port",
                "5: takes a function",
                "8: takes a function",
                "10: takes a function",
                "11: takes an iterator",
                "12: takes a function",
                "13: takes an iterator",
                "14: takes a function",
                "16: takes a function",
                "19: takes a function",
                "22: takes a function",
            ]
        );
    }

    #[test]
    fn what_domain_alone_calls_what_goes_out_and_what_is_not_code_pass() {
        let source = "// a rule never takes a &dyn Tree or a &dyn Fn\n\
             pub const SAID: &str = \"io::Result {{ std::fs }} fn(x) dyn Fn\";\n\
             pub fn a() -> impl Iterator<Item = u8> { [1].into_iter() }\n\
             pub fn b() -> impl Fn(&str) -> bool { |s| s.is_empty() }\n\
             pub fn c<'a, T>(s: &'a str, t: T) -> T { let keep = |x: &dyn Fn() -> u8| x(); t }\n\
             fn d(convert: impl Fn(&str) -> u8) -> u8 { convert(\"x\") }\n\
             pub(crate) fn e<F: Fn() -> u8>(f: F) -> u8 { f() }\n\
             impl<I> Default for Source<I> {}\n\
             struct Private {\n    read: Box<dyn Fn(&str) -> String>,\n}\n\
             use std::fmt::Write;\n\
             #[cfg(test)]\nuse std::fs;\n\
             #[cfg(test)]\nmod tests {\n    pub fn t(tree: &dyn Tree) { std::fs::read(\"}\").unwrap(); }\n}\n";
        assert_eq!(named(source), Vec::<String>::new());
    }

    #[test]
    fn nothing_read_or_a_port_not_found_fails_by_the_floor() {
        assert_eq!(problems(&[]).len(), 1 + KNOWN.len());
        let found = problems(&[(
            "crates/domain/src/tree.rs".into(),
            "pub trait Tree {}\n".into(),
        )]);
        assert_eq!(found.len(), 3, "{found:?}");
        assert!(found.iter().all(|why| why.starts_with("no trait ")));
        assert_eq!(
            problems(&[("crates/domain/src/ports.rs".into(), PORT_TRAITS.into())]),
            Vec::<String>::new()
        );
    }
}
