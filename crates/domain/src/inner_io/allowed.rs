//! The packages the inner layers may import: a list for each layer or level, in Python and TypeScript.
//!
//! An inner layer imports no package until it is allowed one: a package is code Rotproof cannot read, so what it does
//! (a file, a socket, a clock) is unknown, and the check of the I/O cannot judge it. A project allows the packages that
//! do none, by layer or level, in `allowed` under `[dependencies]` of its declaration, and a unit adds its own in its
//! `[[unit]]` table. The layout of a stack allows what its own framework needs (TypeScript's `react` for
//! `presentation.models`).
//!
//! - **A key is a layer or a level of the four judged layers.** A layer's key covers its levels, a level's key adds to
//!   that level: `domain = ["pydantic"]` is for the whole layer, `"presentation.models" = ["zustand"]` for the models
//!   alone. A key for a free layer (`infrastructure`, `composition`, `ui`), for a name the table does not have, or for
//!   a layer or level the project (or the unit) lacks, fails the declaration.
//! - **A name is as the manifest lists it:** a Python distribution compared as PEP 503 compares names (an import name
//!   is mapped to its distribution by `imports`, as the dependencies check does), an npm package as written.
//! - **The declaration keeps no name that is gone:** a name that no manifest of the project lists, or that no file of
//!   the layer (or level) it is allowed for imports, fails, and so does a name allowed twice for the same code.
//! - **Rust has none:** Cargo already stops a crate that names what its `Cargo.toml` does not list, and the
//!   dependencies check fails a dependency nothing uses, so the crate's own `[dependencies]` is its allowlist and a
//!   second list would hold one fact twice.
//! - **It is a person's decision,** never made to pass the check, and no command writes or drafts it.

use std::collections::{BTreeMap, BTreeSet};

use crate::declaration::{Allowed, DECLARATION, Declaration};
use crate::dependencies::python::normalize;
use crate::direction::{Forbidden, Rule};
use crate::layers::{Language, Layout, Place, table};
use crate::units::Unit;

/// What the guide says of this rule in the section on the layers, for a project of `language`.
pub fn guide(language: Language) -> String {
    match language {
        Language::Python => super::GUIDE_ALLOWED
            .replace(
                "{what}",
                "What a package of PyPI does is unknown, so an inner layer imports one only when it is allowed there, by \
                 the distribution name `pyproject.toml` lists (compared as PEP 503 compares names; an import that is of \
                 another distribution is mapped under `imports` in `[dependencies]`).",
            )
            .replace("{default}", ""),
        Language::TypeScript => super::GUIDE_ALLOWED
            .replace(
                "{what}",
                "What a package of npm does is unknown, so an inner layer imports one only when it is allowed there, by \
                 the name `package.json` lists.",
            )
            .replace(
                "{default}",
                "The layout allows `react` in `presentation.models` (the hooks that keep a screen's state), and not \
                 `react-dom`, which draws, so the project does not list `react` (listing it fails). ",
            ),
        Language::Rust => super::GUIDE_ALLOWED_RUST.to_string(),
    }
}

/// Who lists a name.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Scope {
    /// The layout of the stack, for every project of it
    Layout(String),
    /// `allowed` under `[dependencies]`: every unit
    Project,
    /// `allowed` of a `[[unit]]` table, by the unit's name
    Unit(String),
}

impl Scope {
    /// Whether the code of the unit named `unit` is judged by it.
    fn applies(&self, unit: &str) -> bool {
        match self {
            Scope::Layout(_) | Scope::Project => true,
            Scope::Unit(name) => name == unit,
        }
    }

    /// Whether the code of one unit can be judged by both.
    fn meets(&self, other: &Scope) -> bool {
        match (self, other) {
            (Scope::Unit(a), Scope::Unit(b)) => a == b,
            _ => true,
        }
    }

    /// What a message calls it, ending in the key it was written under.
    fn said(&self, key: &str) -> String {
        match self {
            Scope::Layout(stack) => format!("the {stack} layout ({key})"),
            Scope::Project => format!("allowed under [dependencies] ({key})"),
            Scope::Unit(name) => format!("allowed of unit {name:?} ({key})"),
        }
    }

    /// What starts a message about its own list.
    fn lead(&self) -> String {
        match self {
            Scope::Layout(stack) => format!("the {stack} layout"),
            Scope::Project => format!("{DECLARATION}: allowed under [dependencies]"),
            Scope::Unit(name) => format!("{DECLARATION}: allowed of unit {name:?}"),
        }
    }
}

/// A name allowed, as it is compared and as it is written.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Name {
    compared: String,
    shown: String,
}

/// The names one key allows, in one scope.
#[derive(Debug, Clone)]
struct Entry {
    scope: Scope,
    /// A layer (`domain`) or a level (`presentation.models`) of the four judged layers
    key: String,
    names: Vec<Name>,
}

/// Whether the key `key` covers the place named `place`: it is that place, or its layer.
fn covers(key: &str, place: &str) -> bool {
    key == place
        || place
            .strip_prefix(key)
            .is_some_and(|rest| rest.starts_with('.'))
}

/// A name of a package as the language compares it: PEP 503 in Python, as written in TypeScript.
fn compared(language: Language, name: &str) -> String {
    match language {
        Language::Python => normalize(name),
        Language::TypeScript | Language::Rust => name.trim().to_string(),
    }
}

/// The name a stack goes by in a message.
fn stack_name(language: Language) -> &'static str {
    match language {
        Language::Python => "python",
        Language::TypeScript => "typescript",
        Language::Rust => "rust",
    }
}

/// The layers and levels `allowed` may be written for: those of the table the rule judges.
fn judged_keys() -> Vec<String> {
    let mut keys = Vec::new();
    for layer in table().layers {
        if super::LAYERS.contains(&layer.name.as_str()) {
            keys.push(layer.name.clone());
            keys.extend(
                layer
                    .levels
                    .iter()
                    .map(|l| format!("{}.{}", layer.name, l.name)),
            );
        }
    }
    keys
}

/// Every name of the table a key can be: a layer and each of its levels, judged or not.
fn table_keys() -> Vec<String> {
    let mut keys = Vec::new();
    for layer in table().layers {
        keys.push(layer.name.clone());
        keys.extend(
            layer
                .levels
                .iter()
                .map(|l| format!("{}.{}", layer.name, l.name)),
        );
    }
    keys
}

/// The keys under `value` of the key `key`, as an unquoted dotted key writes them: `presentation` holding the table
/// `models` is `presentation.models`.
fn dotted(key: &str, value: &Allowed, found: &mut Vec<String>) {
    match value {
        Allowed::Table(inner) => {
            for (name, value) in inner {
                dotted(&format!("{key}.{name}"), value, found);
            }
        }
        Allowed::Names(_) | Allowed::Other(_) => found.push(key.to_string()),
    }
}

/// Whether the unit named `unit` has the place `key`: it is in its places and not declared absent.
fn has(units: &[Unit], places: &[Place], unit: &str, key: &str) -> bool {
    let Some(own) = units.iter().find(|u| u.name == unit) else {
        return false;
    };
    places.iter().any(|place| {
        place.unit.name == unit
            && place.name == key
            && !own.absent.contains(&place.name)
            && !place
                .parent
                .as_ref()
                .is_some_and(|parent| own.absent.contains(parent))
    })
}

/// The packages the inner layers may import, in every scope that lists some.
#[derive(Debug, Clone, Default)]
pub struct Allowlist {
    entries: Vec<Entry>,
}

/// A package a file of an inner layer imports, on a line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Imported {
    pub line: usize,
    /// What the file says (`pydantic.BaseModel`, `@shop/kernel/utils`)
    pub shown: String,
    /// The package it needs, as the language compares names
    pub package: String,
}

/// A package imported by a file of the layer or level `place` of the unit `unit` ("" for the implicit one), for what
/// the declaration lists that no file imports.
#[derive(Debug, Clone, Copy)]
pub struct Placed<'a> {
    pub unit: &'a str,
    pub place: &'a str,
    pub package: &'a str,
}

impl Allowlist {
    /// What the layout and the declaration allow, and every reason the declaration says it wrongly: from the
    /// declaration and the layout alone, as the units are read.
    pub fn read(
        declaration: &Declaration,
        layout: &Layout,
        units: &[Unit],
        places: &[Place],
    ) -> (Allowlist, Vec<String>) {
        let language = layout.language;
        let mut found = Vec::new();
        let mut entries = Vec::new();
        for (key, names) in &layout.allowed {
            entries.push(Entry {
                scope: Scope::Layout(stack_name(language).to_string()),
                key: key.clone(),
                names: names
                    .iter()
                    .map(|name| Name {
                        compared: compared(language, name),
                        shown: name.clone(),
                    })
                    .collect(),
            });
        }
        let mut scopes: Vec<(Scope, &BTreeMap<String, Allowed>)> =
            vec![(Scope::Project, &declaration.dependencies.allowed)];
        if declaration.units.len() == units.len() {
            scopes.extend(
                declaration
                    .units
                    .iter()
                    .map(|unit| (Scope::Unit(unit.name.clone()), &unit.allowed)),
            );
        }
        for (scope, written) in scopes {
            if written.is_empty() {
                continue;
            }
            if language == Language::Rust {
                found.push(format!(
                    "{} is set, and a Rust project has no such list: a crate's own Cargo.toml [dependencies] is the \
                     list of packages its layer may import (Cargo stops a crate that names what it does not list, \
                     and the dependencies check fails what nothing uses), so there is no second list to keep. \
                     Adding a dependency to the crate of domain, protocol, application or presentation is a \
                     decision of the person: take allowed out",
                    scope.lead()
                ));
                continue;
            }
            Self::read_scope(
                &scope,
                written,
                language,
                units,
                places,
                &mut entries,
                &mut found,
            );
        }
        found.extend(twice(&entries));
        (Allowlist { entries }, found)
    }

    /// One scope's `allowed`: its entries into `entries`, and what is wrong with it into `found`.
    fn read_scope(
        scope: &Scope,
        written: &BTreeMap<String, Allowed>,
        language: Language,
        units: &[Unit],
        places: &[Place],
        entries: &mut Vec<Entry>,
        found: &mut Vec<String>,
    ) {
        let lead = scope.lead();
        let judged = judged_keys();
        for (key, value) in written {
            let names = match value {
                Allowed::Names(names) => names,
                Allowed::Table(_) => {
                    let mut leaves = Vec::new();
                    dotted(key, value, &mut leaves);
                    for leaf in leaves {
                        found.push(format!(
                            "{lead} has the table {leaf}: TOML reads {leaf} = [...] written without quotes as the \
                             key {} inside the table {key}, not as the key {leaf}. Write the key in quotes: \
                             {leaf:?} = [...]",
                            leaf.rsplit('.').next().unwrap_or_default()
                        ));
                    }
                    continue;
                }
                Allowed::Other(other) => {
                    found.push(format!(
                        "{lead} has {key} = {other}, which is no list of names: write {key} = [\"name\"], the names \
                         as the manifest lists them"
                    ));
                    continue;
                }
            };
            if !table_keys().contains(key) {
                found.push(format!(
                    "{lead} has the key {key:?}, which is no layer or level of the table: write one of {}",
                    judged.join(", ")
                ));
                continue;
            }
            if !judged.contains(key) {
                found.push(format!(
                    "{lead} has the key {key:?}, a layer that is free to import any package (infrastructure does the \
                     I/O, composition wires and ui draws): only {} have a list, so take it out",
                    judged.join(", ")
                ));
                continue;
            }
            let present = match scope {
                Scope::Project => units.iter().any(|unit| has(units, places, &unit.name, key)),
                Scope::Unit(name) => has(units, places, name, key),
                Scope::Layout(_) => true,
            };
            if !present {
                let whose = match scope {
                    Scope::Unit(name) => format!("the unit {name:?} lacks"),
                    _ => "no unit of the project has".to_string(),
                };
                found.push(format!(
                    "{lead} has the key {key:?}, and {whose} it (declared absent, or left out by the ui): take it \
                     out"
                ));
                continue;
            }
            let mut read: Vec<Name> = Vec::new();
            for shown in names {
                let name = Name {
                    compared: compared(language, shown),
                    shown: shown.clone(),
                };
                if name.compared.is_empty() {
                    found.push(format!(
                        "{lead} lists an empty name for {key}: write the name the manifest lists"
                    ));
                } else if let Some(earlier) = read.iter().find(|n| n.compared == name.compared) {
                    found.push(format!(
                        "{lead} lists {} twice for {key} (as {:?} and {shown:?}): allow it once",
                        earlier.shown, earlier.shown
                    ));
                } else {
                    read.push(name);
                }
            }
            entries.push(Entry {
                scope: scope.clone(),
                key: key.clone(),
                names: read,
            });
        }
    }

    /// Whether the code of the place `place` of the unit `unit` ("" for the implicit one) may import `package`, as the
    /// language compares names.
    pub fn allows(&self, unit: &str, place: &str, package: &str) -> bool {
        self.entries.iter().any(|entry| {
            entry.scope.applies(unit)
                && covers(&entry.key, place)
                && entry.names.iter().any(|name| name.compared == package)
        })
    }

    /// The findings for the packages the file at `path` imports, whose text is `source`, in the place `place` of the
    /// unit `unit`: one for each line and package that is not allowed there, as a [`Forbidden`] of the package, with
    /// the imports of it the line holds as its names. The approval is made for the file and the package.
    pub fn findings(
        &self,
        path: &str,
        source: &str,
        unit: &str,
        place: &str,
        imported: &[Imported],
    ) -> Vec<Forbidden> {
        let mut lines: BTreeMap<(usize, &str), BTreeSet<&str>> = BTreeMap::new();
        for one in imported
            .iter()
            .filter(|one| !self.allows(unit, place, &one.package))
        {
            lines
                .entry((one.line, one.package.as_str()))
                .or_default()
                .insert(&one.shown);
        }
        let whose = if unit.is_empty() {
            place.to_string()
        } else {
            format!("{place} of unit {unit}")
        };
        lines
            .into_iter()
            .map(|((line, package), shown)| {
                let shown: Vec<&str> = shown.into_iter().collect();
                Forbidden {
                    file: path.to_string(),
                    import: package.to_string(),
                    detail: format!(
                        "{path}:{line}: move the code that needs the package to infrastructure, behind a port the use \
                         case is given, or, when the package does no I/O, ask the person whether to allow it for \
                         {place} (allowed under [dependencies] in {DECLARATION}; an agent never adds it to pass the \
                         check): imports {}, of the package {package}, which {whose} may not import\n{}",
                        shown.join(", "),
                        source.lines().nth(line - 1).unwrap_or("").trim()
                    ),
                    rule: Rule::InnerIo {
                        names: shown.iter().map(|name| name.to_string()).collect(),
                    },
                }
            })
            .collect()
    }

    /// What the declaration allows that is gone: a name that no manifest lists (`listed`, the names of every manifest
    /// read, as the language compares them; `None` when none was read, which says nothing of a name), and one that no
    /// file of the place it is allowed for imports (`imported`: every package the files of the inner layers import).
    /// The layout's names are never judged.
    pub fn stale(&self, imported: &[Placed], listed: Option<&BTreeSet<String>>) -> Vec<String> {
        let mut found = Vec::new();
        for entry in self
            .entries
            .iter()
            .filter(|entry| !matches!(entry.scope, Scope::Layout(_)))
        {
            let lead = entry.scope.lead();
            let key = &entry.key;
            for name in &entry.names {
                if listed.is_some_and(|listed| !listed.contains(&name.compared)) {
                    found.push(format!(
                        "{lead} lists {} for {key}, which no manifest of the project lists as a dependency: take it \
                         out",
                        name.shown
                    ));
                } else if !imported.iter().any(|one| {
                    entry.scope.applies(one.unit)
                        && covers(key, one.place)
                        && one.package == name.compared
                }) {
                    let of = match &entry.scope {
                        Scope::Unit(unit) => format!("{key} of unit {unit:?}"),
                        _ => key.clone(),
                    };
                    found.push(format!(
                        "{lead} lists {} for {key}, which no file of {of} imports: take it out, so that the list \
                         does not keep what is gone",
                        name.shown
                    ));
                }
            }
        }
        found
    }
}

/// What is allowed twice for the same code: a name twice in one entry's scope or in two entries whose keys cover the
/// same place, in scopes one unit can have both of.
fn twice(entries: &[Entry]) -> Vec<String> {
    let mut found = Vec::new();
    for (i, entry) in entries.iter().enumerate() {
        for later in &entries[i + 1..] {
            if !entry.scope.meets(&later.scope)
                || !(covers(&entry.key, &later.key) || covers(&later.key, &entry.key))
            {
                continue;
            }
            for name in &entry.names {
                let Some(other) = later.names.iter().find(|n| n.compared == name.compared) else {
                    continue;
                };
                found.push(match (&entry.scope, &later.scope) {
                    (Scope::Layout(stack), _) => format!(
                        "{} lists {} for {}, which the {stack} layout allows for {} already: take it out",
                        later.scope.lead(),
                        other.shown,
                        later.key,
                        entry.key
                    ),
                    _ => format!(
                        "{DECLARATION}: {} is allowed twice for the same code, by {} and by {}: allow it once",
                        name.shown,
                        entry.scope.said(&entry.key),
                        later.scope.said(&later.key)
                    ),
                });
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::declaration::{Declared, parse_declaration};

    /// The reasons a declaration of `stack` with `ui = "atomic"` and `rest` after it is refused.
    fn refused(stack: &str, rest: &str) -> Vec<String> {
        Declared::new(
            parse_declaration(&format!(
                "stack = \"{stack}\"\nareas = []\nui = \"atomic\"\n{rest}"
            ))
            .unwrap(),
        )
        .unwrap_err()
    }

    fn read(stack: &str, rest: &str) -> Declared {
        Declared::new(
            parse_declaration(&format!(
                "stack = \"{stack}\"\nareas = []\nui = \"atomic\"\n{rest}"
            ))
            .unwrap(),
        )
        .unwrap_or_else(|e| panic!("{e:#?}"))
    }

    #[test]
    fn a_key_covers_its_own_place_and_the_levels_of_a_layer() {
        assert!(covers("domain", "domain") && covers("presentation", "presentation.models"));
        assert!(covers("presentation.models", "presentation.models"));
        assert!(!covers("presentation.models", "presentation"));
        assert!(!covers("presentation.models", "presentation.display"));
        assert!(!covers("protocol", "protocol_x") && !covers("domain", "application"));
    }

    #[test]
    fn what_the_declaration_allows_is_by_layer_and_level_and_added_by_the_unit() {
        let declared = read(
            "python",
            "[dependencies]\nallowed = { domain = [\"Pydantic\"], \"presentation.models\" = [\"zustand\"] }\n",
        );
        let allowed = &declared.allowed;
        // A name is compared as PEP 503 compares names, and a layer's key covers its levels
        assert!(allowed.allows("", "domain", "pydantic"));
        assert!(!allowed.allows("", "application", "pydantic"));
        assert!(allowed.allows("", "presentation.models", "zustand"));
        assert!(!allowed.allows("", "presentation.display", "zustand"));
        assert!(!allowed.allows("", "domain", "zustand"));
    }

    #[test]
    fn the_layout_of_typescript_allows_react_in_the_models_and_python_allows_nothing() {
        let typescript = read("typescript", "");
        assert!(
            typescript
                .allowed
                .allows("", "presentation.models", "react")
        );
        assert!(
            !typescript
                .allowed
                .allows("", "presentation.models", "react-dom")
        );
        assert!(
            !typescript
                .allowed
                .allows("", "presentation.display", "react")
        );
        assert!(!typescript.allowed.allows("", "domain", "react"));
        let python = read("python", "");
        assert!(!python.allowed.allows("", "presentation.models", "react"));
        // What the layout allows is nothing the declaration lists, so it is never judged gone
        assert!(
            typescript
                .allowed
                .stale(&[], Some(&BTreeSet::new()))
                .is_empty()
        );
    }

    #[test]
    fn a_unit_adds_to_what_the_project_allows_for_its_own_code_only() {
        let declared = read(
            "python",
            "[dependencies]\nallowed = { domain = [\"pydantic\"] }\n\n[[unit]]\nname = \"a\"\npath = \"a\"\n\
             allowed = { domain = [\"attrs\"] }\n\n[[unit]]\nname = \"b\"\npath = \"b\"\n",
        );
        let allowed = &declared.allowed;
        assert!(
            allowed.allows("a", "domain", "attrs") && allowed.allows("a", "domain", "pydantic")
        );
        assert!(
            !allowed.allows("b", "domain", "attrs") && allowed.allows("b", "domain", "pydantic")
        );
    }

    #[test]
    fn a_key_the_declaration_cannot_have_is_refused_with_what_to_do() {
        let python = |rest: &str| refused("python", rest);
        let one = |found: Vec<String>| {
            assert_eq!(found.len(), 1, "{found:#?}");
            found[0].clone()
        };
        // Not a layer or level of the table, and a free layer
        let said = one(python("[dependencies]\nallowed = { domane = [\"x\"] }\n"));
        assert!(
            said.starts_with(".config/rotproof.toml: allowed under [dependencies] has the key \"domane\", which is no layer or level of the table: write one of presentation, presentation.models")
                && said.ends_with("protocol.outbound, domain"),
            "{said}"
        );
        for free in [
            "infrastructure",
            "composition",
            "ui",
            "ui.pages",
            "infrastructure.outbound",
        ] {
            let said = one(python(&format!(
                "[dependencies]\nallowed = {{ \"{free}\" = [\"x\"] }}\n"
            )));
            assert!(
                said.contains(&format!(
                    "has the key \"{free}\", a layer that is free to import any package"
                )),
                "{said}"
            );
        }
        // A layer the project lacks: absent, and left out by the ui
        let said = one(python(
            "absent = [\"protocol\"]\n[dependencies]\nallowed = { protocol = [\"x\"] }\n",
        ));
        assert!(
            said.contains(
                "has the key \"protocol\", and no unit of the project has it (declared absent"
            ),
            "{said}"
        );
        let none = refused_with_ui(
            "python",
            "none",
            "[dependencies]\nallowed = { presentation = [\"x\"] }\n",
        );
        assert!(
            none.len() == 1
                && none[0]
                    .contains("has the key \"presentation\", and no unit of the project has it"),
            "{none:?}"
        );
        // A level whose layer is declared absent is absent too
        let said = one(python(
            "absent = [\"presentation\"]\n[dependencies]\nallowed = { \"presentation.models\" = [\"x\"] }\n",
        ));
        assert!(said.contains("no unit of the project has it"), "{said}");
    }

    /// [`refused`] with another `ui`.
    fn refused_with_ui(stack: &str, ui: &str, rest: &str) -> Vec<String> {
        Declared::new(
            parse_declaration(&format!(
                "stack = \"{stack}\"\nareas = []\nui = \"{ui}\"\n{rest}"
            ))
            .unwrap(),
        )
        .unwrap_err()
    }

    #[test]
    fn a_unit_key_for_a_place_the_unit_lacks_fails_while_another_unit_has_it() {
        let found = refused(
            "python",
            "[[unit]]\nname = \"a\"\npath = \"a\"\nabsent = [\"application\"]\nallowed = { application = [\"x\"] }\n\n\
             [[unit]]\nname = \"b\"\npath = \"b\"\n",
        );
        assert_eq!(found.len(), 1, "{found:#?}");
        assert!(
            found[0].starts_with(".config/rotproof.toml: allowed of unit \"a\" has the key \"application\", and the unit \"a\" lacks it"),
            "{found:#?}"
        );
        // The project's key is for the units that have it: one is enough
        let kept = read(
            "python",
            "[dependencies]\nallowed = { application = [\"x\"] }\n[[unit]]\nname = \"a\"\npath = \"a\"\nabsent = [\"application\"]\n\n[[unit]]\nname = \"b\"\npath = \"b\"\n",
        );
        assert!(kept.allowed.allows("b", "application", "x"));
    }

    #[test]
    fn a_key_written_without_quotes_is_a_table_and_the_declaration_says_to_quote_it() {
        for text in [
            "[dependencies]\nallowed = { presentation.models = [\"zustand\"] }\n",
            "[dependencies.allowed]\npresentation.models = [\"zustand\"]\n",
        ] {
            let found = refused("python", text);
            assert_eq!(found.len(), 1, "{found:#?}");
            assert!(
                found[0].contains("has the table presentation.models: TOML reads presentation.models = [...] written without quotes as the key models inside the table presentation")
                    && found[0].ends_with("Write the key in quotes: \"presentation.models\" = [...]"),
                "{found:#?}"
            );
        }
        // The same in a unit
        let found = refused(
            "python",
            "[[unit]]\nname = \"a\"\npath = \"a\"\nallowed = { presentation.models = [\"zustand\"] }\n",
        );
        assert!(
            found[0].contains("allowed of unit \"a\" has the table presentation.models"),
            "{found:#?}"
        );
        // Quoted, it reads
        assert!(
            read(
                "python",
                "[dependencies]\nallowed = { \"presentation.models\" = [\"zustand\"] }\n"
            )
            .allowed
            .allows("", "presentation.models", "zustand")
        );
    }

    #[test]
    fn what_is_not_a_list_of_names_is_refused() {
        for value in ["\"pydantic\"", "1", "[1]", "[\"a\", 2]", "true"] {
            let found = refused(
                "python",
                &format!("[dependencies]\nallowed = {{ domain = {value} }}\n"),
            );
            assert_eq!(found.len(), 1, "{value}: {found:#?}");
            assert!(
                found[0].contains("has domain =") && found[0].contains("no list of names"),
                "{found:#?}"
            );
        }
        let found = refused(
            "python",
            "[dependencies]\nallowed = { domain = [\"\", \"  \"] }\n",
        );
        assert_eq!(found.len(), 2);
        assert!(
            found[0].contains("lists an empty name for domain"),
            "{found:#?}"
        );
    }

    #[test]
    fn a_name_allowed_twice_for_the_same_code_is_refused() {
        let twice = |rest: &str| refused("python", rest);
        // Twice in a list, as PEP 503 reads two spellings
        let found = twice(
            "[dependencies]\nallowed = { domain = [\"python-dotenv\", \"Python_Dotenv\"] }\n",
        );
        assert_eq!(found.len(), 1, "{found:#?}");
        assert!(
            found[0].contains("lists python-dotenv twice for domain"),
            "{found:#?}"
        );
        // A layer's key and a level's key of the same layer
        let found = twice(
            "[dependencies]\nallowed = { presentation = [\"zod\"], \"presentation.models\" = [\"zod\"] }\n",
        );
        assert_eq!(found.len(), 1, "{found:#?}");
        assert!(
            found[0].contains("zod is allowed twice for the same code, by allowed under [dependencies] (presentation) and by allowed under [dependencies] (presentation.models)"),
            "{found:#?}"
        );
        // The project's and the unit's
        let found = twice(
            "[dependencies]\nallowed = { domain = [\"zod\"] }\n[[unit]]\nname = \"a\"\npath = \"a\"\nallowed = { domain = [\"zod\"] }\n",
        );
        assert_eq!(found.len(), 1, "{found:#?}");
        assert!(
            found[0].contains(
                "by allowed under [dependencies] (domain) and by allowed of unit \"a\" (domain)"
            ),
            "{found:#?}"
        );
        // Two units' own lists are kept apart, and so are two levels
        assert!(Declared::new(
            parse_declaration(
                "stack = \"python\"\nareas = []\nui = \"atomic\"\n[[unit]]\nname = \"a\"\npath = \"a\"\nallowed = { domain = [\"zod\"] }\n[[unit]]\nname = \"b\"\npath = \"b\"\nallowed = { domain = [\"zod\"] }\n"
            )
            .unwrap()
        )
        .is_ok());
        assert!(Declared::new(
            parse_declaration(
                "stack = \"python\"\nareas = []\nui = \"atomic\"\n[dependencies]\nallowed = { \"protocol.inbound\" = [\"zod\"], \"protocol.outbound\" = [\"zod\"] }\n"
            )
            .unwrap()
        )
        .is_ok());
        // What the layout allows is said to be allowed already
        let found = refused(
            "typescript",
            "[dependencies]\nallowed = { \"presentation.models\" = [\"react\"] }\n",
        );
        assert_eq!(found.len(), 1, "{found:#?}");
        assert!(
            found[0].starts_with(".config/rotproof.toml: allowed under [dependencies] lists react for presentation.models, which the typescript layout allows for presentation.models already: take it out"),
            "{found:#?}"
        );
    }

    #[test]
    fn a_rust_project_has_no_list_and_a_repository_of_records_has_no_code() {
        for rest in [
            "[dependencies]\nallowed = { domain = [\"serde\"] }\n",
            "[[unit]]\nname = \"a\"\npath = \"a\"\nallowed = { domain = [\"serde\"] }\n",
        ] {
            let found = Declared::new(
                parse_declaration(&format!(
                    "stack = \"rust\"\nareas = []\nui = \"none\"\n{rest}"
                ))
                .unwrap(),
            )
            .unwrap_err();
            assert_eq!(found.len(), 1, "{found:#?}");
            assert!(
                found[0].contains("is set, and a Rust project has no such list: a crate's own Cargo.toml [dependencies] is the list")
                    && found[0].ends_with("is a decision of the person: take allowed out"),
                "{found:#?}"
            );
        }
        let found = Declared::new(
            parse_declaration(
                "stack = \"none\"\nareas = []\n[dependencies]\nallowed = { domain = [\"x\"] }\n",
            )
            .unwrap(),
        )
        .unwrap_err();
        assert_eq!(found.len(), 1);
        assert!(
            found[0].contains("stack = \"none\" has no code")
                && found[0].contains("take out [dependencies]")
        );
    }

    #[test]
    fn a_name_no_manifest_lists_and_a_name_no_file_imports_are_gone() {
        let declared = read(
            "python",
            "[dependencies]\nallowed = { domain = [\"pydantic\", \"attrs\", \"ghost\"], \"presentation.models\" = [\"zustand\"] }\n",
        );
        let listed: BTreeSet<String> = ["pydantic", "attrs", "zustand"].map(String::from).into();
        let imported = [
            Placed {
                unit: "",
                place: "domain",
                package: "pydantic",
            },
            // The models import zustand, which is allowed for the models: another place does not make it used
            Placed {
                unit: "",
                place: "presentation.display",
                package: "zustand",
            },
        ];
        let found = declared.allowed.stale(&imported, Some(&listed));
        assert_eq!(found.len(), 3, "{found:#?}");
        assert!(
            found[0].contains("lists attrs for domain, which no file of domain imports"),
            "{found:#?}"
        );
        assert!(
            found[1].contains(
                "lists ghost for domain, which no manifest of the project lists as a dependency"
            ),
            "{found:#?}"
        );
        assert!(found[2].contains("lists zustand for presentation.models, which no file of presentation.models imports"), "{found:#?}");
        // No manifest read: a name is not judged by it, and a layer's key is used by a file of any of its levels
        let declared = read(
            "python",
            "[dependencies]\nallowed = { presentation = [\"zustand\"] }\n",
        );
        let models = [Placed {
            unit: "",
            place: "presentation.models",
            package: "zustand",
        }];
        assert!(declared.allowed.stale(&models, None).is_empty());
        assert_eq!(declared.allowed.stale(&[], None).len(), 1);
    }
}
