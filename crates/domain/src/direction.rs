//! The direction check: every layer imports only what the table (`layers/table.toml`) allows.
//!
//! - An import from layer A of layer B fails unless B is in A's `imports`. An entry of `imports` names a layer, which
//!   takes in its levels, or one level of another layer (`protocol.inbound`). A level may import what its own
//!   `imports` name, and the levels below it in an ordered layer (`ui`); not the levels beside it otherwise, nor its
//!   layer beside the levels. A place may always import itself, and a layer the levels inside it.
//! - Only direct imports are judged. What the table allows is closed under chaining (a test keeps it so), so a chain
//!   of allowed imports never reaches what its first module may not import.
//! - An import of a module in no layer present (the standard library, a third-party package, a layer declared absent)
//!   is not judged. A Python call of `importlib.import_module` or `__import__` with a string literal is an import of
//!   that module; one built at run time (a variable, an f-string, a relative name) is not seen.
//! - The entry file, which is no layer, is judged as a place of its own that may import what the table's `entry` lists.
//!   Nothing may import it.
//!
//! The layout's `language` says how the code is read. For `python`, a module is named by its dotted name
//! (`python.rs`), and its parts joined with `/` are its path. For `typescript`, an import is resolved to a path
//! (`typescript.rs`). For `rust`, a crate's code file is its `Cargo.toml`, and each dependency it declares
//! (`cargo.rs`) is resolved to the path it names, directly or through its workspace; Cargo compiles a crate only
//! against the crates it declares, so the declarations are its imports. The levels inside a crate are its modules,
//! judged from its code (`modules.rs`), and the entry file through the manifest of its package. In every language,
//! the path sits in the place it lands in as the operating system resolves it (on Windows, another case, dots and
//! spaces at the end and short names too), so a spelling that builds on the machine that runs the check is judged
//! there.
//!
//! The rules between units, for an import whose two sides are in different units (`units.rs`; a project that declares
//! none has one unit, so none of this applies). They are judged first, from the unit each place carries:
//!
//! 1. An import of a unit's code outside its layers (`Landing::InUnit`: the path is in the unit, in none of its
//!    places) fails. Inside its own unit it is not judged, as before.
//! 2. A unit that is not shared is imported only by the composition of an app (a unit with an entry), and not by
//!    that of another app. Every other place fails, the entry file too.
//! 3. A shared unit imports only shared units (judged before 2).
//! 4. Of a shared unit, `application` is read by no other unit's `application` (a composition passes its use cases
//!    in); the `composition` is read by the composition of an app alone; and every other place is read as if it sat
//!    in the importer's own unit, so the table decides, the place itself included. One place is read beyond the
//!    table: a shared unit's `domain`, which the `protocol.inbound` of any unit may read, since a shared unit is
//!    where the value types used across units live, and an input port that could not name them would copy their
//!    shape. It reads nothing of its own unit and nothing of a unit that is not shared, as the table says. A
//!    `composition` is read beyond the table the other way: of a shared unit it reads the `protocol`, the `domain`
//!    (the value types the ports take, which it builds the arguments from) and, an app's, the `composition`, and no
//!    other place.
//! 5. The entry file reads the `composition` of its own unit and nothing of another unit: rules 2 to 4 refuse it.
//!
//! The `composition` of an app, where rule 2 lets it read a unit that is not shared, reads that unit's `composition`
//! and `protocol` (both levels) and no other place; of a shared unit, its `domain` too, which holds the value types
//! the ports take. Each unit, a shared one included, builds its own insides in its own `composition` (a shared client
//! offers a factory there), and the app wires the units to each other. The `composition` of no other unit reads a
//! unit that is not shared at all.
//!
//! Only direct imports are judged between units too, and what these rules allow is closed under chaining but in
//! three places. The chain of an entry file is the point of one: it imports the composition of its unit, which
//! imports what the entry file may not, in its own unit and in others. A `composition` builds its own unit and
//! imports the `protocol` of another (an app's, the `composition` too, and a shared unit's `domain`), which import what
//! a `composition` may not read in another unit, so it reaches them through what it builds and through what it is
//! given (the ports of a `protocol` carry that unit's own `domain`). And whatever reads a `protocol.inbound` reaches,
//! through the data its ports carry, the shared `domain` that `protocol.inbound` may read, whether or not it may import
//! that `domain` itself; this holds in a type alone,
//! since nothing but a `protocol.inbound` imports it, and a `protocol.inbound` that reads the `domain` of another
//! shared unit, which reads its own, closes a cycle (`cycles`) (a test runs every chain of places across units and
//! names the exceptions). `import billing` followed by `billing.protocol.x` is not seen, as a
//! module reached through an attribute never is: rule 1 fails the first, which names no layer. The finding for the
//! first says how to import a layer in the stack of the reader (`Way`), with a layer the unit holds as the example.
//!
//! A forbidden import between units is approved as any other, by its file and the import (`approvals.rs`). A cycle of
//! shared units is not an import but a shape (`cycles`), and cannot be approved.
//!
//! The rules here judge an import once it is resolved to a place; `application` reads the code and resolves it.

use std::collections::{BTreeMap, VecDeque};

use crate::layers::{Place, importable};
use crate::paths::{parent, within};
use crate::units::Unit;

/// The layer that wires, which only the app of a unit may use to reach another unit's `composition`.
const COMPOSITION: &str = "composition";
/// The layer of the use cases, which no other unit's use cases import.
const APPLICATION: &str = "application";
/// The layer of the contracts, both levels of which an app's `composition` reads in a unit that is not shared.
const PROTOCOL: &str = "protocol";
/// The level of the input ports, which reads no `domain` but a shared unit's.
const PROTOCOL_INBOUND: &str = "protocol.inbound";
/// The layer of the concepts, which `protocol.inbound` and every `composition` read in a shared unit.
const DOMAIN: &str = "domain";
/// The places the example of a layer to import in a finding is taken from, the first that the unit holds: the input
/// ports first, since that is what another unit is read for, then the rest in the order of the table.
const EXAMPLES: [&str; 9] = [
    PROTOCOL_INBOUND,
    "protocol.outbound",
    DOMAIN,
    APPLICATION,
    COMPOSITION,
    "infrastructure.inbound",
    "infrastructure.outbound",
    "presentation.models",
    "ui.pages",
];

/// Where an import lands: in a place of a unit, or in a unit's code outside its layers.
#[derive(Debug, Clone, Copy)]
pub enum Landing<'a> {
    /// A layer, a level of one, or an entry file
    Place(&'a Place),
    /// In the directory of the unit, in none of its places
    InUnit(&'a Unit),
}

impl<'a> From<&'a Place> for Landing<'a> {
    fn from(place: &'a Place) -> Self {
        Landing::Place(place)
    }
}

impl Landing<'_> {
    /// The name of the unit it is in, "" for the implicit unit.
    pub fn unit_name(&self) -> &str {
        match self {
            Landing::Place(place) => &place.unit.name,
            Landing::InUnit(unit) => &unit.name,
        }
    }

    /// Whether the unit it is in is shared.
    fn shared(&self) -> bool {
        match self {
            Landing::Place(place) => place.unit.shared,
            Landing::InUnit(unit) => unit.shared,
        }
    }
}

/// How a stack writes the import of a layer of another unit, which the finding for rule 1 gives as an example: what
/// the verb and the example are depends on what the language imports (a module, a package's subpath, a path, a crate).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Way {
    /// Python: a module by its dotted name (`billing.protocol.inbound`). `bare` says that the import names a place by
    /// its own name alone (`domain.song`), which Python looks up from where the program starts, not from the unit
    Module { bare: bool },
    /// TypeScript, a unit that has a package: the specifier through it up to where its layers begin (`@acme/billing`),
    /// which the path of a layer follows (`@acme/billing/protocol/inbound`)
    Package(String),
    /// TypeScript, a unit that has none: the path from the importing file to where its layers begin (`../billing/src`),
    /// which the path of a layer follows (`../billing/src/protocol/inbound`)
    Path(String),
    /// Rust: a dependency on the crate of the layer (`billing_protocol`), since a Cargo dependency is no import of a
    /// module
    Crate,
}

impl Default for Way {
    /// Python's, with no bare name: what is said when an import lands in a place, where rule 1 is not in question
    fn default() -> Self {
        Way::Module { bare: false }
    }
}

impl Way {
    /// What rule 1 says to do instead, for an import that lands in the code of `unit` outside its layers. The example
    /// is a place the unit holds ([`example_of`]), not one it lacks.
    fn advice(&self, unit: &Unit) -> String {
        let name = &unit.name;
        let example = example_of(unit);
        let path = example.replace('.', "/");
        match self {
            Way::Module { .. } => {
                format!("import the module of a layer, such as {name}.{example}")
            }
            Way::Package(base) => {
                format!("import a layer through the package of the unit, such as {base}/{path}")
            }
            Way::Path(base) => format!("import a layer by its path, such as {base}/{path}"),
            Way::Crate => {
                let layer = example.split('.').next().unwrap_or(example);
                format!("depend on the crate of a layer, such as {name}_{layer}")
            }
        }
    }
}

/// The place of `unit` that a finding gives as the example of a layer to import: the first of [`EXAMPLES`] that its
/// `absent` does not take out, as the place or as the layer around it (a unit that lacks `protocol` has no
/// `protocol.inbound`). A unit that lacks them all is given the first.
pub fn example_of(unit: &Unit) -> &'static str {
    let lacks = |place: &str| {
        unit.absent.iter().any(|absent| {
            place == absent
                || place
                    .split_once('.')
                    .is_some_and(|(layer, _)| layer == absent)
        })
    };
    EXAMPLES
        .into_iter()
        .find(|place| !lacks(place))
        .unwrap_or(EXAMPLES[0])
}

/// What the rules say of an import from a place to where it lands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    Allowed,
    /// Where it lands as the finding says it (`in domain`, `in domain of unit core`), and why it fails
    Forbidden {
        described: String,
        reason: String,
    },
}

/// An import from a unit to another unit, both shared: what `cycles` reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edge {
    /// The file that imports, the line, and what it imports as `Forbidden::import` names it
    pub file: String,
    pub line: usize,
    pub import: String,
    /// The names of the importing unit and of the unit imported
    pub from: String,
    pub to: String,
}

/// An import the table forbids: the file that imports, what it imports as the check names it (a Python dotted name,
/// `m.n` for `from m import n`; a TypeScript specifier; a crate's dependency), and the finding. The file and the
/// import are what an approval is keyed by (`approvals.rs`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Forbidden {
    pub file: String,
    pub import: String,
    pub detail: String,
}

/// The import of `import` by `file`, on `line`, that lands in `to`, when the table forbids it from `from`. `what`
/// says what was imported, as "imports x" or "depends on x".
pub fn judged(
    file: &str,
    line: usize,
    import: &str,
    what: &str,
    from: &Place,
    to: &Place,
) -> Option<Forbidden> {
    judged_landing(file, line, import, what, from, to, &Way::default())
}

/// [`judged`] for an import that lands in a place or in a unit's code outside its layers. `way` says how the stack
/// writes an import of a layer, for the finding of the second; for a Python import that names a place by its own name
/// alone (`domain.song`), which Python looks up from where the program starts, not from the unit, a finding for another
/// unit says where the unit's own is.
pub fn judged_landing<'a>(
    file: &str,
    line: usize,
    import: &str,
    what: &str,
    from: &Place,
    to: impl Into<Landing<'a>>,
    way: &Way,
) -> Option<Forbidden> {
    let to = to.into();
    let Verdict::Forbidden { described, reason } = verdict(from, to, way) else {
        return None;
    };
    let hint = match to {
        Landing::Place(to)
            if matches!(way, Way::Module { bare: true }) && from.unit.name != to.unit.name =>
        {
            format!(
                " (Python finds {} from where the program starts; the unit's own layer is {}.{})",
                to.name, from.unit.name, to.name
            )
        }
        _ => String::new(),
    };
    Some(Forbidden {
        file: file.to_string(),
        import: import.to_string(),
        detail: format!("{file}:{line}: {what}, {described}; {reason}{hint}"),
    })
}

/// The import of `import` by `file`, on `line`, as an edge between units, when both its sides are shared units and
/// they are two.
pub fn edge<'a>(
    file: &str,
    line: usize,
    import: &str,
    from: &Place,
    to: impl Into<Landing<'a>>,
) -> Option<Edge> {
    let to = to.into();
    (from.unit.shared && to.shared() && from.unit.name != to.unit_name()).then(|| Edge {
        file: file.to_string(),
        line,
        import: import.to_string(),
        from: from.unit.name.clone(),
        to: to.unit_name().to_string(),
    })
}

/// What the rules say of an import from `from` that lands in `to`, with the words of `way` for rule 1.
pub fn verdict(from: &Place, to: Landing, way: &Way) -> Verdict {
    if from.unit.name == to.unit_name() {
        // The same unit: the table decides, as in a project with one unit. Its code outside its layers is not judged
        return match to {
            Landing::Place(to) if !allowed(from, to) => Verdict::Forbidden {
                described: described(from, to),
                reason: what_it_may_import(from),
            },
            _ => Verdict::Allowed,
        };
    }
    let (f, t) = (&from.unit.name, to.unit_name());
    let to = match to {
        Landing::Place(to) => to,
        Landing::InUnit(unit) => {
            return Verdict::Forbidden {
                described: format!("in the unit {t} outside its layers"),
                reason: way.advice(unit),
            };
        }
    };
    let refuse = |reason: String| Verdict::Forbidden {
        described: format!("in {} of unit {t}", to.name),
        reason,
    };
    let wires = from.name == COMPOSITION && from.unit.app;
    // Read as if it sat in the importer's own unit: the table decides
    let by_table = || {
        if allows(from, to, true) {
            Verdict::Allowed
        } else {
            refuse(what_it_may_import(from))
        }
    };
    if !to.unit.shared {
        if from.unit.shared {
            return refuse(format!(
                "the shared unit {f} imports only shared units, and {t} is not shared"
            ));
        }
        if to.unit.app {
            return refuse(format!(
                "{f} imports no unit that is not shared, and {t} is not: it has an entry, and no unit imports an app"
            ));
        }
        if !wires {
            return refuse(format!(
                "{f} imports no unit that is not shared, and {t} is not: an app's composition wires the two"
            ));
        }
        // The composition of an app, in a unit that is not shared: that unit's composition and protocol, which the
        // table has to let it import too, and no other place, since the unit builds its own insides in its own
        // composition
        if to.name != COMPOSITION && to.layer_name() != PROTOCOL {
            return refuse(format!(
                "the composition of an app imports only the composition and protocol of a unit that is not shared: \
                 {t} builds its own insides in its own composition"
            ));
        }
        return by_table();
    }
    if from.name == PROTOCOL_INBOUND && to.name == DOMAIN {
        // The value types of a shared unit, which an input port may carry: the one place read beyond the table, which
        // lets `protocol.inbound` import nothing
        return Verdict::Allowed;
    }
    if from.layer_name() == APPLICATION && to.layer_name() == APPLICATION {
        return refuse(
            "application never imports another unit's application: composition passes a shared unit's use cases in"
                .to_string(),
        );
    }
    if to.name == COMPOSITION && !wires {
        return refuse(
            "only the composition of an app (a unit with entry) imports another unit's composition"
                .to_string(),
        );
    }
    // A composition, in a shared unit too: that unit's protocol, its domain (the value types the ports take), and its
    // composition for an app's (refused to any other above), which the table has to let it import too, and no other
    // place, since each unit, a shared one included, builds its own insides in its own composition
    if from.name == COMPOSITION
        && to.name != COMPOSITION
        && to.name != DOMAIN
        && to.layer_name() != PROTOCOL
    {
        let (whose, only) = if wires {
            ("an app", "composition, protocol and domain")
        } else {
            ("a unit", "protocol and domain")
        };
        return refuse(format!(
            "the composition of {whose} imports only the {only} of a shared unit: {t} builds its own insides in its \
             own composition"
        ));
    }
    by_table()
}

/// The place or the unit a path from the root lands in: the longest place it is in, or else the unit whose directory
/// it is in, when it is in none of its places. No path is in a unit whose directory is the root (`within` takes ""
/// for no directory), so a unit there is landed in by its places alone.
pub fn landing_of<'a>(path: &str, places: &[&'a Place], units: &'a [Unit]) -> Option<Landing<'a>> {
    let place = place_of(path, places);
    let unit = units
        .iter()
        .filter(|unit| within(path, &unit.path))
        .max_by_key(|unit| unit.path.len());
    match (place, unit) {
        (Some(place), Some(unit)) if place.path.len() < unit.path.len() => {
            Some(Landing::InUnit(unit))
        }
        (Some(place), _) => Some(Landing::Place(place)),
        (None, Some(unit)) => Some(Landing::InUnit(unit)),
        (None, None) => None,
    }
}

/// A finding for each cycle of shared units: the units import each other, so no order of building them exists, and
/// none of them can be taken apart. A group of units that all reach each other is one cycle, shown by the shortest
/// way round from the first of them by name. The finding names, for each step, the first import of it by file and line.
pub fn cycles(edges: &[Edge]) -> Vec<String> {
    let mut next: BTreeMap<&str, BTreeMap<&str, &Edge>> = BTreeMap::new();
    for edge in edges {
        let known = next
            .entry(&edge.from)
            .or_default()
            .entry(&edge.to)
            .or_insert(edge);
        if (&edge.file, edge.line, &edge.import) < (&known.file, known.line, &known.import) {
            *known = edge;
        }
    }
    let reaches = |from: &str, to: &str| {
        let mut seen = Vec::new();
        let mut todo = vec![from];
        while let Some(at) = todo.pop() {
            for &step in next.get(at).into_iter().flat_map(BTreeMap::keys) {
                if !seen.contains(&step) {
                    seen.push(step);
                    todo.push(step);
                }
            }
        }
        seen.contains(&to)
    };
    let mut done: Vec<&str> = Vec::new();
    let mut found = Vec::new();
    for &start in next.keys() {
        if done.contains(&start) || !reaches(start, start) {
            continue;
        }
        // The group of units that reach each other with `start`
        done.extend(
            next.keys()
                .filter(|&&u| reaches(start, u) && reaches(u, start)),
        );
        // The shortest way back to `start`, breadth first, units in the order of their names
        let mut before: BTreeMap<&str, &str> = BTreeMap::new();
        let mut queue = VecDeque::from([start]);
        let last = loop {
            let at = queue
                .pop_front()
                .expect("start reaches itself, so a way back exists");
            let steps = next.get(at).into_iter().flat_map(BTreeMap::keys);
            if steps.clone().any(|&step| step == start) {
                break at;
            }
            for &step in steps {
                if step != start && !before.contains_key(step) {
                    before.insert(step, at);
                    queue.push_back(step);
                }
            }
        };
        let mut ring = vec![start];
        let mut at = last;
        let mut back = Vec::new();
        while at != start {
            back.push(at);
            at = before[at];
        }
        ring.extend(back.into_iter().rev());
        ring.push(start);
        let names = ring.join(" \u{2192} ");
        let imports: Vec<String> = ring
            .windows(2)
            .map(|pair| {
                let edge = next[pair[0]][pair[1]];
                format!("{}:{} imports {}", edge.file, edge.line, edge.import)
            })
            .collect();
        found.push(format!(
            "the shared units import each other in a cycle: {names} ({})",
            imports.join("; ")
        ));
    }
    found
}

/// The directories Cargo looks in for the workspace of a manifest in `dir` that does not name one: `dir`, then each
/// directory above it, up to the root ("").
pub fn workspace_dirs(dir: &str) -> Vec<String> {
    let mut dirs = vec![dir.to_string()];
    let mut at = dir;
    while !at.is_empty() {
        at = parent(at);
        dirs.push(at.to_string());
    }
    dirs
}

/// The place a path from the root sits in: the longest place it is in, by whole names. A Python module is placed by
/// its parts joined with `/`, as the path of its package.
pub fn place_of<'a>(path: &str, places: &[&'a Place]) -> Option<&'a Place> {
    places
        .iter()
        .copied()
        .filter(|place| within(path, &place.path))
        .max_by_key(|place| place.path.len())
}

/// Whether an entry of a place's imports takes in the place `to`: a layer with its levels, or one level.
fn covers(import: &str, to: &Place) -> bool {
    import == to.name || import == to.layer_name()
}

/// Whether the place `from` may import the place `to`, both in the same unit or in the one implicit unit.
fn allowed(from: &Place, to: &Place) -> bool {
    allows(from, to, false)
}

/// What the table says of `from` and `to`. With `as_own`, `to` is read as the place of that name in the unit of
/// `from`, so what is asked is whether `from` may import its own unit's `to`.
fn allows(from: &Place, to: &Place, as_own: bool) -> bool {
    if from.name == to.name && (as_own || from.unit.name == to.unit.name) {
        return true;
    }
    let imported = |to: &Place| from.imports.iter().any(|i| covers(i, to));
    match &from.parent {
        // A layer may import itself, the levels inside it included, and what its imports name
        None => to.layer_name() == from.name || imported(to),
        // A level may import the levels below it, and what its imports name; not its layer beside the levels
        Some(parent) => {
            if to.layer_name() != parent {
                return imported(to);
            }
            from.below.contains(&to.name)
        }
    }
}

/// The place an import lands in, for the message. A level that imports its own layer imports what sits beside the
/// levels, which has no place in their order.
fn described(from: &Place, to: &Place) -> String {
    if to.is_layer() && from.is_level_of(to) {
        return format!("in {} outside its levels", to.name);
    }
    format!("in {}", to.name)
}

/// What a place may import, for the message.
fn what_it_may_import(place: &Place) -> String {
    let importable = importable(place);
    if importable.is_empty() {
        format!("{} imports no other layer", place.name)
    } else {
        format!("{} may import {importable}", place.name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layers::{layout, table};

    fn python_places() -> Vec<Place> {
        layout("python", "atomic").unwrap().places(&table())
    }

    #[test]
    fn what_the_table_allows_is_closed_under_chaining() {
        // Only direct imports are judged, so a chain of allowed imports must never reach what its first may not
        let places = python_places();
        let mut chains = 0;
        for a in &places {
            for b in &places {
                for c in &places {
                    if allowed(a, b) && allowed(b, c) {
                        chains += 1;
                        assert!(
                            allowed(a, c),
                            "{} may import {}, which may import {}, but {} may not import {}",
                            a.name,
                            b.name,
                            c.name,
                            a.name,
                            c.name
                        );
                    }
                }
            }
        }
        assert!(chains > places.len(), "{chains} chains");
    }

    #[test]
    fn levels_beside_each_other_never_import_each_other_and_an_import_may_name_one_level() {
        let table: crate::layers::Table = toml::from_str(
            r#"
            [entry]
            imports = ["edge"]
            role = "x"

            [[layer]]
            name = "edge"
            imports = ["port"]
            role = "x"
            [[layer.level]]
            name = "in"
            imports = ["port.in"]
            role = "x"
            [[layer.level]]
            name = "out"
            imports = ["port.out", "core"]
            role = "x"

            [[layer]]
            name = "port"
            imports = ["core"]
            role = "x"
            [[layer.level]]
            name = "in"
            imports = []
            role = "x"
            [[layer.level]]
            name = "out"
            imports = ["core"]
            role = "x"

            [[layer]]
            name = "core"
            imports = []
            role = "x"
            "#,
        )
        .unwrap();
        let places = layout("python", "atomic").unwrap().places(&table);
        let place = |name: &str| places.iter().find(|p| p.name == name).unwrap();
        let may = |from: &str, to: &str| allowed(place(from), place(to));
        assert_eq!(place("port.out").path, "port/out");
        assert!(place("port.out").below.is_empty());
        // One level named takes in that level alone
        assert!(may("edge.in", "port.in"));
        assert!(!may("edge.in", "port.out") && !may("edge.in", "port") && !may("edge.in", "core"));
        // A layer named takes in its levels
        assert!(may("edge", "port.in") && may("edge", "port.out"));
        // Levels beside each other, in either direction, and the layer beside its levels
        assert!(!may("edge.in", "edge.out") && !may("edge.out", "edge.in"));
        assert!(!may("port.in", "port.out") && !may("port.out", "port"));
        assert!(may("edge.out", "port.out") && may("edge.out", "core"));
        assert!(may("port", "port.in"));
        assert_eq!(
            what_it_may_import(place("edge.in")),
            "edge.in may import `port.in`"
        );
    }

    #[test]
    fn a_module_sits_in_the_longest_place_it_starts_with() {
        let places = python_places();
        let places: Vec<&Place> = places.iter().collect();
        let name =
            |dotted: &str| place_of(&dotted.replace('.', "/"), &places).map(|p| p.name.clone());
        assert_eq!(name("ui.atoms.button"), Some("ui.atoms".into()));
        assert_eq!(name("ui.atoms"), Some("ui.atoms".into()));
        assert_eq!(name("ui.helpers"), Some("ui".into()));
        assert_eq!(name("ui"), Some("ui".into()));
        assert_eq!(name("domain.song"), Some("domain".into()));
        // Whole names only, and not the standard library
        assert_eq!(name("domains.song"), None);
        assert_eq!(name("ui_kit"), None);
        assert_eq!(name("os.path"), None);
    }

    #[test]
    fn a_path_sits_in_the_place_of_the_unit_it_is_in_and_a_place_is_itself_in_its_own_unit() {
        let layout = layout("python", "atomic").unwrap();
        let tag = |name: &str| crate::layers::UnitTag {
            name: name.into(),
            ..Default::default()
        };
        let a = layout.places_in(&table(), "a/a", &tag("a"));
        let b = layout.places_in(&table(), "b/b", &tag("b"));
        let places: Vec<&Place> = a.iter().chain(&b).collect();
        let at =
            |path: &str| place_of(path, &places).map(|p| (p.unit.name.clone(), p.name.clone()));
        assert_eq!(at("b/b/domain/song"), Some(("b".into(), "domain".into())));
        assert_eq!(at("a/a/ui/atoms/x"), Some(("a".into(), "ui.atoms".into())));
        assert_eq!(at("domain/song"), None);
        let domain = |places: &[Place]| places.iter().find(|p| p.name == "domain").unwrap().clone();
        assert!(allowed(&domain(&a), &domain(&a)));
        assert!(domain(&a).is_same(&domain(&a)) && !domain(&a).is_same(&domain(&b)));
    }

    #[test]
    fn the_message_says_what_a_place_may_import() {
        let places = python_places();
        let say = |name: &str| what_it_may_import(places.iter().find(|p| p.name == name).unwrap());
        assert_eq!(say("domain"), "domain imports no other layer");
        assert_eq!(
            say("protocol.outbound"),
            "protocol.outbound may import `domain`"
        );
        assert_eq!(
            say("infrastructure.inbound"),
            "infrastructure.inbound may import `protocol.inbound`"
        );
        assert_eq!(say("ui.atoms"), "ui.atoms imports no other layer");
        assert_eq!(
            say("ui.organisms"),
            "ui.organisms may import the levels below it, and `presentation.display`"
        );
    }

    #[test]
    fn cargo_looks_for_a_workspace_from_the_crate_up_to_the_root() {
        assert_eq!(
            workspace_dirs("crates/domain"),
            ["crates/domain", "crates", ""]
        );
        assert_eq!(workspace_dirs(""), [""]);
    }

    /// Units made in code, in a Python layout: `core` and `steps` are shared, `billing` and `shipping` are neither
    /// shared nor apps, `cli` and `web` are apps.
    struct World {
        units: Vec<Unit>,
        places: Vec<Place>,
    }

    fn unit(name: &str, path: &str, shared: bool, entry: Option<&str>) -> Unit {
        Unit {
            name: name.into(),
            path: path.into(),
            root: path.into(),
            shared,
            entries: entry.map(str::to_string).into_iter().collect(),
            absent: Vec::new(),
            declared: true,
        }
    }

    impl World {
        fn new() -> World {
            let layout = layout("python", "atomic").unwrap();
            let table = table();
            let units = vec![
                unit("core", "core", true, None),
                unit("steps", "steps", true, None),
                unit("billing", "billing", false, None),
                unit("shipping", "shipping", false, None),
                unit("cli", "cli", false, Some("cli/__main__.py")),
                unit("web", "web", false, Some("web/__main__.py")),
            ];
            let mut places = Vec::new();
            for unit in &units {
                places.extend(unit.places(&layout, &table));
                places.extend(unit.entry_places(&layout, &table));
            }
            World { units, places }
        }

        /// `billing.domain`, or `cli.entry` for the entry file of `cli`.
        fn place(&self, dotted: &str) -> &Place {
            let (unit, name) = dotted.split_once('.').unwrap();
            let name = if name == "entry" {
                format!("{unit}/__main__.py")
            } else {
                name.to_string()
            };
            self.places
                .iter()
                .find(|p| p.unit.name == unit && p.name == name)
                .unwrap_or_else(|| panic!("no place {dotted}"))
        }

        fn unit(&self, name: &str) -> &Unit {
            self.units.iter().find(|u| u.name == name).unwrap()
        }

        /// What the rules say of an import from `from` to `to` (`core.domain`, or `core.(outside)` for the code of a
        /// unit outside its layers): `Ok`, or the finding.
        fn says(&self, from: &str, to: &str) -> Result<(), String> {
            let from = self.place(from);
            let landing = match to.strip_suffix(".(outside)") {
                Some(unit) => Landing::InUnit(self.unit(unit)),
                None => Landing::Place(self.place(to)),
            };
            match judged_landing("f.py", 3, "x", "imports x", from, landing, &Way::default()) {
                Some(forbidden) => Err(forbidden.detail),
                None => Ok(()),
            }
        }
    }

    const APPS_ONLY: &str =
        "only the composition of an app (a unit with entry) imports another unit's composition";
    const NO_APPLICATION: &str = "application never imports another unit's application: composition passes a \
                                  shared unit's use cases in";

    #[test]
    fn an_import_between_units_is_judged_by_the_kind_of_each_unit_and_its_places() {
        let world = World::new();
        let shared = |from: &str, to: &str| {
            format!("the shared unit {from} imports only shared units, and {to} is not shared")
        };
        let wires = |from: &str, to: &str| {
            format!(
                "{from} imports no unit that is not shared, and {to} is not: an app's composition wires the two"
            )
        };
        let app = |from: &str, to: &str| {
            format!(
                "{from} imports no unit that is not shared, and {to} is not: it has an entry, and no unit imports an app"
            )
        };
        let outside = |unit: &str| {
            format!(
                "in the unit {unit} outside its layers; import the module of a layer, such as {unit}.protocol.inbound"
            )
        };
        let insides = |to: &str| {
            format!(
                "the composition of an app imports only the composition and protocol of a unit that is not shared: \
                 {to} builds its own insides in its own composition"
            )
        };
        let shared_insides = |whose: &str, to: &str| {
            let only = if whose == "an app" {
                "composition, protocol and domain"
            } else {
                "protocol and domain"
            };
            format!(
                "the composition of {whose} imports only the {only} of a shared unit: {to} builds its own insides in \
                 its own composition"
            )
        };
        // (from, to, the finding after `imports x, `, or None when it is allowed)
        let rows: Vec<(&str, &str, Option<String>)> = vec![
            // The same unit: the table, as in a project of one unit, and its code outside the layers is not judged
            ("billing.application", "billing.domain", None),
            ("billing.domain", "billing.domain", None),
            ("billing.application", "billing.(outside)", None),
            ("cli.entry", "cli.composition", None),
            (
                "billing.domain",
                "billing.application",
                Some("in application; domain imports no other layer".into()),
            ),
            (
                "cli.entry",
                "cli.domain",
                Some("in domain; cli/__main__.py may import `composition`".into()),
            ),
            // Rule 1: the code of another unit outside its layers
            ("billing.application", "shipping.(outside)", Some(outside("shipping"))),
            ("billing.application", "core.(outside)", Some(outside("core"))),
            ("cli.composition", "billing.(outside)", Some(outside("billing"))),
            // Rule 3: a shared unit imports only shared units
            (
                "core.application",
                "billing.domain",
                Some(format!("in domain of unit billing; {}", shared("core", "billing"))),
            ),
            (
                "core.composition",
                "billing.composition",
                Some(format!("in composition of unit billing; {}", shared("core", "billing"))),
            ),
            (
                "steps.domain",
                "cli.domain",
                Some(format!("in domain of unit cli; {}", shared("steps", "cli"))),
            ),
            // Rule 2: a unit that is not shared is imported by the composition of an app, and no other app, and
            // through that unit's composition and protocol alone
            ("cli.composition", "billing.composition", None),
            ("cli.composition", "billing.protocol", None),
            ("cli.composition", "billing.protocol.inbound", None),
            ("cli.composition", "billing.protocol.outbound", None),
            (
                "cli.composition",
                "billing.domain",
                Some(format!("in domain of unit billing; {}", insides("billing"))),
            ),
            (
                "cli.composition",
                "billing.application",
                Some(format!("in application of unit billing; {}", insides("billing"))),
            ),
            (
                "cli.composition",
                "billing.infrastructure.outbound",
                Some(format!(
                    "in infrastructure.outbound of unit billing; {}",
                    insides("billing")
                )),
            ),
            (
                "cli.composition",
                "shipping.ui.atoms",
                Some(format!("in ui.atoms of unit shipping; {}", insides("shipping"))),
            ),
            (
                "billing.composition",
                "shipping.composition",
                Some(format!("in composition of unit shipping; {}", wires("billing", "shipping"))),
            ),
            (
                "billing.application",
                "shipping.protocol.inbound",
                Some(format!("in protocol.inbound of unit shipping; {}", wires("billing", "shipping"))),
            ),
            (
                "cli.application",
                "billing.domain",
                Some(format!("in domain of unit billing; {}", wires("cli", "billing"))),
            ),
            (
                "cli.infrastructure.inbound",
                "billing.protocol.inbound",
                Some(format!("in protocol.inbound of unit billing; {}", wires("cli", "billing"))),
            ),
            (
                "cli.entry",
                "billing.composition",
                Some(format!("in composition of unit billing; {}", wires("cli", "billing"))),
            ),
            (
                "cli.composition",
                "web.composition",
                Some(format!("in composition of unit web; {}", app("cli", "web"))),
            ),
            (
                "cli.composition",
                "web.domain",
                Some(format!("in domain of unit web; {}", app("cli", "web"))),
            ),
            (
                "billing.composition",
                "cli.composition",
                Some(format!("in composition of unit cli; {}", app("billing", "cli"))),
            ),
            // Rule 4: a shared unit, read as the importer's own
            ("billing.application", "core.protocol.inbound", None),
            ("billing.application", "core.protocol.outbound", None),
            ("billing.application", "core.domain", None),
            ("billing.domain", "core.domain", None),
            ("billing.infrastructure.outbound", "core.protocol.outbound", None),
            ("billing.infrastructure.outbound", "core.domain", None),
            ("billing.infrastructure.inbound", "core.protocol.inbound", None),
            ("billing.ui.pages", "core.protocol.inbound", None),
            ("billing.ui.atoms", "core.ui.atoms", None),
            ("billing.ui.organisms", "core.ui.molecules", None),
            ("steps.domain", "core.domain", None),
            ("steps.application", "core.protocol.inbound", None),
            ("steps.application", "core.domain", None),
            (
                "billing.domain",
                "core.protocol.inbound",
                Some("in protocol.inbound of unit core; domain imports no other layer".into()),
            ),
            (
                "billing.infrastructure.inbound",
                "core.protocol.outbound",
                Some(
                    "in protocol.outbound of unit core; infrastructure.inbound may import `protocol.inbound`"
                        .into(),
                ),
            ),
            (
                "billing.infrastructure.inbound",
                "core.domain",
                Some("in domain of unit core; infrastructure.inbound may import `protocol.inbound`".into()),
            ),
            (
                "billing.infrastructure.inbound",
                "core.infrastructure.outbound",
                Some(
                    "in infrastructure.outbound of unit core; infrastructure.inbound may import \
                     `protocol.inbound`"
                        .into(),
                ),
            ),
            (
                "billing.ui.atoms",
                "core.ui.molecules",
                Some("in ui.molecules of unit core; ui.atoms imports no other layer".into()),
            ),
            // Rule 4a: no unit's application imports another's
            (
                "billing.application",
                "core.application",
                Some(format!("in application of unit core; {NO_APPLICATION}")),
            ),
            (
                "steps.application",
                "core.application",
                Some(format!("in application of unit core; {NO_APPLICATION}")),
            ),
            (
                "cli.application",
                "core.application",
                Some(format!("in application of unit core; {NO_APPLICATION}")),
            ),
            // Rule 4b: the composition of a shared unit is imported by the composition of an app
            ("cli.composition", "core.composition", None),
            ("web.composition", "steps.composition", None),
            // ... which reads of a shared unit its protocol and its domain (the value types the ports take), whoever
            // the composition is, and no other place: a unit builds its own insides, a shared one included
            ("cli.composition", "core.protocol.inbound", None),
            ("cli.composition", "core.protocol.outbound", None),
            ("cli.composition", "core.domain", None),
            ("steps.composition", "core.protocol.inbound", None),
            ("steps.composition", "core.protocol.outbound", None),
            ("steps.composition", "core.domain", None),
            ("core.composition", "steps.domain", None),
            ("billing.composition", "core.domain", None),
            (
                "cli.composition",
                "core.application",
                Some(format!("in application of unit core; {}", shared_insides("an app", "core"))),
            ),
            (
                "cli.composition",
                "core.infrastructure.outbound",
                Some(format!(
                    "in infrastructure.outbound of unit core; {}",
                    shared_insides("an app", "core")
                )),
            ),
            (
                "cli.composition",
                "core.infrastructure.inbound",
                Some(format!(
                    "in infrastructure.inbound of unit core; {}",
                    shared_insides("an app", "core")
                )),
            ),
            (
                "cli.composition",
                "core.ui.pages",
                Some(format!("in ui.pages of unit core; {}", shared_insides("an app", "core"))),
            ),
            (
                "cli.composition",
                "core.presentation.models",
                Some(format!(
                    "in presentation.models of unit core; {}",
                    shared_insides("an app", "core")
                )),
            ),
            (
                "steps.composition",
                "core.application",
                Some(format!("in application of unit core; {}", shared_insides("a unit", "core"))),
            ),
            (
                "steps.composition",
                "core.infrastructure.outbound",
                Some(format!(
                    "in infrastructure.outbound of unit core; {}",
                    shared_insides("a unit", "core")
                )),
            ),
            (
                "billing.composition",
                "core.application",
                Some(format!("in application of unit core; {}", shared_insides("a unit", "core"))),
            ),
            (
                "billing.composition",
                "core.infrastructure.outbound",
                Some(format!(
                    "in infrastructure.outbound of unit core; {}",
                    shared_insides("a unit", "core")
                )),
            ),
            // ... and of a unit that is not shared, an app's reads its composition and protocol alone, and no domain
            (
                "cli.composition",
                "shipping.domain",
                Some(format!("in domain of unit shipping; {}", insides("shipping"))),
            ),
            // Rule 4: the input ports of any unit read the domain of a shared unit, and nothing else of any other
            ("billing.protocol.inbound", "core.domain", None),
            ("steps.protocol.inbound", "core.domain", None),
            ("cli.protocol.inbound", "core.domain", None),
            ("billing.protocol", "core.domain", None),
            (
                "core.protocol.inbound",
                "core.domain",
                Some("in domain; protocol.inbound imports no other layer".into()),
            ),
            (
                "billing.protocol.inbound",
                "billing.domain",
                Some("in domain; protocol.inbound imports no other layer".into()),
            ),
            (
                "billing.protocol.inbound",
                "shipping.domain",
                Some(format!("in domain of unit shipping; {}", wires("billing", "shipping"))),
            ),
            (
                "core.protocol.inbound",
                "billing.domain",
                Some(format!("in domain of unit billing; {}", shared("core", "billing"))),
            ),
            (
                "billing.protocol.inbound",
                "core.application",
                Some("in application of unit core; protocol.inbound imports no other layer".into()),
            ),
            (
                "billing.protocol.inbound",
                "core.protocol.outbound",
                Some("in protocol.outbound of unit core; protocol.inbound imports no other layer".into()),
            ),
            (
                "billing.composition",
                "core.composition",
                Some(format!("in composition of unit core; {APPS_ONLY}")),
            ),
            (
                "core.composition",
                "steps.composition",
                Some(format!("in composition of unit steps; {APPS_ONLY}")),
            ),
            (
                "cli.application",
                "core.composition",
                Some(format!("in composition of unit core; {APPS_ONLY}")),
            ),
            // Rule 5: the entry file reads the composition of its own unit and nothing of another
            (
                "cli.entry",
                "core.composition",
                Some(format!("in composition of unit core; {APPS_ONLY}")),
            ),
            (
                "cli.entry",
                "core.domain",
                Some("in domain of unit core; cli/__main__.py may import `composition`".into()),
            ),
            (
                "cli.entry",
                "core.protocol.inbound",
                Some("in protocol.inbound of unit core; cli/__main__.py may import `composition`".into()),
            ),
            (
                "cli.entry",
                "web.composition",
                Some(format!("in composition of unit web; {}", app("cli", "web"))),
            ),
        ];
        for (from, to, ends) in rows {
            let said = world.says(from, to);
            match ends {
                None => assert_eq!(said, Ok(()), "{from} -> {to}"),
                Some(ends) => assert_eq!(
                    said,
                    Err(format!("f.py:3: imports x, {ends}")),
                    "{from} -> {to}"
                ),
            }
        }
    }

    #[test]
    fn the_same_unit_is_judged_as_before_and_only_a_composition_or_a_shared_unit_is_crossed() {
        let world = World::new();
        let (mut crossing, mut same, mut ports_read) = (0, 0, 0);
        for from in &world.places {
            for to in &world.places {
                let landing = Landing::Place(to);
                if from.unit.name == to.unit.name {
                    same += 1;
                    // As it was before there were units: the table, with the same words
                    let was = judged("f.py", 1, "x", "imports x", from, to);
                    assert_eq!(was.is_none(), allowed(from, to));
                    assert_eq!(
                        verdict(from, landing, &Way::default()) == Verdict::Allowed,
                        allowed(from, to)
                    );
                    if let Some(was) = was {
                        assert_eq!(
                            was.detail,
                            format!(
                                "f.py:1: imports x, {}; {}",
                                described(from, to),
                                what_it_may_import(from)
                            )
                        );
                    }
                    continue;
                }
                if verdict(from, landing, &Way::default()) != Verdict::Allowed {
                    continue;
                }
                crossing += 1;
                let name = format!(
                    "{}.{} -> {}.{}",
                    from.unit.name, from.name, to.unit.name, to.name
                );
                let wires = from.name == "composition" && from.unit.app;
                // A unit that is not shared is imported by the composition of an app alone, and no app is imported
                assert!(to.unit.shared || (wires && !to.unit.app), "{name}");
                // A composition reads another unit through its protocol, and an app's through its composition too, and
                // the domain of a shared unit
                assert!(
                    from.name != "composition"
                        || to.layer_name() == "protocol"
                        || (wires && to.name == "composition")
                        || (to.unit.shared && to.name == "domain"),
                    "{name}"
                );
                // A shared unit imports only shared ones
                assert!(!from.unit.shared || to.unit.shared, "{name}");
                // What is imported is what the table lets the importer import in its own unit, but for the domain of a
                // shared unit, which an input port reads and the table does not let it
                let ports =
                    from.name == "protocol.inbound" && to.name == "domain" && to.unit.shared;
                ports_read += usize::from(ports);
                assert!(allows(from, to, true) != ports, "{name}");
                // The application of a unit never reads the application of another
                assert!(
                    from.layer_name() != "application" || to.layer_name() != "application",
                    "{name}"
                );
                // The composition of a unit is read by the composition of an app alone
                assert!(to.name != "composition" || wires, "{name}");
            }
            // The code of another unit outside its layers is judged allowed in the unit's own alone
            for unit in &world.units {
                assert_eq!(
                    verdict(from, Landing::InUnit(unit), &Way::default()) == Verdict::Allowed,
                    unit.name == from.unit.name,
                    "{} -> {}",
                    from.name,
                    unit.name
                );
            }
        }
        assert!(same > 0 && crossing > 0, "{same} {crossing}");
        // Every input port of a unit reads the domain of each shared unit but its own
        assert_eq!(ports_read, 6 * 2 - 2, "{ports_read}");
    }

    #[test]
    fn what_the_rules_allow_between_units_is_closed_under_chaining_but_in_four_places() {
        // Only direct imports are judged, so a chain of allowed imports must never reach what its first may not. Across
        // units this holds because a place asks no more of a place of another unit than the table asks of the same
        // place of its own, but in the places below, each named by what it is for (a test counts each, so that none is
        // there for nothing):
        // - the entry file of an app, here as in a project of one unit (the table is not closed over it either): it
        //   imports its composition and, through it, what that composition wires, which is what an entry file is for;
        // - a composition, which builds its own unit and so reaches, through the places it imports there, what they
        //   read in other units and it may not: only the protocol of another unit is its to read (and its `domain`, in
        //   a shared one), and an app's its composition too, each unit builds its own insides; and through those, what
        //   the composition builds there and what the ports of the protocol carry of that unit's own `domain`, as in
        //   any unit;
        // - what reads a `protocol.inbound`, which reaches the shared `domain` it reads by the type of its data, in no
        //   import: the one thing that imports that `domain` is the `protocol.inbound`;
        // - and, in that, the `protocol.inbound` of a shared unit that reads the `domain` of another, which reads its
        //   own: the shared units import each other in a cycle, which `cycles` fails and no approval covers
        let world = World::new();
        let may = |from: &Place, to: &Place| {
            verdict(from, Landing::Place(to), &Way::default()) == Verdict::Allowed
        };
        let entry = |p: &Place| p.name.ends_with("__main__.py");
        let mut chains = 0;
        let mut through = [0; 5];
        for a in &world.places {
            for b in &world.places {
                for c in &world.places {
                    if !(may(a, b) && may(b, c)) {
                        continue;
                    }
                    chains += 1;
                    if may(a, c) {
                        continue;
                    }
                    let other = |p: &Place, q: &Place| p.unit.name != q.unit.name;
                    let known = [
                        entry(a) && b.name == "composition" && !other(a, b),
                        a.name == "composition"
                            && other(a, b)
                            && (b.name == "composition" || b.layer_name() == "protocol"),
                        a.name == "composition" && !other(a, b) && other(a, c),
                        b.name == "protocol.inbound"
                            && c.name == "domain"
                            && c.unit.shared
                            && other(b, c),
                        a.name == "protocol.inbound"
                            && b.name == "domain"
                            && c.name == "domain"
                            && !other(a, c),
                    ];
                    let at = known.iter().position(|&known| known);
                    let name = |p: &Place| format!("{}.{}", p.unit.name, p.name);
                    assert!(
                        at.is_some(),
                        "{} may import {}, which may import {}, but {} may not import {}",
                        name(a),
                        name(b),
                        name(c),
                        name(a),
                        name(c)
                    );
                    through[at.unwrap()] += 1;
                }
            }
        }
        assert!(chains > world.places.len(), "{chains} chains");
        assert!(through.iter().all(|&n| n > 0), "{through:?}");
    }

    #[test]
    fn a_bare_name_that_lands_in_another_unit_says_where_python_finds_it_and_where_the_unit_has_its_own()
     {
        let layout = layout("python", "atomic").unwrap();
        let world = World::new();
        // A unit at the root, whose `domain` is the one Python finds from where the program starts
        let main = unit("main", "", false, None);
        let domain = main
            .places(&layout, &table())
            .into_iter()
            .find(|p| p.name == "domain")
            .unwrap();
        assert_eq!(domain.path, "domain");
        let from = world.place("billing.application");
        let say = |import: &str, to: &Place, bare: bool| {
            judged_landing(
                "b.py",
                4,
                import,
                &format!("imports {import}"),
                from,
                Landing::Place(to),
                &Way::Module { bare },
            )
        };
        let found = say("domain.song", &domain, true).unwrap();
        assert_eq!(
            found.detail,
            "b.py:4: imports domain.song, in domain of unit main; billing imports no unit that is not shared, and \
             main is not: an app's composition wires the two (Python finds domain from where the program starts; \
             the unit's own layer is billing.domain)"
        );
        assert_eq!(found.import, "domain.song");
        // Not said when the import is not a bare name, when it is allowed, or when the unit is the same
        assert!(
            !say("domain.song", &domain, false)
                .unwrap()
                .detail
                .contains("Python finds")
        );
        assert!(say("domain.song", world.place("core.domain"), true).is_none());
        let inside = judged_landing(
            "b.py",
            1,
            "domain.song",
            "imports domain.song",
            world.place("billing.domain"),
            Landing::Place(world.place("billing.application")),
            &Way::Module { bare: true },
        )
        .unwrap();
        assert!(!inside.detail.contains("Python finds"));
    }

    #[test]
    fn the_finding_for_the_code_outside_the_layers_says_how_the_stack_imports_a_layer() {
        let world = World::new();
        let from = world.place("billing.application");
        let words = |way: Way| {
            judged_landing(
                "f",
                2,
                "core",
                "imports core",
                from,
                Landing::InUnit(world.unit("core")),
                &way,
            )
            .unwrap()
            .detail
        };
        let said = |reason: &str| {
            format!("f:2: imports core, in the unit core outside its layers; {reason}")
        };
        assert_eq!(
            words(Way::Module { bare: false }),
            said("import the module of a layer, such as core.protocol.inbound")
        );
        assert_eq!(words(Way::Module { bare: true }), words(Way::default()));
        assert_eq!(
            words(Way::Package("@acme/core".into())),
            said(
                "import a layer through the package of the unit, such as @acme/core/protocol/inbound"
            )
        );
        assert_eq!(
            words(Way::Path("../core/src".into())),
            said("import a layer by its path, such as ../core/src/protocol/inbound")
        );
        assert_eq!(
            words(Way::Crate),
            said("depend on the crate of a layer, such as core_protocol")
        );
        // The words are those of rule 1 alone: a place of another unit fails by its own rule, whatever the stack
        let place = judged_landing(
            "f",
            2,
            "x",
            "imports x",
            world.place("core.application"),
            Landing::Place(world.place("billing.domain")),
            &Way::Crate,
        )
        .unwrap();
        assert!(!place.detail.contains("such as"), "{}", place.detail);
    }

    #[test]
    fn the_example_of_a_layer_is_a_place_the_unit_holds_whatever_the_stack_says() {
        // (what the unit lacks, the example it is given: its place, then the layer of it for Rust)
        let rows: [(&[&str], &str, &str); 8] = [
            (&[], "protocol.inbound", "protocol"),
            (&["domain"], "protocol.inbound", "protocol"),
            // A level that is out, and a layer that is out with the levels it holds
            (&["protocol.inbound"], "protocol.outbound", "protocol"),
            (&["protocol"], "domain", "domain"),
            (&["protocol", "domain"], "application", "application"),
            // An app, which holds only its composition and infrastructure
            (
                &["domain", "application", "protocol"],
                "composition",
                "composition",
            ),
            (
                &["domain", "application", "protocol", "composition"],
                "infrastructure.inbound",
                "infrastructure",
            ),
            // A unit that holds none of what is listed has the first, which the structure check has failed already
            (
                &[
                    "protocol",
                    "domain",
                    "application",
                    "composition",
                    "infrastructure",
                    "presentation",
                    "ui",
                ],
                "protocol.inbound",
                "protocol",
            ),
        ];
        for (lacks, example, layer) in rows {
            let mut unit = unit("kernel", "kernel", true, None);
            unit.absent = lacks.iter().map(|name| name.to_string()).collect();
            assert_eq!(example_of(&unit), example, "{lacks:?}");
            let from = World::new();
            let from = from.place("billing.application");
            let said = |way: Way| {
                judged_landing(
                    "f",
                    1,
                    "kernel",
                    "imports kernel",
                    from,
                    Landing::InUnit(&unit),
                    &way,
                )
                .unwrap()
                .detail
            };
            assert!(
                said(Way::Module { bare: false }).ends_with(&format!("such as kernel.{example}")),
                "{lacks:?}"
            );
            assert!(
                said(Way::Package("@acme/kernel/src".into())).ends_with(&format!(
                    "such as @acme/kernel/src/{}",
                    example.replace('.', "/")
                )),
                "{lacks:?}"
            );
            assert!(
                said(Way::Path("../kernel".into()))
                    .ends_with(&format!("such as ../kernel/{}", example.replace('.', "/"))),
                "{lacks:?}"
            );
            assert!(
                said(Way::Crate).ends_with(&format!("such as kernel_{layer}")),
                "{lacks:?}"
            );
        }
    }

    #[test]
    fn a_path_lands_in_a_place_or_in_the_code_of_a_unit_outside_its_places() {
        let layout = layout("python", "atomic").unwrap();
        let table = table();
        let units = vec![
            unit("a", "a", false, None),
            unit("inner", "a/inner", false, None),
            unit("main", "", false, None),
        ];
        let places: Vec<Place> = units
            .iter()
            .flat_map(|u| u.places(&layout, &table))
            .collect();
        let places: Vec<&Place> = places.iter().collect();
        let at = |path: &str| match landing_of(path, &places, &units) {
            Some(Landing::Place(p)) => Some(format!("place {} of {}", p.name, p.unit.name)),
            Some(Landing::InUnit(u)) => Some(format!("unit {}", u.name)),
            None => None,
        };
        assert_eq!(at("a/domain/song"), Some("place domain of a".into()));
        assert_eq!(at("a/ui/atoms/x"), Some("place ui.atoms of a".into()));
        // The unit's package, and a module of it that is in no layer
        assert_eq!(at("a"), Some("unit a".into()));
        assert_eq!(at("a/helpers"), Some("unit a".into()));
        // A unit inside the directory of another is its own, from its directory down
        assert_eq!(at("a/inner"), Some("unit inner".into()));
        assert_eq!(at("a/inner/helpers"), Some("unit inner".into()));
        assert_eq!(at("a/inner/domain/x"), Some("place domain of inner".into()));
        // At the root, the places are the unit's, and nothing else lands in it
        assert_eq!(at("domain/song"), Some("place domain of main".into()));
        assert_eq!(at("os/path"), None);
        assert_eq!(at("domains/song"), None);
    }

    #[test]
    fn shared_units_that_import_each_other_are_one_finding_by_cycle() {
        let edge = |from: &str, to: &str, file: &str, line: usize, import: &str| Edge {
            file: file.into(),
            line,
            import: import.into(),
            from: from.into(),
            to: to.into(),
        };
        assert!(cycles(&[]).is_empty());
        // One way only is no cycle
        assert!(cycles(&[edge("steps", "core", "steps/a.py", 1, "core.domain.a")]).is_empty());
        // Two units, each step shown by its first import by file, line and import
        let two = [
            edge("steps", "core", "steps/domain/b.py", 1, "core.domain.c"),
            edge(
                "core",
                "steps",
                "core/application/y.py",
                9,
                "steps.domain.z",
            ),
            edge(
                "core",
                "steps",
                "core/application/x.py",
                3,
                "steps.domain.a",
            ),
            edge(
                "core",
                "steps",
                "core/application/x.py",
                3,
                "steps.domain.0",
            ),
        ];
        assert_eq!(
            cycles(&two),
            [
                "the shared units import each other in a cycle: core \u{2192} steps \u{2192} core \
                 (core/application/x.py:3 imports steps.domain.0; steps/domain/b.py:1 imports core.domain.c)"
            ]
        );
        // Units that all reach each other are one cycle, shown by the shortest way round from the first by name
        let three = [
            edge("a", "b", "a/1.py", 1, "b.x"),
            edge("b", "c", "b/1.py", 1, "c.x"),
            edge("c", "a", "c/1.py", 1, "a.x"),
            edge("a", "c", "a/2.py", 2, "c.y"),
            edge("c", "b", "c/2.py", 2, "b.y"),
        ];
        assert_eq!(
            cycles(&three),
            [
                "the shared units import each other in a cycle: a \u{2192} c \u{2192} a \
                 (a/2.py:2 imports c.y; c/1.py:1 imports a.x)"
            ]
        );
        // Two cycles that share no unit are two findings, in the order of the names, though one unit imports the other
        let apart = [
            edge("p", "q", "p/1.py", 1, "q.x"),
            edge("q", "p", "q/1.py", 1, "p.x"),
            edge("m", "n", "m/1.py", 1, "n.x"),
            edge("n", "m", "n/1.py", 1, "m.x"),
            edge("m", "p", "m/2.py", 1, "p.x"),
        ];
        let found = cycles(&apart);
        assert_eq!(found.len(), 2, "{found:?}");
        assert!(found[0].contains(": m \u{2192} n \u{2192} m "), "{found:?}");
        assert!(found[1].contains(": p \u{2192} q \u{2192} p "), "{found:?}");
    }

    #[test]
    fn only_an_import_from_a_shared_unit_to_another_shared_unit_is_an_edge() {
        let world = World::new();
        let edge = |from: &str, to: Landing| edge("f.py", 2, "x", world.place(from), to);
        let to = |name: &str| Landing::Place(world.place(name));
        let found = edge("core.application", to("steps.domain")).unwrap();
        assert_eq!(
            (found.from.as_str(), found.to.as_str(), found.line),
            ("core", "steps", 2)
        );
        // The code of a shared unit outside its layers is the unit's too
        assert!(edge("core.application", Landing::InUnit(world.unit("steps"))).is_some());
        assert!(edge("core.application", to("core.domain")).is_none());
        assert!(edge("core.application", to("billing.domain")).is_none());
        assert!(edge("billing.application", to("core.domain")).is_none());
        assert!(edge("cli.composition", to("core.composition")).is_none());
    }
}
