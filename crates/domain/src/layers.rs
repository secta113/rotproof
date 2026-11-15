//! The layers: what they are (the table, the same in every stack) and where they live (one layout per stack). What a
//! project declares of them is in `declaration.rs`.
//!
//! The table and the layouts are data files in `layers/`, built into the binary, so reviewing a layout means reading
//! one file.

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::paths::join;

/// The stack of a repository that keeps records only: no layers, and no structure to check
pub const RECORDS_ONLY: &str = "none";
/// The UI of a project with no screens: none of the layers that name a UI in the table
pub const NO_UI: &str = "none";

const TABLE: &str = include_str!("../../../layers/table.toml");
/// Stack -> its layout
pub const STACKS: [(&str, &str); 3] = [
    ("python", include_str!("../../../layers/python.toml")),
    (
        "typescript",
        include_str!("../../../layers/typescript.toml"),
    ),
    ("rust", include_str!("../../../layers/rust.toml")),
];

/// One level of a layer, as the table describes it.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Level {
    pub name: String,
    /// Layers, or levels of other layers as `<layer>.<level>`
    pub imports: Vec<String>,
    pub role: String,
}

/// One layer, as the table describes it, with its levels.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Layer {
    pub name: String,
    /// The UI it belongs to: there only in a project that declares this UI. `None` for a layer of every project
    #[serde(default)]
    pub ui: Option<String>,
    /// Layers, or levels of other layers as `<layer>.<level>`
    pub imports: Vec<String>,
    pub role: String,
    /// Whether a level may import the levels below it, as listed (the atomic levels of `ui`). Otherwise the levels
    /// never import each other
    #[serde(default)]
    pub ordered: bool,
    /// Its levels, top first. A layer with levels holds nothing beside them
    #[serde(default, rename = "level")]
    pub levels: Vec<Level>,
}

/// The file the program starts from, as the table describes it: no layer, and nothing imports it.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    pub imports: Vec<String>,
    pub role: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Table {
    pub entry: Entry,
    #[serde(rename = "layer")]
    pub layers: Vec<Layer>,
}

/// The table built into Rotproof.
pub fn table() -> Table {
    toml::from_str(TABLE).expect("layers/table.toml is valid: a test reads it")
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct File {
    pub path: String,
    pub text: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Shape {
    /// `{name}` is the layer's name
    pub path: String,
    pub files: Vec<File>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Override {
    pub files: Vec<File>,
}

/// The layers of a stack that hold no view, and what a view is written in there.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NoView {
    /// Layers of the table. One this project does not have (its UI is "none") is not judged
    pub layers: Vec<String>,
    /// The extensions of a file that holds a view (`tsx`), in any case
    pub extensions: Vec<String>,
    /// Where the view goes instead, said in the finding
    pub goes: String,
}

/// A directory besides a layout's `scope` where code is looked for, as code outside the layers when no layer holds it.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AlsoCode {
    /// The directory, from the root
    pub scope: String,
    /// What a code file in it is named, as [`Layout::code`] says
    pub code: String,
}

/// Where the layers live in one stack.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Layout {
    /// The directory, from the root, where code is looked for ("" for the root)
    pub scope: String,
    /// What a code file is named: `*`, `*.<extension>` or an exact name
    pub code: String,
    /// Directories besides `scope` where code is looked for, each with the name of its code files: the root package's
    /// `src/` in a Rust layout, whose code is looked for as crates in `crates/`
    #[serde(default)]
    pub also_code: Vec<AlsoCode>,
    /// Paths that are not layers and hold code all the same, as `tests/`
    pub not_layers: Vec<String>,
    /// Each line of a layer's documentation starts with this
    pub doc_prefix: String,
    /// The UIs a project of this stack may declare besides [`NO_UI`], in the order the declaration lists them
    #[serde(default)]
    pub uis: Vec<String>,
    /// The UI this layout is for, which [`layout`] sets
    #[serde(skip)]
    pub ui: String,
    /// Layers of the table this layout does not have: those of the UIs other than [`Layout::ui`], which [`layout`]
    /// sets
    #[serde(skip)]
    pub without: Vec<String>,
    pub layer: Shape,
    /// The levels of the layers that have them: in `path`, `{layer}` is the layer's name and `{name}` the level's.
    /// Required while this stack has a layer with levels
    pub level: Option<Shape>,
    /// Layer -> files that replace the ones in `layer`
    #[serde(default)]
    pub layers: BTreeMap<String, Override>,
    /// The file the program starts from, from the root: code outside the layers that imports only what the table's
    /// `entry` allows. `None` while this stack has none
    #[serde(default)]
    pub entry: Option<String>,
    /// What `rotproof create` writes into an entry file that a unit declares and the tree lacks, whatever the file is
    /// named: the layout's entry file has no code of its own to copy (Python and TypeScript write none at the root),
    /// so the text is here. `None` while this stack has no entry file
    #[serde(default)]
    pub entry_text: Option<String>,
    /// The manifest of the package that holds an entry file a unit declares, which `rotproof create` writes beside
    /// its `src/` when it is not there (a Rust bin crate: a file with no manifest above it is no crate). `{package}` is
    /// the package's name, and `{dependencies}` its dependency on the unit's own `composition`, or nothing. `None`
    /// while an entry file of this stack needs no package
    #[serde(default)]
    pub entry_package: Option<String>,
    /// What a package's manifest is named: `pyproject.toml`, `package.json`, `Cargo.toml`, as [`Layout::code`] says
    /// one. A project that declares units keeps one to the root of each
    pub manifest: String,
    /// The code files a unit's own directory holds besides its layers, the nested units and its entry files, by name:
    /// a Python package's `__init__.py`, which imports no layer. Empty while this stack has none
    #[serde(default)]
    pub unit_files: Vec<String>,
    /// Where code outside the layers goes in this stack, said in the finding for it
    #[serde(default)]
    pub where_code_goes: Option<String>,
    /// The layers where a file of a view fails, by its extension. `None` while this stack tells a view by no
    /// extension
    #[serde(default)]
    pub no_view: Option<NoView>,
    /// The language Rotproof reads the code as
    pub language: Language,
    /// Files the stack's toolchain needs at the root, written once with the project's files (`project.rs`)
    #[serde(default)]
    pub files: Vec<File>,
    /// What is written in place of `files` when the project declares units, when the layout has any: a stack whose
    /// root differs with units (a workspace of crates, and no root package whose entry file depends on one of them)
    #[serde(default)]
    pub unit_root_files: Vec<File>,
}

/// A language Rotproof reads: the imports for the direction check, and the comments for the marker check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    /// With Ruff's parser (`python.rs`)
    Python,
    /// TypeScript and JavaScript, with oxc (`typescript.rs`)
    TypeScript,
    /// The dependencies each crate declares in its `Cargo.toml` (`cargo.rs`), and the comments of `.rs` files with
    /// Rotproof's own scanner (`rust.rs`)
    Rust,
}

/// The unit a place belongs to, as much of it as a rule on places needs (`units.rs` has the rest). A project that
/// declares no unit has the one implicit unit, whose tag is the default but for `app`: no name, not shared.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UnitTag {
    /// The unit's name, "" for the implicit unit
    pub name: String,
    /// Whether any unit may import it
    pub shared: bool,
    /// Whether it has an entry file: the unit that wires the others
    pub app: bool,
}

/// One place in the tree a layout makes: a layer (`domain`) or a level of a layer (`ui.pages`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Place {
    /// The name `absent` uses: `domain`, `ui.pages`
    pub name: String,
    /// From the root, with `/`
    pub path: String,
    /// The layer it sits in, for a level
    pub parent: Option<String>,
    /// Its role, as the table writes it
    pub role: String,
    /// The layers, and levels of other layers, it may import besides itself, as the table lists them, without those
    /// this stack does not have
    pub imports: Vec<String>,
    /// For a level of an ordered layer, the levels below it, which it may import too: `ui.molecules` and `ui.atoms`
    /// for `ui.organisms`. Empty otherwise
    pub below: Vec<String>,
    /// Path from the root -> text
    pub files: Vec<(String, String)>,
    /// The unit it sits in. Two places of one name in two units are two places
    pub unit: UnitTag,
}

impl Place {
    /// Whether this is a layer, and not a level of one.
    pub fn is_layer(&self) -> bool {
        self.parent.is_none()
    }

    /// Whether `other` is this very place: the same name, in the same unit.
    pub fn is_same(&self, other: &Place) -> bool {
        self.name == other.name && self.unit.name == other.unit.name
    }

    /// Whether this is a level of `layer`.
    pub fn is_level_of(&self, layer: &Place) -> bool {
        self.parent.as_ref() == Some(&layer.name) && self.unit.name == layer.unit.name
    }

    /// The layer this place is, or sits in: the name of a layer, or of the layer of a level.
    pub fn layer_name(&self) -> &str {
        self.parent.as_deref().unwrap_or(&self.name)
    }
}

/// The layout of a stack for a project whose UI is `ui`, or `None` when Rotproof has no stack by that name or the stack
/// offers no such UI ([`uis`]).
pub fn layout(stack: &str, ui: &str) -> Option<Layout> {
    let mut layout = stack_layout(stack)?;
    if ui != NO_UI && !layout.uis.iter().any(|u| u == ui) {
        return None;
    }
    layout.ui = ui.to_string();
    layout.without = table()
        .layers
        .iter()
        .filter(|l| l.ui.as_ref().is_some_and(|u| u != ui))
        .map(|l| l.name.clone())
        .collect();
    Some(layout)
}

/// The layout of a stack as its file writes it, before a UI is chosen.
fn stack_layout(stack: &str) -> Option<Layout> {
    STACKS
        .iter()
        .find(|(name, _)| *name == stack)
        .map(|(name, text)| {
            toml::from_str(text)
                .unwrap_or_else(|e| panic!("layers/{name}.toml is valid, a test reads it: {e}"))
        })
}

/// The UIs a project of `stack` may declare, [`NO_UI`] last, or `None` when Rotproof has no stack by that name.
pub fn uis(stack: &str) -> Option<Vec<String>> {
    let layout = stack_layout(stack)?;
    Some(layout.uis.into_iter().chain([NO_UI.to_string()]).collect())
}

/// The UI a project of `stack` shows when none is chosen: the first it offers, or [`NO_UI`] when it offers none. For
/// what Rotproof says of a stack without a project, as the guide of `rotproof guide --stack`.
pub fn fullest_ui(stack: &str) -> String {
    uis(stack)
        .and_then(|uis| uis.into_iter().next())
        .unwrap_or_else(|| NO_UI.to_string())
}

/// The UI a new declaration for `stack` gets from `--ui`, and the one `rotproof guide --stack` shows when it is named.
/// Named, it is one the stack offers; not named, it is [`NO_UI`] where the stack offers nothing else, and asked for
/// where it does: a project with screens and one without both start from the same command, and a default would make
/// the layers of one of them. A repository of records only has no UI.
pub fn chosen_ui(stack: &str, ui: Option<&str>) -> Result<String, String> {
    let Some(offered) = uis(stack) else {
        if let Some(ui) = ui {
            return Err(format!(
                "stack {stack:?} has no layers, so it has no UI: run it without --ui {ui}"
            ));
        }
        return Ok(String::new());
    };
    match ui {
        Some(ui) if offered.iter().any(|u| u == ui) => Ok(ui.to_string()),
        Some(ui) => Err(format!(
            "unknown UI {ui:?} for the {stack} stack (known: {})",
            offered.join(", ")
        )),
        None if offered == [NO_UI] => Ok(NO_UI.to_string()),
        None => Err(format!(
            "name the UI with --ui ({}): \"none\" for a project with no screens",
            offered.join(", ")
        )),
    }
}

impl Layout {
    /// Every place of this layout, layers first in the table's order, each followed by its levels: where it lives, and
    /// what the table says it may import. The checks, the documentation of each layer and Rotproof's guide all read
    /// this, so they cannot disagree.
    pub fn places(&self, table: &Table) -> Vec<Place> {
        self.places_in(table, &self.scope, &UnitTag::default())
    }

    /// The places of the unit `unit` whose layers sit in the directory `base`, from the root: every place of
    /// [`Layout::places`] moved from the layout's `scope` to `base` and tagged with `unit`. With `base` the scope
    /// itself, it is the very same places.
    pub fn places_in(&self, table: &Table, base: &str, unit: &UnitTag) -> Vec<Place> {
        let mut out = Vec::new();
        // A place imports only the layers this stack has, and their levels: a Rust crate cannot import `ui`
        let has = |name: &str| !self.leaves_out(name);
        let imports = |imports: &[String]| -> Vec<String> {
            imports.iter().filter(|n| has(n)).cloned().collect()
        };
        for layer in table.layers.iter().filter(|l| has(&l.name)) {
            let files = self
                .layers
                .get(&layer.name)
                .map_or(&self.layer.files, |o| &o.files);
            let mut place = Place {
                name: layer.name.clone(),
                path: self.moved(&self.layer_path(&layer.name), base),
                parent: None,
                role: layer.role.clone(),
                imports: imports(&layer.imports),
                below: Vec::new(),
                files: Vec::new(),
                unit: unit.clone(),
            };
            let package = package_of(unit, &layer.name);
            place.files = render(
                files,
                &place.path,
                &layer.name,
                &package,
                &self.doc_prefix,
                &doc(&place),
            );
            out.push(place);
            if layer.levels.is_empty() {
                continue;
            }
            let shape = self
                .level
                .as_ref()
                .expect("a layout with a layer that has levels says where they live: a test reads every layout");
            for (i, level) in layer.levels.iter().enumerate() {
                let below = if layer.ordered {
                    layer.levels[i + 1..]
                        .iter()
                        .map(|lower| format!("{}.{}", layer.name, lower.name))
                        .collect()
                } else {
                    Vec::new()
                };
                let mut place = Place {
                    name: format!("{}.{}", layer.name, level.name),
                    path: self.moved(
                        &shape
                            .path
                            .replace("{layer}", &layer.name)
                            .replace("{name}", &level.name),
                        base,
                    ),
                    parent: Some(layer.name.clone()),
                    role: level.role.clone(),
                    imports: imports(&level.imports),
                    below,
                    files: Vec::new(),
                    unit: unit.clone(),
                };
                place.files = render(
                    &shape.files,
                    &place.path,
                    &level.name,
                    &package,
                    &self.doc_prefix,
                    &doc(&place),
                );
                out.push(place);
            }
        }
        out
    }

    /// The entry file as a place of its own, named by its path, for the direction check and the guide: it may import
    /// what the table's `entry` lists. No place's imports take it in, and no check asks for it to be there.
    pub fn entry(&self, table: &Table) -> Option<Place> {
        let path = self.entry.as_ref()?;
        Some(self.entry_at(table, path, &UnitTag::default()))
    }

    /// The entry file `path` of the unit `unit`, as a place of its own: what [`Layout::entry`] says of the layout's.
    pub fn entry_at(&self, table: &Table, path: &str, unit: &UnitTag) -> Place {
        Place {
            name: path.to_string(),
            path: path.to_string(),
            parent: None,
            role: table.entry.role.clone(),
            imports: table
                .entry
                .imports
                .iter()
                .filter(|n| !self.leaves_out(n))
                .cloned()
                .collect(),
            below: Vec::new(),
            files: Vec::new(),
            unit: unit.clone(),
        }
    }

    /// A path of this layout, which every layout writes from the root and inside its `scope`, moved to the directory
    /// `base` in place of the scope.
    pub fn moved(&self, path: &str, base: &str) -> String {
        let inside = if self.scope.is_empty() {
            path
        } else {
            path.strip_prefix(&format!("{}/", self.scope))
                .expect("a layout's layers sit in its scope: a test reads every layout")
        };
        join(base, inside)
    }

    /// The files this stack's toolchain needs at the root, written once: those of `unit_root_files` for a project that
    /// declares units, when the layout has any, and `files` otherwise.
    pub fn root_files(&self, units: bool) -> &[File] {
        if units && !self.unit_root_files.is_empty() {
            &self.unit_root_files
        } else {
            &self.files
        }
    }

    /// Where the layer `name` lives in this stack, from the root.
    pub fn layer_path(&self, name: &str) -> String {
        self.layer.path.replace("{name}", name)
    }

    /// Whether this layout has no place by the name `name`, a layer or a level (`ui`, `ui.pages`), because the layer it
    /// sits in is of a UI this layout is not for.
    pub fn leaves_out(&self, name: &str) -> bool {
        let layer = name.split_once('.').map_or(name, |(layer, _)| layer);
        self.without.iter().any(|w| w == layer)
    }

    /// Whether a file name is code in this stack.
    ///
    /// Case does not count: Windows runs `stray.PY` with Python and reads `cargo.toml` as `Cargo.toml`, so either is
    /// code that has to sit in a layer.
    pub fn is_code(&self, file_name: &str) -> bool {
        named_by(&self.code, file_name)
    }

    /// Whether a file name is code in the directory `dir`, one of `also_code`'s, or in any other this layout reads.
    pub fn is_code_in(&self, dir: &str, file_name: &str) -> bool {
        match self.also_code.iter().find(|also| also.scope == dir) {
            Some(also) => named_by(&also.code, file_name),
            None => self.is_code(file_name),
        }
    }

    /// The text of an entry file `rotproof create` makes, with no leading blank line.
    pub fn entry_file(&self) -> Option<&str> {
        self.entry_text.as_deref().map(str::trim_start)
    }

    /// Whether a file name is a package's manifest in this stack, in any case.
    pub fn is_manifest(&self, file_name: &str) -> bool {
        named_by(&self.manifest, file_name)
    }

    /// Whether a code file of this name may sit in a unit's own directory, in any case ([`Layout::unit_files`]).
    pub fn is_unit_file(&self, file_name: &str) -> bool {
        self.unit_files
            .iter()
            .any(|own| own.eq_ignore_ascii_case(file_name))
    }

    /// The paths that are not layers ([`Layout::not_layers`]) of a unit whose layers sit in `base`.
    pub fn not_layers_in(&self, base: &str) -> Vec<String> {
        self.not_layers
            .iter()
            .map(|path| self.moved(path, base))
            .collect()
    }

    /// Whether the file named `name`, in a project of this layout, has comments to read. Not every code file has them,
    /// and one that has them is not always a code file: a Rust layout's code files are its manifests.
    pub fn has_comments(&self, name: &str) -> bool {
        match self.language {
            // Every code file of a Python layout is a `.py` file: a test reads every layout
            Language::Python => self.is_code(name),
            // Every file in `src/` is code in the layout; only source has comments to read
            Language::TypeScript => self.is_code(name) && crate::code::is_source(name),
            // The code files of a Rust layout are its crates' manifests; the comments are in the `.rs` files beside
            // them
            Language::Rust => crate::rust::is_source(name),
        }
    }
}

/// Whether `file_name` is named by `code`: `*`, `*.<extension>` or an exact name, in any case.
fn named_by(code: &str, file_name: &str) -> bool {
    let name = file_name.to_ascii_lowercase();
    match code.strip_prefix('*') {
        Some("") => true,
        Some(suffix) => name.ends_with(&suffix.to_ascii_lowercase()),
        None => name == code.to_ascii_lowercase(),
    }
}

impl NoView {
    /// Whether the file `path` is a view by its extension.
    pub fn holds(&self, path: &str) -> bool {
        let name = path.to_ascii_lowercase();
        self.extensions
            .iter()
            .any(|e| name.ends_with(&format!(".{}", e.to_ascii_lowercase())))
    }
}

/// The names in backticks, joined as a sentence: "`a`, `b` and `c`".
pub fn listed(names: &[String]) -> String {
    let names: Vec<String> = names.iter().map(|n| format!("`{n}`")).collect();
    match names.split_last() {
        None => String::new(),
        Some((last, [])) => last.clone(),
        Some((last, rest)) => format!("{} and {last}", rest.join(", ")),
    }
}

/// What a place may import, as one phrase for its documentation, the guide's table and the direction check:
/// "the levels below it, and `domain`". Empty when it imports no other layer.
pub fn importable(place: &Place) -> String {
    match (place.below.is_empty(), place.imports.is_empty()) {
        (true, true) => String::new(),
        (true, false) => listed(&place.imports),
        (false, true) => "the levels below it".to_string(),
        (false, false) => format!("the levels below it, and {}", listed(&place.imports)),
    }
}

/// The documentation of a place: its role, then what it may import.
fn doc(place: &Place) -> String {
    let importable = importable(place);
    let imports = if importable.is_empty() {
        "Imports no other layer.".to_string()
    } else {
        format!("May import {importable}.")
    };
    format!("{}\n\n{imports}", place.role.trim())
}

/// The name of the package a layer is in `unit`, where a layer is one (a Rust crate): its own name, or, in a declared
/// unit, behind the unit's (`billing_domain`), which is what keeps the crates of two units from being named alike.
pub fn package_of(unit: &UnitTag, layer: &str) -> String {
    if unit.name.is_empty() {
        layer.to_string()
    } else {
        format!("{}_{layer}", unit.name)
    }
}

/// The files of one place, from the root: `{name}`, `{package}` and `{doc}` filled in, each line of the documentation
/// with the stack's prefix.
fn render(
    files: &[File],
    at: &str,
    name: &str,
    package: &str,
    prefix: &str,
    doc: &str,
) -> Vec<(String, String)> {
    let doc: Vec<String> = doc
        .lines()
        .map(|line| {
            if line.is_empty() {
                prefix.trim_end().to_string()
            } else {
                format!("{prefix}{line}")
            }
        })
        .collect();
    let doc = doc.join("\n");
    files
        .iter()
        .map(|file| {
            (
                format!("{at}/{}", file.path),
                file.text
                    .replace("{name}", name)
                    .replace("{package}", package)
                    .replace("{doc}", &doc),
            )
        })
        .collect()
}

/// Every stack a declaration may name, `none` last.
pub fn known_stacks() -> Vec<&'static str> {
    STACKS
        .iter()
        .map(|(name, _)| *name)
        .chain([RECORDS_ONLY])
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::within;

    #[test]
    fn every_definition_reads() {
        let table = table();
        let levels: usize = table.layers.iter().map(|l| l.levels.len()).sum();
        assert!(table.layers.len() >= 5 && levels >= 5, "{levels} levels");
        assert!(!table.entry.role.trim().is_empty(), "the entry has no role");
        for import in &table.entry.imports {
            assert!(
                table.layers.iter().any(|l| &l.name == import),
                "the entry imports unknown {import}"
            );
        }
        // What an import may name: a layer, or a level as `<layer>.<level>`
        let names: Vec<String> = table
            .layers
            .iter()
            .flat_map(|layer| {
                std::iter::once(layer.name.clone()).chain(
                    layer
                        .levels
                        .iter()
                        .map(|level| format!("{}.{}", layer.name, level.name)),
                )
            })
            .collect();
        for layer in &table.layers {
            assert!(!layer.role.trim().is_empty(), "{} has no role", layer.name);
            assert!(
                !layer.ordered || !layer.levels.is_empty(),
                "{} is ordered with no levels",
                layer.name
            );
            let own = |import: &String| {
                import == &layer.name || import.starts_with(&format!("{}.", layer.name))
            };
            let entries = std::iter::once((&layer.name, &layer.imports, &layer.role)).chain(
                layer
                    .levels
                    .iter()
                    .map(|level| (&level.name, &level.imports, &level.role)),
            );
            for (name, imports, role) in entries {
                assert!(!role.trim().is_empty(), "{name} has no role");
                for import in imports {
                    assert!(names.contains(import), "{name} imports unknown {import}");
                    // A level reaches the levels beside it only by `ordered`, and a layer holds its own levels
                    assert!(!own(import), "{name} names its own layer: {import}");
                }
            }
        }
        // A UI a layer names is one a layout offers, and the reverse: a UI with no layer would change nothing, and a
        // layer of a UI no stack offers would never be made
        let named: std::collections::BTreeSet<&String> =
            table.layers.iter().filter_map(|l| l.ui.as_ref()).collect();
        let offered: std::collections::BTreeSet<String> = STACKS
            .iter()
            .flat_map(|(stack, _)| stack_layout(stack).unwrap().uis)
            .collect();
        assert_eq!(named, offered.iter().collect());
        assert!(!offered.contains(NO_UI), "{NO_UI} is every stack's already");
        let every = STACKS.iter().flat_map(|(stack, _)| {
            uis(stack)
                .unwrap()
                .into_iter()
                .map(move |ui| (stack, layout(stack, &ui).unwrap()))
        });
        for (stack, layout) in every {
            let stack = &format!("{stack} with ui {}", layout.ui);
            let no_view = layout.no_view.iter().flat_map(|n| &n.layers);
            for name in layout
                .without
                .iter()
                .chain(layout.layers.keys())
                .chain(no_view)
            {
                assert!(
                    table.layers.iter().any(|l| &l.name == name),
                    "{stack}: no layer {name}"
                );
            }
            // The Python reader names a module after its `.py` file, and the marker check reads every code file of a
            // Python layout without asking
            if layout.language == Language::Python {
                assert_eq!(layout.code, "*.py", "{stack}: code a Python layout reads");
            }
            // The direction check of a Rust layout reads the code files as manifests
            if layout.language == Language::Rust {
                assert_eq!(
                    layout.code, "Cargo.toml",
                    "{stack}: code a Rust layout reads"
                );
            }
            // What a unit moves from the scope to its own directory is in the scope, and the files a unit's directory
            // holds are code that the unit's own check would otherwise find outside the layers
            for path in &layout.not_layers {
                assert!(
                    layout.scope.is_empty() || within(path, &layout.scope),
                    "{stack}: {path} is not in the scope {}",
                    layout.scope
                );
            }
            assert!(!layout.manifest.is_empty(), "{stack}: no manifest");
            assert_eq!(
                layout.entry.is_some(),
                layout.entry_file().is_some(),
                "{stack}: an entry file and its text go together"
            );
            // An entry file's package, where it needs one, is a manifest with its name and its dependencies to fill in
            if let Some(package) = &layout.entry_package {
                assert!(
                    layout.language == Language::Rust
                        && package.contains("[package]")
                        && package.contains("name = \"{package}\"")
                        && package.contains("{dependencies}"),
                    "{stack}: {package}"
                );
            }
            if let Some(file) = layout
                .files
                .iter()
                .find(|f| Some(&f.path) == layout.entry.as_ref())
            {
                assert_eq!(
                    layout.entry_file(),
                    Some(file.text.trim_start()),
                    "{stack}: the text of the entry file it writes with the project"
                );
            }
            for file in &layout.unit_files {
                assert!(layout.is_code(file), "{stack}: {file} is no code");
            }
            // A directory besides the scope holds no layer, so the code in it is outside every layer, and the entry
            // file in it is the one code there that is not
            for also in &layout.also_code {
                assert!(
                    !also.scope.is_empty()
                        && !within(&also.scope, &layout.scope)
                        && !within(&layout.scope, &also.scope),
                    "{stack}: {} overlaps the scope {}",
                    also.scope,
                    layout.scope
                );
                if let Some(entry) = &layout.entry {
                    assert!(
                        !within(entry, &also.scope) || layout.is_code_in(&also.scope, entry),
                        "{stack}: the entry file {entry} is not code in {}",
                        also.scope
                    );
                }
            }
            let places = layout.places(&table);
            assert!(places.len() >= 5, "{stack} makes {} places", places.len());
            // The entry file sits outside every layer, as code in the scope where code is looked for
            if let Some(entry) = layout.entry(&table) {
                let path = &entry.path;
                assert!(
                    !places.iter().any(|p| within(path, &p.path)),
                    "{stack}: the entry file {path} sits in a layer"
                );
                if layout.language == Language::Rust {
                    // Read through the manifest of its package, which the layout writes with it
                    let manifest = layout.files.iter().find(|f| f.path == "Cargo.toml");
                    assert!(
                        path == "src/main.rs"
                            && layout.files.iter().any(|f| &f.path == path)
                            && manifest.is_some_and(|m| m.text.contains("[package]")
                                && m.text.contains("composition = ")),
                        "{stack}: the entry file {path} has no package that depends on composition"
                    );
                } else {
                    assert!(
                        (layout.scope.is_empty() || within(path, &layout.scope))
                            && layout.is_code(path),
                        "{stack}: the entry file {path} is not code in the scope"
                    );
                }
                assert!(
                    !entry.imports.is_empty(),
                    "{stack}: the entry imports nothing"
                );
            }
            for place in &places {
                assert!(
                    !place.files.is_empty(),
                    "{stack}: {} has no file",
                    place.name
                );
                for (path, text) in &place.files {
                    assert!(path.starts_with(&format!("{}/", place.path)), "{path}");
                    assert!(
                        !text.contains("{name}")
                            && !text.contains("{package}")
                            && !text.contains("{doc}"),
                        "{stack}: {path} has a placeholder left: {text}"
                    );
                    // A crate that cannot import `ui` is not told it may
                    assert!(
                        !layout
                            .without
                            .iter()
                            .any(|w| text.contains(&format!("`{w}`"))),
                        "{stack}: {path} names a layer the stack does not have: {text}"
                    );
                }
            }
        }
    }

    #[test]
    fn a_layer_is_documented_as_its_stack_writes_it() {
        let table = table();
        let python = layout("python", "atomic").unwrap().places(&table);
        let domain = python.iter().find(|p| p.name == "domain").unwrap();
        assert_eq!(domain.files[0].0, "domain/__init__.py");
        assert!(
            domain.files[0]
                .1
                .starts_with("\"\"\"The concepts and rules")
        );
        assert!(
            domain.files[0]
                .1
                .ends_with("Imports no other layer.\n\"\"\"\n")
        );
        let inbound = python
            .iter()
            .find(|p| p.name == "infrastructure.inbound")
            .unwrap();
        assert_eq!(inbound.files[0].0, "infrastructure/inbound/__init__.py");
        assert!(
            inbound.files[0]
                .1
                .contains("\nMay import `protocol.inbound`.\n")
        );
        assert!(
            !python
                .iter()
                .any(|p| p.name == "handler" || p.name == "utils")
        );

        let rust = layout("rust", "none").unwrap().places(&table);
        assert!(!rust.iter().any(|p| p.name.starts_with("ui")));
        let protocol = rust.iter().find(|p| p.name == "protocol").unwrap();
        let paths: Vec<&str> = protocol.files.iter().map(|(p, _)| p.as_str()).collect();
        assert_eq!(
            paths,
            ["crates/protocol/Cargo.toml", "crates/protocol/src/lib.rs"]
        );
        let lib = &protocol.files[1].1;
        assert!(lib.starts_with("//! The contracts") && lib.contains("\n//!\n"));
        // A crate compiles a module only when its `lib.rs` declares it
        for layer in rust.iter().filter(|p| p.is_layer()) {
            let lib = layer
                .files
                .iter()
                .find(|(path, _)| path.ends_with("/src/lib.rs"))
                .map(|(_, text)| text)
                .unwrap_or_else(|| panic!("{} has no lib.rs", layer.name));
            let levels: Vec<&Place> = rust.iter().filter(|p| p.is_level_of(layer)).collect();
            let declared = lib.lines().filter(|l| l.starts_with("pub mod ")).count();
            assert_eq!(declared, levels.len(), "{}: {lib}", layer.name);
            for level in levels {
                let module = level.name.rsplit('.').next().unwrap();
                assert!(
                    lib.contains(&format!("\npub mod {module};\n")),
                    "{}",
                    level.name
                );
            }
        }

        let typescript = layout("typescript", "atomic").unwrap().places(&table);
        let atoms = typescript.iter().find(|p| p.name == "ui.atoms").unwrap();
        assert_eq!(atoms.path, "src/ui/atoms");
        assert_eq!(atoms.parent.as_deref(), Some("ui"));
        assert!(
            atoms.files[0]
                .1
                .contains("\n * Imports no other layer.\n */\n")
        );
        let pages = typescript.iter().find(|p| p.name == "ui.pages").unwrap();
        assert!(pages.files[0].1.contains(
            "May import the levels below it, and `presentation.models`, `presentation.display` and \
                     `protocol.inbound`"
        ));
    }

    #[test]
    fn a_rust_crate_is_named_for_its_unit_and_the_root_lists_the_crates() {
        let table = table();
        let rust = layout("rust", "none").unwrap();
        // The implicit unit names a crate as its layer; a declared unit puts its own name before it
        let unit = UnitTag {
            name: "billing".into(),
            ..UnitTag::default()
        };
        let named = |places: &[Place], name: &str| -> Vec<String> {
            places
                .iter()
                .filter(|p| p.name == name)
                .flat_map(|p| {
                    p.files
                        .iter()
                        .filter(|(path, _)| path.ends_with("Cargo.toml"))
                })
                .map(|(path, text)| {
                    let name = text.lines().find(|l| l.starts_with("name = ")).unwrap();
                    format!("{path} {name}")
                })
                .collect()
        };
        let implicit = rust.places(&table);
        let inside = rust.places_in(&table, "crates/billing", &unit);
        for layer in ["domain", "protocol", "infrastructure"] {
            assert_eq!(
                named(&implicit, layer),
                [format!("crates/{layer}/Cargo.toml name = \"{layer}\"")]
            );
            assert_eq!(
                named(&inside, layer),
                [format!(
                    "crates/billing/{layer}/Cargo.toml name = \"billing_{layer}\""
                )]
            );
        }
        // A level is a module of its crate, and says nothing of the crate's name
        let level = inside
            .iter()
            .find(|p| p.name == "protocol.inbound")
            .unwrap();
        assert_eq!(level.path, "crates/billing/protocol/src/inbound");
        // The root lists each crate on a line of its own, with and without units, and holds no package with units
        assert!(
            rust.files
                .iter()
                .all(|f| f.path != "Cargo.toml" || f.text.contains("members = [{members}]"))
        );
        let root = &rust.root_files(true);
        assert_eq!(root.len(), 1);
        assert!(root[0].text.contains("members = [{members}]"));
        assert!(!root[0].text.contains("[package]") && !root[0].text.contains("composition"));
        assert_eq!(rust.root_files(false).len(), 2);
        // Another stack that has no files of its own for units writes what it writes
        let typescript = layout("typescript", "atomic").unwrap();
        assert!(typescript.root_files(true).is_empty());
    }

    #[test]
    fn a_place_is_asked_what_it_is_and_a_layout_what_it_leaves_out() {
        let table = table();
        let rust = layout("rust", "none").unwrap();
        assert_eq!(rust.layer_path("domain"), "crates/domain");
        // A level is left out with its layer, and a name no layer has is left out by nothing
        assert!(rust.leaves_out("ui") && rust.leaves_out("ui.pages"));
        assert!(!rust.leaves_out("infrastructure.inbound") && !rust.leaves_out("nothing"));
        let atomic = layout("typescript", "atomic").unwrap();
        assert!(!atomic.leaves_out("ui.pages"));
        let places = atomic.places(&table);
        let ui = places.iter().find(|p| p.name == "ui").unwrap();
        let pages = places.iter().find(|p| p.name == "ui.pages").unwrap();
        let domain = places.iter().find(|p| p.name == "domain").unwrap();
        assert!(ui.is_layer() && !pages.is_layer());
        assert!(pages.is_level_of(ui) && !pages.is_level_of(domain) && !ui.is_level_of(ui));
        assert_eq!((ui.layer_name(), pages.layer_name()), ("ui", "ui"));
    }

    #[test]
    fn code_is_matched_by_name() {
        let mut layout = layout("python", "atomic").unwrap();
        assert!(layout.is_code("a.py") && !layout.is_code("a.pyi") && !layout.is_code("README.md"));
        // Windows runs these with Python all the same
        assert!(layout.is_code("stray.PY") && layout.is_code("Stray.Py"));
        layout.code = "*".into();
        assert!(layout.is_code("anything"));
        layout.code = "Cargo.toml".into();
        assert!(layout.is_code("Cargo.toml") && !layout.is_code("Cargo.lock"));
        assert!(layout.is_code("cargo.toml") && layout.is_code("CARGO.TOML"));
        // The root package's `src/` of a Rust layout has its own code files
        let rust = super::layout("rust", NO_UI).unwrap();
        assert!(rust.is_code_in("src", "pricing.rs") && rust.is_code_in("src", "PRICING.RS"));
        assert!(!rust.is_code_in("src", "Cargo.toml") && !rust.is_code_in("src", "data.json"));
        assert!(rust.is_code_in("crates", "Cargo.toml") && !rust.is_code_in("crates", "a.rs"));
    }

    #[test]
    fn the_comments_to_read_are_in_the_source_of_the_language() {
        let python = layout("python", "atomic").unwrap();
        assert!(python.has_comments("a.py") && !python.has_comments("a.pyi"));
        // Source of TypeScript and JavaScript, not what its layout calls code and a reader skips
        let typescript = layout("typescript", "atomic").unwrap();
        assert!(typescript.has_comments("a.ts") && typescript.has_comments("a.jsx"));
        assert!(!typescript.has_comments("a.json"));
        // A Rust layout's code files are manifests; the comments are in the `.rs` files
        let rust = layout("rust", "none").unwrap();
        assert!(rust.has_comments("lib.rs") && !rust.has_comments("Cargo.toml"));
    }
}
