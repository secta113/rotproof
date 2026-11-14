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

use crate::layers::{Layout, Place, RECORDS_ONLY, known_stacks, layout, table, uis};
use crate::paths::within;
use crate::version::VERSION;

/// Where a project declares its structure, from the root
pub const DECLARATION: &str = ".config/rotproof.toml";
/// What to do when the declaration is missing, with every stack: a test keeps the list equal to [`known_stacks`]
pub const MISSING: &str = "missing: .config/rotproof.toml. Write it with `rotproof init --stack <stack>` (python, \
                           typescript, rust, or none for records only) and `--ui atomic` or `--ui none` for python \
                           and typescript";
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
const DECLINED_COMMENT: &str = "# Updates of the project's files this project declines, by name, such as \"claude-deny-approvals\".\n\
                                # `rotproof init` says when one cannot be applied without a person\n";

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
    format!(
        "{head}\n# {stacks}\nstack = \"{stack}\"\n\n{areas}\n{UI_COMMENT}# {offered}\nui = \"{ui}\"\n\n\
         # Layers this project does not have, such as \"ui.templates\". Delete the directory too\n\
         absent = []\n\n\
         # Paths outside the layers that Rotproof does not look into, such as \"scripts\" (helper scripts, generated\n\
         # or vendored code). A path that holds a layer, or does not exist, fails\n\
         unchecked = []\n{upgrades}"
    )
}

/// A declaration together with the layout it names. `Err` is every reason the two do not fit, for the check.
#[derive(Debug)]
pub struct Declared {
    pub declaration: Declaration,
    /// `None` for a repository that keeps records only (`stack = "none"`)
    pub layout: Option<Layout>,
    pub places: Vec<Place>,
    /// What `absent` names in a layer the declared UI leaves out (`ui` with `ui = "none"`): it says nothing, and the
    /// structure check fails on it. It does not stop `rotproof create`, so an upgrade that adds `ui` still runs
    pub left_out: Vec<String>,
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
            if let Some(ui) = &declaration.ui {
                found.push(format!(
                    "{DECLARATION}: stack = \"none\" has no layers, so it has no ui: take out ui = {ui:?}"
                ));
            }
            if !found.is_empty() {
                return Err(found);
            }
            return Ok(Declared {
                declaration,
                layout: None,
                places: Vec::new(),
                left_out: Vec::new(),
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
        let places = layout.places(&table());
        let (left_out, unknown): (Vec<String>, Vec<String>) = declaration
            .absent
            .iter()
            .filter(|name| !places.iter().any(|p| &p.name == *name))
            .cloned()
            .partition(|name| layout.leaves_out(name));
        if !unknown.is_empty() {
            return Err(unknown
                .iter()
                .map(|name| {
                    format!(
                        "{DECLARATION}: absent names {name:?}, which the {} layout does not have",
                        declaration.stack
                    )
                })
                .collect());
        }
        Ok(Declared {
            declaration,
            layout: Some(layout),
            places,
            left_out,
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

    /// Whether the file at `path` sits in a path the project lists in `unchecked`. A path that holds or sits in a layer
    /// switches nothing off, as the structure check says of it: the code in that layer is read all the same.
    pub fn is_unchecked(&self, path: &str) -> bool {
        self.unchecked()
            .filter(|skip| self.overlapped(skip).is_none())
            .any(|skip| within(path, skip))
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
        assert_eq!(left.left_out, ["ui", "presentation.display"]);
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
}
