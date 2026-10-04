//! Markdown read the way GitHub reads it: the text it shows, headings and links. The records link to each other with
//! standard markdown links (OKF 0.2, section 6.1), and a link to a heading uses GitHub's anchor for that heading.

use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use pulldown_cmark::{Event, Parser, Tag, TagEnd};
use regex::Regex;
use unicode_general_category::{GeneralCategory, get_general_category};

// The target of a link in HTML, which GitHub renders as a link too: in double quotes, or in single quotes
static HREF: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(?i)\bhref\s*=\s*(?:"([^"]*)"|'([^']*)')"#).unwrap());

/// The text without the parts GitHub does not render as text: HTML comments and fenced code blocks. Every check that
/// reads the structure of a document (its sections, headings and links) reads this, so a heading or a link that a
/// reader of the page cannot see counts for nothing.
///
/// Read as GFM reads them. A fence is a line of 3 or more backticks or tildes, indented by up to 3 spaces; a backtick
/// fence has no backtick after it on the line, or the line is inline code. It is closed by a line of the same
/// character, at least as long, indented by up to 3 spaces, and an unclosed fence runs to the end. A comment runs from
/// `<!--` to the next `-->`, across lines, and the text around it stays; inside a fence, `<!--` is code.
pub fn visible(text: &str) -> String {
    let mut out = String::new();
    let mut fence: Option<(char, usize)> = None;
    // Inside a comment: the text before it on its first line, which the text after its end joins
    let mut comment: Option<String> = None;
    for line in text.split_inclusive('\n') {
        if let Some((mark, length)) = fence {
            if fence_length(line, mark).is_some_and(|n| n >= length) && is_fence_end(line, mark) {
                fence = None;
            }
            continue;
        }
        let mut rest = line;
        let mut shown = match comment.take() {
            Some(before) => match rest.find("-->") {
                Some(end) => {
                    rest = &rest[end + 3..];
                    before
                }
                None => {
                    comment = Some(before);
                    continue;
                }
            },
            None => {
                if let Some(opened) = fence_opening(line) {
                    fence = Some(opened);
                    continue;
                }
                String::new()
            }
        };
        let mut open = false;
        while let Some(start) = rest.find("<!--") {
            shown.push_str(&rest[..start]);
            match rest[start + 4..].find("-->") {
                Some(end) => rest = &rest[start + 4 + end + 3..],
                None => {
                    open = true;
                    rest = "";
                    break;
                }
            }
        }
        shown.push_str(rest);
        if open {
            comment = Some(shown);
        } else {
            out.push_str(&shown);
        }
    }
    out
}

/// The run of `mark` that starts a line, after up to 3 spaces: its length, when it is long enough for a fence.
fn fence_length(line: &str, mark: char) -> Option<usize> {
    let indent = line.len() - line.trim_start_matches(' ').len();
    let length = line[indent..].chars().take_while(|&c| c == mark).count();
    (indent <= 3 && length >= 3).then_some(length)
}

/// The fence a line opens: its character and its length.
fn fence_opening(line: &str) -> Option<(char, usize)> {
    ['`', '~'].into_iter().find_map(|mark| {
        let length = fence_length(line, mark)?;
        // The marks are ASCII, so the run's length in characters is its length in bytes
        let info = &line.trim_start_matches(' ')[length..];
        (mark == '~' || !info.contains('`')).then_some((mark, length))
    })
}

/// Whether a line that starts with a run of `mark` has nothing after the run, as a closing fence has.
fn is_fence_end(line: &str, mark: char) -> bool {
    line.trim_start_matches(' ')
        .trim_start_matches(mark)
        .trim()
        .is_empty()
}

/// Letters, numbers and marks: the Unicode categories GitHub keeps in an anchor.
fn is_letter_number_or_mark(c: char) -> bool {
    use GeneralCategory::*;
    matches!(
        get_general_category(c),
        UppercaseLetter
            | LowercaseLetter
            | TitlecaseLetter
            | ModifierLetter
            | OtherLetter
            | DecimalNumber
            | LetterNumber
            | OtherNumber
            | NonspacingMark
            | SpacingMark
            | EnclosingMark
    )
}

/// GitHub's anchor for a heading: lowercase, keep letters, digits, marks, `-`, `_` and spaces, then spaces to `-`.
///
/// Characters outside those classes (punctuation and symbols, full-width ones included) are dropped. Each space
/// becomes one hyphen, so two spaces give two hyphens.
///
/// The categories come from the Unicode version of `unicode-general-category`, newer than the 15.0 of Python 3.12: the
/// 5057 code points assigned after 15.0 are kept here and dropped by the Python checks (measured 2026-10-02). Every
/// other code point gives the same anchor in both.
pub fn slug(heading: &str) -> String {
    heading
        .to_lowercase()
        .chars()
        .filter(|&c| matches!(c, ' ' | '-' | '_') || is_letter_number_or_mark(c))
        .map(|c| if c == ' ' { '-' } else { c })
        .collect()
}

/// The level and the text of a heading, when the line is one, read as GFM reads it: up to 3 spaces, 1 to 6 `#`, then
/// a space or a tab before the text. A closing run of `#` after a space is not part of the text. Every check that looks
/// for a heading (the sections of a document, the dates of the log, the anchors) reads it here.
pub fn heading(line: &str) -> Option<(usize, &str)> {
    let indent = line.len() - line.trim_start_matches(' ').len();
    let rest = &line[indent..];
    let level = rest.len() - rest.trim_start_matches('#').len();
    let rest = &rest[level..];
    if indent > 3
        || !(1..=6).contains(&level)
        || !(rest.is_empty() || rest.starts_with([' ', '\t']))
    {
        return None;
    }
    let text = rest.trim_matches([' ', '\t']);
    let open = text.trim_end_matches('#');
    let text = if open.is_empty() || open.ends_with([' ', '\t']) {
        open.trim_end_matches([' ', '\t'])
    } else {
        text
    };
    Some((level, text))
}

/// Every heading anchor in a markdown text. A repeated heading gets `-1`, `-2`, ... as on GitHub.
pub fn anchors(text: &str) -> HashSet<String> {
    let mut seen: HashMap<String, usize> = HashMap::new();
    let mut found = HashSet::new();
    let shown = visible(text);
    for (_, heading) in shown.lines().filter_map(heading) {
        if heading.is_empty() {
            continue;
        }
        let base = slug(heading);
        let count = seen.entry(base.clone()).or_insert(0);
        found.insert(if *count == 0 {
            base.clone()
        } else {
            format!("{base}-{count}")
        });
        *count += 1;
    }
    found
}

/// `(text, target)` for every link a reader of the rendered page can follow, in order: inline and reference links,
/// images, and `href` in HTML. Read by a CommonMark parser, so a title (`[a](b.md "title")`), a target in angle
/// brackets (`<a b.md>`), parentheses in a target (`b(1).md`) and escaped brackets in the text read as GitHub reads
/// them. Inline code in the text keeps its backticks.
///
/// A reference link is read when its definition is in `text`. One defined elsewhere in the document is not seen.
pub fn links(text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    // Links whose text is still being read. An image may sit inside a link
    let mut open: Vec<(String, String)> = Vec::new();
    for event in Parser::new(text) {
        match event {
            Event::Start(Tag::Link { dest_url, .. } | Tag::Image { dest_url, .. }) => {
                open.push((String::new(), dest_url.into_string()));
            }
            Event::End(TagEnd::Link | TagEnd::Image) => out.extend(open.pop()),
            Event::Text(shown) => {
                if let Some((text, _)) = open.last_mut() {
                    text.push_str(&shown);
                }
            }
            Event::Code(code) => {
                if let Some((text, _)) = open.last_mut() {
                    text.push_str(&format!("`{code}`"));
                }
            }
            Event::SoftBreak | Event::HardBreak => {
                if let Some((text, _)) = open.last_mut() {
                    text.push(' ');
                }
            }
            Event::Html(html) | Event::InlineHtml(html) => out.extend(
                HREF.captures_iter(&html)
                    .filter_map(|caps| caps.get(1).or(caps.get(2)))
                    .map(|target| (String::new(), target.as_str().to_string())),
            ),
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// heading -> the anchor GitHub gave it. Each value was read from a page that GitHub rendered, so a change to
    /// `slug` that drifts from GitHub fails here instead of breaking links silently.
    const SEEN_ON_GITHUB: &[(&str, &str)] = &[
        // project-template README.md, rendered 2026-10-01
        ("project-template", "project-template"),
        ("始め方", "始め方"),
        ("雛形の改善を取り込む", "雛形の改善を取り込む"),
        (
            "中身（写した先の根になるもの）",
            "中身写した先の根になるもの",
        ),
        // project-template docs/log.md, rendered 2026-10-02: a slash, an ideographic comma and spaces
        (
            "配る中身を template/ に下ろし、copier で写す",
            "配る中身を-template-に下ろしcopier-で写す",
        ),
        (
            "雛形を作り、グローバルの既定を実体にした",
            "雛形を作りグローバルの既定を実体にした",
        ),
    ];

    #[test]
    fn anchors_match_github() {
        for (heading, anchor) in SEEN_ON_GITHUB {
            assert_eq!(slug(heading), *anchor, "{heading}");
        }
    }

    #[test]
    fn repeated_and_hidden_headings() {
        // GFM lets a fence be indented by up to 3 spaces, and the closing fence by its own amount
        let text = "# Same\n\n## Same\n\n<!--\n## Hidden\n-->\n\n```\n## Code\n```\n\n   ```\n## Indented\n ```\n";
        let expected: HashSet<String> = ["same", "same-1"].map(String::from).into();
        assert_eq!(anchors(text), expected);
    }

    /// GFM's headings: indented by up to 3 spaces, and without the closing run of `#` some write after the text
    #[test]
    fn headings_as_gfm_reads_them() {
        let text = " # One space\n   ## Three spaces\n    # Four spaces is code\n## Closed ##\n#No space\n";
        let expected: HashSet<String> = ["one-space", "three-spaces", "closed"]
            .map(String::from)
            .into();
        assert_eq!(anchors(text), expected);
    }

    /// What GFM hides, and what only looks hidden. Every heading named "Hidden" must give no anchor.
    #[test]
    fn hidden_as_gfm_hides() {
        let text = "\
# Shown

~~~
# Hidden: a tilde fence
~~~

````
```
# Hidden: a shorter fence does not close a longer one
```
````

~~~
```
# Hidden: a fence of the other character does not close it
~~~

``` not a fence, inline code ```

# Shown after inline code

```
<!--
```

# Shown after a comment opener inside code

Text <!-- one line --> and text <!--
# Hidden: a comment that starts mid-line
--> text

# Shown after a comment

```
# Hidden: an unclosed fence runs to the end
";
        let expected: HashSet<String> = [
            "shown",
            "shown-after-inline-code",
            "shown-after-a-comment-opener-inside-code",
            "shown-after-a-comment",
        ]
        .map(String::from)
        .into();
        assert_eq!(anchors(text), expected);
        assert_eq!(visible("a <!-- b --> c\n<!--\nd\n-->\ne\n"), "a  c\n\ne\n");
    }

    #[test]
    fn links_are_found_in_order() {
        let text = "[a](/x.md#y)、[`b`](../c.py) and [d](https://example.com)";
        let pairs = [
            ("a", "/x.md#y"),
            ("`b`", "../c.py"),
            ("d", "https://example.com"),
        ];
        let expected: Vec<(String, String)> = pairs
            .iter()
            .map(|(t, l)| (t.to_string(), l.to_string()))
            .collect();
        assert_eq!(links(text), expected);
    }

    #[test]
    fn every_form_a_reader_can_follow_is_read() {
        let text = concat!(
            "[double](a.md \"title\") [single](b.md 'title') [paren](c.md (title))\n",
            "[angle](<d e.md>) [nested](f(1).md) ![image](g.png)\n",
            "[full][ref] [collapsed][] [collapsed]\n",
            "<a href=\"h.md\">html</a> <a HREF='i.md'>single</a> <https://example.com>\n",
            r"[Evil \](fake.md) \[hacked](j.md)",
            "\n\n[ref]: k.md\n[collapsed]: l.md\n",
        );
        let targets: Vec<String> = links(text).into_iter().map(|(_, target)| target).collect();
        assert_eq!(
            targets,
            [
                "a.md",
                "b.md",
                "c.md",
                "d e.md",
                "f(1).md",
                "g.png",
                "k.md",
                "l.md",
                "l.md",
                "h.md",
                "i.md",
                "https://example.com",
                "j.md"
            ]
        );
        // The escaped brackets stay in the text of the one link
        assert_eq!(links(text).last().unwrap().0, "Evil ](fake.md) [hacked");
    }

    #[test]
    fn what_a_reader_cannot_follow_is_not_a_link() {
        let text = "`[code](a.md)`\n\n```\n[fenced](b.md)\n```\n\n<!-- [comment](c.md) -->\n\n\\[escaped](d.md)\n";
        assert_eq!(links(text), Vec::<(String, String)>::new());
    }
}
