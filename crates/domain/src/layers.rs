//! The layers: what they are (the table, the same in every stack) and where they live (one layout per stack). What a
//! project declares of them is in `declaration.rs`.
//!
//! The table and the layouts are data files in `layers/`, built into the binary, so reviewing a layout means reading
//! one file.

use std::collections::BTreeMap;

use serde::Deserialize;

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

/// Where the layers live in one stack.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Layout {
    /// The directory, from the root, where code is looked for ("" for the root)
    pub scope: String,
    /// What a code file is named: `*`, `*.<extension>` or an exact name
    pub code: String,
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
}

impl Place {
    /// Whether this is a layer, and not a level of one.
    pub fn is_layer(&self) -> bool {
        self.parent.is_none()
    }

    /// Whether this is a level of `layer`.
    pub fn is_level_of(&self, layer: &Place) -> bool {
        self.parent.as_ref() == Some(&layer.name)
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

impl Layout {
    /// Every place of this layout, layers first in the table's order, each followed by its levels: where it lives, and
    /// what the table says it may import. The checks, the documentation of each layer and Rotproof's guide all read
    /// this, so they cannot disagree.
    pub fn places(&self, table: &Table) -> Vec<Place> {
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
                path: self.layer_path(&layer.name),
                parent: None,
                role: layer.role.clone(),
                imports: imports(&layer.imports),
                below: Vec::new(),
                files: Vec::new(),
            };
            place.files = render(
                files,
                &place.path,
                &layer.name,
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
                    path: shape
                        .path
                        .replace("{layer}", &layer.name)
                        .replace("{name}", &level.name),
                    parent: Some(layer.name.clone()),
                    role: level.role.clone(),
                    imports: imports(&level.imports),
                    below,
                    files: Vec::new(),
                };
                place.files = render(
                    &shape.files,
                    &place.path,
                    &level.name,
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
        Some(Place {
            name: path.clone(),
            path: path.clone(),
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
        })
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
        let name = file_name.to_ascii_lowercase();
        match self.code.strip_prefix('*') {
            Some("") => true,
            Some(suffix) => name.ends_with(&suffix.to_ascii_lowercase()),
            None => name == self.code.to_ascii_lowercase(),
        }
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

/// The files of one place, from the root: `{name}` and `{doc}` filled in, each line of the documentation with the
/// stack's prefix.
fn render(files: &[File], at: &str, name: &str, prefix: &str, doc: &str) -> Vec<(String, String)> {
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
                file.text.replace("{name}", name).replace("{doc}", &doc),
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
                        !text.contains("{name}") && !text.contains("{doc}"),
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
    }
}
