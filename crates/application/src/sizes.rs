//! The size of the code of a project, read from the tree: for each unit, how many files and lines of code each layer
//! and level holds, the entry files, and the code in the paths `moving` and `unchecked` list, and the files of a layer
//! with the most lines. `rotproof sizes` prints it, and `rotproof check` sums it up in a line.
//!
//! A line of code is counted by `domain::sizes`. The files are the ones the checks of what the program does read
//! (`tree::program_files`): a test is none, and no file is counted twice. Nothing here fails: a file that cannot be
//! read as UTF-8 is left out, as the checks that read it say.

use std::collections::BTreeSet;
use std::io;

use crate::direction::{present_places, test_dirs};
use crate::tree::{exactly, is_file, program_files, program_files_in, read_code};
use domain::declaration::Declared;
use domain::definitions::Kind;
use domain::layers::{Language, Layout, table};
use domain::paths::{file_name, inside, within};
use domain::sizes::{lines_of_code, spans};
use protocol::inbound::{Defined, Longest, Part, Size, Sizes, UnitSizes};
use protocol::outbound::{Parsers, Tree};

/// How many of the longest files of a layer are said.
const LONGEST: usize = 3;

/// What `rotproof sizes` calls the code in the paths `moving` and `unchecked` list that no unit's root holds.
pub const OUTSIDE_THE_UNITS: &str = "outside the units";

/// What a read of a file is for, in the finding a file that is not UTF-8 would make, which this command drops.
const WHAT: &str = "lines of code";

/// A file of code and the lines of code it holds.
struct Counted {
    path: String,
    lines: usize,
}

/// The size of the code of `declared`: one entry for each unit, in the order it declares them, and one more, last, for
/// the code in `moving` and `unchecked` that sits in no unit's root. Empty for a repository of records only. An error
/// is a directory that could not be walked, or a file that could not be read at all.
pub fn measure(tree: &dyn Tree, parsers: &dyn Parsers, declared: &Declared) -> io::Result<Sizes> {
    let Some(layout) = &declared.layout else {
        return Ok(Sizes::default());
    };
    let mut moving = outside(tree, parsers, declared, layout, Part::Moving)?;
    let mut unchecked = outside(tree, parsers, declared, layout, Part::Unchecked)?;
    let places = present_places(tree, declared);
    let entries = declared.entries(&table());
    let mut units = Vec::new();
    for (i, unit) in declared.units.iter().enumerate() {
        let mut parts = Vec::new();
        let mut domain_defines = None;
        let mut unread = Vec::new();
        for layer in places
            .iter()
            .filter(|p| p.is_layer() && p.unit.name == unit.name)
        {
            let files = program_files(tree, layout, layer, WHAT, &mut unread)?;
            if layer.name == "domain" {
                domain_defines = Some(definitions(parsers, layout, &files));
            }
            let counts: Vec<Counted> = files
                .iter()
                .map(|(path, source)| count(parsers, layout, path, source))
                .collect();
            parts.push(size(
                &layer.name,
                Part::Layer,
                &counts.iter().collect::<Vec<_>>(),
            ));
            for level in places.iter().filter(|p| p.is_level_of(layer)) {
                let inner: Vec<&Counted> = counts
                    .iter()
                    .filter(|file| within(&file.path, &level.path))
                    .collect();
                parts.push(size(&level.name, Part::Level, &inner));
            }
        }
        let mut entry_files = Vec::new();
        for entry in entries
            .iter()
            .filter(|e| e.unit.name == unit.name && is_file(tree, &e.path))
        {
            if let Some(source) = read_code(tree, &entry.path, WHAT, &mut unread)? {
                entry_files.push(count(parsers, layout, &entry.path, &source));
            }
        }
        if !entry_files.is_empty() {
            parts.push(size(
                "entry",
                Part::Entry,
                &entry_files.iter().collect::<Vec<_>>(),
            ));
        }
        let here = |file: &Counted| belongs(declared, &file.path) == Some(i);
        parts.extend(outside_parts(declared, &mut moving, &mut unchecked, here));
        units.push(UnitSizes {
            name: unit.name.clone(),
            parts,
            domain_defines,
        });
    }
    // What no unit's root holds has an entry of its own, when there is any
    let loose = |file: &Counted| belongs(declared, &file.path).is_none();
    let parts = outside_parts(declared, &mut moving, &mut unchecked, loose);
    if parts.iter().any(|part| part.files > 0) {
        units.push(UnitSizes {
            name: OUTSIDE_THE_UNITS.to_string(),
            parts,
            domain_defines: None,
        });
    }
    Ok(Sizes { units })
}

/// The rows for `moving` and `unchecked` of the files `here` picks out of the two, which are taken from them: a row
/// for each field the declaration lists a path in, with no file when none of its code is `here`.
fn outside_parts(
    declared: &Declared,
    moving: &mut Vec<Counted>,
    unchecked: &mut Vec<Counted>,
    here: impl Fn(&Counted) -> bool,
) -> Vec<Size> {
    let mut parts = Vec::new();
    let listed = [
        (
            "moving",
            Part::Moving,
            !declared.declaration.moving.is_empty(),
        ),
        (
            "unchecked",
            Part::Unchecked,
            !declared.declaration.unchecked.is_empty(),
        ),
    ];
    for (name, part, listed) in listed {
        let files = match part {
            Part::Moving => &mut *moving,
            _ => &mut *unchecked,
        };
        if !listed {
            continue;
        }
        let (mine, rest): (Vec<Counted>, Vec<Counted>) =
            std::mem::take(files).into_iter().partition(&here);
        *files = rest;
        parts.push(size(name, part, &mine.iter().collect::<Vec<_>>()));
    }
    parts
}

/// The unit whose root holds `path`, the deepest one, by its place in the declaration. A project of one unit has every
/// path in it.
fn belongs(declared: &Declared, path: &str) -> Option<usize> {
    if declared.units.len() == 1 {
        return Some(0);
    }
    declared
        .units
        .iter()
        .enumerate()
        .filter(|(_, unit)| inside(path, &unit.root))
        .max_by_key(|(_, unit)| unit.root.len())
        .map(|(i, _)| i)
}

/// The row of a part made of `files`. The longest files are said of a layer alone.
fn size(name: &str, part: Part, files: &[&Counted]) -> Size {
    let mut longest: Vec<&Counted> = files
        .iter()
        .copied()
        .filter(|file| file.lines > 0)
        .collect();
    longest.sort_by(|a, b| b.lines.cmp(&a.lines).then_with(|| a.path.cmp(&b.path)));
    Size {
        name: name.to_string(),
        part,
        files: files.len(),
        lines: files.iter().map(|file| file.lines).sum(),
        longest: if part == Part::Layer {
            longest
                .into_iter()
                .take(LONGEST)
                .map(|file| Longest {
                    path: file.path.clone(),
                    lines: file.lines,
                })
                .collect()
        } else {
            Vec::new()
        },
    }
}

/// The lines of code of one file: the reader of its language gives what is not code (comments, and docstrings in
/// Python), and a Rust file loses its tests.
fn count(parsers: &dyn Parsers, layout: &Layout, path: &str, source: &str) -> Counted {
    let skipped = match layout.language {
        Language::Python => {
            let read = parsers.python(source, path);
            let mut skipped = spans(&read.comments);
            skipped.extend(read.docstrings);
            skipped
        }
        Language::TypeScript => spans(&parsers.typescript(source, path).comments),
        Language::Rust => {
            let mut skipped = spans(&domain::rust::comments(source));
            skipped.extend(domain::rust::test_spans(&domain::rust::code(source)));
            skipped
        }
    };
    Counted {
        path: path.to_string(),
        lines: lines_of_code(source, &skipped),
    }
}

/// How many functions and how many types the `files` define, by the outline of each: a file the reader gives up on
/// defines none, and in Rust what is under `#[cfg(test)]` is no definition of the program.
fn definitions(parsers: &dyn Parsers, layout: &Layout, files: &[(String, String)]) -> Defined {
    let mut found = Defined::default();
    for (path, source) in files {
        let Ok(outline) = parsers.outline(layout.language, source, path) else {
            continue;
        };
        let tests = match layout.language {
            Language::Rust => domain::rust::test_spans(&domain::rust::code(source)),
            Language::Python | Language::TypeScript => Vec::new(),
        };
        for definition in outline.iter().filter(|d| {
            !tests
                .iter()
                .any(|(from, to)| (*from..*to).contains(&d.start))
        }) {
            match definition.kind {
                Kind::Function => found.functions += 1,
                Kind::Type => found.types += 1,
                Kind::Other => {}
            }
        }
    }
    found
}

/// The code in the paths the declaration lists in `moving` or in `unchecked` (`part`), as the files `moving` and
/// `unchecked` hold: each code file once, a test none, and a file that is moving not `unchecked` too. A path that is
/// not there, or holds or sits in a layer, has none, as it is the structure check's to say.
fn outside(
    tree: &dyn Tree,
    parsers: &dyn Parsers,
    declared: &Declared,
    layout: &Layout,
    part: Part,
) -> io::Result<Vec<Counted>> {
    let paths: Vec<&str> = match part {
        Part::Moving => declared.moving().collect(),
        _ => declared.unchecked().collect(),
    };
    let tests = test_dirs(tree, layout, &declared.units);
    let mut unread = Vec::new();
    let mut seen = BTreeSet::new();
    let mut found = Vec::new();
    for path in paths
        .into_iter()
        .filter(|path| declared.overlapped(path).is_none())
    {
        let Ok((path, is_dir)) = exactly(tree, path) else {
            continue;
        };
        let files = if is_dir {
            program_files_in(tree, layout, &path, WHAT, &mut unread)?
        } else if layout.has_comments(file_name(&path)) && !layout.is_test_file(&path) {
            read_code(tree, &path, WHAT, &mut unread)?
                .map(|source| vec![(path, source)])
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        for (file, source) in files {
            if tests.iter().any(|dir| within(&file, dir))
                || (part == Part::Unchecked && declared.is_moving(&file))
                || !seen.insert(file.clone())
            {
                continue;
            }
            found.push(count(parsers, layout, &file, &source));
        }
    }
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::fake::Fake;
    use domain::declaration::parse_declaration;
    use infrastructure::outbound::readers::Readers;

    /// The size of the project of `declaration` that holds `files`.
    fn measured(declaration: &str, files: &[(&str, &str)]) -> Sizes {
        let declared = Declared::new(parse_declaration(declaration).unwrap()).unwrap();
        measure(&Fake::new(files), &Readers, &declared).unwrap()
    }

    /// The files and the lines of the part `name` of the unit at `unit`.
    fn part(sizes: &Sizes, unit: usize, name: &str) -> (usize, usize) {
        let found = sizes.units[unit]
            .parts
            .iter()
            .find(|part| part.name == name)
            .unwrap_or_else(|| panic!("no part {name}: {sizes:#?}"));
        (found.files, found.lines)
    }

    fn defined(functions: usize, types: usize) -> Option<Defined> {
        Some(Defined { functions, types })
    }

    const PYTHON: &str = "stack = \"python\"\nareas = []\nui_design = \"none\"\n";

    #[test]
    fn python_lines_of_code_leave_out_blank_lines_comments_and_docstrings_but_not_other_strings() {
        let rules = "\"\"\"Rules.\"\"\"\n\n# a comment\ndef total(a, b):\n    \"\"\"Add.\n\n    More.\n    \"\"\"\n    \
                     return a + b  # trailing\n\n\nclass Money:\n    \"\"\"Money.\"\"\"\n    x = \"\"\"not\nadocstring\"\"\"\n";
        let sizes = measured(
            PYTHON,
            &[
                ("domain/__init__.py", "\"\"\"The domain.\"\"\"\n"),
                ("domain/rules.py", rules),
                ("application/__init__.py", ""),
                ("application/use.py", "x = 1\n\n\ny = 2\n"),
            ],
        );
        // `def`, `return`, `class`, and the two lines of the string that is a value
        assert_eq!(part(&sizes, 0, "domain"), (2, 5));
        assert_eq!(part(&sizes, 0, "application"), (2, 2));
        assert_eq!(sizes.units[0].domain_defines, defined(1, 1));
        assert_eq!(sizes.units.len(), 1);
        assert_eq!(sizes.units[0].name, "");
    }

    #[test]
    fn typescript_lines_of_code_leave_out_comments_and_the_files_the_layout_names_as_tests() {
        let a = "// c\nexport const a = 1; // t\n\n/* block\n   more */\nexport function f() {\n  return a;\n}\n";
        let sizes = measured(
            "stack = \"typescript\"\nareas = []\nui_design = \"none\"\n",
            &[
                ("src/domain/index.ts", "/** The domain. */\nexport {};\n"),
                ("src/domain/a.ts", a),
                ("src/domain/a.test.ts", a),
                ("src/domain/a.spec.tsx", a),
                ("src/domain/__tests__/helper.ts", a),
                // The name is the whole of what makes a file a test
                ("src/domain/b.tests.ts", "export const b = 2;\n"),
                ("src/domain/notes.json", "{\"a\": 1}\n"),
            ],
        );
        assert_eq!(part(&sizes, 0, "domain"), (3, 6));
    }

    #[test]
    fn rust_lines_of_code_leave_out_comments_the_items_under_cfg_test_and_the_files_of_a_test_module()
     {
        let lib = "//! Doc.\n\npub mod a;\n\n#[cfg(test)]\nmod tests;\n\n/// doc\npub fn f() -> u32 {\n    // inside\n    1\n}\n\n\
                   #[cfg(test)]\nmod more {\n    fn t() {\n        let _ = \"multi\nline\";\n    }\n}\n";
        let sizes = measured(
            "stack = \"rust\"\nareas = []\nui_design = \"none\"\n",
            &[
                ("crates/domain/Cargo.toml", "[package]\nname = \"domain\"\n"),
                ("crates/domain/src/lib.rs", lib),
                ("crates/domain/src/a.rs", "pub struct A;\n"),
                ("crates/domain/src/tests.rs", "fn t() {}\nfn u() {}\n"),
                (
                    "crates/infrastructure/Cargo.toml",
                    "[package]\nname = \"infrastructure\"\n",
                ),
                ("crates/infrastructure/src/lib.rs", "pub mod inbound;\n"),
                (
                    "crates/infrastructure/src/inbound/mod.rs",
                    "pub fn i() {}\n",
                ),
                ("src/main.rs", "fn main() {}\n"),
            ],
        );
        // `pub mod a;`, the head of `f`, its `1` and its `}`, and the struct
        assert_eq!(part(&sizes, 0, "domain"), (2, 5));
        assert_eq!(sizes.units[0].domain_defines, defined(1, 1));
        // A level is counted in its layer too
        assert_eq!(part(&sizes, 0, "infrastructure"), (2, 2));
        assert_eq!(part(&sizes, 0, "infrastructure.inbound"), (1, 1));
        assert_eq!(part(&sizes, 0, "entry"), (1, 1));
    }

    #[test]
    fn the_longest_files_of_a_layer_are_the_three_with_the_most_lines_longest_first() {
        let long = |n: usize| "x = 1\n".repeat(n);
        let (a, b, c, d) = (long(4), long(9), long(2), long(9));
        let sizes = measured(
            PYTHON,
            &[
                ("domain/a.py", &a),
                ("domain/b.py", &b),
                ("domain/c.py", &c),
                ("domain/d.py", &d),
                ("domain/e.py", "\n# nothing\n"),
            ],
        );
        let domain = sizes.units[0]
            .parts
            .iter()
            .find(|part| part.name == "domain")
            .unwrap();
        let said: Vec<(&str, usize)> = domain
            .longest
            .iter()
            .map(|file| (file.path.as_str(), file.lines))
            .collect();
        // Equal files by their path, and none with no lines
        assert_eq!(
            said,
            [("domain/b.py", 9), ("domain/d.py", 9), ("domain/a.py", 4)]
        );
        assert_eq!(domain.files, 5);
    }

    #[test]
    fn moving_and_unchecked_are_a_row_each_and_a_file_in_both_is_moving_only() {
        let sizes = measured(
            &format!("{PYTHON}moving = [\"legacy\"]\nunchecked = [\"legacy/gen\", \"vendor\"]\n"),
            &[
                ("domain/__init__.py", ""),
                ("legacy/a.py", "a = 1\n# c\n\nb = 2\n"),
                ("legacy/gen/g.py", "g = 1\n"),
                ("vendor/v.py", "v = 1\nw = 2\nz = 3\n"),
                ("vendor/notes.txt", "not code\n"),
            ],
        );
        assert_eq!(part(&sizes, 0, "moving"), (2, 3));
        assert_eq!(part(&sizes, 0, "unchecked"), (1, 3));
        // The rows are there only when the declaration lists a path
        let none = measured(
            PYTHON,
            &[("domain/__init__.py", ""), ("legacy/a.py", "a = 1\n")],
        );
        assert!(
            none.units[0]
                .parts
                .iter()
                .all(|part| part.name != "moving" && part.name != "unchecked")
        );
    }

    #[test]
    fn code_that_is_moving_leaves_out_its_tests_in_typescript_and_rust() {
        let sizes = measured(
            "stack = \"typescript\"\nareas = []\nui_design = \"none\"\nmoving = [\"src/legacy\"]\n",
            &[
                ("src/domain/index.ts", ""),
                ("src/legacy/a.ts", "export const a = 1;\n"),
                ("src/legacy/a.test.ts", "export const t = 1;\n"),
                ("src/legacy/__tests__/b.ts", "export const t = 1;\n"),
            ],
        );
        assert_eq!(part(&sizes, 0, "moving"), (1, 1));
        let sizes = measured(
            "stack = \"rust\"\nareas = []\nui_design = \"none\"\nmoving = [\"old\"]\n",
            &[
                ("crates/domain/Cargo.toml", "[package]\nname = \"domain\"\n"),
                (
                    "old/lib.rs",
                    "mod a;\n#[cfg(test)]\nmod tests;\n#[cfg(test)]\nmod t {\n    fn x() {}\n}\n",
                ),
                ("old/a.rs", "fn a() {}\n"),
                ("old/tests.rs", "fn t() {}\n"),
            ],
        );
        assert_eq!(part(&sizes, 0, "moving"), (2, 2));
    }

    #[test]
    fn a_project_of_units_counts_each_unit_and_what_no_unit_holds_apart() {
        let declaration = format!(
            "{PYTHON}moving = [\"billing/old\", \"legacy\"]\nunchecked = [\"gen\"]\n\n[[unit]]\nname = \"core\"\npath = \"core\"\n\n\
             [[unit]]\nname = \"billing\"\npath = \"billing\"\n"
        );
        let sizes = measured(
            &declaration,
            &[
                ("core/domain/a.py", "a = 1\n"),
                ("billing/domain/a.py", "a = 1\nb = 2\n"),
                ("billing/old/x.py", "x = 1\ny = 2\nz = 3\n"),
                ("legacy/y.py", "y = 1\n"),
                ("gen/z.py", "z = 1\n"),
            ],
        );
        let names: Vec<&str> = sizes.units.iter().map(|unit| unit.name.as_str()).collect();
        assert_eq!(names, ["core", "billing", OUTSIDE_THE_UNITS]);
        assert_eq!(part(&sizes, 0, "domain"), (1, 1));
        assert_eq!(part(&sizes, 1, "domain"), (1, 2));
        assert_eq!(part(&sizes, 0, "moving"), (0, 0));
        assert_eq!(part(&sizes, 1, "moving"), (1, 3));
        assert_eq!(part(&sizes, 2, "moving"), (1, 1));
        assert_eq!(part(&sizes, 2, "unchecked"), (1, 1));
    }

    #[test]
    fn a_repository_of_records_only_has_nothing_to_measure() {
        let sizes = measured("stack = \"none\"\nareas = []\n", &[("a.py", "a = 1\n")]);
        assert_eq!(sizes, Sizes::default());
    }
}
