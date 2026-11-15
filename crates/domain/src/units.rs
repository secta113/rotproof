//! The units of a project: the parts of its code that each hold the layers (a package of a monorepo, a module of a
//! modular monolith).
//!
//! A project that declares none has one implicit unit, which holds the layers where the layout puts them and has the
//! layout's entry file, so every check reads units the same way whether or not the project says so. A project that
//! declares some (`[[unit]]` in `.config/rotproof.toml`) has those and no implicit one: [`declared`] makes them from
//! what the declaration says, and says every reason it cannot be read, from the declaration and the layout alone.

use crate::declaration::{UnitDeclaration, unreadable};
use crate::layers::{Language, Layout, Place, Table, UnitTag};
use crate::paths::{Miswritten, file_name, inside, miswritten, parent, within};
use crate::python_stdlib::is_standard_library;

/// One unit: where its layers sit, and what it is to the others.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unit {
    /// "" for the implicit unit, which no message names
    pub name: String,
    /// The directory, from the root, where its layers sit
    pub path: String,
    /// The directory, from the root, where its manifest is
    pub root: String,
    /// Whether any unit may import it
    pub shared: bool,
    /// Its entry files, from the root: the unit has them when it is an app
    pub entries: Vec<String>,
    /// The names of the places it lacks, as `absent` writes them
    pub absent: Vec<String>,
    /// Whether the project declares it. The implicit unit is not declared
    pub declared: bool,
}

impl Unit {
    /// The one unit of a project that declares none: its layers sit in the layout's `scope`, and its entry is the
    /// layout's, so it is an app when the layout has an entry. `absent` is what the project declares absent.
    pub fn implicit(layout: &Layout, absent: &[String]) -> Unit {
        Unit {
            name: String::new(),
            path: layout.scope.clone(),
            root: layout.scope.clone(),
            shared: false,
            entries: layout.entry.iter().cloned().collect(),
            absent: absent.to_vec(),
            declared: false,
        }
    }

    /// What a place says of the unit it sits in.
    pub fn tag(&self) -> UnitTag {
        UnitTag {
            name: self.name.clone(),
            shared: self.shared,
            app: !self.entries.is_empty(),
        }
    }

    /// The places of this unit, as `layout` makes them with its layers in the unit's `path`.
    pub fn places(&self, layout: &Layout, table: &Table) -> Vec<Place> {
        layout.places_in(table, &self.path, &self.tag())
    }

    /// The name Python imports this unit by and the directory it is looked up in: the last name of its `path` and the
    /// directory that holds it. `None` for a unit that is not declared or sits at the root of the project, which has no
    /// directory to name it (what its imports start with is the layer, from the root).
    pub fn python_import(&self) -> Option<(&str, &str)> {
        (self.declared && !self.path.is_empty())
            .then(|| (file_name(&self.path), parent(&self.path)))
    }

    /// The file `path` that sits directly in this unit's directory and belongs to its package (a Python unit's
    /// `__init__.py`: [`Layout::unit_files`]), as a place of its own, named by its path: it imports no layer, and what
    /// it imports of another unit is judged as the rules between units say.
    pub fn own_file_place(&self, layout: &Layout, table: &Table, path: &str) -> Place {
        let mut place = layout.entry_at(table, path, &self.tag());
        place.imports.clear();
        place.role = format!(
            "The package file of unit {}: it imports no layer.",
            self.name
        );
        place
    }

    /// The entry files of this unit, each as a place of its own.
    pub fn entry_places(&self, layout: &Layout, table: &Table) -> Vec<Place> {
        let tag = self.tag();
        self.entries
            .iter()
            .map(|path| layout.entry_at(table, path, &tag))
            .collect()
    }
}

/// The units `declarations` name, and every reason they cannot be read with `layout` of the stack `stack`: each a line
/// for the declaration, those of each unit in the order the project declares them, then those of the units together.
/// The units are made all the same, so that one failure does not hide the others; a project that fails is not read.
pub fn declared(
    stack: &str,
    layout: &Layout,
    table: &Table,
    declarations: &[UnitDeclaration],
) -> (Vec<Unit>, Vec<String>) {
    let mut units: Vec<Unit> = declarations.iter().map(read).collect();
    let mut found = Vec::new();
    // Whether the path of a unit is written well, which what compares it with the others needs
    let fit: Vec<bool> = declarations
        .iter()
        .zip(&mut units)
        .map(|(declaration, unit)| own(stack, layout, table, declaration, unit, &mut found))
        .collect();
    let mut said: Vec<&str> = Vec::new();
    for (i, unit) in units.iter().enumerate() {
        let again = units[..i].iter().any(|earlier| earlier.name == unit.name);
        if again && !said.contains(&unit.name.as_str()) {
            said.push(&unit.name);
            found.push(unreadable(&format!("two units are named {:?}", unit.name)));
        }
    }
    if layout.language == Language::Python {
        imported_alike(&units, &fit, &mut found);
    }
    together(layout, table, &units, &fit, &mut found);
    (units, found)
}

/// The units whose directories have one name, each name said once with their paths, in the order the project declares
/// them: Python imports a unit as the name of its directory, so a name that two directories have is one import.
fn imported_alike(units: &[Unit], fit: &[bool], found: &mut Vec<String>) {
    let mut named: Vec<(&str, Vec<&str>)> = Vec::new();
    for (unit, _) in units.iter().zip(fit).filter(|(_, fit)| **fit) {
        let Some((name, _)) = unit.python_import() else {
            continue;
        };
        match named.iter_mut().find(|(known, _)| *known == name) {
            Some((_, paths)) if !paths.contains(&unit.path.as_str()) => paths.push(&unit.path),
            Some(_) => {}
            None => named.push((name, vec![&unit.path])),
        }
    }
    for (name, paths) in named.iter().filter(|(_, paths)| paths.len() > 1) {
        found.push(unreadable(&format!(
            "two units are imported as {name} ({})",
            paths.join(", ")
        )));
    }
}

/// The unit a declaration names, as it is compared: no reason it cannot be read yet.
fn read(declaration: &UnitDeclaration) -> Unit {
    let path = written(&declaration.path);
    Unit {
        name: declaration.name.clone(),
        root: declaration.root.as_deref().map_or(path.clone(), written),
        path,
        shared: declaration.shared,
        entries: declaration.entry.to_vec(),
        absent: declaration.absent.clone(),
        declared: true,
    }
}

/// A path of a unit as it is compared: `.` is the root, and a `/` at its end counts for nothing, as in `unchecked`.
fn written(path: &str) -> String {
    if path == "." {
        String::new()
    } else {
        path.trim_end_matches('/').to_string()
    }
}

/// What a message shows of a path that may be the root.
fn shown(path: &str) -> &str {
    if path.is_empty() { "." } else { path }
}

/// A name a unit can have: it starts with a lowercase letter, then lowercase letters, digits and `_`. It is the name of
/// a crate's prefix in Rust and of the package a Python unit is imported as, so it is no more than both can take.
fn is_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// The names of the directories a unit's own directory must not be named like: a layer's, and a path that is no layer
/// (`tests`), which the unit around it would read as one.
fn layer_names(layout: &Layout, table: &Table) -> Vec<String> {
    let layers = layout.places(table);
    let layers = layers.iter().filter(|p| p.parent.is_none());
    layers
        .map(|p| file_name(&p.path).to_string())
        .chain(layout.not_layers.iter().map(|p| file_name(p).to_string()))
        .collect()
}

/// What is wrong with one unit alone, added to `found`; and whether its path is written well. Its entry files that
/// are not are left out of `unit`, so that the units are not compared by them.
fn own(
    stack: &str,
    layout: &Layout,
    table: &Table,
    declaration: &UnitDeclaration,
    unit: &mut Unit,
    found: &mut Vec<String>,
) -> bool {
    let name = unit.name.clone();
    if !is_name(&name) {
        found.push(unreadable(&format!(
            "unit name {name:?} is not a name: start with a lowercase letter, then lowercase letters, digits and _"
        )));
    }
    let (mut fit, mut root_fit) = (true, true);
    let mut paths = vec![(
        "path",
        declaration.path.as_str(),
        "packages/billing/src/billing",
    )];
    if let Some(root) = &declaration.root {
        paths.push(("root", root, "packages/billing"));
    }
    for (field, raw, example) in paths {
        // `.` is the root; every other path is written as `unchecked` writes one
        let Some(why) = (raw != ".")
            .then(|| miswritten(raw.trim_end_matches('/')))
            .flatten()
        else {
            continue;
        };
        if field == "path" {
            fit = false;
        } else {
            root_fit = false;
        }
        found.push(unreadable(&match why {
            Miswritten::Backslash => format!(
                "unit {name:?} has {field} {raw:?}: separate a path with /, which every platform reads"
            ),
            Miswritten::NotFromRoot => format!(
                "unit {name:?} has {field} {raw:?}: write a path from the root, such as {example}"
            ),
        }));
    }
    if fit && !unit.path.is_empty() {
        let directory = file_name(&unit.path);
        let path = &unit.path;
        if layout.language == Language::Python && directory != name {
            // Python imports a unit by the name of its directory
            found.push(unreadable(&format!(
                "unit {name:?} is at {path}: Python imports a unit as the name of its directory, so name the unit \
                 {directory:?}, or the directory {name:?}"
            )));
        }
        if layout.language == Language::Python && is_standard_library(directory) {
            found.push(unreadable(&format!(
                "unit {name:?} is at {path}/, named like a module of Python's standard library: Python would \
                 import one or the other depending on how it starts; name the directory otherwise"
            )));
        }
        if layer_names(layout, table).iter().any(|n| n == directory) {
            found.push(unreadable(&format!(
                "unit {name:?} is at {path}, named like a layer: name its directory otherwise, as it would be read \
                 as the layer {directory} of the unit around it"
            )));
        }
    }
    if fit && root_fit && !inside(&unit.path, &unit.root) {
        found.push(unreadable(&format!(
            "unit {name:?} has root {:?}, which does not hold its path {:?}",
            shown(&unit.root),
            shown(&unit.path)
        )));
    }
    let example = layout.entry.as_deref().map_or("main", file_name);
    let mut entries = Vec::new();
    for entry in &declaration.entry.to_vec() {
        if miswritten(entry).is_some() {
            found.push(unreadable(&format!(
                "unit {name:?} has entry {entry:?}: write a path from the root, such as apps/cli/{example}"
            )));
        } else if !layout.has_comments(file_name(entry)) {
            found.push(unreadable(&format!(
                "unit {name:?} declares entry {entry:?}, which is no code of the {stack} stack"
            )));
        } else {
            entries.push(entry.clone());
        }
    }
    if unit.shared && !declaration.entry.to_vec().is_empty() {
        found.push(unreadable(&format!(
            "unit {name:?} is shared and declares entry: an app (a unit with entry) wires the units and no unit \
             imports it, so a unit is not both"
        )));
    }
    unit.entries = entries;
    fit
}

/// What is wrong with the units together, added to `found`: those whose path is written well are compared.
fn together(layout: &Layout, table: &Table, units: &[Unit], fit: &[bool], found: &mut Vec<String>) {
    for (i, unit) in units.iter().enumerate().filter(|(i, _)| fit[*i]) {
        let name = &unit.name;
        for (j, around) in units.iter().enumerate().filter(|(j, _)| fit[*j] && *j != i) {
            if unit.path == around.path {
                if i < j {
                    found.push(unreadable(&format!(
                        "units {name:?} and {:?} both have path {:?}",
                        around.name,
                        shown(&unit.path)
                    )));
                }
            } else if inside(&unit.path, &around.path) {
                // A unit may sit inside another, beside its layers, and nowhere else in it
                let places = layout.places_in(table, &around.path, &around.tag());
                let layer = places
                    .iter()
                    .find(|p| p.parent.is_none() && within(&unit.path, &p.path));
                if let Some(layer) = layer {
                    found.push(unreadable(&format!(
                        "unit {name:?} is at {}, inside the layer {} of unit {:?}: a unit sits beside the layers of \
                         the unit around it",
                        unit.path, layer.name, around.name
                    )));
                }
            }
        }
        for entry in &unit.entries {
            let first = units
                .iter()
                .position(|other| other.entries.contains(entry))
                .expect("the unit itself has it");
            if first != i {
                found.push(unreadable(&format!(
                    "units {:?} and {name:?} both declare entry {entry:?}",
                    units[first].name
                )));
                continue;
            }
            // The unit that holds an entry is the innermost one whose path holds it: the unit that declares it, or
            // none
            let inner = units
                .iter()
                .enumerate()
                .filter(|(j, around)| fit[*j] && inside(entry, &around.path))
                .max_by_key(|(_, around)| around.path.len());
            if let Some((_, around)) = inner.filter(|(_, around)| around.name != *name) {
                found.push(unreadable(&format!(
                    "unit {name:?} declares entry {entry:?}, inside the path of unit {:?} ({}/)",
                    around.name,
                    shown(&around.path)
                )));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::declaration::{Declared, parse_declaration};
    use crate::layers::{STACKS, layout, table};

    fn stacks() -> Vec<Layout> {
        STACKS
            .iter()
            .map(|(stack, _)| {
                let ui = if *stack == "rust" { "none" } else { "atomic" };
                layout(stack, ui).unwrap()
            })
            .collect()
    }

    #[test]
    fn the_implicit_unit_sits_in_the_scope_and_has_the_entry_of_the_layout() {
        for layout in stacks() {
            let unit = Unit::implicit(&layout, &["ui".to_string()]);
            assert_eq!(unit.path, layout.scope);
            assert_eq!(unit.root, layout.scope);
            assert_eq!(
                unit.entries,
                layout.entry.iter().cloned().collect::<Vec<_>>()
            );
            assert_eq!(unit.absent, ["ui"]);
            assert!(!unit.declared && !unit.shared && unit.name.is_empty());
            assert_eq!(
                unit.tag(),
                UnitTag {
                    name: String::new(),
                    shared: false,
                    app: layout.entry.is_some()
                }
            );
        }
    }

    #[test]
    fn the_places_of_the_implicit_unit_are_the_places_of_the_layout() {
        let table = table();
        for layout in stacks() {
            let unit = Unit::implicit(&layout, &[]);
            let from_layout = layout.places(&table);
            // The unit's tag says it is an app: that is the one thing that differs from the layout's own
            let mut tagged = unit.places(&layout, &table);
            assert!(tagged.iter().all(|p| p.unit == unit.tag()));
            for place in &mut tagged {
                place.unit = UnitTag::default();
            }
            assert_eq!(tagged, from_layout);
            assert_eq!(
                layout.places_in(&table, &layout.scope, &UnitTag::default()),
                from_layout
            );
            let entries = unit.entry_places(&layout, &table);
            assert_eq!(entries.len(), layout.entry.iter().count());
            for (entry, own) in entries.iter().zip(layout.entry(&table)) {
                assert_eq!(entry.unit, unit.tag());
                assert_eq!((&entry.name, &entry.path), (&own.name, &own.path));
            }
        }
    }

    #[test]
    fn a_unit_elsewhere_has_the_same_places_moved_and_tagged() {
        let table = table();
        for layout in stacks() {
            let tag = UnitTag {
                name: "billing".into(),
                shared: true,
                app: false,
            };
            let base = "packages/billing/src/billing";
            let own = layout.places(&table);
            let moved = layout.places_in(&table, base, &tag);
            assert_eq!(moved.len(), own.len());
            for (moved, own) in moved.iter().zip(&own) {
                assert_eq!((&moved.name, &moved.parent), (&own.name, &own.parent));
                assert_eq!((&moved.imports, &moved.below), (&own.imports, &own.below));
                assert_eq!(moved.unit, tag);
                let inside = if layout.scope.is_empty() {
                    own.path.as_str()
                } else {
                    own.path
                        .strip_prefix(&format!("{}/", layout.scope))
                        .unwrap()
                };
                assert_eq!(moved.path, format!("{base}/{inside}"));
                // Its files are made where it is
                assert!(
                    moved
                        .files
                        .iter()
                        .all(|(path, _)| path.starts_with(&format!("{}/", moved.path)))
                );
            }
        }
    }

    #[test]
    fn places_of_one_name_in_two_units_are_two_places() {
        let table = table();
        let layout = layout("python", "atomic").unwrap();
        let tag = |name: &str| UnitTag {
            name: name.into(),
            shared: false,
            app: false,
        };
        let a = layout.places_in(&table, "a/a", &tag("a"));
        let b = layout.places_in(&table, "b/b", &tag("b"));
        for (a, b) in a.iter().zip(&b) {
            assert_eq!(a.name, b.name);
            assert!(a.is_same(a));
            assert!(!a.is_same(b), "{} of a is {} of b", a.name, b.name);
        }
        // A level is a level of the layer of its own unit only
        let layer = |places: &[Place]| places.iter().find(|p| p.name == "ui").unwrap().clone();
        let level = |places: &[Place]| {
            places
                .iter()
                .find(|p| p.name == "ui.atoms")
                .unwrap()
                .clone()
        };
        assert!(level(&a).is_level_of(&layer(&a)));
        assert!(!level(&a).is_level_of(&layer(&b)));
    }

    /// What reading the `[[unit]]` tables `unit_tables`, after the fields a stack needs, gives: the units, or every
    /// line it fails with, without the name of the file.
    fn read(stack: &str, unit_tables: &str) -> Result<Vec<Unit>, Vec<String>> {
        let text = format!(
            "stack = \"{stack}\"\nareas = []\nui = \"none\"\nfiles = \"2.0.0\"\n{unit_tables}"
        );
        let declaration = parse_declaration(&text).unwrap_or_else(|e| panic!("{e}:\n{text}"));
        match Declared::new(declaration) {
            Ok(declared) => Ok(declared.units),
            Err(found) => Err(found
                .into_iter()
                .map(|line| {
                    line.strip_prefix(".config/rotproof.toml: ")
                        .unwrap()
                        .to_string()
                })
                .collect()),
        }
    }

    /// The lines reading `unit_tables` fails with, which it must.
    fn fails(stack: &str, unit_tables: &str) -> Vec<String> {
        read(stack, unit_tables).expect_err(unit_tables)
    }

    /// One `[[unit]]` table: `more` is the lines after its `path`, each ending in a newline.
    fn unit(name: &str, path: &str, more: &str) -> String {
        format!("[[unit]]\nname = \"{name}\"\npath = \"{path}\"\n{more}")
    }

    const NOT_A_NAME: &str =
        "is not a name: start with a lowercase letter, then lowercase letters, digits and _";
    const FROM_THE_ROOT: &str = "write a path from the root, such as packages/billing/src/billing";

    #[test]
    fn the_units_of_a_monorepo_are_read_with_what_each_says_of_itself() {
        let tables = format!(
            "{}\n{}\n{}",
            unit(
                "core",
                "packages/core/src/core",
                "shared = true\nroot = \"packages/core\"\n"
            ),
            unit(
                "billing",
                "packages/billing/src/billing",
                "absent = [\"ui\"]\n"
            ),
            unit(
                "cli",
                "apps/cli/src/cli",
                "entry = [\"apps/cli/__main__.py\", \"apps/cli/src/other.py\"]\n"
            ),
        );
        let units = read("python", &tables).unwrap();
        let names: Vec<&str> = units.iter().map(|u| u.name.as_str()).collect();
        assert_eq!(names, ["core", "billing", "cli"]);
        assert!(units.iter().all(|u| u.declared));
        // What is left out is the path: the manifest is where the layers are
        assert_eq!(units[0].root, "packages/core");
        assert_eq!(units[1].root, "packages/billing/src/billing");
        assert!(units[0].shared && !units[1].shared && !units[2].shared);
        assert_eq!(units[1].absent, ["ui"]);
        assert_eq!(
            units[2].entries,
            ["apps/cli/__main__.py", "apps/cli/src/other.py"]
        );
        assert!(!units[0].tag().app && !units[1].tag().app && units[2].tag().app);
    }

    #[test]
    fn an_entry_is_a_string_or_a_list_and_a_unit_has_none_by_default() {
        let entries = |more: &str| {
            read("python", &unit("cli", "cli", more)).unwrap()[0]
                .entries
                .clone()
        };
        assert_eq!(
            entries("entry = \"cli/__main__.py\"\n"),
            ["cli/__main__.py"]
        );
        assert_eq!(
            entries("entry = [\"cli/__main__.py\"]\n"),
            ["cli/__main__.py"]
        );
        assert!(entries("").is_empty() && entries("entry = []\n").is_empty());
        // Neither a string nor a list of them, and a field Rotproof does not know
        for more in ["entry = 1\n", "colour = \"red\"\n"] {
            let text = format!(
                "stack = \"python\"\nareas = []\nui = \"none\"\n{}",
                unit("a", "a", more)
            );
            assert!(parse_declaration(&text).is_err(), "{more}");
        }
    }

    #[test]
    fn a_path_is_the_root_or_written_from_it_and_a_slash_at_its_end_counts_for_nothing() {
        let tables = format!(
            "{}\n{}",
            unit("a", "packages/a/src/", "root = \"packages/a/\"\n"),
            unit("b", ".", "")
        );
        let units = read("typescript", &tables).unwrap();
        assert_eq!(
            (units[0].path.as_str(), units[0].root.as_str()),
            ("packages/a/src", "packages/a")
        );
        assert_eq!((units[1].path.as_str(), units[1].root.as_str()), ("", ""));
        // The root of a unit may be the root of the project
        assert!(read("typescript", &unit("a", "packages/a/src", "root = \".\"\n")).is_ok());
        for path in ["./a", "a/./b", "../a", "/a", "a//b", "C:/a", ""] {
            assert_eq!(
                fails("typescript", &unit("x", path, "")),
                [format!("unit \"x\" has path {path:?}: {FROM_THE_ROOT}")]
            );
        }
        // The TOML of the test writes one backslash as two
        assert_eq!(
            fails("typescript", &unit("x", "a\\\\b", "")),
            ["unit \"x\" has path \"a\\\\b\": separate a path with /, which every platform reads"]
        );
        assert_eq!(
            fails("typescript", &unit("x", "a", "root = \"./a\"\n")),
            ["unit \"x\" has root \"./a\": write a path from the root, such as packages/billing"]
        );
    }

    #[test]
    fn a_name_is_a_lowercase_letter_then_lowercase_letters_digits_and_underscores() {
        for name in ["a", "billing", "b2b", "order_items", "x_"] {
            assert!(read("typescript", &unit(name, "p", "")).is_ok(), "{name}");
        }
        for name in ["Bad Name", "Billing", "1a", "a-b", "_a", "", "名前", "a.b"] {
            assert_eq!(
                fails("typescript", &unit(name, "p", "")),
                [format!("unit name {name:?} {NOT_A_NAME}")]
            );
        }
        // A name is said once, however many units have it
        let tables = format!(
            "{}\n{}\n{}",
            unit("a", "p", ""),
            unit("a", "q", ""),
            unit("a", "r", "")
        );
        assert_eq!(fails("typescript", &tables), ["two units are named \"a\""]);
    }

    #[test]
    fn python_imports_a_unit_as_the_name_of_its_directory() {
        assert!(
            read(
                "python",
                &unit("billing", "packages/billing/src/billing", "")
            )
            .is_ok()
        );
        // At the root there is no directory to name it
        assert!(read("python", &unit("whatever", ".", "")).is_ok());
        assert_eq!(
            fails("python", &unit("billing", "packages/billing/src", "")),
            [
                "unit \"billing\" is at packages/billing/src: Python imports a unit as the name of its directory, so \
              name the unit \"src\", or the directory \"billing\""
            ]
        );
        // Not the other stacks
        assert!(read("typescript", &unit("billing", "packages/billing/src", "")).is_ok());
        assert!(read("rust", &unit("billing", "packages/billing", "")).is_ok());
    }

    #[test]
    fn a_python_unit_is_not_named_like_a_module_of_the_standard_library() {
        // Python would find the module or the directory first, depending on where the program starts
        for (path, name) in [
            ("queue", "queue"),
            ("packages/queue/src/queue", "queue"),
            ("string", "string"),
            ("a/distutils", "distutils"),
            ("a/tomllib", "tomllib"),
        ] {
            assert_eq!(
                fails("python", &unit(name, path, "")),
                [format!(
                    "unit \"{name}\" is at {path}/, named like a module of Python's standard library: Python would \
                     import one or the other depending on how it starts; name the directory otherwise"
                )],
                "{path}"
            );
        }
        // The name of the directory is what Python imports, whatever the unit is called, and each reason is said
        assert_eq!(
            fails("python", &unit("queues", "a/queue", "")).len(),
            2,
            "a unit that is not named as its directory, which is a module of the library"
        );
        // Not another stack's, not a name that merely contains one, not the name of a unit at the root
        assert!(read("typescript", &unit("queue", "packages/queue/src", "")).is_ok());
        assert!(read("rust", &unit("queue", "packages/queue", "")).is_ok());
        assert!(read("python", &unit("queue_jobs", "queue_jobs", "")).is_ok());
        assert!(read("python", &unit("queue", ".", "")).is_ok());
    }

    #[test]
    fn two_python_units_are_not_imported_as_one_name() {
        let two =
            |a: &str, b: &str| format!("{}\n{}", unit("billing", a, ""), unit("billing2", b, ""));
        let tables = format!(
            "{}\n{}",
            unit("billing", "a/billing", ""),
            unit("billing", "b/billing", "")
        );
        assert_eq!(
            fails("python", &tables),
            [
                "two units are named \"billing\"".to_string(),
                "two units are imported as billing (a/billing, b/billing)".to_string()
            ]
        );
        // Whether or not they are named alike, and at the root of the project or below it
        let tables = format!(
            "{}\n{}\n{}",
            unit("billing", "billing", ""),
            unit("shipping", "packages/shipping/src/shipping", ""),
            unit("other", "packages/billing/src/billing", "")
        );
        let found = fails("python", &tables);
        assert_eq!(
            found,
            [
                "unit \"other\" is at packages/billing/src/billing: Python imports a unit as the name of its \
                 directory, so name the unit \"billing\", or the directory \"other\"",
                "two units are imported as billing (billing, packages/billing/src/billing)"
            ]
        );
        // Said once with every path, the units in the order they are declared
        let tables = format!(
            "{}\n{}\n{}\n{}",
            unit("a", "x/a", ""),
            unit("b", "x/b", ""),
            unit("a", "y/a", ""),
            unit("a", "z/a", "")
        );
        assert_eq!(
            fails("python", &tables)[1],
            "two units are imported as a (x/a, y/a, z/a)"
        );
        // Two units at one path are one directory: that is said already
        let tables = format!(
            "{}\n{}",
            unit("billing", "p/billing", ""),
            unit("billing", "p/billing", "")
        );
        assert_eq!(
            fails("python", &tables),
            [
                "two units are named \"billing\"",
                "units \"billing\" and \"billing\" both have path \"p/billing\""
            ]
        );
        // Units of other directories, a unit at the root, and the same directory name in another stack
        assert!(read("python", &two("a/billing", "b/shipping")).is_err_and(|e| e.len() == 1));
        let tables = format!(
            "{}\n{}",
            unit("a", ".", ""),
            unit("b", "packages/b/src/b", "")
        );
        assert!(read("python", &tables).is_ok());
        assert!(read("typescript", &two("a/x", "b/x")).is_ok());
    }

    #[test]
    fn a_python_unit_is_imported_from_the_directory_that_holds_it() {
        let table = table();
        let layout = layout("python", "atomic").unwrap();
        let tables = format!(
            "{}\n{}\n{}",
            unit("billing", "packages/billing/src/billing", ""),
            unit("core", "core", ""),
            unit("shop", ".", "")
        );
        let units = read("python", &tables).unwrap();
        assert_eq!(
            units[0].python_import(),
            Some(("billing", "packages/billing/src"))
        );
        assert_eq!(units[1].python_import(), Some(("core", "")));
        // At the root, nothing names it; and the implicit unit is no unit that is declared
        assert_eq!(units[2].python_import(), None);
        assert_eq!(Unit::implicit(&layout, &[]).python_import(), None);
        // The `__init__.py` of a unit is a place that imports no layer, and is the unit's
        let place =
            units[0].own_file_place(&layout, &table, "packages/billing/src/billing/__init__.py");
        assert!(place.imports.is_empty() && place.below.is_empty() && place.is_layer());
        assert_eq!(place.name, "packages/billing/src/billing/__init__.py");
        assert_eq!(place.unit, units[0].tag());
    }

    #[test]
    fn two_units_do_not_share_a_path_and_one_sits_in_another_beside_its_layers() {
        let two = format!("{}\n{}", unit("a", "pkg/x/", ""), unit("b", "pkg/x", ""));
        assert_eq!(
            fails("typescript", &two),
            ["units \"a\" and \"b\" both have path \"pkg/x\""]
        );
        let two = format!("{}\n{}", unit("a", ".", ""), unit("b", ".", ""));
        assert_eq!(
            fails("typescript", &two),
            ["units \"a\" and \"b\" both have path \".\""]
        );
        // Beside its layers, however deep
        let nested = format!(
            "{}\n{}\n{}",
            unit("a", "pkg", ""),
            unit("b", "pkg/x", ""),
            unit("c", "pkg/x/y/z", "")
        );
        assert!(read("typescript", &nested).is_ok());
        let nested = format!("{}\n{}", unit("a", ".", ""), unit("b", "apps/b", ""));
        assert!(read("typescript", &nested).is_ok());
        // Inside a layer of the unit around it, in either order of declaring them
        for tables in [
            format!(
                "{}\n{}",
                unit("a", "pkg", ""),
                unit("x", "pkg/domain/x", "")
            ),
            format!(
                "{}\n{}",
                unit("x", "pkg/domain/x", ""),
                unit("a", "pkg", "")
            ),
        ] {
            assert_eq!(
                fails("typescript", &tables),
                [
                    "unit \"x\" is at pkg/domain/x, inside the layer domain of unit \"a\": a unit sits beside the \
                  layers of the unit around it"
                ]
            );
        }
        // Inside a level, which is inside its layer
        let tables = format!(
            "{}\n{}",
            unit("a", ".", ""),
            unit("x", "protocol/inbound/x", "")
        );
        assert_eq!(
            fails("typescript", &tables),
            [
                "unit \"x\" is at protocol/inbound/x, inside the layer protocol of unit \"a\": a unit sits beside the \
              layers of the unit around it"
            ]
        );
    }

    #[test]
    fn a_unit_is_not_in_a_directory_named_like_a_layer_or_a_path_that_is_not_one() {
        for (stack, path, directory) in [
            ("typescript", "apps/domain", "domain"),
            ("python", "domain", "domain"),
            ("rust", "packages/composition", "composition"),
            ("python", "packages/tests", "tests"),
        ] {
            // Python names the unit as its directory
            let name = if stack == "python" { directory } else { "x" };
            assert_eq!(
                fails(stack, &unit(name, path, "")),
                [format!(
                    "unit \"{name}\" is at {path}, named like a layer: name its directory otherwise, as it would be \
                     read as the layer {directory} of the unit around it"
                )],
                "{stack}"
            );
        }
        // A layer of a UI the layout does not have is no layer here
        assert!(read("typescript", &unit("x", "apps/ui", "")).is_ok());
    }

    #[test]
    fn the_root_of_a_unit_is_its_path_or_a_directory_above() {
        for root in ["packages", "packages/a/src", "."] {
            let more = format!("root = \"{root}\"\n");
            assert!(
                read("typescript", &unit("a", "packages/a/src", &more)).is_ok(),
                "{root}"
            );
        }
        // A name that begins as the path does is another directory
        for root in ["y", "packages/a/s"] {
            let more = format!("root = \"{root}\"\n");
            assert_eq!(
                fails("typescript", &unit("a", "packages/a/src", &more)),
                [format!(
                    "unit \"a\" has root \"{root}\", which does not hold its path \"packages/a/src\""
                )]
            );
        }
        assert_eq!(
            fails("typescript", &unit("a", ".", "root = \"y\"\n")),
            ["unit \"a\" has root \"y\", which does not hold its path \".\""]
        );
    }

    #[test]
    fn an_entry_is_code_of_the_stack_written_from_the_root_in_its_own_unit() {
        let ts = |more: &str| read("typescript", &unit("a", "apps/a/src", more));
        assert!(ts("entry = \"apps/a/src/main.tsx\"\n").is_ok());
        // Beside the unit's directory is in no other unit's either
        assert!(ts("entry = \"apps/a/main.tsx\"\n").is_ok());
        assert!(
            read(
                "rust",
                &unit("a", "apps/a", "entry = \"apps/a/src/bin/w.rs\"\n")
            )
            .is_ok()
        );
        assert_eq!(
            fails("typescript", &unit("a", "p", "entry = \"./main.tsx\"\n")),
            [
                "unit \"a\" has entry \"./main.tsx\": write a path from the root, such as apps/cli/main.tsx"
            ]
        );
        assert_eq!(
            fails(
                "python",
                &unit("a", "a", "entry = [\"a/run.sh\", \"a/__main__.py\"]\n")
            ),
            ["unit \"a\" declares entry \"a/run.sh\", which is no code of the python stack"]
        );
        assert_eq!(
            fails("rust", &unit("a", "a", "entry = \"a/lib.py\"\n")),
            ["unit \"a\" declares entry \"a/lib.py\", which is no code of the rust stack"]
        );
        // In the path of another unit
        let tables = format!(
            "{}\n{}",
            unit("a", "apps/a", "entry = \"apps/b/src/main.rs\"\n"),
            unit("b", "apps/b/src", "")
        );
        assert_eq!(
            fails("rust", &tables),
            [
                "unit \"a\" declares entry \"apps/b/src/main.rs\", inside the path of unit \"b\" (apps/b/src/)"
            ]
        );
        // The innermost unit that holds it is its own, though the unit around holds it too
        let tables = format!(
            "{}\n{}",
            unit("a", ".", ""),
            unit("b", "apps/b", "entry = \"apps/b/src/main.rs\"\n")
        );
        assert!(read("rust", &tables).is_ok());
        let tables = format!(
            "{}\n{}",
            unit("a", ".", ""),
            unit("b", "apps/b", "entry = \"main.rs\"\n")
        );
        assert_eq!(
            fails("rust", &tables),
            ["unit \"b\" declares entry \"main.rs\", inside the path of unit \"a\" (./)"]
        );
        let tables = format!(
            "{}\n{}",
            unit("cli", "apps/cli", "entry = \"apps/main.rs\"\n"),
            unit("web", "apps/web", "entry = [\"apps/main.rs\"]\n")
        );
        assert_eq!(
            fails("rust", &tables),
            ["units \"cli\" and \"web\" both declare entry \"apps/main.rs\""]
        );
    }

    #[test]
    fn a_shared_unit_is_not_an_app() {
        let more = "shared = true\nentry = \"core/src/main.ts\"\n";
        assert_eq!(
            fails("typescript", &unit("core", "core/src", more)),
            [
                "unit \"core\" is shared and declares entry: an app (a unit with entry) wires the units and no unit \
              imports it, so a unit is not both"
            ]
        );
        assert!(
            read(
                "typescript",
                &unit("core", "core/src", "shared = true\nentry = []\n")
            )
            .is_ok()
        );
    }

    #[test]
    fn what_a_unit_lacks_is_its_own_and_names_places_of_the_layout() {
        let more = "absent = [\"domain\", \"protocol.inbound\"]\n";
        let units = read("typescript", &unit("app", "app", more)).unwrap();
        assert_eq!(units[0].absent, ["domain", "protocol.inbound"]);
        assert_eq!(
            fails(
                "typescript",
                &unit("app", "app", "absent = [\"domain\", \"domainx\"]\n")
            ),
            ["unit \"app\" names \"domainx\" in absent, which the typescript layout does not have"]
        );
        // The top level is the one unit's while the project has no unit of its own, and nobody's once it has
        let text = format!(
            "stack = \"typescript\"\nareas = []\nui = \"none\"\nabsent = [\"domain\"]\nfiles = \"2.0.0\"\n{}",
            unit("a", "a", "")
        );
        assert_eq!(
            Declared::new(parse_declaration(&text).unwrap()).unwrap_err(),
            [
                ".config/rotproof.toml: absent lists [\"domain\"], and the project declares units: name what each unit \
              lacks in its own absent, and take this one out"
            ]
        );
    }

    #[test]
    fn every_reason_is_said_at_once() {
        let tables = format!(
            "{}\n{}",
            unit(
                "Bad",
                "./a",
                "shared = true\nentry = \"x.txt\"\nabsent = [\"nowhere\"]\n"
            ),
            unit("b", "b", "root = \"c\"\n")
        );
        let found = fails("typescript", &tables);
        assert_eq!(found.len(), 6, "{found:#?}");
        assert!(found[0].starts_with("unit name \"Bad\""), "{found:#?}");
        for said in [
            "has path \"./a\"",
            "which is no code",
            "is shared and declares entry",
            "has root \"c\"",
            "names \"nowhere\" in absent",
        ] {
            assert!(found.iter().any(|f| f.contains(said)), "{said}: {found:#?}");
        }
    }
}
