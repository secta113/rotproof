//! The units of a project: the parts of its code that each hold the layers (a package of a monorepo, a module of a
//! modular monolith).
//!
//! A project that declares none has one implicit unit, which holds the layers where the layout puts them and has the
//! layout's entry file, so every check reads units the same way whether or not the project says so.

use crate::layers::{Layout, Place, Table, UnitTag};

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

    /// The entry files of this unit, each as a place of its own.
    pub fn entry_places(&self, layout: &Layout, table: &Table) -> Vec<Place> {
        let tag = self.tag();
        self.entries
            .iter()
            .map(|path| layout.entry_at(table, path, &tag))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
}
