//! The first sentence of a paragraph of markdown, in any language, as an index shows it: the paragraph read as
//! markdown reads it, and a full stop inside parentheses or inline code ending no sentence.

/// The first sentence of the first paragraph of `text`, for the index. Bold text at the start counts as a sentence on
/// its own. Works for any language: a sentence ends at "。" or at "." followed by a space or the end of the paragraph,
/// but not inside parentheses ("（）" or "()", nested or not, either kind closing either) or inline code, where a full
/// stop ends a sentence of the aside, not of the paragraph: "実装した（PR #1。CI は pass）。" is one sentence. A
/// parenthesis or inline code left open is read as text, so the sentence ends at the first full stop after it that no
/// closed bracket holds, as it would without the bracket, and not at the end of the paragraph.
///
/// The paragraph is read as markdown reads it (see `paragraph`), so a sentence wrapped onto the next line is one
/// sentence, and a paragraph with no full stop is shown whole.
pub fn first_sentence(text: &str) -> String {
    let paragraph = paragraph(text);
    if let Some(rest) = paragraph.strip_prefix("**")
        && let Some(end) = rest.find('*')
        && end > 0
        && rest[end..].starts_with("**")
    {
        return paragraph[..end + 4].to_string();
    }
    match sentence_end(&paragraph) {
        Some(end) => paragraph[..end].to_string(),
        None => paragraph,
    }
}

/// The first paragraph of `text` as one line: its lines trimmed (which takes a `\r` too) and joined with a space,
/// except between two lines of Japanese, which are joined with none, as a space would split a word that was wrapped
/// and Japanese is not written with spaces. The paragraph is the first line, and the lines after it up to the first
/// that markdown would not read as part of it: a blank line, or one that starts a heading, a fenced code block, a
/// quote, a table row or a list item (so a list's first item is a paragraph of its own, as is the line before a list).
fn paragraph(text: &str) -> String {
    let mut lines = text.split('\n').map(str::trim);
    let mut paragraph = lines.next().unwrap_or("").to_string();
    if paragraph.is_empty() {
        return paragraph;
    }
    for line in lines {
        if line.is_empty() || starts_a_block(line) {
            break;
        }
        let both_wide = paragraph.chars().next_back().is_some_and(is_wide)
            && line.chars().next().is_some_and(is_wide);
        if !both_wide {
            paragraph.push(' ');
        }
        paragraph.push_str(line);
    }
    paragraph
}

/// Whether a trimmed line starts a block that interrupts a paragraph: `#` (a heading, with a space or nothing after),
/// a fence of three backticks or tildes, `>`, `|`, a bullet (`-`, `*` or `+` with a space or nothing after) or a number
/// and a `.` or `)` with a space or nothing after. A line that only starts with such a character is text:
/// `**bold**`, `#123`, `1.5 times`.
fn starts_a_block(line: &str) -> bool {
    // What follows a marker: nothing, or a space (the line is trimmed, so a tab or more spaces came from inside it)
    let alone = |rest: &str| rest.is_empty() || rest.starts_with(' ');
    let hashes = line.chars().take_while(|&c| c == '#').count();
    let digits = line.chars().take_while(char::is_ascii_digit).count();
    line.starts_with("```")
        || line.starts_with("~~~")
        || line.starts_with(['>', '|'])
        || ((1..=6).contains(&hashes) && alone(&line[hashes..]))
        || line.strip_prefix(['-', '*', '+']).is_some_and(alone)
        || (digits > 0 && line[digits..].strip_prefix(['.', ')']).is_some_and(alone))
}

/// Whether `c` is written without spaces between words: kana, ideographs, and the punctuation and forms of full width.
fn is_wide(c: char) -> bool {
    matches!(c,
        '\u{3000}'..='\u{30FF}' // punctuation, hiragana, katakana
        | '\u{3400}'..='\u{4DBF}' // ideographs, extension A
        | '\u{4E00}'..='\u{9FFF}' // ideographs
        | '\u{F900}'..='\u{FAFF}' // compatibility ideographs
        | '\u{FF00}'..='\u{FF60}' // forms of full width
        | '\u{FFE0}'..='\u{FFE6}')
}

/// Where the first sentence of a paragraph ends, after its full stop, as `first_sentence` reads it; none when no full
/// stop ends one, and the paragraph is one sentence.
fn sentence_end(line: &str) -> Option<usize> {
    // How deep in parentheses; the length of the run of backticks that opened inline code; and, while inside either,
    // where the text after the outermost opening starts, which is read again as text when that opening is not closed
    let mut depth = 0usize;
    let mut code: Option<usize> = None;
    let mut opened: Option<usize> = None;
    let mut chars = line.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        let after = i + c.len_utf8();
        if c == '`' {
            let mut run = 1;
            while chars.next_if(|&(_, next)| next == '`').is_some() {
                run += 1;
            }
            // A run of another length inside inline code is code too
            match code {
                Some(length) if length == run => {
                    code = None;
                    if depth == 0 {
                        opened = None;
                    }
                }
                None => {
                    code = Some(run);
                    if depth == 0 {
                        opened = Some(i + run);
                    }
                }
                Some(_) => {}
            }
            continue;
        }
        if code.is_some() {
            continue;
        }
        match c {
            '（' | '(' => {
                if depth == 0 {
                    opened = Some(after);
                }
                depth += 1;
            }
            '）' | ')' if depth > 0 => {
                depth -= 1;
                if depth == 0 {
                    opened = None;
                }
            }
            '。' if depth == 0 => return Some(after),
            '.' if depth == 0 && chars.peek().is_none_or(|(_, next)| next.is_whitespace()) => {
                return Some(after);
            }
            _ => {}
        }
    }
    let start = opened?;
    sentence_end(&line[start..]).map(|end| start + end)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_sentence_ends_at_a_full_stop_in_any_language() {
        let cases = [
            ("Not yet. Measured by hand.", "Not yet."),
            (
                "発火済み・未着手。手元の Rust は古い。",
                "発火済み・未着手。",
            ),
            ("**Bold first.** Then more.", "**Bold first.**"),
            ("Version 1.75 is old. Update it.", "Version 1.75 is old."),
            ("Ends at the end.", "Ends at the end."),
            ("No full stop", "No full stop"),
            ("  Padded.  \nsecond line.", "Padded."),
            ("**unclosed bold. Then", "**unclosed bold."),
            ("", ""),
        ];
        for (text, expected) in cases {
            assert_eq!(first_sentence(text), expected, "{text:?}");
        }
    }

    #[test]
    fn first_sentence_does_not_end_inside_brackets() {
        let pr = "実装した、push 済み・未 merge（2026-10-24、secta113/rotproof の PR #1。CI は `3be3976` で全ジョブ pass）。";
        let cases = [
            // A full stop inside parentheses, full-width or ASCII, ends a sentence of the aside, not of the line
            (format!("{pr}Rotproof のブランチ。"), pr),
            (
                "Done (see PR #1. CI passed). Then more.".into(),
                "Done (see PR #1. CI passed).",
            ),
            // Nested, and either kind closing either
            (
                "済んだ（基準は「a」（ユーザー。全文）。b）。次。".into(),
                "済んだ（基準は「a」（ユーザー。全文）。b）。",
            ),
            ("A (b (c. d) e. f) g. Next.".into(), "A (b (c. d) e. f) g."),
            ("A（b (c。d）e)。Next。".into(), "A（b (c。d）e)。"),
            // Inside inline code, with any run of backticks; a parenthesis in code is code
            (
                "Ran `a. b` and passed. Then.".into(),
                "Ran `a. b` and passed.",
            ),
            ("`x。y` を足した。次。".into(), "`x。y` を足した。"),
            ("Grep `(foo` found 3. Next.".into(), "Grep `(foo` found 3."),
            (
                "Wrote ``a ` b. c`` here. Next.".into(),
                "Wrote ``a ` b. c`` here.",
            ),
            // Left open on the line, a bracket is read as text: the sentence does not run to the end of the line
            ("A (unclosed. Next sentence. Third".into(), "A (unclosed."),
            ("前置き（閉じない。次の文。".into(), "前置き（閉じない。"),
            ("Ran `a. b and more. Next".into(), "Ran `a."),
            ("A (b. c) d (e. f".into(), "A (b. c) d (e."),
            ("A (x (b. c) d. e".into(), "A (x (b. c) d."),
            ("A (no full stop".into(), "A (no full stop"),
            // A closing parenthesis with no opening one is text
            ("1) first. 2) second.".into(), "1) first."),
            // No full stop outside the brackets: the line is one sentence
            ("Text (a. b) more".into(), "Text (a. b) more"),
        ];
        for (text, expected) in cases {
            assert_eq!(first_sentence(&text), expected, "{text:?}");
        }
    }

    #[test]
    fn first_sentence_reads_a_wrapped_paragraph_as_one() {
        let cases = [
            // The first sentence wrapped before its full stop
            (
                "Done from the tag `v1.0.0` (2026-01-02, run\n12345 passed). Then more.",
                "Done from the tag `v1.0.0` (2026-01-02, run 12345 passed).",
            ),
            (
                "It was wrapped\nonto two lines. Next.",
                "It was wrapped onto two lines.",
            ),
            // Wrapped inside parentheses, and inside inline code
            (
                "Done (see the\nrun. It passed). Then.",
                "Done (see the run. It passed).",
            ),
            ("Ran `a.\nb` and passed. Then.", "Ran `a. b` and passed."),
            // Bold wrapped; and a sentence that ends in the second line of a paragraph of three
            ("**Bold\nfirst.** Then more.", "**Bold first.**"),
            ("One\ntwo. Three\nfour.", "One two."),
            // A full stop at the end of a line ends the sentence, as before
            ("First.\nSecond line.", "First."),
            // Line endings and the padding of each line are not text
            ("  One\r\n  two.\r\nThree.", "One two."),
            ("One\r\ntwo", "One two"),
            // The paragraph ends at a blank line: with no full stop it is shown whole, and not the next paragraph
            (
                "No full stop\nhere\n\nSecond paragraph. More.",
                "No full stop here",
            ),
            ("Padded  \n\n  \nnext.", "Padded"),
            // Or where another block starts: a heading, a fence, a quote, a table row, a list item
            ("Before the list\n- one. two\n- three.", "Before the list"),
            ("- one\n  wrapped. Two\n- three.", "- one wrapped."),
            ("- one\n- two.", "- one"),
            ("1) one\n2) two.", "1) one"),
            ("Before a heading\n## Sub\nText.", "Before a heading"),
            ("Before a fence\n```\ncode.\n```", "Before a fence"),
            ("Before a quote\n> quoted.", "Before a quote"),
            ("| a | b |\n|---|---|\n| 1 | 2 |", "| a | b |"),
            // A line that only starts with such a character is text
            (
                "Fixed\n#123 and\n1.5 times\n**bold** too. Next.",
                "Fixed #123 and 1.5 times **bold** too.",
            ),
            ("A\n-not a bullet. B", "A -not a bullet."),
            // Japanese is joined with no space between two lines of it, and with one next to anything else
            (
                "実装した、push 済み・未\nmerge。次。",
                "実装した、push 済み・未 merge。",
            ),
            ("実装した。\n次。", "実装した。"),
            (
                "実装した、手元の\n環境で確かめた。次。",
                "実装した、手元の環境で確かめた。",
            ),
            (
                "実装した（PR #1。\nCI は通った）。次。",
                "実装した（PR #1。 CI は通った）。",
            ),
            (
                "実装した（確認。\n通った）。次。",
                "実装した（確認。通った）。",
            ),
            ("Rust の\nビルド。", "Rust のビルド。"),
            ("Rust\nのビルド。", "Rust のビルド。"),
            ("ビルドは\nRust で。", "ビルドは Rust で。"),
            ("全角（\n括弧）。", "全角（括弧）。"),
        ];
        for (text, expected) in cases {
            assert_eq!(first_sentence(text), expected, "{text:?}");
        }
    }
}
