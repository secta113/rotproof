//! The suppression check: no comment in the code switches the type checker off, and a comment that changes what it
//! checks is named.
//!
//! - **Fails:** a comment that silences the type checker for a line or a file. In Python, `# type: ignore` (PEP 484)
//!   and `# <checker>: ignore` for each checker in [`CHECKERS`], with or without a list of codes, and the file-level
//!   `ignore-errors` of mypy and Pyrefly. In TypeScript, `@ts-ignore`, `@ts-expect-error` and `@ts-nocheck`, where the
//!   compiler reads them. A
//!   validated type is what makes the data from outside trustworthy, and a comment that hides the checker's error
//!   hides the place where it is missing. Lint suppressions (`eslint-disable`, `# noqa`) are not read: which lint
//!   guards the types is the project's choice.
//! - **Warns:** any other comment that sets a checker's options in the file (`# pyright: basic`,
//!   `# mypy: allow-untyped-defs`), unless every option in it only makes the checker stricter: `strict` and
//!   `report<Rule>=error` for Pyright, and for mypy an option that starts with `disallow-`, `warn-`, `strict-` or
//!   `enable-error-code`. Each option is judged by how it starts, never by the name of a rule. A warning fails nothing.
//! - Rust's compiler has no comment that silences a type error, so Rust is not read.
//!
//! The comments are the ones the marker check reads, in the same files (`markers.rs`).

use crate::layers::Language;
use crate::text::line_of;

/// The Python type checkers whose comments are read: `# <checker>: ...`.
pub const CHECKERS: [&str; 4] = ["mypy", "pyright", "pyrefly", "ty"];

/// The TypeScript directives that silence the checker.
const DIRECTIVES: [&str; 3] = ["@ts-ignore", "@ts-expect-error", "@ts-nocheck"];

/// The heading a comment that silences the checker is printed under.
pub const HEADING: &str = "no comment switches the type checker off (fix the type instead: data from outside \
                           becomes a validated type where it comes in, in infrastructure; a comment that hides an \
                           error hides where that is missing)";

const GUIDE_PYTHON: &str = include_str!("../../../project/rotproof/suppressions-python.md.in");
const GUIDE_TYPESCRIPT: &str =
    include_str!("../../../project/rotproof/suppressions-typescript.md.in");

/// What the guide says of this rule in the section on the layers, for a project in `language`: nothing for Rust,
/// whose compiler has no comment that switches it off.
pub fn guide(language: Language) -> &'static str {
    match language {
        Language::Python => GUIDE_PYTHON,
        Language::TypeScript => GUIDE_TYPESCRIPT,
        Language::Rust => "",
    }
}

/// What a comment does to the type checker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    /// It switches the checker off for a line or a file: fails
    Silences,
    /// It sets the checker's options in a way that may check less: a warning
    Weakens,
}

/// Each line of a source whose comments switch the checker off or change what it checks, as `path:line` and the line
/// under it, with what the comment does, from the comments the reader of `language` found (byte offset, text).
pub fn in_file(
    language: Language,
    path: &str,
    source: &str,
    comments: &[(usize, String)],
) -> Vec<(String, Effect)> {
    let lines: Vec<&str> = source.lines().collect();
    comments
        .iter()
        .filter_map(|(at, comment)| {
            let (offset, effect) = judged(language, comment)?;
            let line = line_of(source, at + offset);
            Some((format!("{path}:{line}\n{}", lines[line - 1].trim()), effect))
        })
        .collect()
}

/// What one comment does to the checker, and the offset in it where that is said; `None` when it does nothing to it.
pub fn judged(language: Language, comment: &str) -> Option<(usize, Effect)> {
    match language {
        Language::Python => python(comment),
        Language::TypeScript => typescript(comment),
        Language::Rust => None,
    }
}

/// A Python comment, `#` to the end of the line, which may hold several (`# noqa  # type: ignore`): the strongest
/// effect of any of them.
fn python(comment: &str) -> Option<(usize, Effect)> {
    let mut found: Option<(usize, Effect)> = None;
    for (at, _) in comment.match_indices('#') {
        let rest = &comment[at + 1..];
        let end = rest.find('#').unwrap_or(rest.len());
        let Some(effect) = python_one(rest[..end].trim()) else {
            continue;
        };
        if found.is_none_or(|(_, seen)| seen == Effect::Weakens && effect == Effect::Silences) {
            found = Some((at, effect));
        }
    }
    found
}

/// One `#` comment without its `#`: `type: ignore`, `pyright: basic` and the like.
fn python_one(text: &str) -> Option<Effect> {
    let (name, value) = text.split_once(':')?;
    let (name, value) = (name.trim(), value.trim());
    let ignores = value.starts_with("ignore");
    if name == "type" {
        return ignores.then_some(Effect::Silences);
    }
    if !CHECKERS.contains(&name) {
        return None;
    }
    if ignores {
        return Some(Effect::Silences);
    }
    let weakens = options(value).iter().any(|option| !stricter(name, option));
    weakens.then_some(Effect::Weakens)
}

/// The options of a checker's comment, split at the commas outside double quotes
/// (`disable-error-code="misc,attr-defined"` is one).
fn options(value: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut quoted = false;
    let mut start = 0;
    for (at, c) in value.char_indices() {
        match c {
            '"' => quoted = !quoted,
            ',' if !quoted => {
                out.push(value[start..at].trim());
                start = at + 1;
            }
            _ => {}
        }
    }
    out.push(value[start..].trim());
    out.retain(|option| !option.is_empty());
    out
}

/// Whether one option of the checker `name` only makes it stricter, judged by how it starts.
fn stricter(name: &str, option: &str) -> bool {
    match name {
        "pyright" => {
            option == "strict"
                || option.split_once('=').is_some_and(|(rule, level)| {
                    rule.trim().starts_with("report") && level.trim() == "error"
                })
        }
        "mypy" => {
            // mypy takes `_` for `-` in an option's name
            let option = option.replace('_', "-");
            ["disallow-", "warn-", "strict-", "enable-error-code"]
                .iter()
                .any(|start| option.starts_with(start))
        }
        _ => false,
    }
}

/// A TypeScript comment, read where the compiler reads a directive (TypeScript 6's `scanner.ts`, measured with tsc
/// 6.0.3 and 7.0.2 on 2026-11-14): at the start of a `//` comment, after its slashes and spaces, and at the start of
/// the last line of a `/* */` comment, after its `/`, `*` and spaces; not on another line of it. The compiler matches
/// the start of the name alone, so `@ts-ignored` silences it too, and so it fails here. `@ts-nocheck` works in a `//`
/// comment only, and only before the code; it fails anywhere in one, as it means nothing further down.
fn typescript(comment: &str) -> Option<(usize, Effect)> {
    let starts = |text: &str, directives: &[&str]| {
        let text = text.trim_start_matches(|c: char| c == '/' || c == '*' || c.is_whitespace());
        directives
            .iter()
            .any(|directive| text.starts_with(directive))
    };
    if comment.starts_with("//") {
        return starts(comment, &DIRECTIVES).then_some((0, Effect::Silences));
    }
    let last = comment.rfind('\n').map_or(0, |at| at + 1);
    starts(&comment[last..], &DIRECTIVES[..2]).then_some((last, Effect::Silences))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn py(comment: &str) -> Option<Effect> {
        judged(Language::Python, comment).map(|(_, effect)| effect)
    }

    fn ts(comment: &str) -> Option<Effect> {
        judged(Language::TypeScript, comment).map(|(_, effect)| effect)
    }

    #[test]
    fn a_python_comment_that_silences_a_checker_fails() {
        for comment in [
            "# type: ignore",
            "# type: ignore[assignment]",
            "#type:ignore",
            "# pyright: ignore",
            "# pyright: ignore [reportPrivateUsage]",
            "# pyrefly: ignore[bad-return]",
            "# pyrefly: ignore-errors",
            "# mypy: ignore-errors",
            "# mypy: ignore_errors",
            "# ty: ignore[invalid-argument-type]",
            "# noqa  # type: ignore",
            "# pyright: basic  # type: ignore",
        ] {
            assert_eq!(py(comment), Some(Effect::Silences), "{comment}");
        }
    }

    #[test]
    fn a_python_comment_that_may_check_less_warns() {
        for comment in [
            "# pyright: basic",
            "# pyright: standard",
            "# pyright: strict, reportPrivateUsage=false",
            "# pyright: reportAny=warning",
            "# pyright: reportAny=true",
            "# mypy: allow-untyped-defs",
            "# mypy: no-strict-optional",
            "# mypy: disable-error-code=\"misc\"",
            "# mypy: disallow-any-generics, allow-untyped-defs",
            "# pyrefly: anything",
            "# ty: anything",
        ] {
            assert_eq!(py(comment), Some(Effect::Weakens), "{comment}");
        }
    }

    #[test]
    fn a_python_comment_that_only_checks_more_or_names_no_checker_passes() {
        for comment in [
            "# pyright: strict",
            "# pyright: reportAny=error, reportPrivateUsage = error",
            "# mypy: disallow-any-explicit",
            "# mypy: warn_unreachable, strict-equality",
            "# mypy: enable-error-code=\"possibly-undefined,truthy-bool\"",
            "# type: int",
            "# noqa: E501",
            "# eslint-disable",
            "# the type: ignore it, said nobody",
            "# a plain comment",
        ] {
            assert_eq!(py(comment), None, "{comment}");
        }
    }

    #[test]
    fn a_typescript_directive_fails_where_the_compiler_reads_it() {
        for comment in [
            "// @ts-ignore",
            "// @ts-ignore: unreachable",
            "//@ts-expect-error",
            "/// @ts-expect-error",
            "//// @ts-ignore",
            "// @ts-nocheck",
            "/* @ts-ignore */",
            "/*@ts-ignore*/",
            "/**\n * @ts-expect-error */",
            "/*\n @ts-ignore */",
            // The compiler matches the start of the name alone
            "// @ts-ignored",
            "// @ts-ignore-foo",
            "// @ts-nocheck1",
        ] {
            assert_eq!(ts(comment), Some(Effect::Silences), "{comment}");
        }
        for comment in [
            "// @ts-check",
            "// do not write @ts-ignore here",
            "// eslint-disable-next-line",
            "/* plain */",
            // Not on the last line of a block comment, where the compiler does not read it
            "/**\n * @ts-expect-error\n */",
            "/* @ts-ignore\n more\n */",
            // `@ts-nocheck` works in a `//` comment only
            "/* @ts-nocheck */",
        ] {
            assert_eq!(ts(comment), None, "{comment}");
        }
    }

    #[test]
    fn rust_is_not_read() {
        assert_eq!(judged(Language::Rust, "// @ts-ignore"), None);
        assert_eq!(judged(Language::Rust, "// type: ignore"), None);
    }

    #[test]
    fn a_finding_is_named_at_the_line_of_the_directive() {
        let source = "x = 1\ny = f()  # type: ignore\nz = 3  # pyright: basic\n";
        let comments = [
            (15, "# type: ignore".to_string()),
            (37, "# pyright: basic".into()),
        ];
        assert_eq!(
            in_file(Language::Python, "a.py", source, &comments),
            [
                (
                    "a.py:2\ny = f()  # type: ignore".to_string(),
                    Effect::Silences
                ),
                ("a.py:3\nz = 3  # pyright: basic".into(), Effect::Weakens),
            ]
        );
        let source = "const a = 1;\n/**\n * why\n * @ts-ignore */\nconst b = 2;\n";
        let comments = [(13, "/**\n * why\n * @ts-ignore */".to_string())];
        assert_eq!(
            in_file(Language::TypeScript, "src/a.ts", source, &comments),
            [("src/a.ts:4\n* @ts-ignore */".to_string(), Effect::Silences)]
        );
    }
}
