//! The outline of a file, as the definitions a reader found in it, and the one rule that reads the text of a key
//! from it: each language's reader gives the outline (`python::outline` and `typescript::outline` in
//! `infrastructure`, `rust::outline` here), and a key of `follows` selects its pieces here, the same for every
//! language.

/// One definition of a file, as a reader found it: a function, a class, a method, a type, a variant. Every language
/// is read into the same form, which [`pieces`] reads a key from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Definition {
    /// Its names from the outermost: `["Table", "add"]` for the method `add` of `Table`, `["f"]` for a function at the
    /// top
    pub names: Vec<String>,
    /// Where its source starts in the file, in bytes: at its first decorator or attribute, or at its own start
    pub start: usize,
    /// Where its source ends, in bytes
    pub end: usize,
}

/// The source of every definition of `outline`, the outline of `source`, whose names are `dotted` (`Table.add`), in the
/// order they appear in the file. Empty when the file defines no such name. A definition whose range is not in `source`
/// is skipped: a reader gives ranges of the text it read.
pub fn pieces<'s>(source: &'s str, outline: &[Definition], dotted: &str) -> Vec<&'s str> {
    let wanted: Vec<&str> = dotted.split('.').collect();
    let mut found: Vec<&Definition> = outline
        .iter()
        .filter(|d| {
            d.names
                .iter()
                .map(String::as_str)
                .eq(wanted.iter().copied())
        })
        .collect();
    found.sort_by_key(|d| d.start);
    found
        .into_iter()
        .filter_map(|d| source.get(d.start..d.end))
        .collect()
}

/// The text a key that names `dotted` is hashed on: the [`pieces`] of it joined with `\n`, so a name that is more than
/// one piece (an overload and its implementation, a getter and its setter, the `impl` blocks of a type) is hashed on
/// all of them. `None` when there is no piece: the file does not define it. Every language joins here.
pub fn selected(source: &str, outline: &[Definition], dotted: &str) -> Option<String> {
    let pieces = pieces(source, outline, dotted);
    (!pieces.is_empty()).then(|| pieces.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn definition(names: &[&str], start: usize, end: usize) -> Definition {
        Definition {
            names: names.iter().map(|n| n.to_string()).collect(),
            start,
            end,
        }
    }

    #[test]
    fn a_key_selects_the_definitions_whose_names_are_all_of_it() {
        let source = "fn f() {}\nstruct T;\nimpl T { fn f() {} }\nfn g() {}\n";
        let outline = [
            definition(&["f"], 0, 9),
            definition(&["T"], 10, 19),
            definition(&["T"], 20, 40),
            definition(&["T", "f"], 29, 38),
            definition(&["g"], 41, 50),
        ];
        let at = |dotted: &str| pieces(source, &outline, dotted);
        // The function at the top, not the method of the same name
        assert_eq!(at("f"), ["fn f() {}"]);
        assert_eq!(at("T.f"), ["fn f() {}"]);
        assert_eq!(at("g"), ["fn g() {}"]);
        // The names are the whole of the key: not the start of a longer one, and not the end of it
        assert_eq!(at("T"), ["struct T;", "impl T { fn f() {} }"]);
        assert!(at("T.f.x").is_empty() && at("x.f").is_empty() && at("").is_empty());
        assert!(at("h").is_empty());
    }

    #[test]
    fn the_pieces_of_a_name_are_joined_in_the_order_they_appear_with_a_line_break() {
        let source = "aaa\nbbb\nccc\n";
        // A reader may give the outline in any order: the file's order is the one that counts
        let outline = [
            definition(&["x"], 8, 11),
            definition(&["y"], 4, 7),
            definition(&["x"], 0, 3),
        ];
        assert_eq!(selected(source, &outline, "x").as_deref(), Some("aaa\nccc"));
        assert_eq!(selected(source, &outline, "y").as_deref(), Some("bbb"));
        assert_eq!(selected(source, &outline, "z"), None);
        let reversed = [
            definition(&["x"], 0, 3),
            definition(&["y"], 4, 7),
            definition(&["x"], 8, 11),
        ];
        assert_eq!(
            selected(source, &outline, "x"),
            selected(source, &reversed, "x")
        );
        // The order is part of the text
        assert_ne!(
            selected(source, &outline, "x"),
            selected("ccc\nbbb\naaa\n", &outline, "x")
        );
    }

    #[test]
    fn a_range_that_is_not_in_the_text_is_no_piece() {
        let source = "é\nabc\n";
        // Out of the text, and in the middle of a character
        let outline = [
            definition(&["a"], 3, 30),
            definition(&["b"], 1, 3),
            definition(&["c"], 3, 6),
        ];
        assert_eq!(selected(source, &outline, "a"), None);
        assert_eq!(selected(source, &outline, "b"), None);
        assert_eq!(selected(source, &outline, "c").as_deref(), Some("abc"));
    }
}
