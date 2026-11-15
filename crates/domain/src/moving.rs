//! The code that is moving into the layers: which files to move first.
//!
//! The paths a project lists in `moving` hold code that is not in a layer yet, which no check judges. `rotproof check`
//! counts it and says what to move next, so that the count falls only when a file is moved.
//!
//! - **The order is the import graph's, from its leaves.** A file imports files that are moving too, and cannot go
//!   into a layer before what it imports is where it belongs: the onion is built from the inside. A file that imports
//!   no other file still moving comes first. A file that imports one still moving waits for it, and an import of a
//!   file in a layer or of a package outside the project is no wait.
//! - **Files that import each other are one group.** Neither goes first: they are moved together, and shown together.
//! - **Each file is given the layers it could go in,** those in which the direction check would pass it, from what it
//!   imports of the layers and from what the layers import of it: the layers' rules decide it, never what the code is
//!   about. Where a file may go among the layers that pass is for the person who moves it, as what a part means
//!   (`domain` or `application`) is decided by reading it. Nothing here tells that a file that opens a socket is
//!   `infrastructure`: that needs a table of the modules that do I/O, which is not built, and it would narrow the
//!   layers given here (see `candidates`).
//! - **No date is set,** and nothing fails: the count and the next files stay as long as the files do.
//!
//! The graph is built by `application`, which reads the code and resolves each import to a file or a place; the rules
//! here take it as plain values.

use std::collections::{BTreeMap, BTreeSet};

use crate::direction::{Landing, Verdict, Way, verdict};
use crate::layers::Place;
use crate::paths::within;
use crate::units::Unit;

/// The most groups `rotproof check` names as next to move: the rest are counted
pub const SHOWN: usize = 5;

/// A file that is moving, what it imports, and who imports it.
#[derive(Debug, Clone, Default)]
pub struct File<'a> {
    /// From the root, with `/`
    pub path: String,
    /// The files it imports among those that are moving, never itself
    pub needs: BTreeSet<String>,
    /// Where each of its other imports lands: in a place of a layer, or in a unit's code outside its layers. An import
    /// of a package outside the project, or of the standard library, lands nowhere and is not here
    pub landings: Vec<Landing<'a>>,
    /// The places of the files in the layers (and the entry files) that import it: each one of them is judged when it
    /// is moved, as an import of the place it goes in
    pub readers: Vec<&'a Place>,
}

/// The layers a file could be moved into.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Candidates {
    /// Every layer and level there is: what the file imports and what imports it say nothing against any
    Any,
    /// These alone, innermost first, as the findings name a place (`domain`, `infrastructure.outbound`, and `domain of
    /// unit core` where the project declares units). None at all when empty: what it imports and what imports it
    /// cannot both be satisfied by one place
    Only(Vec<String>),
}

/// A file to move, with where it could go.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Next {
    pub path: String,
    pub candidates: Candidates,
}

/// What `rotproof check` says of the code that is moving.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Order {
    /// How many files are moving
    pub total: usize,
    /// Whether the files were put in an order: false for a language whose files' imports are not read
    pub ordered: bool,
    /// The groups to move next, the first [`SHOWN`] of those that wait for no other file: one file alone, or the files
    /// that import each other, each in the byte order of its path, and the groups in the byte order of their first
    pub next: Vec<Vec<Next>>,
    /// How many files are in the groups that wait for no other file and are not in `next`
    pub more: usize,
}

impl Order {
    /// Files that are counted and not ordered.
    pub fn unordered(total: usize) -> Order {
        Order {
            total,
            ..Order::default()
        }
    }
}

/// The files of `files` put in order: the groups that wait for no file still moving come first, and a file or group
/// that waits for another comes only once that one is gone. Each file that is named has the layers it could go in,
/// among `places` (those the project has, with the layers of every unit) and by the rules between `units`.
pub fn order(files: &[File], places: &[&Place], units: &[Unit]) -> Order {
    let groups = components(files);
    let needs: BTreeMap<&str, &BTreeSet<String>> = files
        .iter()
        .map(|file| (file.path.as_str(), &file.needs))
        .collect();
    // A group waits for no other when everything its files import is in the group, or is no file still moving
    let ready: Vec<&Vec<String>> = groups
        .iter()
        .filter(|group| {
            group.iter().all(|path| {
                needs[path.as_str()].iter().all(|need| {
                    group.binary_search(need).is_ok() || !needs.contains_key(need.as_str())
                })
            })
        })
        .collect();
    let by_path: BTreeMap<&str, &File> = files
        .iter()
        .map(|file| (file.path.as_str(), file))
        .collect();
    let next: Vec<Vec<Next>> = ready
        .iter()
        .take(SHOWN)
        .map(|group| {
            group
                .iter()
                .map(|path| Next {
                    path: path.clone(),
                    candidates: candidates(by_path[path.as_str()], places, units),
                })
                .collect()
        })
        .collect();
    let more = ready.iter().skip(SHOWN).map(|group| group.len()).sum();
    Order {
        total: files.len(),
        ordered: true,
        next,
        more,
    }
}

/// The layers `file` could go in: the places of its home unit (the unit whose directory holds it, else every unit's)
/// that are not a layer with levels, which holds only its levels, and in which every import it makes, as an import
/// from there, and every import of it by a file in a layer, as an import of there, passes the direction check. What
/// the project's own rules say of an import between units applies as it does to any other.
///
/// The one place for what a file's own imports say beyond the layers: a table of the modules that do I/O would remove
/// here the layers that a file importing one cannot go in (`domain`, `protocol`, `application`), and nothing else here
/// would change.
fn candidates(file: &File, places: &[&Place], units: &[Unit]) -> Candidates {
    let home = units
        .iter()
        .filter(|unit| unit.path.is_empty() || within(&file.path, &unit.path))
        .max_by_key(|unit| unit.path.len());
    let possible: Vec<&&Place> = places
        .iter()
        .filter(|place| home.is_none_or(|unit| place.unit.name == unit.name))
        .filter(|place| !places.iter().any(|other| other.is_level_of(place)))
        .collect();
    let allowed = |place: &Place| {
        let imports = file
            .landings
            .iter()
            .all(|landing| passes(verdict(place, *landing, &Way::default())));
        imports
            && file
                .readers
                .iter()
                .all(|reader| passes(verdict(reader, Landing::Place(place), &Way::default())))
    };
    let passing: Vec<&&Place> = possible
        .iter()
        .copied()
        .filter(|place| allowed(place))
        .collect();
    if !possible.is_empty() && passing.len() == possible.len() {
        return Candidates::Any;
    }
    // Innermost first: the order of the table is outermost first
    Candidates::Only(
        passing
            .iter()
            .rev()
            .map(|place| match place.unit.name.as_str() {
                "" => place.name.clone(),
                unit => format!("{} of unit {unit}", place.name),
            })
            .collect(),
    )
}

/// Whether the direction check lets the import through.
fn passes(verdict: Verdict) -> bool {
    verdict == Verdict::Allowed
}

/// The groups of files that import each other, each file in one: a file that imports no file that imports it is a group
/// of its own. Each group is in the byte order of its paths, and the groups in the byte order of their first path.
fn components(files: &[File]) -> Vec<Vec<String>> {
    let index: BTreeMap<&str, usize> = files
        .iter()
        .enumerate()
        .map(|(i, file)| (file.path.as_str(), i))
        .collect();
    let needs: Vec<Vec<usize>> = files
        .iter()
        .map(|file| {
            file.needs
                .iter()
                .filter_map(|need| index.get(need.as_str()).copied())
                .collect()
        })
        .collect();
    // Tarjan's algorithm, with its own stack of calls so that a long chain of imports cannot overflow the real one
    let unseen = usize::MAX;
    let mut number = vec![unseen; files.len()];
    let mut low = vec![0; files.len()];
    let mut open = vec![false; files.len()];
    let mut stack: Vec<usize> = Vec::new();
    let mut found: Vec<Vec<String>> = Vec::new();
    let mut counter = 0;
    for root in 0..files.len() {
        if number[root] != unseen {
            continue;
        }
        let mut calls: Vec<(usize, usize)> = vec![(root, 0)];
        number[root] = counter;
        low[root] = counter;
        counter += 1;
        stack.push(root);
        open[root] = true;
        while let Some(&(at, next)) = calls.last() {
            if let Some(&to) = needs[at].get(next) {
                if let Some(call) = calls.last_mut() {
                    call.1 += 1;
                }
                if number[to] == unseen {
                    number[to] = counter;
                    low[to] = counter;
                    counter += 1;
                    stack.push(to);
                    open[to] = true;
                    calls.push((to, 0));
                } else if open[to] {
                    low[at] = low[at].min(number[to]);
                }
                continue;
            }
            calls.pop();
            if let Some(&(parent, _)) = calls.last() {
                low[parent] = low[parent].min(low[at]);
            }
            if low[at] == number[at] {
                let mut group = Vec::new();
                while let Some(top) = stack.pop() {
                    open[top] = false;
                    group.push(files[top].path.clone());
                    if top == at {
                        break;
                    }
                }
                group.sort();
                found.push(group);
            }
        }
    }
    found.sort();
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(path: &str, needs: &[&str]) -> File<'static> {
        File {
            path: path.to_string(),
            needs: needs.iter().map(|need| need.to_string()).collect(),
            ..File::default()
        }
    }

    /// The order of files that import nothing of a layer, in a project with no layer to put them in.
    fn order_of(files: &[File]) -> Order {
        order(files, &[], &[])
    }

    fn groups(order: &Order) -> Vec<Vec<&str>> {
        order
            .next
            .iter()
            .map(|group| group.iter().map(|next| next.path.as_str()).collect())
            .collect()
    }

    #[test]
    fn the_files_that_import_no_file_still_moving_come_first() {
        let order = order_of(&[
            file("a.py", &["b.py"]),
            file("b.py", &["c.py"]),
            file("c.py", &[]),
            file("d.py", &[]),
            file("e.py", &["a.py"]),
        ]);
        assert_eq!(order.total, 5);
        assert!(order.ordered);
        assert_eq!(groups(&order), [["c.py"], ["d.py"]]);
        assert_eq!(order.more, 0);
    }

    #[test]
    fn files_that_import_each_other_are_one_group_that_waits_for_what_it_imports_outside() {
        let files = [
            file("a.py", &["b.py"]),
            file("b.py", &["c.py", "a.py"]),
            file("c.py", &["b.py"]),
            file("d.py", &["a.py"]),
            file("e.py", &["x.py"]),
            file("x.py", &["y.py"]),
            file("y.py", &["x.py"]),
        ];
        let order = order_of(&files);
        // a, b and c are one group of three that imports nothing outside it; d waits for it; e waits for x and y, which
        // import each other and nothing else
        assert_eq!(
            groups(&order),
            [vec!["a.py", "b.py", "c.py"], vec!["x.py", "y.py"]]
        );
    }

    #[test]
    fn a_group_that_waits_for_another_is_not_next() {
        let order = order_of(&[
            file("a.py", &["b.py"]),
            file("b.py", &["a.py", "c.py"]),
            file("c.py", &[]),
        ]);
        assert_eq!(groups(&order), [["c.py"]]);
    }

    #[test]
    fn a_file_that_imports_itself_is_a_file_alone() {
        let order = order_of(&[file("a.py", &["a.py"])]);
        assert_eq!(groups(&order), [["a.py"]]);
    }

    #[test]
    fn only_the_first_groups_are_named_and_the_files_of_the_rest_are_counted() {
        let files: Vec<File> = (0..8)
            .map(|i| file(&format!("m{i}.py"), &[]))
            .chain([file("x.py", &["y.py"]), file("y.py", &["x.py"])])
            .collect();
        let order = order_of(&files);
        assert_eq!(order.total, 10);
        assert_eq!(order.next.len(), SHOWN);
        // Three single files and a group of two: the groups are in the byte order of their first path
        assert_eq!(order.more, 5);
    }

    #[test]
    fn a_long_chain_does_not_overflow_the_stack() {
        let count = 100_000;
        let files: Vec<File> = (0..count)
            .map(|i| File {
                path: format!("f{i:06}.py"),
                needs: (i + 1 < count)
                    .then(|| format!("f{:06}.py", i + 1))
                    .into_iter()
                    .collect(),
                ..File::default()
            })
            .collect();
        let order = order_of(&files);
        assert_eq!(order.total, count);
        assert_eq!(groups(&order), [[format!("f{:06}.py", count - 1).as_str()]]);
    }

    // Where a file could go

    use crate::layers::{UnitTag, layout, table};

    fn named<'a>(places: &'a [Place], name: &str) -> &'a Place {
        places.iter().find(|p| p.name == name).unwrap()
    }

    /// The places of a project of one unit (Python, no UI), and what a file that imports `lands` of them, which
    /// `readers` import, could be moved into.
    fn candidates_in_one_unit(lands: &[&str], readers: &[&str]) -> Candidates {
        let layout = layout("python", "none").unwrap();
        let places = layout.places(&table());
        let all: Vec<&Place> = places.iter().collect();
        let file = File {
            path: "legacy/a.py".to_string(),
            landings: lands
                .iter()
                .map(|name| Landing::Place(named(&places, name)))
                .collect(),
            readers: readers.iter().map(|name| named(&places, name)).collect(),
            ..File::default()
        };
        let order = order(&[file], &all, &[Unit::implicit(&layout, &[])]);
        order.next[0][0].candidates.clone()
    }

    #[test]
    fn a_file_that_imports_nothing_of_a_layer_and_no_layer_imports_could_go_anywhere() {
        assert_eq!(candidates_in_one_unit(&[], &[]), Candidates::Any);
    }

    #[test]
    fn what_a_file_imports_of_the_layers_narrows_where_it_could_go_to_where_the_table_allows_it() {
        // `domain` imports no layer, and `application` and `protocol.inbound` import `domain` but not `infrastructure`
        assert_eq!(
            candidates_in_one_unit(&["infrastructure.outbound"], &[]),
            Candidates::Only(vec![
                "infrastructure.outbound".to_string(),
                "composition".to_string()
            ])
        );
        let Candidates::Only(inside) = candidates_in_one_unit(&["domain"], &[]) else {
            panic!("`domain` is imported by some layers and not by others");
        };
        assert!(inside.contains(&"domain".to_string()));
        assert!(inside.contains(&"application".to_string()));
        assert!(!inside.contains(&"infrastructure.inbound".to_string()));
        // A layer with levels holds only them
        assert!(!inside.contains(&"infrastructure".to_string()));
    }

    #[test]
    fn what_the_layers_import_of_a_file_narrows_where_it_could_go_to_what_they_may_import() {
        assert_eq!(
            candidates_in_one_unit(&[], &["domain"]),
            Candidates::Only(vec!["domain".to_string()])
        );
        // Imported by `domain` and importing `infrastructure.outbound`: no place passes both
        assert_eq!(
            candidates_in_one_unit(&["infrastructure.outbound"], &["domain"]),
            Candidates::Only(Vec::new())
        );
    }

    #[test]
    fn a_file_in_a_unit_goes_in_a_layer_of_that_unit_and_one_in_no_unit_in_a_layer_of_any() {
        let layout = layout("python", "none").unwrap();
        let tag = |name: &str, shared: bool| UnitTag {
            name: name.to_string(),
            shared,
            app: false,
        };
        let core = layout.places_in(&table(), "core", &tag("core", true));
        let billing = layout.places_in(&table(), "billing", &tag("billing", false));
        let all: Vec<&Place> = core.iter().chain(&billing).collect();
        let unit = |name: &str, shared: bool| Unit {
            name: name.to_string(),
            path: name.to_string(),
            root: name.to_string(),
            shared,
            entries: Vec::new(),
            absent: Vec::new(),
            declared: true,
        };
        let units = [unit("core", true), unit("billing", false)];
        let file = |path: &str| File {
            path: path.to_string(),
            // A shared unit's `domain`, which any layer that may import its own may import
            landings: vec![Landing::Place(named(&core, "domain"))],
            ..File::default()
        };
        let only = |path: &str| match &order(&[file(path)], &all, &units).next[0][0].candidates {
            Candidates::Only(names) => names.clone(),
            Candidates::Any => vec!["any".to_string()],
        };
        let in_billing = only("billing/legacy/x.py");
        assert!(!in_billing.is_empty());
        assert!(
            in_billing
                .iter()
                .all(|name| name.ends_with(" of unit billing")),
            "{in_billing:?}"
        );
        assert!(in_billing.contains(&"domain of unit billing".to_string()));
        // `infrastructure.inbound` imports `protocol.inbound` alone, as it does in a shared unit
        assert!(!in_billing.contains(&"infrastructure.inbound of unit billing".to_string()));
        // Outside every unit, the file could go in a layer of either, and the names say which
        let nowhere = only("elsewhere/x.py");
        assert!(nowhere.contains(&"domain of unit billing".to_string()));
        assert!(nowhere.contains(&"domain of unit core".to_string()));
    }
}
