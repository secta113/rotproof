//! The project's declaration, `.config/rotproof.toml`: what it says, how it is read, and the fields `rotproof create`
//! and `rotproof init` add to it.
//!
//! - **The declaration is the project's.** Rotproof reads it, and writes into it only what a project lacks (a field of
//!   [`ADDED`], `files`, `declined`), each under a comment that says what the field is, and never changes a value the
//!   project wrote.
//! - **A declaration is read in two steps.** [`Declaration`] is what the file says; [`Declared`] is the declaration
//!   together with the layout it names, which fails with every reason the two do not fit.
//! - **An upgrade reads less.** [`fields`] reads only what an upgrade needs, from a declaration written by an older
//!   Rotproof that may lack a field the running one requires.

use serde::Deserialize;
use toml_edit::{Array, DocumentMut, Item, Value};

use crate::layers::{Layout, Place, RECORDS_ONLY, Table, known_stacks, layout, table, uis};
use crate::markers::TodoLink;
use crate::paths::within;
use crate::units::{self, Unit};
use crate::version::VERSION;

/// Where a project declares its structure, from the root
pub const DECLARATION: &str = ".config/rotproof.toml";
/// What to do when the declaration is missing, with every stack: a test keeps the list equal to [`known_stacks`]
pub const MISSING: &str = "missing: .config/rotproof.toml. Write it with `rotproof init --stack <stack>` (python, \
                           typescript, rust, or none for records only) and `--ui atomic` or `--ui none` for python \
                           and typescript (`--ui rendered` too for python)";
/// Which command brings a project's files into line, said the same way wherever a file or a field is missing: after an
/// upgrade of Rotproof it is `rotproof init`, which also sets `files` and says what to do by hand, and `rotproof
/// create` only otherwise
pub const INIT_OR_CREATE: &str =
    "run `rotproof init` after an upgrade of Rotproof, `rotproof create` otherwise";

/// What every command says of a declaration that cannot be read, for `why`.
pub fn unreadable(why: &str) -> String {
    format!("{DECLARATION}: {why}")
}

/// What a project declares in `.config/rotproof.toml`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Declaration {
    pub stack: String,
    /// The areas the records are grouped by, in this order. Required, so a declaration written before areas existed
    /// fails with the field it lacks rather than with every record's tag
    pub areas: Vec<String>,
    /// The UI, which picks the layers of the screens: one of [`uis`] of the stack. Required with a stack that has
    /// layers, and `rotproof create` adds it ([`ADDED`]); never set for a repository of records only. Read as optional
    /// so that a declaration without it reaches the check that names it, rather than failing to read
    #[serde(default)]
    pub ui: Option<String>,
    #[serde(default)]
    pub absent: Vec<String>,
    #[serde(default)]
    pub unchecked: Vec<String>,
    /// Paths of the project's own code that is not in a layer yet. Rotproof does not check it, as it does not check
    /// `unchecked`, and no deadline is set for it; it is a field of its own because `unchecked` is for what is never
    /// moved (generated or vendored code), and what is still to move is counted. Optional, and never written by
    /// `rotproof create`
    #[serde(default)]
    pub moving: Vec<String>,
    /// How the reference of an issue is written in a comment that keeps the first marker of the marker check, one of
    /// [`TodoLink::KNOWN`]. Optional, and never written by `rotproof init` or `rotproof create`: absent, every marker
    /// fails. Read as a string so that an unknown value reaches the check that names it. A declaration that holds the
    /// key fails to read in a Rotproof that lacks it, as it does for any field it does not know
    #[serde(default)]
    pub todo_link: Option<String>,
    /// The most approvals, entries of the approvals file (`approvals.rs`), the project holds: `rotproof check` fails
    /// above it. Optional, and never written by `rotproof init` or `rotproof create`: absent, there is no limit.
    /// Raising it is the project's decision, which shows in the diff of this file
    #[serde(default)]
    pub approved_max: Option<u32>,
    /// The version of Rotproof the project's files are up to (`upgrade.rs`). `None` is the first release; only
    /// `rotproof init` writes it
    #[serde(default)]
    pub files: Option<String>,
    /// The updates of the project's files it declines, by name (`upgrade.rs`)
    #[serde(default)]
    pub declined: Vec<String>,
    /// What the declaration says of the dependencies the manifests list (`dependencies.rs`): the distribution each
    /// import of Python is of, and the dependencies the code uses without importing them. Optional, and never written
    /// by `rotproof init` or `rotproof create`
    #[serde(default)]
    pub dependencies: DependenciesDeclaration,
    /// The units the code is made of, each a `[[unit]]` table. None: the whole project is one unit. Read as the file
    /// says, and checked against the layout in [`Declared::new`]
    #[serde(default, rename = "unit")]
    pub units: Vec<UnitDeclaration>,
}

/// What is wrong with the `[dependencies]` of a declaration of a stack with code: `imports` is Python's, since an
/// import and a dependency are one name in the others, and a name is written once as the manifest lists it.
fn dependency_problems(declaration: &Declaration) -> Vec<String> {
    let dependencies = &declaration.dependencies;
    let mut found = Vec::new();
    if declaration.stack != "python" && !dependencies.imports.is_empty() {
        found.push(format!(
            "{DECLARATION}: imports under [dependencies] says which distribution a Python import is of, and the {} \
             stack imports a package by the name its manifest lists: take it out",
            declaration.stack
        ));
    }
    for (name, distribution) in &dependencies.imports {
        if name.is_empty() || name.split('.').any(str::is_empty) || distribution.trim().is_empty() {
            found.push(format!(
                "{DECLARATION}: imports under [dependencies] maps {name:?} to {distribution:?}: write an import name \
                 (dotted when it is a package below another, as in google.cloud.storage) and the distribution that \
                 provides it"
            ));
        }
    }
    for name in &dependencies.without_import {
        if name.trim().is_empty() {
            found.push(format!(
                "{DECLARATION}: without_import under [dependencies] lists an empty name: write the name the \
                 manifest lists"
            ));
        }
    }
    found
}

/// The `[dependencies]` table of the declaration: what the manifests and the tree cannot say, which the check of the
/// dependencies reads (`dependencies.rs`). The verdict is decided from this and the tree alone.
#[derive(Debug, Default, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DependenciesDeclaration {
    /// Python only: the distribution each import is of, by the name the code imports (`dotenv = "python-dotenv"`).
    /// A name may be dotted, and covers the modules below it (`google.cloud.storage`). An import that is not here is of
    /// the distribution named like its first part
    #[serde(default)]
    pub imports: std::collections::BTreeMap<String, String>,
    /// The dependencies the code uses without importing them, by the name the manifest lists: a package another one
    /// loads (`openpyxl` for `pandas`), a driver chosen by name, a `-sys` crate. They are never unused
    #[serde(default)]
    pub without_import: Vec<String>,
}

/// One `[[unit]]` of the declaration: a part of the code that holds the layers.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnitDeclaration {
    /// What the unit is called: the prefix of a crate's name in Rust, and in Python the name its package is imported as
    pub name: String,
    /// The directory, from the root, where its layers sit: `.` for the root
    pub path: String,
    /// Whether any unit may import it
    #[serde(default)]
    pub shared: bool,
    /// The files the program starts from, when the unit is an app: an array of paths, even for one
    #[serde(default)]
    pub entry: Entries,
    /// The directory, from the root, where the unit's manifest is: its `path` when it says none
    #[serde(default)]
    pub root: Option<String>,
    /// The layers and levels the unit does not have, as the top-level `absent` names them for a project of one unit
    #[serde(default)]
    pub absent: Vec<String>,
}

/// The `entry` of a unit: an array of paths, which is the one way to write it. A string reads as `One` only so that the
/// declaration can be refused with how to write it (`entry = ["x"]`), naming the unit, as `Unit` does; a project
/// would otherwise hold two ways to write one thing, and a document that follows the units would fail when one was
/// changed for the other.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum Entries {
    One(String),
    Many(Vec<String>),
}

impl Default for Entries {
    fn default() -> Self {
        Entries::Many(Vec::new())
    }
}

impl Entries {
    /// The paths, in the order they are written.
    pub fn to_vec(&self) -> Vec<String> {
        match self {
            Entries::One(path) => vec![path.clone()],
            Entries::Many(paths) => paths.clone(),
        }
    }
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
        // A field a newer Rotproof requires: the upgrade is not to look the field up
        match ADDED
            .iter()
            .find(|f| why == format!("missing field `{}`", f.name))
        {
            Some(_) => missing_field(&why),
            None => why,
        }
    })
}

/// What a declaration that lacks a field of [`ADDED`] says: which command adds it. A declaration written by an older
/// Rotproof lacks one, and `rotproof create` adds it but does not set `files` nor say what the upgrade asks of the
/// project by hand, so the upgrade is `rotproof init`. The declaration cannot tell the two cases apart when it cannot
/// be read, so the words are those of every other file that is missing.
fn missing_field(why: &str) -> String {
    format!("{why}: {INIT_OR_CREATE}; either adds it")
}

/// A layer that holds only its levels, with every one of them in the `absent` of a unit and the layer itself not: the
/// layer would be made as a directory with nothing in it, and `rotproof create` would list it as missing. The layer is
/// what is absent, and one way to say so, as `rotproof check` pins a unit's table on what it says, so the declaration
/// is refused and says which to write.
fn levels_without_their_layer(units: &[Unit], places: &[Place]) -> Vec<String> {
    let mut found = Vec::new();
    for unit in units {
        let own = || places.iter().filter(|p| p.unit.name == unit.name);
        for layer in own().filter(|p| p.is_layer() && !unit.absent.contains(&p.name)) {
            let mut levels = own().filter(|p| p.parent.as_ref() == Some(&layer.name));
            if levels.clone().next().is_some() && levels.all(|p| unit.absent.contains(&p.name)) {
                let name = &layer.name;
                let whose = if unit.declared {
                    format!("unit {:?} names", unit.name)
                } else {
                    "absent names".to_string()
                };
                found.push(format!(
                    "{DECLARATION}: {whose} every level of {name} in absent, but not {name} itself, which holds only \
                     its levels: list {name}, so that it is not made as a directory with nothing in it"
                ));
            }
        }
    }
    found
}

/// What `areas` is, as the declaration says it above the field.
const AREAS_COMMENT: &str = "# The areas the knowledge documents are grouped by, in this order, such as \"billing\" or \"records\".\n\
                             # Every knowledge document has exactly one of them in tags, and its index groups by them\n";

/// What `ui` is, as the declaration says it above the field.
const UI_COMMENT: &str = "# The UI of this project, which picks the layers of its screens: \"none\" for a project with no screens\n\
                          # (a command-line tool, a library, a server), so none is made or checked\n";

/// What `files` is, as the declaration says it above the field.
const FILES_COMMENT: &str = "# The version of Rotproof the project's files are up to. Written by `rotproof init`: after upgrading\n\
                             # Rotproof, run it again, and it updates them\n";

/// What `declined` is, as the declaration says it above the field.
const DECLINED_COMMENT: &str = "# Updates of the project's files this project declines, by name, such as \"claude-remove-deny-approvals\".\n\
                                # `rotproof init` says when one cannot be applied without a person\n";

/// What `[[unit]]` is, as the declaration says it, commented out. It is the last thing in the file, so that a project
/// that uncomments it leaves `files` and `declined` above, in no unit's table. `{path}`, `{root}`, `{cli}` and
/// `{entry}` are where a unit sits in the stack ([`units_comment`]).
const UNITS_COMMENT: &str = "\n\
# Units: parts of the code that each hold the layers (a package of a monorepo, a module of a modular monolith).\n\
# Without [[unit]], the whole project is one unit. With units, absent above must stay empty: each unit says what it\n\
# lacks in its own absent. A unit that is not shared imports no other unit; the composition of an app wires them.\n\
# [[unit]]\n\
# name = \"billing\"\n\
# path = \"{path}\"   # where its layers sit\n\
# shared = true   # any unit may import it\n\
# root = \"{root}\"   # where its manifest is (default: path)\n\
#\n\
# [[unit]]\n\
# name = \"cli\"\n\
# path = \"{cli}\"\n\
# entry = [\"{entry}\"]   # an app: its composition wires the units\n";

/// [`UNITS_COMMENT`] for `stack`: where a unit's layers and manifest sit, and its entry, depend on it.
fn units_comment(stack: &str) -> String {
    let (billing, root, cli, entry) = match stack {
        "typescript" => (
            "packages/billing/src",
            "packages/billing",
            "apps/cli/src",
            "apps/cli/src/main.tsx",
        ),
        "rust" => (
            "packages/billing",
            "packages/billing",
            "apps/cli",
            "apps/cli/src/main.rs",
        ),
        _ => (
            "packages/billing/src/billing",
            "packages/billing",
            "apps/cli/src/cli",
            "apps/cli/__main__.py",
        ),
    };
    UNITS_COMMENT
        .replace("{path}", billing)
        .replace("{root}", root)
        .replace("{cli}", cli)
        .replace("{entry}", entry)
}

/// A field the declaration requires that `rotproof create` adds when it is missing, so a declaration written by an
/// older Rotproof fails only until the upgrade runs `rotproof init` (or `rotproof create`), never on its shape.
pub struct Added {
    pub name: &'static str,
    /// What it is, as `rotproof init` writes it above the field
    pub comment: &'static str,
    /// Where the first value came from, and what to do with it, written under the comment
    pub first_value: &'static str,
    /// Required only with a stack that has layers: a repository of records only never gets it
    pub layers_only: bool,
}

/// Every field `rotproof create` adds. Each has a rule for its first value in `create.rs`.
pub const ADDED: [Added; 2] = [
    Added {
        name: "areas",
        comment: AREAS_COMMENT,
        first_value: "# Added by `rotproof create` with the tags the records use, sorted by name: put them in the order\n\
                      # the index files should show them\n",
        layers_only: false,
    },
    Added {
        name: "ui",
        comment: UI_COMMENT,
        first_value: "# Added by `rotproof create`: the UI whose layers the tree held, or \"none\" when it held none\n",
        layers_only: true,
    },
];

/// The names of the fields of [`ADDED`] the declaration `text` lacks, in that order. `None` when it is not TOML: then
/// its own error stands.
pub fn lacking(text: &str) -> Option<Vec<&'static str>> {
    let document = text.parse::<DocumentMut>().ok()?;
    let records_only = document.get("stack").and_then(Item::as_str) == Some(RECORDS_ONLY);
    Some(
        ADDED
            .iter()
            .filter(|field| !(field.layers_only && records_only))
            .map(|field| field.name)
            .filter(|name| !document.contains_key(name))
            .collect(),
    )
}

/// The first value of a field `rotproof create` adds: a list (`areas`) or a name (`ui`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum First {
    List(Vec<String>),
    Name(String),
}

/// The declaration `text` with each field of `values` (a name of [`ADDED`], and its first value) added under its
/// comment, and the fields added as `name = value`. The comments and the values already there are kept, and the
/// lines end with `\n`, as `text` was read: the caller writes it as the project wrote the file
/// ([`Written`](crate::tree::Written)).
pub fn completed(text: &str, values: &[(&str, First)]) -> (String, Vec<String>) {
    let mut document = text
        .parse::<DocumentMut>()
        .expect("lacking read it as TOML");
    let mut added = Vec::new();
    for (name, value) in values {
        let field = ADDED
            .iter()
            .find(|field| field.name == *name)
            .unwrap_or_else(|| panic!("{name} is a field of ADDED"));
        let value = match value {
            First::List(items) => {
                let mut array = Array::new();
                array.extend(items.iter().map(String::as_str));
                Value::Array(array)
            }
            First::Name(name) => Value::from(name.as_str()),
        };
        added.push(add_field(
            &mut document,
            field.name,
            Item::Value(value),
            &format!("{}{}", field.comment, field.first_value),
        ));
    }
    (document.to_string(), added)
}

/// Add the field `name` with `value` to `document`, after a blank line and under `comment`, and say what was added as
/// `name = value`. One place writes a field under its comment, so the three fields Rotproof adds look alike.
fn add_field(document: &mut DocumentMut, name: &str, value: Item, comment: &str) -> String {
    document.insert(name, value);
    document
        .key_mut(name)
        .expect("inserted just before")
        .leaf_decor_mut()
        .set_prefix(format!("\n{comment}"));
    format!("{name} = {}", document[name])
}

/// The declaration `rotproof init` writes for a stack and, with layers, its UI: every field with what it means, so the
/// project edits it rather than looking it up.
pub fn declaration_text(stack: &str, ui: &str) -> String {
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
    let offered = uis(stack).unwrap_or_default().join(" | ");
    // A view a route renders takes the data the route passes it, so a project whose views are templates has no state
    // of a screen to hold: its `presentation` is declared absent, and taken out of `absent` to have it
    let rendered = layout(stack, ui).is_some_and(|layout| layout.templates.is_some());
    let absent = if rendered {
        "# Layers this project does not have, such as \"ui.templates\". Delete the directory too. `presentation` is\n\
         # absent because a template holds no state of its own: take it out, and run `rotproof create --yes`, to keep\n\
         # the state of the screens in models\n\
         absent = [\"presentation\"]"
    } else {
        "# Layers this project does not have, such as \"ui.templates\". Delete the directory too\n\
         absent = []"
    };
    format!(
        "{head}\n# {stacks}\nstack = \"{stack}\"\n\n{areas}\n{UI_COMMENT}# {offered}\nui = \"{ui}\"\n\n\
         {absent}\n\n\
         # Paths outside the layers that Rotproof does not look into, such as \"scripts\" (helper scripts, generated\n\
         # or vendored code). A path that holds a layer, or does not exist, fails\n\
         unchecked = []\n\n\
         # Paths of the project's own code that is not in a layer yet, such as \"legacy\". Rotproof does not check it\n\
         # and sets no date; `rotproof check` counts it and says what to move first. A path that holds a layer, or\n\
         # does not exist, fails\n\
         moving = []\n{upgrades}{}",
        units_comment(stack)
    )
}

/// A declaration together with the layout it names. `Err` is every reason the two do not fit, for the check.
#[derive(Debug)]
pub struct Declared {
    pub declaration: Declaration,
    /// `None` for a repository that keeps records only (`stack = "none"`)
    pub layout: Option<Layout>,
    /// The units of the project, in the order it declares them: the one implicit unit when it declares none, and none
    /// for a repository of records only
    pub units: Vec<Unit>,
    /// The places of every unit, a unit's together, in the order of `units`
    pub places: Vec<Place>,
    /// What `absent` names in a layer the declared UI leaves out (`ui` with `ui = "none"`): it says nothing, and the
    /// structure check fails on it. It does not stop `rotproof create`, so an upgrade that adds `ui` still runs. Each
    /// is the name of the unit that lists it ("" for the implicit unit) and the name it lists
    pub left_out: Vec<(String, String)>,
    /// How a comment that keeps the first marker names its issue, from `todo_link`: `None` when it is not declared
    pub todo_link: Option<TodoLink>,
}

impl Declared {
    pub fn new(declaration: Declaration) -> Result<Self, Vec<String>> {
        if declaration.stack == RECORDS_ONLY {
            let mut found = Vec::new();
            for (field, values) in [
                ("absent", &declaration.absent),
                ("unchecked", &declaration.unchecked),
                ("moving", &declaration.moving),
            ] {
                if !values.is_empty() {
                    found.push(format!(
                        "{DECLARATION}: stack = \"none\" has no layers, so {field} must be empty: {values:?}"
                    ));
                }
            }
            if let Some(ui) = &declaration.ui {
                found.push(format!(
                    "{DECLARATION}: stack = \"none\" has no layers, so it has no ui: take out ui = {ui:?}"
                ));
            }
            if let Some(link) = &declaration.todo_link {
                found.push(format!(
                    "{DECLARATION}: stack = \"none\" has no layers, so it has no code to read and no todo_link: take out todo_link = {link:?}"
                ));
            }
            if !declaration.units.is_empty() {
                found.push(format!(
                    "{DECLARATION}: stack = \"none\" has no layers, so it has no units: take out [[unit]]"
                ));
            }
            let dependencies = &declaration.dependencies;
            if !dependencies.imports.is_empty() || !dependencies.without_import.is_empty() {
                found.push(format!(
                    "{DECLARATION}: stack = \"none\" has no code, so it has no dependencies to judge: take out \
                     [dependencies]"
                ));
            }
            if !found.is_empty() {
                return Err(found);
            }
            return Ok(Declared {
                declaration,
                layout: None,
                units: Vec::new(),
                places: Vec::new(),
                left_out: Vec::new(),
                todo_link: None,
            });
        }
        let Some(offered) = uis(&declaration.stack) else {
            return Err(vec![format!(
                "{DECLARATION}: unknown stack {:?} (known: {})",
                declaration.stack,
                known_stacks().join(", ")
            )]);
        };
        let Some(ui) = &declaration.ui else {
            return Err(vec![unreadable(&missing_field("missing field `ui`"))]);
        };
        let Some(layout) = layout(&declaration.stack, ui) else {
            return Err(vec![format!(
                "{DECLARATION}: unknown ui {ui:?} for the {} stack (known: {})",
                declaration.stack,
                offered.join(", ")
            )]);
        };
        let table = table();
        let mut found = Vec::new();
        let units = if declaration.units.is_empty() {
            vec![Unit::implicit(&layout, &declaration.absent)]
        } else {
            if !declaration.absent.is_empty() {
                found.push(format!(
                    "{DECLARATION}: absent lists {:?}, and the project declares units: name what each unit lacks in its own absent, and take this one out",
                    declaration.absent
                ));
            }
            let (units, problems) =
                units::declared(&declaration.stack, &layout, &table, &declaration.units);
            found.extend(problems);
            units
        };
        let places: Vec<Place> = units
            .iter()
            .flat_map(|unit| unit.places(&layout, &table))
            .collect();
        let mut left_out: Vec<(String, String)> = Vec::new();
        for unit in &units {
            for name in unit
                .absent
                .iter()
                .filter(|name| !places.iter().any(|p| &p.name == *name))
            {
                if !layout.leaves_out(name) {
                    found.push(if unit.declared {
                        format!(
                            "{DECLARATION}: unit {:?} names {name:?} in absent, which the {} layout does not have",
                            unit.name, declaration.stack
                        )
                    } else {
                        format!(
                            "{DECLARATION}: absent names {name:?}, which the {} layout does not have",
                            declaration.stack
                        )
                    });
                } else if !left_out.contains(&(unit.name.clone(), name.clone())) {
                    left_out.push((unit.name.clone(), name.clone()));
                }
            }
        }
        found.extend(levels_without_their_layer(&units, &places));
        found.extend(dependency_problems(&declaration));
        let todo_link = declaration.todo_link.as_deref().and_then(|value| {
            let link = TodoLink::parse(value);
            if link.is_none() {
                found.push(format!(
                    "{DECLARATION}: unknown todo_link {value:?} (known: {})",
                    TodoLink::KNOWN.join(", ")
                ));
            }
            link
        });
        if !found.is_empty() {
            return Err(found);
        }
        Ok(Declared {
            declaration,
            layout: Some(layout),
            units,
            places,
            left_out,
            todo_link,
        })
    }

    /// The entry files of every unit, each as a place of its own: what the checks that read code outside the layers
    /// start from. Empty for a repository of records only, and for a stack without an entry file.
    pub fn entries(&self, table: &Table) -> Vec<Place> {
        let Some(layout) = &self.layout else {
            return Vec::new();
        };
        self.units
            .iter()
            .flat_map(|unit| unit.entry_places(layout, table))
            .collect()
    }

    /// The directories, from the root, where code is looked for: the layout's `scope`, then the root of each unit that
    /// is not inside one before it, so a file is read once.
    pub fn scopes(&self) -> Vec<String> {
        let Some(layout) = &self.layout else {
            return Vec::new();
        };
        let mut scopes = vec![layout.scope.clone()];
        for unit in &self.units {
            if !scopes
                .iter()
                .any(|scope| scope.is_empty() || within(&unit.root, scope))
            {
                scopes.push(unit.root.clone());
            }
        }
        scopes
    }

    /// The unit a place sits in.
    pub fn unit_of(&self, place: &Place) -> Option<&Unit> {
        self.units.iter().find(|unit| unit.name == place.unit.name)
    }

    /// Declared absent itself, or inside a layer declared absent, by the unit it sits in.
    pub fn is_absent(&self, place: &Place) -> bool {
        let absent = self
            .unit_of(place)
            .map_or(&self.declaration.absent, |unit| &unit.absent);
        absent.contains(&place.name) || place.parent.as_ref().is_some_and(|p| absent.contains(p))
    }

    /// The paths in `unchecked`, as compared: without a `/` at the end.
    pub fn unchecked(&self) -> impl Iterator<Item = &str> {
        self.declaration
            .unchecked
            .iter()
            .map(|path| path.trim_end_matches('/'))
    }

    /// The paths in `moving`, as compared: without a `/` at the end.
    pub fn moving(&self) -> impl Iterator<Item = &str> {
        self.declaration
            .moving
            .iter()
            .map(|path| path.trim_end_matches('/'))
    }

    /// The layer or level a path in `unchecked` or `moving` holds or sits in. Such a path switches nothing off: a
    /// layer cannot be.
    pub fn overlapped(&self, path: &str) -> Option<&Place> {
        self.places
            .iter()
            .find(|p| within(&p.path, path) || within(path, &p.path))
    }

    /// Whether the file at `path` sits in a path the project lists in `unchecked`. A path that holds or sits in a layer
    /// switches nothing off, as the structure check says of it: the code in that layer is read all the same.
    pub fn is_unchecked(&self, path: &str) -> bool {
        self.unchecked()
            .filter(|skip| self.overlapped(skip).is_none())
            .any(|skip| within(path, skip))
    }

    /// Whether the file at `path` sits in a path the project lists in `moving`, with the same limit as
    /// [`Declared::is_unchecked`]: a path that holds or sits in a layer switches nothing off.
    pub fn is_moving(&self, path: &str) -> bool {
        self.moving()
            .filter(|moving| self.overlapped(moving).is_none())
            .any(|moving| within(path, moving))
    }

    /// Whether no check reads the file at `path`: it sits in a path of `unchecked` or of `moving`.
    pub fn is_not_checked(&self, path: &str) -> bool {
        self.is_unchecked(path) || self.is_moving(path)
    }
}

/// What an upgrade reads from a declaration, before the rest of it is checked: a declaration written by an older
/// Rotproof may lack a field the running one requires, which the upgrade adds.
#[derive(Debug, PartialEq, Eq)]
pub struct Fields {
    pub stack: Option<String>,
    pub ui: Option<String>,
    pub absent: Vec<String>,
    /// `None` when the declaration has no `files`
    pub files: Option<String>,
    pub declined: Vec<String>,
}

/// The fields of the declaration `text` an upgrade needs, or why they cannot be read.
pub fn fields(text: &str) -> Result<Fields, String> {
    let document = text
        .parse::<DocumentMut>()
        .map_err(|e| format!("{DECLARATION}: {}", e.message()))?;
    let string = |name: &str| -> Result<Option<String>, String> {
        match document.get(name) {
            None => Ok(None),
            Some(item) => item
                .as_str()
                .map(|s| Some(s.to_string()))
                .ok_or_else(|| format!("{DECLARATION}: {name} is not a string")),
        }
    };
    let names = |name: &str| -> Result<Vec<String>, String> {
        match document.get(name) {
            None => Ok(Vec::new()),
            Some(item) => item
                .as_array()
                .and_then(|array| {
                    array
                        .iter()
                        .map(|v| v.as_str().map(str::to_string))
                        .collect::<Option<Vec<_>>>()
                })
                .ok_or_else(|| format!("{DECLARATION}: {name} is not a list of names")),
        }
    };
    Ok(Fields {
        stack: string("stack")?,
        ui: string("ui")?,
        absent: names("absent")?,
        files: string("files")?,
        declined: names("declined")?,
    })
}

/// The declaration `text` with `files` set to `running`, and `declined` added when it lacks one, each under its
/// comment, and the fields added as `name = value`, as `rotproof create` names those it adds. Every other comment and
/// value is kept, and the lines end with `\n`, as `text` was read: the caller writes it as the project wrote the file
/// ([`Written`](crate::tree::Written)).
pub fn with_files(text: &str, running: &str) -> Result<(String, Vec<String>), String> {
    let mut document = text
        .parse::<DocumentMut>()
        .map_err(|e| format!("{DECLARATION}: {}", e.message()))?;
    let mut added = Vec::new();
    match document.get_mut("files") {
        Some(item) => {
            // The value changes, its comment and place stay
            let decor = item.as_value().map(|v| v.decor().clone());
            *item = toml_edit::value(running);
            if let (Some(decor), Some(value)) = (decor, item.as_value_mut()) {
                *value.decor_mut() = decor;
            }
        }
        None => added.push(add_field(
            &mut document,
            "files",
            toml_edit::value(running),
            FILES_COMMENT,
        )),
    }
    if !document.contains_key("declined") {
        added.push(add_field(
            &mut document,
            "declined",
            toml_edit::value(Array::new()),
            DECLINED_COMMENT,
        ));
    }
    Ok((document.to_string(), added))
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
    fn the_declaration_init_writes_reads_back() {
        for stack in known_stacks() {
            for ui in uis(stack).unwrap_or_else(|| vec![String::new()]) {
                let declaration: Declaration = parse_declaration(&declaration_text(stack, &ui))
                    .unwrap_or_else(|e| panic!("{stack}: {e}"));
                assert_eq!(declaration.stack, stack);
                assert_eq!(declaration.ui, (!ui.is_empty()).then_some(ui.clone()));
                assert!(Declared::new(declaration).is_ok(), "{stack} {ui}");
            }
        }
    }

    #[test]
    fn a_project_whose_views_are_templates_starts_without_presentation_and_may_have_it() {
        let text = declaration_text("python", "rendered");
        assert!(text.contains("\nabsent = [\"presentation\"]\n"), "{text}");
        assert!(
            text.contains("# atomic | rendered | none\nui = \"rendered\""),
            "{text}"
        );
        let present = |text: &str| -> Vec<String> {
            let declared = Declared::new(parse_declaration(text).unwrap()).unwrap();
            declared
                .places
                .iter()
                .filter(|place| !declared.is_absent(place))
                .map(|place| place.name.clone())
                .collect()
        };
        let names = present(&text);
        assert!(names.contains(&"ui.pages".to_string()) && names.contains(&"ui.atoms".to_string()));
        assert!(
            !names.iter().any(|n| n.starts_with("presentation")),
            "{names:?}"
        );
        // The same project with `presentation` taken out of `absent` has it, and one with the atomic UI starts with it
        let kept = present(&text.replace("absent = [\"presentation\"]", "absent = []"));
        assert!(kept.contains(&"presentation.models".to_string()));
        assert!(declaration_text("python", "atomic").contains("\nabsent = []\n"));
    }

    #[test]
    fn the_ui_picks_the_layers_of_the_screens() {
        let declared = |text: &str| Declared::new(parse_declaration(text).unwrap());
        let names = |declared: &Declared| -> Vec<String> {
            declared.places.iter().map(|p| p.name.clone()).collect()
        };
        let atomic = declared("stack = \"python\"\nareas = []\nui = \"atomic\"\n").unwrap();
        assert!(names(&atomic).contains(&"ui.pages".to_string()));
        assert!(names(&atomic).contains(&"presentation.models".to_string()));
        let none = declared("stack = \"python\"\nareas = []\nui = \"none\"\n").unwrap();
        assert!(
            !names(&none)
                .iter()
                .any(|n| n.starts_with("ui") || n.starts_with("presentation")),
            "{:?}",
            names(&none)
        );
        // Nothing imports a layer that is not there: composition loses ui and presentation
        let composition = none
            .places
            .iter()
            .find(|p| p.name == "composition")
            .unwrap();
        assert!(!composition.imports.contains(&"ui".to_string()));
        // absent naming a layer the UI leaves out reads, so an upgrade still runs, and is kept for the check to name
        let left = declared("stack = \"python\"\nareas = []\nui = \"none\"\nabsent = [\"ui\", \"presentation.display\"]\n")
            .unwrap();
        let names = |declared: &Declared| -> Vec<String> {
            declared
                .left_out
                .iter()
                .map(|(_, name)| name.clone())
                .collect()
        };
        assert_eq!(names(&left), ["ui", "presentation.display"]);
        assert_eq!(
            declared("stack = \"python\"\nareas = []\n").unwrap_err(),
            [
                ".config/rotproof.toml: missing field `ui`: run `rotproof init` after an upgrade of Rotproof, `rotproof create` otherwise; either adds it"
            ]
        );
        assert_eq!(
            declared("stack = \"rust\"\nareas = []\nui = \"atomic\"\n").unwrap_err(),
            [".config/rotproof.toml: unknown ui \"atomic\" for the rust stack (known: none)"]
        );
        assert!(
            declared("stack = \"none\"\nareas = []\nui = \"none\"\n").unwrap_err()[0]
                .contains("has no ui")
        );
    }

    #[test]
    fn todo_link_is_one_of_two_values_and_needs_code_to_read() {
        let declared = |text: &str| Declared::new(parse_declaration(text).unwrap());
        let python = "stack = \"python\"\nareas = []\nui = \"none\"\n";
        assert_eq!(declared(python).unwrap().todo_link, None);
        assert_eq!(
            declared(&format!("{python}todo_link = \"number\"\n"))
                .unwrap()
                .todo_link,
            Some(TodoLink::Number)
        );
        assert_eq!(
            declared("stack = \"rust\"\nareas = []\nui = \"none\"\ntodo_link = \"key\"\n")
                .unwrap()
                .todo_link,
            Some(TodoLink::Key)
        );
        // Any other value fails, naming the two known ones, as an unknown ui does
        for value in ["Number", "issue", "", "#"] {
            assert_eq!(
                declared(&format!("{python}todo_link = {value:?}\n")).unwrap_err(),
                [format!(
                    ".config/rotproof.toml: unknown todo_link {value:?} (known: number, key)"
                )]
            );
        }
        // Not a string, or twice as bad: every reason is named
        assert!(parse_declaration(&format!("{python}todo_link = 1\n")).is_err());
        assert_eq!(
            declared(&format!(
                "{python}todo_link = \"x\"\nabsent = [\"nowhere\"]\n"
            ))
            .unwrap_err()
            .len(),
            2
        );
        // No layers, no code to read: whatever the value, it says nothing
        for value in ["number", "key", "x"] {
            let found = declared(&format!(
                "stack = \"none\"\nareas = []\ntodo_link = \"{value}\"\n"
            ))
            .unwrap_err();
            assert_eq!(found.len(), 1);
            assert!(
                found[0].contains("stack = \"none\" has no layers")
                    && found[0].contains(&format!("take out todo_link = \"{value}\"")),
                "{found:?}"
            );
        }
        // The declaration init writes does not hold it: it is documented in the specification alone
        for stack in known_stacks() {
            assert!(!declaration_text(stack, "none").contains("todo_link"));
        }
    }

    #[test]
    fn approved_max_is_a_count_that_nothing_writes() {
        let python = "stack = \"python\"\nareas = []\nui = \"none\"\n";
        let max = |text: &str| parse_declaration(text).map(|declaration| declaration.approved_max);
        assert_eq!(max(python), Ok(None));
        assert_eq!(max(&format!("{python}approved_max = 3\n")), Ok(Some(3)));
        assert_eq!(max(&format!("{python}approved_max = 0\n")), Ok(Some(0)));
        // Not a count: a negative number, a string, a fraction
        for value in ["-1", "\"3\"", "1.5"] {
            assert!(
                max(&format!("{python}approved_max = {value}\n")).is_err(),
                "{value}"
            );
        }
        // The declaration init writes does not hold it: it is documented in the specification alone
        for stack in known_stacks() {
            assert!(!declaration_text(stack, "none").contains("approved_max"));
        }
    }

    #[test]
    fn a_field_create_adds_is_named_with_the_commands_that_add_it() {
        // After an upgrade the command is init, which sets `files`, and create otherwise: the declaration, which
        // cannot be read without the field, says both
        assert_eq!(
            parse_declaration("stack = \"none\"\n").unwrap_err(),
            "missing field `areas`: run `rotproof init` after an upgrade of Rotproof, `rotproof create` otherwise; \
             either adds it"
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
        let (text, added) = completed(old, &[("areas", First::List(tags))]);
        assert_eq!(added, ["areas = [\"billing\", \"ops\"]"]);
        assert!(text.starts_with(old), "{text}");
        assert!(text.contains(&format!("\n{AREAS_COMMENT}")), "{text}");
        assert_eq!(parse_declaration(&text).unwrap().areas, ["billing", "ops"]);
        // ui only with layers
        let python = "stack = \"python\"\nareas = []\n";
        assert_eq!(lacking(python), Some(vec!["ui"]));
        let (text, added) = completed(python, &[("ui", First::Name("none".into()))]);
        assert_eq!(added, ["ui = \"none\""]);
        assert!(text.contains(&format!("\n{UI_COMMENT}")), "{text}");
        assert_eq!(
            parse_declaration(&text).unwrap().ui.as_deref(),
            Some("none")
        );
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
    fn a_file_in_a_path_listed_in_unchecked_is_skipped() {
        let declared = Declared::new(
            parse_declaration(
                "stack = \"python\"\nareas = []\nui = \"atomic\"\nunchecked = [\"scripts/\", \"gen\"]\n",
            )
            .unwrap(),
        )
        .unwrap();
        assert!(declared.is_unchecked("scripts/run.py"));
        assert!(declared.is_unchecked("gen/a/b.py"));
        assert!(!declared.is_unchecked("generated/a.py"));
        assert!(!declared.is_unchecked("domain/scripts/x.py"));
    }

    #[test]
    fn a_path_in_unchecked_that_holds_or_sits_in_a_layer_skips_nothing() {
        let declared = Declared::new(
            parse_declaration(
                "stack = \"python\"\nareas = []\nui = \"atomic\"\nunchecked = [\"domain\", \"ui/atoms/gen/\"]\n",
            )
            .unwrap(),
        )
        .unwrap();
        assert!(!declared.is_unchecked("domain/song.py"));
        assert!(!declared.is_unchecked("ui/atoms/gen/x.py"));
    }

    #[test]
    fn a_path_in_moving_is_not_checked_as_one_in_unchecked_is_and_is_not_the_same_field() {
        let declared = Declared::new(
            parse_declaration(
                "stack = \"python\"\nareas = []\nui = \"atomic\"\nunchecked = [\"gen\"]\nmoving = [\"legacy/\", \"old.py\", \"domain\"]\n",
            )
            .unwrap(),
        )
        .unwrap();
        assert!(declared.is_moving("legacy/a/b.py"));
        assert!(declared.is_moving("old.py"));
        assert!(!declared.is_moving("legacy2/a.py"));
        // A layer is read all the same, whatever lists it
        assert!(!declared.is_moving("domain/song.py"));
        // The two fields keep their own paths, and either keeps a file out of every check
        assert!(!declared.is_unchecked("legacy/a.py") && !declared.is_moving("gen/x.py"));
        assert!(declared.is_not_checked("gen/x.py") && declared.is_not_checked("legacy/a.py"));
        assert!(!declared.is_not_checked("domain/song.py"));
        assert_eq!(
            declared.moving().collect::<Vec<_>>(),
            ["legacy", "old.py", "domain"]
        );
    }

    #[test]
    fn a_project_that_declares_no_unit_is_one_implicit_unit() {
        for (stack, ui) in [
            ("python", "atomic"),
            ("typescript", "none"),
            ("rust", "none"),
        ] {
            let declared = Declared::new(
                parse_declaration(&format!(
                    "stack = \"{stack}\"\nareas = []\nui = \"{ui}\"\nabsent = []\n"
                ))
                .unwrap(),
            )
            .unwrap();
            let layout = declared.layout.as_ref().unwrap();
            let [unit] = &declared.units[..] else {
                panic!("{stack}: {:?}", declared.units);
            };
            assert_eq!(unit.path, layout.scope);
            assert!(!unit.declared && unit.name.is_empty());
            // The places are those of the unit, and so those of the layout
            assert_eq!(declared.places, unit.places(layout, &table()));
            assert_eq!(declared.places.len(), layout.places(&table()).len());
            assert!(declared.places.iter().all(|p| p.unit == unit.tag()));
            // The entry is the layout's, and the code is looked for where the layout says
            let entries = declared.entries(&table());
            assert_eq!(
                entries.iter().map(|e| &e.path).collect::<Vec<_>>(),
                layout.entry.iter().collect::<Vec<_>>()
            );
            assert_eq!(declared.scopes(), std::slice::from_ref(&layout.scope));
        }
        let none =
            Declared::new(parse_declaration("stack = \"none\"\nareas = []\n").unwrap()).unwrap();
        assert!(
            none.units.is_empty() && none.entries(&table()).is_empty() && none.scopes().is_empty()
        );
    }

    #[test]
    fn the_places_of_every_unit_are_kept_apart() {
        let mut declared = Declared::new(
            parse_declaration(
                "stack = \"typescript\"
areas = []
ui = \"none\"
",
            )
            .unwrap(),
        )
        .unwrap();
        let layout = declared.layout.clone().unwrap();
        let table = table();
        let one = declared.places.len();
        let mut billing = declared.units[0].clone();
        billing.name = "billing".into();
        billing.path = "packages/billing/src".into();
        billing.root = "packages/billing".into();
        billing.entries = vec!["apps/cli/main.ts".into()];
        let mut inside = billing.clone();
        inside.name = "inside".into();
        inside.path = "src/inside".into();
        inside.root = "src/inside".into();
        inside.entries = Vec::new();
        for unit in [&billing, &inside] {
            declared.places.extend(unit.places(&layout, &table));
        }
        declared.units.extend([billing, inside]);
        assert_eq!(declared.places.len(), 3 * one);
        // The same name in each unit
        let domains: Vec<&Place> = declared
            .places
            .iter()
            .filter(|p| p.name == "domain")
            .collect();
        assert_eq!(domains.len(), 3);
        assert!(!domains[0].is_same(domains[1]) && !domains[1].is_same(domains[2]));
        assert_eq!(domains[1].path, "packages/billing/src/domain");
        // Every unit's entries, and where code is looked for: the root of a unit that the scope holds adds nothing
        let entries = declared.entries(&table);
        assert_eq!(
            entries.iter().map(|e| e.path.as_str()).collect::<Vec<_>>(),
            ["src/main.tsx", "apps/cli/main.ts"]
        );
        assert!(entries[1].unit.app && entries[1].unit.name == "billing");
        assert_eq!(declared.scopes(), ["src", "packages/billing"]);
    }

    #[test]
    fn setting_files_names_every_field_it_adds() {
        let (text, added) = with_files("stack = \"none\"\n", "0.5.0").unwrap();
        assert_eq!(added, ["files = \"0.5.0\"", "declined = []"]);
        let read = fields(&text).unwrap();
        assert_eq!(read.files.as_deref(), Some("0.5.0"));
        // files set, not added: only declined is added
        let (text, added) = with_files("files = \"0.4.0\" # kept\n", "0.5.0").unwrap();
        assert_eq!(added, ["declined = []"]);
        assert!(text.starts_with("files = \"0.5.0\" # kept\n"), "{text:?}");
        // Both there: nothing added
        let (_, added) = with_files(&text, "0.5.0").unwrap();
        assert!(added.is_empty(), "{added:?}");
    }

    /// A declaration of a stack with `ui = "none"` and the `[[unit]]` tables `units`, its fields above them.
    fn with_units(stack: &str, units: &str) -> String {
        format!("stack = \"{stack}\"\nareas = []\nui = \"none\"\nfiles = \"2.0.0\"\n{units}")
    }

    const MONOREPO: &str = "[[unit]]
name = \"core\"
path = \"packages/core/src/core\"
shared = true

[[unit]]
name = \"billing\"
path = \"packages/billing/src/billing\"
root = \"packages/billing\"
absent = [\"application\"]

[[unit]]
name = \"cli\"
path = \"apps/cli/src/cli\"
entry = [\"apps/cli/__main__.py\"]
absent = [\"domain\", \"application\"]
";

    #[test]
    fn declared_units_replace_the_implicit_one_and_their_places_and_entries_flow_through_declared()
    {
        let declared =
            Declared::new(parse_declaration(&with_units("python", MONOREPO)).unwrap()).unwrap();
        let layout = declared.layout.as_ref().unwrap();
        let table = table();
        let names: Vec<&str> = declared.units.iter().map(|u| u.name.as_str()).collect();
        assert_eq!(names, ["core", "billing", "cli"]);
        // Every unit's places, a unit's together, each in its own directory and tagged with it
        assert_eq!(declared.places.len(), 3 * layout.places(&table).len());
        for unit in &declared.units {
            let own: Vec<&Place> = declared
                .places
                .iter()
                .filter(|p| p.unit.name == unit.name)
                .collect();
            assert_eq!(own.len(), layout.places(&table).len(), "{}", unit.name);
            assert!(
                own.iter()
                    .all(|p| p.path.starts_with(&format!("{}/", unit.path)))
            );
            assert!(own.iter().all(|p| p.unit == unit.tag()));
        }
        let domains: Vec<&str> = declared
            .places
            .iter()
            .filter(|p| p.name == "domain")
            .map(|p| p.path.as_str())
            .collect();
        assert_eq!(
            domains,
            [
                "packages/core/src/core/domain",
                "packages/billing/src/billing/domain",
                "apps/cli/src/cli/domain"
            ]
        );
        assert!(declared.places.iter().any(|p| p.unit.shared));
        // The entry of the app is the only one: a unit that declares none has none, and no implicit entry is added
        let entries = declared.entries(&table);
        assert_eq!(
            entries.iter().map(|e| e.path.as_str()).collect::<Vec<_>>(),
            ["apps/cli/__main__.py"]
        );
        assert!(entries[0].unit.app && entries[0].unit.name == "cli");
        // Code is looked for in the scope of the layout, which a Python project has at its root
        assert_eq!(declared.scopes(), [""]);
        // What each lacks is its own: the unit that says so lacks it, and the others do not
        let absent = |unit: &str, layer: &str| {
            let place = declared
                .places
                .iter()
                .find(|p| p.unit.name == unit && p.name == layer)
                .unwrap();
            declared.is_absent(place)
        };
        assert!(
            absent("billing", "application")
                && absent("cli", "application")
                && absent("cli", "domain")
        );
        assert!(!absent("core", "application") && !absent("billing", "domain"));
        // A level is as absent as its layer
        let mut declared = declared;
        declared.units[2].absent.push("infrastructure".into());
        let level = declared
            .places
            .iter()
            .find(|p| p.unit.name == "cli" && p.name == "infrastructure.inbound")
            .unwrap();
        assert!(declared.is_absent(level));
    }

    #[test]
    fn what_a_unit_lacks_that_the_ui_leaves_out_is_kept_with_the_unit_that_lists_it() {
        let text = with_units(
            "python",
            "[[unit]]\nname = \"a\"\npath = \"a\"\nabsent = [\"ui\"]\n\n[[unit]]\nname = \"b\"\npath = \"b\"\nabsent = [\"ui\", \"presentation\"]\n",
        );
        let declared = Declared::new(parse_declaration(&text).unwrap()).unwrap();
        let kept: Vec<(&str, &str)> = declared
            .left_out
            .iter()
            .map(|(unit, name)| (unit.as_str(), name.as_str()))
            .collect();
        assert_eq!(kept, [("a", "ui"), ("b", "ui"), ("b", "presentation")]);
    }

    #[test]
    fn units_are_for_a_stack_with_layers_and_the_top_level_absent_is_for_no_unit_of_its_own() {
        let none = "stack = \"none\"\nareas = []\n[[unit]]\nname = \"a\"\npath = \"a\"\n";
        assert_eq!(
            Declared::new(parse_declaration(none).unwrap()).unwrap_err(),
            [
                ".config/rotproof.toml: stack = \"none\" has no layers, so it has no units: take out [[unit]]"
            ]
        );
        // Empty, as `rotproof init` writes it, it names nothing
        let empty = with_units("python", "[[unit]]\nname = \"a\"\npath = \"a\"\n")
            .replace("ui = \"none\"\n", "ui = \"none\"\nabsent = []\n");
        assert!(Declared::new(parse_declaration(&empty).unwrap()).is_ok());
        // Without units it is the one unit's, as it was: the failure is the same line
        let one = "stack = \"python\"\nareas = []\nui = \"none\"\nabsent = [\"nowhere\"]\n";
        assert_eq!(
            Declared::new(parse_declaration(one).unwrap()).unwrap_err(),
            [
                ".config/rotproof.toml: absent names \"nowhere\", which the python layout does not have"
            ]
        );
    }

    #[test]
    fn every_level_of_a_layer_in_absent_without_the_layer_is_refused_in_a_unit_and_in_a_project_of_one()
     {
        let refused = |text: String| Declared::new(parse_declaration(&text).unwrap()).unwrap_err();
        let one = |absent: &str| {
            format!(
                "stack = \"python\"
areas = []
ui = \"none\"
absent = {absent}
"
            )
        };
        let unit = |absent: &str| {
            with_units(
                "python",
                &format!(
                    "[[unit]]
name = \"app\"
path = \"app\"
absent = {absent}
"
                ),
            )
        };
        let both = r#"["infrastructure.inbound", "infrastructure.outbound"]"#;
        let said = |who: &str| {
            format!(
                "{DECLARATION}: {who} every level of infrastructure in absent, but not infrastructure itself, which holds only its levels: list infrastructure, so that it is not made as a directory with nothing in it"
            )
        };
        assert_eq!(refused(one(both)), [said("absent names")]);
        assert_eq!(refused(unit(both)), [said("unit \"app\" names")]);
        // The layer itself, with or without its levels, a level but not the other, and the layer's own levels of
        // another unit: each reads, and the layer is absent in the first two
        for ok in [
            r#"["infrastructure"]"#,
            r#"["infrastructure", "infrastructure.inbound", "infrastructure.outbound"]"#,
            r#"["infrastructure.inbound"]"#,
            r#"["protocol.inbound", "domain"]"#,
        ] {
            assert!(
                Declared::new(parse_declaration(&one(ok)).unwrap()).is_ok(),
                "{ok}"
            );
            assert!(
                Declared::new(parse_declaration(&unit(ok)).unwrap()).is_ok(),
                "{ok}"
            );
        }
        // A unit is judged by its own absent: another that lists the layer does not excuse it
        let two = with_units(
            "python",
            &format!(
                "[[unit]]
name = \"a\"
path = \"a\"
absent = [\"infrastructure\"]

[[unit]]
name = \"b\"
path = \"b\"
absent = {both}
"
            ),
        );
        let said = refused(two);
        assert_eq!(said.len(), 1, "{said:?}");
        assert!(said[0].contains("unit \"b\" names every level of infrastructure"));
    }

    #[test]
    fn the_declaration_init_writes_reads_back_with_its_example_of_units_taken_out_of_the_comment() {
        for stack in known_stacks().into_iter().filter(|s| *s != RECORDS_ONLY) {
            let text = declaration_text(stack, "none");
            // Last in the file: what is added to the end of the file by hand goes to the last unit, which is no mistake
            // of Rotproof's, but the upgrade's `files` and `declined` are above it
            let at = text.find("# Units:").unwrap();
            assert!(text.find("files = ").unwrap() < at && text.find("declined = ").unwrap() < at);
            // The example is comments alone, so the declaration is as without it
            assert!(
                parse_declaration(&text).unwrap().units.is_empty(),
                "{stack}"
            );
            // Taken out of the comment, the example is two units that read
            let example: String = text[at..]
                .lines()
                .skip(3)
                .map(|line| format!("{}\n", line.strip_prefix("# ").unwrap_or("")))
                .collect();
            let uncommented = format!("{}{example}", &text[..at]);
            let declared = Declared::new(
                parse_declaration(&uncommented)
                    .unwrap_or_else(|e| panic!("{stack}: {e}\n{uncommented}")),
            )
            .unwrap_or_else(|e| panic!("{stack}: {e:?}\n{uncommented}"));
            let [billing, cli] = &declared.units[..] else {
                panic!("{stack}: {:?}", declared.units);
            };
            assert!(billing.shared && !cli.shared, "{stack}");
            assert_eq!(cli.entries.len(), 1, "{stack}");
            assert_eq!(billing.root, "packages/billing", "{stack}");
        }
        // A repository of records only has no units to say
        assert!(!declaration_text(RECORDS_ONLY, "").contains("[[unit]]"));
    }

    #[test]
    fn fields_added_to_a_declaration_with_units_go_above_them() {
        let text = format!("stack = \"python\"\nui = \"none\"\n\n{MONOREPO}");
        assert_eq!(lacking(&text), Some(vec!["areas"]));
        let (text, added) = completed(&text, &[("areas", First::List(vec!["a".into()]))]);
        assert_eq!(added, ["areas = [\"a\"]"]);
        let declaration = parse_declaration(&text).unwrap_or_else(|e| panic!("{e}\n{text}"));
        assert_eq!(declaration.areas, ["a"]);
        assert_eq!(declaration.units.len(), 3);
        let (text, added) = with_files(&text, "2.0.0").unwrap();
        assert_eq!(added, ["files = \"2.0.0\"", "declined = []"]);
        let declaration = parse_declaration(&text).unwrap_or_else(|e| panic!("{e}\n{text}"));
        assert_eq!(declaration.files.as_deref(), Some("2.0.0"));
        assert_eq!(declaration.units.len(), 3);
        // A unit is not read by the upgrade, which reads the fields above them
        assert_eq!(fields(&text).unwrap().absent, Vec::<String>::new());
    }
}
