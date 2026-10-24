//! The layers: what they are (the table, the same in every stack), where they live (one layout per stack), and what a
//! project declares (`.config/rotproof.toml`).
//!
//! The table and the layouts are data files in `layers/`, built into the binary, so reviewing a layout means reading
//! one file.

use std::collections::BTreeMap;

use serde::Deserialize;
use toml_edit::{Array, DocumentMut, Item, Value};
use utils::paths::within;

use crate::upgrade::VERSION;

/// Where a project declares its structure, from the root
pub const DECLARATION: &str = ".config/rotproof.toml";
/// What to do when the declaration is missing, with every stack: a test keeps the list equal to [`known_stacks`]
pub const MISSING: &str = "missing: .config/rotproof.toml. Write it with `rotproof init --stack <stack>` (python, \
                           typescript, rust, or none for records only)";
/// The stack of a repository that keeps records only: no layers, and no structure to check
pub const RECORDS_ONLY: &str = "none";

/// What every command says of a declaration that cannot be read, for `why`.
pub fn unreadable(why: &str) -> String {
    format!("{DECLARATION}: {why}")
}

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
    /// Layers of the table this stack does not have
    #[serde(default)]
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

/// The layout of a stack, or `None` when Rotproof has none by that name.
pub fn layout(stack: &str) -> Option<Layout> {
    STACKS
        .iter()
        .find(|(name, _)| *name == stack)
        .map(|(name, text)| {
            toml::from_str(text)
                .unwrap_or_else(|e| panic!("layers/{name}.toml is valid, a test reads it: {e}"))
        })
}

impl Layout {
    /// Every place of this layout, layers first in the table's order, each followed by its levels: where it lives, and
    /// what the table says it may import. The checks, the documentation of each layer and Rotproof's guide all read
    /// this, so they cannot disagree.
    pub fn places(&self, table: &Table) -> Vec<Place> {
        let mut out = Vec::new();
        // A place imports only the layers this stack has, and their levels: a Rust crate cannot import `ui`
        let has = |name: &str| {
            let layer = name.split_once('.').map_or(name, |(layer, _)| layer);
            !self.without.iter().any(|w| w == layer)
        };
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
                path: self.layer.path.replace("{name}", &layer.name),
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
                .filter(|n| !self.without.contains(n))
                .cloned()
                .collect(),
            below: Vec::new(),
            files: Vec::new(),
        })
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

/// What a project declares in `.config/rotproof.toml`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Declaration {
    pub stack: String,
    /// The areas the records are grouped by, in this order. Required, so a declaration written before areas existed
    /// fails with the field it lacks rather than with every record's tag
    pub areas: Vec<String>,
    #[serde(default)]
    pub absent: Vec<String>,
    #[serde(default)]
    pub unchecked: Vec<String>,
    /// The version of Rotproof the project's files are up to (`upgrade.rs`). `None` is the first release; only
    /// `rotproof init` writes it
    #[serde(default)]
    pub files: Option<String>,
    /// The updates of the project's files it declines, by name (`upgrade.rs`)
    #[serde(default)]
    pub declined: Vec<String>,
}

/// What is wrong with a list of areas. An area becomes a heading, so it has text, no space at either end, and no other
/// area differs from it only in case.
pub fn area_problems(areas: &[String]) -> Vec<String> {
    let mut found = Vec::new();
    for (i, area) in areas.iter().enumerate() {
        if area.trim().is_empty() {
            found.push(format!("{DECLARATION}: areas has an empty area"));
        } else if area.trim() != area {
            found.push(format!(
                "{DECLARATION}: areas has {area:?}, with a space at one end"
            ));
        } else if area.contains(['\n', '\r']) {
            // A heading is one line: the rest would become text, or a heading of its own
            found.push(format!(
                "{DECLARATION}: areas has {area:?}, on more than one line: an area is one heading"
            ));
        }
        if let Some(earlier) = areas[..i]
            .iter()
            .find(|earlier| earlier.to_lowercase() == area.to_lowercase())
        {
            found.push(format!(
                "{DECLARATION}: areas has {earlier:?} and {area:?}, two names for one heading"
            ));
        }
    }
    found
}

/// The declaration `text` holds, or why it cannot be read.
pub fn parse_declaration(text: &str) -> Result<Declaration, String> {
    toml::from_str(text).map_err(|e| {
        let why = e.message().to_string();
        // A field a newer Rotproof requires: the upgrade is to run `rotproof create`, not to look the field up
        match ADDED
            .iter()
            .find(|f| why == format!("missing field `{}`", f.name))
        {
            Some(_) => format!("{why}: run `rotproof create`, which adds it"),
            None => why,
        }
    })
}

/// What `areas` is, as the declaration says it above the field.
const AREAS_COMMENT: &str = "# The areas the records are grouped by, in this order, such as \"billing\" or \"records\". Every spec,\n\
                             # work item, milestone and knowledge document has exactly one of them in tags, and the index files\n\
                             # group by them\n";

/// A field the declaration requires that `rotproof create` adds when it is missing, so a declaration written by an
/// older Rotproof fails only until the upgrade runs `rotproof create`, never on its shape.
pub struct Added {
    pub name: &'static str,
    /// What it is, as `rotproof init` writes it above the field
    pub comment: &'static str,
    /// Where the first value came from, and what to do with it, written under the comment
    pub first_value: &'static str,
}

/// Every field `rotproof create` adds. Each has a rule for its first value in `create.rs`.
pub const ADDED: [Added; 1] = [Added {
    name: "areas",
    comment: AREAS_COMMENT,
    first_value: "# Added by `rotproof create` with the tags the records use, sorted by name: put them in the order\n\
                  # the index files should show them\n",
}];

/// The names of the fields of [`ADDED`] the declaration `text` lacks, in that order. `None` when it is not TOML: then
/// its own error stands.
pub fn lacking(text: &str) -> Option<Vec<&'static str>> {
    let document = text.parse::<DocumentMut>().ok()?;
    Some(
        ADDED
            .iter()
            .map(|field| field.name)
            .filter(|name| !document.contains_key(name))
            .collect(),
    )
}

/// The declaration `text` with each field of `values` (a name of [`ADDED`], and its first value) added under its
/// comment, and the fields added as `name = value`. The comments and the values already there are kept, and the
/// lines end as in the file the project wrote: with `\r\n` when `crlf`, as `text` was read with `\n`.
pub fn completed(text: &str, values: &[(&str, Vec<String>)], crlf: bool) -> (String, Vec<String>) {
    let mut document = text
        .parse::<DocumentMut>()
        .expect("lacking read it as TOML");
    let mut added = Vec::new();
    for (name, value) in values {
        let field = ADDED
            .iter()
            .find(|field| field.name == *name)
            .unwrap_or_else(|| panic!("{name} is a field of ADDED"));
        let mut array = Array::new();
        array.extend(value.iter().map(String::as_str));
        document.insert(field.name, Item::Value(Value::Array(array)));
        let mut key = document
            .key_mut(field.name)
            .expect("the field was inserted just before");
        key.leaf_decor_mut()
            .set_prefix(format!("\n{}{}", field.comment, field.first_value));
        added.push(format!("{} = {}", field.name, document[field.name]));
    }
    let text = document.to_string();
    (
        if crlf {
            text.replace('\n', "\r\n")
        } else {
            text
        },
        added,
    )
}

/// Every stack a declaration may name, `none` last.
pub fn known_stacks() -> Vec<&'static str> {
    STACKS
        .iter()
        .map(|(name, _)| *name)
        .chain([RECORDS_ONLY])
        .collect()
}

/// The declaration `rotproof init` writes for a stack: every field with what it means, so the project edits it rather
/// than looking it up.
pub fn declaration_text(stack: &str) -> String {
    let head = "# What Rotproof keeps in this project. Edit it, then run `rotproof create` to make what is missing.\n\
                # `rotproof check` fails when the tree and this file differ, either way.\n";
    let stacks = known_stacks().join(" | ");
    let areas = format!("{AREAS_COMMENT}areas = []\n");
    let upgrades =
        format!("\n{FILES_COMMENT}files = \"{VERSION}\"\n\n{DECLINED_COMMENT}declined = []\n");
    if stack == RECORDS_ONLY {
        return format!(
            "{head}\n# {stacks}. \"none\": records only (docs/), no layers to make or check\n\
             stack = \"{stack}\"\n\n{areas}{upgrades}"
        );
    }
    format!(
        "{head}\n# {stacks}\nstack = \"{stack}\"\n\n{areas}\n\
         # Layers this project does not have, such as \"ui\" or \"ui.templates\". Delete the directory too\n\
         absent = []\n\n\
         # Paths outside the layers that Rotproof does not look into, such as \"scripts\" (helper scripts, generated\n\
         # or vendored code). A path that holds a layer, or does not exist, fails\n\
         unchecked = []\n{upgrades}"
    )
}

/// What `declined` is, as the declaration says it above the field.
pub const DECLINED_COMMENT: &str = "# Updates of the project's files this project declines, by name, such as \"claude-deny-approvals\".\n\
                                    # `rotproof init` says when one cannot be applied without a person\n";

/// What `files` is, as the declaration says it above the field.
pub const FILES_COMMENT: &str = "# The version of Rotproof the project's files are up to. Written by `rotproof init`: after upgrading\n\
                                 # Rotproof, run it again, and it updates them\n";

/// A declaration together with the layout it names. `Err` is every reason the two do not fit, for the check.
#[derive(Debug)]
pub struct Declared {
    pub declaration: Declaration,
    /// `None` for a repository that keeps records only (`stack = "none"`)
    pub layout: Option<Layout>,
    pub places: Vec<Place>,
}

impl Declared {
    pub fn new(declaration: Declaration) -> Result<Self, Vec<String>> {
        if declaration.stack == RECORDS_ONLY {
            let mut found = Vec::new();
            for (field, values) in [
                ("absent", &declaration.absent),
                ("unchecked", &declaration.unchecked),
            ] {
                if !values.is_empty() {
                    found.push(format!(
                        "{DECLARATION}: stack = \"none\" has no layers, so {field} must be empty: {values:?}"
                    ));
                }
            }
            if !found.is_empty() {
                return Err(found);
            }
            return Ok(Declared {
                declaration,
                layout: None,
                places: Vec::new(),
            });
        }
        let Some(layout) = layout(&declaration.stack) else {
            return Err(vec![format!(
                "{DECLARATION}: unknown stack {:?} (known: {})",
                declaration.stack,
                known_stacks().join(", ")
            )]);
        };
        let places = layout.places(&table());
        let unknown: Vec<String> = declaration
            .absent
            .iter()
            .filter(|name| !places.iter().any(|p| &p.name == *name))
            .map(|name| {
                format!(
                    "{DECLARATION}: absent names {name:?}, which the {} layout does not have",
                    declaration.stack
                )
            })
            .collect();
        if !unknown.is_empty() {
            return Err(unknown);
        }
        Ok(Declared {
            declaration,
            layout: Some(layout),
            places,
        })
    }

    /// Declared absent itself, or inside a layer declared absent.
    pub fn is_absent(&self, place: &Place) -> bool {
        let absent = &self.declaration.absent;
        absent.contains(&place.name) || place.parent.as_ref().is_some_and(|p| absent.contains(p))
    }

    /// The paths in `unchecked`, as compared: without a `/` at the end.
    pub fn unchecked(&self) -> impl Iterator<Item = &str> {
        self.declaration
            .unchecked
            .iter()
            .map(|path| path.trim_end_matches('/'))
    }

    /// The layer or level a path in `unchecked` holds or sits in. Such a path switches nothing off: a layer cannot be.
    pub fn overlapped(&self, path: &str) -> Option<&Place> {
        self.places
            .iter()
            .find(|p| within(&p.path, path) || within(path, &p.path))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_line_for_a_missing_declaration_lists_every_stack() {
        let listed = MISSING.split_once('(').unwrap().1;
        for stack in known_stacks() {
            assert!(listed.contains(stack), "{stack} is not in: {MISSING}");
        }
        // Nothing more: a stack removed from `layers/` leaves the line too
        assert_eq!(listed.matches(',').count() + 1, known_stacks().len());
    }

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
        for (stack, _) in STACKS {
            let layout = layout(stack).unwrap();
            for name in layout.without.iter().chain(layout.layers.keys()) {
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
        let python = layout("python").unwrap().places(&table);
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

        let rust = layout("rust").unwrap().places(&table);
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
        for layer in rust.iter().filter(|p| p.parent.is_none()) {
            let lib = layer
                .files
                .iter()
                .find(|(path, _)| path.ends_with("/src/lib.rs"))
                .map(|(_, text)| text)
                .unwrap_or_else(|| panic!("{} has no lib.rs", layer.name));
            let levels: Vec<&Place> = rust
                .iter()
                .filter(|p| p.parent.as_ref() == Some(&layer.name))
                .collect();
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

        let typescript = layout("typescript").unwrap().places(&table);
        let atoms = typescript.iter().find(|p| p.name == "ui.atoms").unwrap();
        assert_eq!(atoms.path, "src/ui/atoms");
        assert_eq!(atoms.parent.as_deref(), Some("ui"));
        assert!(
            atoms.files[0]
                .1
                .contains("\n * Imports no other layer.\n */\n")
        );
        let pages = typescript.iter().find(|p| p.name == "ui.pages").unwrap();
        assert!(
            pages.files[0]
                .1
                .contains("May import the levels below it, and `application`")
        );
    }

    #[test]
    fn the_declaration_init_writes_reads_back() {
        for stack in known_stacks() {
            let declaration: Declaration = parse_declaration(&declaration_text(stack))
                .unwrap_or_else(|e| panic!("{stack}: {e}"));
            assert_eq!(declaration.stack, stack);
            assert!(Declared::new(declaration).is_ok(), "{stack}");
        }
    }

    #[test]
    fn a_field_create_adds_is_named_with_the_command_that_adds_it() {
        assert_eq!(
            parse_declaration("stack = \"none\"\n").unwrap_err(),
            "missing field `areas`: run `rotproof create`, which adds it"
        );
        assert_eq!(
            parse_declaration("areas = []\n").unwrap_err(),
            "missing field `stack`"
        );
    }

    #[test]
    fn a_lacking_field_is_added_under_its_comment_and_the_rest_is_kept() {
        let old = "# The project's own note\nstack = \"none\"\n";
        assert_eq!(lacking(old), Some(vec!["areas"]));
        assert_eq!(lacking("stack = \"none\"\nareas = []\n"), Some(Vec::new()));
        assert_eq!(lacking("stack = "), None);
        let tags = vec!["billing".to_string(), "ops".to_string()];
        let (text, added) = completed(old, &[("areas", tags)], false);
        assert_eq!(added, ["areas = [\"billing\", \"ops\"]"]);
        assert!(text.starts_with(old), "{text}");
        assert!(text.contains(&format!("\n{AREAS_COMMENT}")), "{text}");
        assert_eq!(parse_declaration(&text).unwrap().areas, ["billing", "ops"]);
        let (crlf, _) = completed(old, &[("areas", Vec::new())], true);
        assert!(!crlf.replace("\r\n", "").contains('\n'), "{crlf:?}");
    }

    #[test]
    fn areas_are_distinct_headings() {
        let good: Vec<String> = ["rotproof", "記録", "Records of billing"]
            .map(String::from)
            .into();
        assert_eq!(area_problems(&good), Vec::<String>::new());
        let bad = [
            vec![""],
            vec!["  "],
            vec![" rotproof"],
            vec!["rotproof "],
            vec!["rotproof", "Rotproof"],
            vec!["rotproof", "rotproof"],
            // A heading is one line, or the rest of the area becomes a heading of its own
            vec!["injected\n# EVIL_AREA"],
            vec!["a\rb"],
        ];
        for areas in bad {
            let areas: Vec<String> = areas.into_iter().map(String::from).collect();
            assert_eq!(area_problems(&areas).len(), 1, "{areas:?}");
        }
    }

    #[test]
    fn code_is_matched_by_name() {
        let mut layout = layout("python").unwrap();
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
