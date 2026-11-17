//! What the inner layers read of the code outside the layers that no check reads.
//!
//! The direction check lets a layer import the code of its own unit that sits in no layer, which is where a path in
//! `unchecked` or `moving` is: the code there is not judged, and neither is what an import of it brings in. The check
//! of the I/O of the inner layers judges the file that imports it and not the file it imports, so I/O hidden in a
//! helper in such a path is seen by no check. `rotproof check` lists every such import on every run, apart from the
//! findings, and fails nothing for it: the list says where to look, and the helper is moved into a layer, or out of
//! the inner layers' reach, by a person.
//!
//! Only the code of the four layers the I/O check judges (`domain`, `protocol`, `application`, `presentation` and
//! their levels) is read; a path in `unchecked` or `moving` that holds or sits in a layer switches nothing off and is
//! not one.

use crate::declaration::Declared;
use crate::inner_io::is_judged;
use crate::layers::Place;

/// One import from the code of an inner layer into a path of `unchecked` or `moving`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Outside {
    /// The file that imports, from the root
    pub file: String,
    pub line: usize,
    /// What it imports as the direction check names it: a Python dotted name, a TypeScript specifier, the name of a
    /// crate's dependency
    pub import: String,
    /// The field of the declaration that lists the path it lands in
    pub listed: &'static str,
}

impl Outside {
    /// The line printed for it: `domain/x.py:3 imports legacy.io_helper, in a path of moving`.
    pub fn said(&self) -> String {
        format!(
            "{}:{} imports {}, in a path of {}",
            self.file, self.line, self.import, self.listed
        )
    }
}

/// Whether an import from the place `from` is worth asking where it lands: `from` is a place of an inner layer, and the
/// declaration lists a path in `unchecked` or `moving` at all.
pub fn asks(declared: &Declared, from: &Place) -> bool {
    is_judged(from)
        && !(declared.declaration.unchecked.is_empty() && declared.declaration.moving.is_empty())
}

/// The import of `import` by `file` on `line`, from the place `from`, when it lands in a path of `unchecked` or
/// `moving`. `lands` is every path the import may be of (a Python module is `legacy/io_helper.py` or
/// `legacy/io_helper/__init__.py`).
pub fn outside(
    declared: &Declared,
    from: &Place,
    file: &str,
    line: usize,
    import: &str,
    lands: &[String],
) -> Option<Outside> {
    if !asks(declared, from) {
        return None;
    }
    let listed = if lands.iter().any(|path| declared.is_unchecked(path)) {
        "unchecked"
    } else if lands.iter().any(|path| declared.is_moving(path)) {
        "moving"
    } else {
        return None;
    };
    Some(Outside {
        file: file.to_string(),
        line,
        import: import.to_string(),
        listed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::declaration::parse_declaration;

    fn declared(rest: &str) -> Declared {
        Declared::new(
            parse_declaration(&format!(
                "stack = \"python\"\nareas = []\nui = \"none\"\n{rest}"
            ))
            .unwrap(),
        )
        .unwrap()
    }

    fn place(declared: &Declared, name: &str) -> Place {
        declared
            .places
            .iter()
            .find(|place| place.name == name)
            .unwrap()
            .clone()
    }

    fn said(declared: &Declared, from: &str, lands: &[&str]) -> Option<String> {
        let lands: Vec<String> = lands.iter().map(|path| path.to_string()).collect();
        outside(
            declared,
            &place(declared, from),
            "x.py",
            3,
            "legacy.helper",
            &lands,
        )
        .map(|found| found.said())
    }

    #[test]
    fn an_inner_layer_importing_a_path_of_unchecked_or_moving_is_named_with_the_field_that_lists_it()
     {
        let declared = declared("unchecked = [\"scripts\"]\nmoving = [\"legacy\"]\n");
        assert_eq!(
            said(
                &declared,
                "domain",
                &["legacy/helper.py", "legacy/helper/__init__.py"]
            ),
            Some("x.py:3 imports legacy.helper, in a path of moving".to_string())
        );
        assert_eq!(
            said(&declared, "application", &["scripts/helper.py"]),
            Some("x.py:3 imports legacy.helper, in a path of unchecked".to_string())
        );
        // A file named alone is a path as well
        let declared = self::declared("moving = [\"legacy/helper.py\"]\n");
        assert!(said(&declared, "domain", &["legacy/helper.py"]).is_some());
    }

    #[test]
    fn what_is_not_an_inner_layer_or_not_a_listed_path_is_not_named() {
        let declared = declared("unchecked = [\"scripts\"]\nmoving = [\"legacy\"]\n");
        // `infrastructure` and `composition` are free to read it
        assert_eq!(
            said(&declared, "infrastructure.outbound", &["legacy/helper.py"]),
            None
        );
        assert_eq!(said(&declared, "composition", &["legacy/helper.py"]), None);
        // A path that is listed in neither
        assert_eq!(said(&declared, "domain", &["other/helper.py"]), None);
    }

    #[test]
    fn a_declaration_that_lists_no_path_asks_for_nothing() {
        let none = declared("");
        assert!(!asks(&none, &place(&none, "domain")));
        let some = declared("moving = [\"legacy\"]\n");
        assert!(asks(&some, &place(&some, "domain")));
        assert!(!asks(&some, &place(&some, "composition")));
    }
}
