//! Rust's names that do I/O in part: the allowlist of `std::env`, the clocks, the net of `Path` method names, and the
//! `extern` blocks, which cannot be read.
//!
//! - **An allowlist names what is pure; everything else of `std::env` is I/O.** It starts narrow, since a name added
//!   later is a patch and a name taken away is a major change. The functions of the environment (`var`, `vars`,
//!   `set_var`, `home_dir`, `temp_dir`, ...) are the environment check's and are never reported twice; `args` and
//!   `args_os` are the command-line arguments (`argv`); every other name (`current_dir`, `set_current_dir`,
//!   `current_exe`, `Args`, ...) is I/O, an entry of its own by its name under `std::env`.
//! - **A reference counts as a call:** `let f = Instant::now;` is the clock as `Instant::now()` is. Names are found as
//!   the scanner finds the items of a module ([`crate::rust::items`]): by the whole path, through a `use`, through a
//!   rename, from `::std`.
//! - **The clock** is `std::time::SystemTime::now`, `std::time::Instant::now` and `UNIX_EPOCH.elapsed()` (also written
//!   `SystemTime::UNIX_EPOCH.elapsed()`), which reads the time now as `now` does. A glob of `std::time` fails, as the
//!   names it brings in cannot be read.
//! - **A method is judged by its name on any receiver** when the name is one of a path's, because the type of what it
//!   is called on cannot be read: the net. It has no gate on the file. A method the project itself defines, of the same
//!   name and the same number of parameters (not counting `self`), is left out of it ([`excluded`]). `std::path::Path`
//!   named by its path (`Path::exists`, `.filter(Path::is_file)`) is the standard library's whatever the project
//!   defines, so it is judged by the name alone; `std::path::absolute` is a row of the table.
//! - **What cannot be read fails:** an `extern` block (`extern "C" { ... }`, `unsafe extern "C" { ... }`) declares what
//!   another language defines.
//!
//! Only the code outside `#[cfg(test)]` is judged: the caller gives the code with those items blanked.

use crate::code::Defined;
use crate::environment;
use crate::rust::{Item, Token, bound_names, code, items, tokens};
use crate::text::line_of;

use super::groups::{Excluded, Group};
use super::{Cause, Found};

/// The names of `std::env` that are pure: the constants of the platform, and the functions and the types that only
/// work on the text of a path list.
pub const ENV_PURE: [&str; 6] = [
    "consts",
    "split_paths",
    "join_paths",
    "VarError",
    "JoinPathsError",
    "SplitPaths",
];

/// The names of `std::env` that read the command-line arguments.
pub const ENV_ARGV: [&str; 2] = ["args", "args_os"];

/// The clocks of `std::time`: the type, whose `now` reads the time.
pub const CLOCK_TYPES: [&str; 2] = ["SystemTime", "Instant"];

/// The methods of a path that do I/O whatever they are given.
pub const NET_ALWAYS: [&str; 6] = [
    "canonicalize",
    "read_dir",
    "read_link",
    "try_exists",
    "symlink_metadata",
    "is_symlink",
];

/// The methods of a path that do I/O when called with no argument: with one, the method is another's.
pub const NET_NO_ARGUMENT: [&str; 4] = ["exists", "is_file", "is_dir", "metadata"];

fn finding(line: usize, entry: &str, name: Option<String>, cause: Cause) -> Found {
    Found {
        line,
        entry: entry.to_string(),
        name,
        cause,
    }
}

fn clock(line: usize, name: String) -> Found {
    finding(
        line,
        Group::Clock.key(),
        Some(name),
        Cause::Group(Group::Clock),
    )
}

/// Every place `code` (the code alone, `#[cfg(test)]` items blanked) of the file `source` does I/O by a name this
/// module judges: `std::env`, the clocks, a method of a path, and an `extern` block. `excluded` is the methods the
/// project defines, which the net leaves out.
pub fn judge(source: &str, code: &str, excluded: &Excluded) -> Vec<Found> {
    let mut found = Vec::new();
    in_env(source, code, &mut found);
    in_clocks(source, code, &mut found);
    in_path(source, code, &mut found);
    in_methods(source, code, excluded, &mut found);
    let tokens = tokens(code);
    for (i, (at, token)) in tokens.iter().enumerate() {
        // `extern "C" {`: the string is blanked, so the brace follows. `extern crate` and `extern "C" fn` are no block
        if matches!(token, Token::Word(word) if word == "extern")
            && matches!(tokens.get(i + 1), Some((_, Token::Punct('{'))))
        {
            found.push(finding(line_of(source, *at), "extern", None, Cause::Extern));
        }
    }
    found
}

/// The names of `std::env`: the pure ones and the environment check's pass, the arguments are a group, the rest is I/O
/// by its name.
fn in_env(source: &str, code: &str, out: &mut Vec<Found>) {
    for (at, item) in items(code, &["std", "env"]) {
        let line = line_of(source, at);
        match item {
            Item::Named(name) => {
                if ENV_PURE.contains(&name.as_str()) || environment::RUST.contains(&name.as_str()) {
                    continue;
                }
                if ENV_ARGV.contains(&name.as_str()) {
                    out.push(finding(
                        line,
                        Group::Argv.key(),
                        Some(format!("std::env::{name}")),
                        Cause::Group(Group::Argv),
                    ));
                } else {
                    out.push(finding(line, "std::env", Some(name), Cause::Used));
                }
            }
            // A glob above `std::env` (`use std::*;`) is found by the table's rows, once
            Item::Glob(of) if of.len() == 2 => {
                out.push(finding(line, "std::env", None, Cause::Glob(of.join("::"))));
            }
            Item::Glob(_) => {}
        }
    }
}

/// The clocks: `now` of `SystemTime` and `Instant`, `UNIX_EPOCH.elapsed()`, and a glob of `std::time`.
fn in_clocks(source: &str, code: &str, out: &mut Vec<Found>) {
    for class in CLOCK_TYPES {
        for (at, item) in items(code, &["std", "time", class]) {
            if item == Item::Named("now".to_string()) {
                out.push(clock(
                    line_of(source, at),
                    format!("std::time::{class}::now"),
                ));
            }
        }
    }
    for (at, item) in items(code, &["std", "time"]) {
        if let Item::Glob(of) = item
            && of.len() == 2
        {
            out.push(finding(
                line_of(source, at),
                "std::time",
                None,
                Cause::Glob(of.join("::")),
            ));
        }
    }
    // `UNIX_EPOCH.elapsed()` is the time now since the epoch: by a path to it (`std::time::UNIX_EPOCH`,
    // `SystemTime::UNIX_EPOCH`), or by a name a `use` binds to it
    let tokens = tokens(code);
    let name = "std::time::UNIX_EPOCH.elapsed";
    for module in [&["std", "time"][..], &["std", "time", "SystemTime"][..]] {
        for (at, item) in items(code, module) {
            if item == Item::Named("UNIX_EPOCH".to_string()) && elapsed_after_path(&tokens, at) {
                out.push(clock(line_of(source, at), name.to_string()));
            }
        }
    }
    let bound = bound_names(code, &["std", "time", "UNIX_EPOCH"]);
    for (i, (at, token)) in tokens.iter().enumerate() {
        let Token::Word(word) = token else { continue };
        let inside_path = i > 0 && matches!(tokens[i - 1].1, Token::PathSep | Token::Punct('.'));
        if bound.contains(word) && !inside_path && elapsed_at(&tokens, i + 1) {
            out.push(clock(line_of(source, *at), name.to_string()));
        }
    }
}

/// Whether the path that starts at the byte `at` is followed by `.elapsed(`.
fn elapsed_after_path(tokens: &[(usize, Token)], at: usize) -> bool {
    let Ok(mut i) = tokens.binary_search_by_key(&at, |(offset, _)| *offset) else {
        return false;
    };
    while matches!(tokens.get(i), Some((_, Token::Word(_))))
        && matches!(tokens.get(i + 1), Some((_, Token::PathSep)))
    {
        i += 2;
    }
    // The last name of the path
    elapsed_at(tokens, i + 1)
}

/// Whether the tokens from `i` are `.elapsed(`.
fn elapsed_at(tokens: &[(usize, Token)], i: usize) -> bool {
    matches!(tokens.get(i), Some((_, Token::Punct('.'))))
        && matches!(tokens.get(i + 1), Some((_, Token::Word(word))) if word == "elapsed")
        && matches!(tokens.get(i + 2), Some((_, Token::Punct('('))))
}

/// A method of `std::path::Path` named by its path: the standard library's, whatever the project defines.
fn in_path(source: &str, code: &str, out: &mut Vec<Found>) {
    for (at, item) in items(code, &["std", "path", "Path"]) {
        let Item::Named(name) = item else { continue };
        if NET_ALWAYS.contains(&name.as_str()) || NET_NO_ARGUMENT.contains(&name.as_str()) {
            let used = format!("Path::{name}");
            out.push(finding(line_of(source, at), &name, Some(used), Cause::Used));
        }
    }
}

/// The net: a call of a method whose name is a path's, on any receiver, which is I/O unless the project defines a
/// method of that name and the same number of parameters (`excluded`).
fn in_methods(source: &str, code: &str, excluded: &Excluded, out: &mut Vec<Found>) {
    let tokens = tokens(code);
    for (i, (at, token)) in tokens.iter().enumerate() {
        let Token::Word(name) = token else { continue };
        let always = NET_ALWAYS.contains(&name.as_str());
        if !(always || NET_NO_ARGUMENT.contains(&name.as_str())) {
            continue;
        }
        // A method: `.name(`, not a range (`a..name`)
        let dot = i > 0
            && tokens[i - 1].1 == Token::Punct('.')
            && !(i > 1
                && tokens[i - 2].1 == Token::Punct('.')
                && tokens[i - 2].0 + 1 == tokens[i - 1].0);
        if !dot {
            continue;
        }
        let Some(open) = call_open(&tokens, i + 1) else {
            continue;
        };
        let Some(given) = arguments(&tokens, open) else {
            continue;
        };
        if (always || given == 0) && !excluded.contains(name, given) {
            out.push(finding(line_of(source, *at), name, None, Cause::Method));
        }
    }
}

/// The index of the `(` of a call whose callee ends before token `i`: at `i`, or after a turbofish (`::<T>`).
fn call_open(tokens: &[(usize, Token)], i: usize) -> Option<usize> {
    match tokens.get(i).map(|(_, token)| token) {
        Some(Token::Punct('(')) => Some(i),
        Some(Token::PathSep) if matches!(tokens.get(i + 1), Some((_, Token::Punct('<')))) => {
            let mut angles = 0usize;
            for (j, (_, token)) in tokens.iter().enumerate().skip(i + 1) {
                match token {
                    Token::Punct('<') => angles += 1,
                    Token::Punct('>') if !arrow(tokens, j) => {
                        angles = angles.saturating_sub(1);
                        if angles == 0 {
                            return matches!(tokens.get(j + 1), Some((_, Token::Punct('('))))
                                .then_some(j + 1);
                        }
                    }
                    _ => {}
                }
            }
            None
        }
        _ => None,
    }
}

/// Whether the `>` at token `j` ends `->`, which closes no angle group.
fn arrow(tokens: &[(usize, Token)], j: usize) -> bool {
    j > 0 && tokens[j - 1].1 == Token::Punct('-') && tokens[j - 1].0 + 1 == tokens[j].0
}

/// How many arguments the call that opens at the token `open` has: the pieces between the commas outside any group,
/// a trailing comma not counted, and the parameters of a closure (`|a, b| a + b`) not split. `None` when it is not
/// closed.
fn arguments(tokens: &[(usize, Token)], open: usize) -> Option<usize> {
    let (mut depth, mut count) = (0usize, 0usize);
    let (mut seen, mut start) = (false, true);
    let mut j = open + 1;
    while j < tokens.len() {
        match &tokens[j].1 {
            Token::Punct('(' | '[' | '{') => {
                depth += 1;
                (seen, start) = (true, false);
            }
            Token::Punct(')' | ']' | '}') if depth == 0 => {
                return Some(count + usize::from(seen));
            }
            Token::Punct(')' | ']' | '}') => depth -= 1,
            Token::Punct(',') if depth == 0 => {
                count += usize::from(seen);
                (seen, start) = (false, true);
            }
            Token::Punct('|') if depth == 0 && start => {
                // The parameters of a closure, up to the `|` that closes them
                j += 1;
                while j < tokens.len() && tokens[j].1 != Token::Punct('|') {
                    j += 1;
                }
                (seen, start) = (true, false);
            }
            Token::Word(word) if depth == 0 && start && word == "move" => seen = true,
            _ => (seen, start) = (true, false),
        }
        j += 1;
    }
    None
}

/// Every method an `impl` block or a `trait` of the source defines, with how many parameters it takes, not counting
/// `self`: what the net leaves out when the project has one of its own. The whole file is read, a test too.
pub fn defined(source: &str) -> Vec<Defined> {
    let tokens = tokens(&code(source));
    let mut found = Vec::new();
    for (i, (_, token)) in tokens.iter().enumerate() {
        let Token::Word(word) = token else { continue };
        let container = match word.as_str() {
            "trait" => true,
            // `-> impl Trait` and `x: impl Trait` are types; an item starts after the end of another or an attribute
            "impl" => match i.checked_sub(1).map(|before| &tokens[before].1) {
                None => true,
                Some(Token::Punct(';' | '}' | '{' | ']')) => true,
                Some(Token::Word(before)) => before == "unsafe" || before == "default",
                Some(_) => false,
            },
            _ => false,
        };
        let Some(open) = container.then(|| body_open(&tokens, i + 1)).flatten() else {
            continue;
        };
        let mut depth = 0usize;
        for j in open + 1..tokens.len() {
            match &tokens[j].1 {
                Token::Punct('{') => depth += 1,
                Token::Punct('}') if depth == 0 => break,
                Token::Punct('}') => depth -= 1,
                Token::Word(keyword) if depth == 0 && keyword == "fn" => {
                    if let Some(method) = signature(&tokens, j + 1) {
                        found.push(method);
                    }
                }
                _ => {}
            }
        }
    }
    found.sort();
    found.dedup();
    found
}

/// The index of the `{` that opens the body of the `impl` or `trait` whose head starts at token `from`: the first one
/// outside the generics, and `None` for one that ends in `;`.
fn body_open(tokens: &[(usize, Token)], from: usize) -> Option<usize> {
    let (mut angles, mut nested) = (0usize, 0usize);
    for (j, (_, token)) in tokens.iter().enumerate().skip(from) {
        match token {
            Token::Punct('<') => angles += 1,
            Token::Punct('>') if !arrow(tokens, j) => angles = angles.saturating_sub(1),
            Token::Punct('(' | '[') => nested += 1,
            Token::Punct(')' | ']') => nested = nested.saturating_sub(1),
            Token::Punct('{') if angles == 0 && nested == 0 => return Some(j),
            Token::Punct(';') if angles == 0 && nested == 0 => return None,
            _ => {}
        }
    }
    None
}

/// The method whose name is at token `at` (after `fn`), with its number of parameters.
fn signature(tokens: &[(usize, Token)], at: usize) -> Option<Defined> {
    let Some((_, Token::Word(name))) = tokens.get(at) else {
        return None;
    };
    let mut i = at + 1;
    // Generics: `fn f<T: Fn() -> u8>(...)`
    if tokens.get(i).map(|(_, token)| token) == Some(&Token::Punct('<')) {
        let mut angles = 0usize;
        while i < tokens.len() {
            match &tokens[i].1 {
                Token::Punct('<') => angles += 1,
                Token::Punct('>') if !arrow(tokens, i) => {
                    angles -= 1;
                    if angles == 0 {
                        i += 1;
                        break;
                    }
                }
                _ => {}
            }
            i += 1;
        }
    }
    if tokens.get(i).map(|(_, token)| token) != Some(&Token::Punct('(')) {
        return None;
    }
    // The pieces between the commas outside any group and any angle brackets, each as the tokens up to its `:`
    let (mut depth, mut angles) = (0usize, 0usize);
    let mut pieces: Vec<Vec<&Token>> = vec![Vec::new()];
    let mut in_pattern = true;
    for (j, (_, token)) in tokens.iter().enumerate().skip(i + 1) {
        match token {
            Token::Punct('(' | '[' | '{') => depth += 1,
            Token::Punct(')' | ']' | '}') if depth == 0 => break,
            Token::Punct(')' | ']' | '}') => depth -= 1,
            Token::Punct('<') if depth == 0 => angles += 1,
            Token::Punct('>') if depth == 0 && !arrow(tokens, j) => {
                angles = angles.saturating_sub(1);
            }
            Token::Punct(',') if depth == 0 && angles == 0 => {
                pieces.push(Vec::new());
                in_pattern = true;
                continue;
            }
            Token::Punct(':') if depth == 0 && angles == 0 => in_pattern = false,
            _ => {}
        }
        if in_pattern && depth == 0 {
            pieces.last_mut()?.push(token);
        }
    }
    // A trailing comma leaves an empty piece
    pieces.retain(|piece| !piece.is_empty());
    let has_self = pieces.first().is_some_and(|piece| {
        piece
            .iter()
            .any(|t| matches!(t, Token::Word(w) if w == "self"))
    });
    Some(Defined {
        name: name.clone(),
        parameters: pieces.len() - usize::from(has_self),
    })
}

/// The methods of the project that the net leaves out, from the methods its `impl` blocks and traits define: the ones
/// the net would judge a call of, by that many parameters.
pub fn excluded(defined: &[Defined]) -> Excluded {
    Excluded::new(
        defined
            .iter()
            .filter(|method| {
                let (name, parameters) = (method.name.as_str(), method.parameters);
                NET_ALWAYS.contains(&name) || (NET_NO_ARGUMENT.contains(&name) && parameters == 0)
            })
            .map(|method| (method.name.clone(), method.parameters))
            .collect(),
    )
}

/// What the guide says of the names of Rust: the allowlist of `std::env` in full, the clocks, the net and what fails.
pub fn listing() -> String {
    let join = |names: &[&str]| {
        names
            .iter()
            .map(|name| format!("`{name}`"))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let mut text = String::from(
        "  What the standard library does only in part is judged by the name used: the lists below are Rotproof's, a \
         project cannot change them, and a name in none of them is not I/O. Names are found as the items of a module are: \
         by the whole path, through a `use` of the module or of one above it, through a rename, and from `::std`.\n",
    );
    text.push_str(&format!(
        "  - **`std::env`:** pure are {} and nothing else. `args` and `args_os` are the command-line arguments (`argv`); \
         the functions of the environment (`var`, `vars`, `set_var`, `home_dir`, `temp_dir`, ...) are the environment \
         check's, which is not approved and not reported twice; every other name (`current_dir`, `set_current_dir`, \
         `current_exe`, `Args`) is I/O, an entry of its own by its name, `std::env`. A glob of `std::env` fails.\n",
        join(&ENV_PURE)
    ));
    text.push_str(
        "  - **The clock** (`clock`): `std::time::SystemTime::now`, `std::time::Instant::now`, and `UNIX_EPOCH.elapsed()` \
         (`std::time::UNIX_EPOCH` and `SystemTime::UNIX_EPOCH`, as written or through a `use`). A reference counts as a \
         call: `let now = Instant::now;` is the clock. A glob of `std::time` fails, as the names it brings in cannot be \
         read. The clocks of other crates (`chrono::Utc::now`, `time::OffsetDateTime::now_utc`) are in no table, and are \
         not seen.\n",
    );
    text.push_str(&format!(
        "  - **The command-line arguments** (`argv`): `std::env::args` and `std::env::args_os`; the outer layers and the \
         entry file read them. Randomness has no name in `std`.\n  - **A method of a path is judged by its name on any \
         receiver,** because the type of what it is called on cannot be read: always {} (`.canonicalize()`, \
         `.read_dir()`), and with no argument {} (`.exists()`, `.is_file()`; `.exists(x)` is another method). \
         `std::path::absolute` is I/O under `std::path`. A method the project defines in an `impl` or a `trait`, with the \
         same name and the same number of parameters (not counting `self`), anywhere in the project, is left out of the \
         net, and `rotproof check` prints every name left out on every run, so that it is not silent. `Path::exists` and \
         the like, named by their path (`.filter(Path::is_file)`), are the standard library's and are judged whatever \
         the project defines.\n",
        join(&NET_ALWAYS),
        join(&NET_NO_ARGUMENT)
    ));
    text.push_str(
        "  - **What cannot be read fails:** an `extern` block (`extern \"C\"` and `unsafe extern \"C\"`, with a body of declarations) \
         declares what another language defines (entry `extern`). `extern crate` and `extern \"C\" fn` are no block.\n",
    );
    text.push_str(
        "  - **Not seen:** a method of a path on a value whose type the scanner cannot know, where the project defines \
         no method of that name and that many parameters (`store.exists()`, left out only by a definition), \
         `instant.elapsed()` on an `Instant` held in a variable (the `Instant::now` that made it is seen), a name bound in \
         one file and used through another (`pub use std::time::Instant;` in a module and `reexport::Instant::now()` in \
         the next), a path a macro builds from pieces, and the clocks of crates. Writing the code so that the check does \
         not see it fixes nothing: the I/O is the same.\n",
    );
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rust::without_tests;

    /// What a source is found to do, as `line entry (name)`, the net leaving out `excluded`.
    fn said_with(source: &str, excluded: &Excluded) -> Vec<String> {
        let mut said: Vec<String> = judge(source, &without_tests(&code(source)), excluded)
            .into_iter()
            .map(|one| {
                format!(
                    "{} {}{}",
                    one.line,
                    one.entry,
                    one.name
                        .map(|name| format!(" ({name})"))
                        .unwrap_or_default()
                )
            })
            .collect();
        // The check keeps one finding for a line and a name
        said.sort();
        said.dedup();
        said
    }

    fn said(source: &str) -> Vec<String> {
        said_with(source, &Excluded::default())
    }

    #[test]
    fn the_pure_names_of_std_env_pass_and_the_environment_checks_are_left_to_it() {
        let source = "\
use std::env::{consts, split_paths, join_paths, VarError, JoinPathsError, SplitPaths};
fn a() { let _ = std::env::consts::OS; let _ = std::env::join_paths([\"a\"]); }
fn b() { let _ = std::env::var(\"A\"); let _ = std::env::vars(); let _ = std::env::home_dir(); }
fn c() { std::env::set_var(\"A\", \"b\"); let _ = std::env::temp_dir(); let _ = std::env::var_os(\"B\"); }
";
        assert!(said(source).is_empty(), "{:?}", said(source));
    }

    #[test]
    fn every_other_name_of_std_env_is_io_by_its_name_and_the_arguments_are_a_group() {
        let source = "\
use std::env;
fn a() { let _ = env::current_dir(); let _ = std::env::set_current_dir(\"x\"); let _ = env::current_exe(); }
fn b() { let _ = env::args(); let _ = std::env::args_os(); let _: std::env::Args; }
fn c() { let _ = std::env::temp_dir(); let _ = std::env::split_paths(\"a\"); }
";
        assert_eq!(
            said(source),
            [
                "2 std::env (current_dir)",
                "2 std::env (current_exe)",
                "2 std::env (set_current_dir)",
                "3 argv (std::env::args)",
                "3 argv (std::env::args_os)",
                "3 std::env (Args)",
            ]
        );
        // The same through renames and a leading `::`
        assert_eq!(
            said("use std::env as e;\nfn a() { e::current_dir(); ::std::env::args(); }\n"),
            ["2 argv (std::env::args)", "2 std::env (current_dir)"]
        );
        // A glob of the module fails; one above it is the table's
        assert_eq!(said("use std::env::*;\n"), ["1 std::env"]);
        assert!(said("use std::*;\n").is_empty());
    }

    #[test]
    fn the_clocks_are_found_as_calls_and_as_references_through_renames_and_use() {
        let source = "\
use std::time::{Instant, SystemTime as ST};
fn a() { let _ = ST::now(); }
fn b() { let f = Instant::now; }
fn c() { let _ = std::time::SystemTime::now(); }
use std as s;
fn d() { let _ = s::time::Instant::now(); }
use std::time;
fn e() { let _ = time::SystemTime::now(); }
fn f() { let _ = ::std::time::Instant::now(); }
";
        let system = "std::time::SystemTime::now";
        let instant = "std::time::Instant::now";
        assert_eq!(
            said(source),
            [
                format!("2 clock ({system})"),
                format!("3 clock ({instant})"),
                format!("4 clock ({system})"),
                format!("6 clock ({instant})"),
                format!("8 clock ({system})"),
                format!("9 clock ({instant})"),
            ]
        );
    }

    #[test]
    fn the_epoch_read_as_the_time_now_is_the_clock_and_a_date_is_not() {
        let source = "\
use std::time::{SystemTime, UNIX_EPOCH, Duration};
fn a() { let _ = UNIX_EPOCH.elapsed(); }
fn b() { let _ = SystemTime::UNIX_EPOCH.elapsed(); }
fn c() { let _ = std::time::UNIX_EPOCH.elapsed(); }
fn d() { let _ = std::time::SystemTime::UNIX_EPOCH.elapsed(); }
use std::time::UNIX_EPOCH as E;
fn e() { let _ = E.elapsed(); }
fn f(x: SystemTime) { let _ = x.duration_since(UNIX_EPOCH); let _ = Duration::from_secs(1); let _ = UNIX_EPOCH; }
";
        let name = "std::time::UNIX_EPOCH.elapsed";
        assert_eq!(
            said(source),
            [2, 3, 4, 5, 7].map(|line| format!("{line} clock ({name})"))
        );
    }

    #[test]
    fn what_is_no_clock_of_std_passes_and_a_glob_of_std_time_fails() {
        let source = "\
use std::time::{Duration, SystemTimeError};
struct Instant;
impl Instant { fn now() -> u8 { 1 } }
fn a() { let _ = Instant::now(); let _ = Duration::from_secs(1); }
fn b(t: Duration) -> Duration { t.elapsed() }
";
        // `Instant` is the project's own: no `use` binds it to `std::time`
        assert!(said(source).is_empty(), "{:?}", said(source));
        assert_eq!(said("use std::time::*;\n"), ["1 std::time"]);
    }

    #[test]
    fn the_method_net_has_names_always_judged_and_names_judged_with_no_argument() {
        let source = "\
fn a(p: &Path) {
    p.canonicalize(); p.read_dir(); p.read_link(); p.try_exists(); p.symlink_metadata(); p.is_symlink();
    p.exists(); p.is_file(); p.is_dir(); p.metadata();
}
fn b(p: &Path, f: F) {
    p.exists(1); p.is_file(x, y); p.metadata(|a, b| a); p.is_dir(f());
    p.canonicalize(1, 2); a..exists(); p.exists::<>();
    p.get(); p.exist(); exists(); self.is_file;
}
";
        assert_eq!(
            said(source),
            [
                "2 canonicalize",
                "2 is_symlink",
                "2 read_dir",
                "2 read_link",
                "2 symlink_metadata",
                "2 try_exists",
                "3 exists",
                "3 is_dir",
                "3 is_file",
                "3 metadata",
                "7 canonicalize",
                "7 exists",
            ]
        );
    }

    #[test]
    fn a_definition_is_no_use_and_a_method_named_by_the_standard_path_is_judged_whatever_is_defined()
     {
        let source = "\
struct S;
impl S { fn exists(&self) -> bool { true } fn read_dir(&self, p: &str) {} }
fn is_file(a: u8) {}
fn a() { let _ = std::path::Path::exists; let _: Vec<_> = v.iter().filter(|p| Path::is_file(p)).collect(); }
use std::path::Path;
fn b() { v.iter().filter(Path::is_dir); let _ = Path::new(\"x\"); }
";
        // The definitions on lines 2 and 3 are found by no net, as no `.name(` is on them
        assert_eq!(
            said(source),
            [
                "4 exists (Path::exists)",
                "4 is_file (Path::is_file)",
                "6 is_dir (Path::is_dir)"
            ]
        );
        // And defined or not, the standard path's name is judged
        let excluded = excluded(&defined(source));
        assert_eq!(said_with(source, &excluded).len(), 3);
    }

    #[test]
    fn the_methods_a_project_defines_leave_the_net_by_name_and_number_of_parameters() {
        let source = "\
fn a(s: S, p: &Path) {
    s.exists(); s.read_dir(p); s.read_dir(); s.is_file(); p.metadata(); s.canonicalize(1, 2);
}
";
        let defined = defined(
            "impl Tree for Fake { fn exists(&self) -> bool { true } fn read_dir(&self, p: &str) -> u8 { 1 } }\n\
             trait Port { fn is_file(&self, path: &str) -> bool; fn canonicalize(&self, a: u8, b: &str); }\n",
        );
        let excluded = excluded(&defined);
        // `exists()`, `read_dir(p)` and `canonicalize(1, 2)` are the project's; `is_file(path)` takes one and is called
        // with none, `read_dir()` takes none, and `metadata()` is nobody's
        assert_eq!(
            said_with(source, &excluded),
            ["2 is_file", "2 metadata", "2 read_dir"]
        );
        assert_eq!(
            excluded.line().as_deref(),
            Some(
                "I/O net: left out, as the project defines a method of the same name and number of parameters: \
                 canonicalize(2), exists(0), read_dir(1)"
            )
        );
    }

    #[test]
    fn what_a_project_defines_is_read_from_its_impl_blocks_and_traits_with_the_parameters_counted()
    {
        let source = "\
pub struct S;
impl S {
    pub fn a(&self) {}
    fn b(&mut self, x: u8, y: HashMap<K, V>) -> u8 { fn inner(a: u8) {} 1 }
    pub(crate) async fn c<T: Fn(u8, u8) -> u8>(self, f: T) {}
    fn d(x: u8, (a, b): (u8, u8), z: [u8; 2],) {}
    fn e(self: Box<Self>, w: Vec<(u8, u8)>) {}
    fn f<'a>(&'a self) -> impl Fn(u8, u8) -> u8 { |a, b| a }
    fn g() {}
}
impl<T: Clone> Trait for Wrapper<T> where T: Fn() -> u8 { fn h(&self, a: &str) {} }
trait Port { fn i(&self, a: u8, b: u8); fn j(&self) -> u8 { 1 } }
fn free(a: u8) {}
fn k() -> impl Iterator<Item = u8> { fn not_a_method(a: u8) {} std::iter::empty() }
unsafe impl Send for S { fn l(&self) {} }
mod m { impl X { fn n(&self, a: u8) {} } }
";
        let found: Vec<(String, usize)> = defined(source)
            .into_iter()
            .map(|d| (d.name, d.parameters))
            .collect();
        let want = [
            ("a", 0),
            ("b", 2),
            ("c", 1),
            ("d", 3),
            ("e", 1),
            ("f", 0),
            ("g", 0),
            ("h", 1),
            ("i", 2),
            ("j", 0),
            ("l", 0),
            ("n", 1),
        ];
        let mut want: Vec<(String, usize)> =
            want.iter().map(|(n, p)| (n.to_string(), *p)).collect();
        want.sort();
        let mut found = found;
        found.sort();
        assert_eq!(found, want);
    }

    #[test]
    fn an_extern_block_fails_and_the_other_extern_forms_do_not() {
        let source = "\
extern \"C\" { fn abs(x: i32) -> i32; }
unsafe extern \"C\" { pub safe fn sqrt(x: f64) -> f64; }
extern { fn old(); }
extern crate alloc;
extern \"C\" fn callback() {}
type F = extern \"C\" fn(u8);
";
        assert_eq!(said(source), ["1 extern", "2 extern", "3 extern"]);
    }

    #[test]
    fn the_items_under_cfg_test_are_free() {
        let source = "\
pub fn a() { let _ = Instant::now(); }
#[cfg(test)]
mod tests {
    use std::time::Instant;
    fn t(p: &Path) { Instant::now(); p.exists(); std::env::current_dir(); }
    extern \"C\" { fn x(); }
}
";
        // `Instant` is imported inside the test module only, so line 1 names nothing of `std`
        assert!(said(source).is_empty(), "{:?}", said(source));
        let used = "use std::time::Instant;\npub fn a() { Instant::now(); }\n#[cfg(test)]\nmod tests { fn t(p: &Path) { p.exists(); Instant::now(); } }\n";
        assert_eq!(said(used), ["2 clock (std::time::Instant::now)"]);
    }

    #[test]
    fn the_guide_prints_the_allowlist_in_full_and_what_fails() {
        let text = listing();
        for name in ENV_PURE.iter().chain(&NET_ALWAYS).chain(&NET_NO_ARGUMENT) {
            assert!(text.contains(&format!("`{name}`")), "{name}");
        }
        for name in [
            "`extern`",
            "`std::time::Instant::now`",
            "`UNIX_EPOCH.elapsed()`",
            "`argv`",
        ] {
            assert!(text.contains(name), "{name}");
        }
    }
}
