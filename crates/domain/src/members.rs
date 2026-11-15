//! The members of a Rust workspace that already exists: what `rotproof create` adds to the root `Cargo.toml` when a
//! unit's crates are made ([`extended`]), the one change it makes to a file it wrote once.
//!
//! It changes `members` in two ways and no other. It adds the crates it made that no member names or matches. And it
//! replaces the glob Rotproof wrote before units ([`GLOB`]) with the crates the glob matched, once the glob would match
//! a directory that has no `Cargo.toml`, as the directory of a unit that holds its crates does (`crates/billing`):
//! Cargo fails on it, and so would every project that nests its crates. A glob the project wrote itself, any other
//! member and every other key stay as they are.
//!
//! An entry file a unit declares is compiled only through a manifest above it, so `rotproof create` makes the package
//! that holds it too ([`entry_package`]), which the workspace then names.

use std::collections::BTreeSet;

use toml_edit::{DocumentMut, Value};

use crate::layers::{Layout, Place, package_of};
use crate::paths::{file_name, parent, relative};

/// The glob of `members` that Rotproof wrote before units, every directory in `crates/`.
pub const GLOB: &str = "crates/*";

/// A manifest changed: its new text, and what changed in it, for a person to be told.
#[derive(Debug, PartialEq, Eq)]
pub struct Edited {
    pub text: String,
    pub said: Vec<String>,
}

/// The package that holds an entry file a unit declares, which `rotproof create` makes with the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Package {
    /// The directory of the manifest, from the root: what the workspace lists as a member
    pub dir: String,
    /// The manifest's path, from the root
    pub manifest: String,
    pub text: String,
}

/// The package for the entry file `entry` of the unit `unit`, which is its `nth` entry, made from the layout's
/// `entry_package`: its manifest sits in the directory above `src/`, as Cargo reads a bin crate (`<dir>/src/main.rs`
/// is the crate of `<dir>/Cargo.toml`), and it is named for the unit, or for the unit and its directory from the
/// second entry on, so that two packages are never named alike. `composition` is the unit's own `composition` crate,
/// which the package depends on by path; a unit that lacks it depends on nothing. `Ok(None)` when the layout has no
/// such package. `Err` says why an entry has no place for one, which a person fixes in the declaration.
pub fn entry_package(
    layout: &Layout,
    unit: &str,
    nth: usize,
    entry: &str,
    composition: Option<&Place>,
) -> Result<Option<Package>, String> {
    let Some(template) = &layout.entry_package else {
        return Ok(None);
    };
    let Some(dir) = entry.strip_suffix("/src/main.rs") else {
        return Err(if entry == "src/main.rs" {
            format!(
                "unit {unit:?} declares entry {entry:?}, at the root, where the workspace is: a unit's entry file sits in a \
                 package of its own, such as apps/{unit}/src/main.rs"
            )
        } else {
            format!(
                "unit {unit:?} declares entry {entry:?}: a Rust entry file is <dir>/src/main.rs, so that its package is \
                 <dir>/Cargo.toml"
            )
        });
    };
    let name = if nth == 0 {
        unit.to_string()
    } else {
        format!("{unit}_{}", file_name(dir))
    };
    let dependencies = composition.map_or(String::new(), |place| {
        format!(
            "\n[dependencies]\n{} = {{ path = \"{}\" }}\n",
            package_of(&place.unit, &place.name),
            relative(dir, &place.path)
        )
    });
    Ok(Some(Package {
        dir: dir.to_string(),
        manifest: format!("{dir}/{}", layout.manifest),
        text: template
            .trim_start()
            .replace("{package}", &name)
            .replace("{dependencies}", &dependencies),
    }))
}

/// The manifest `text` with the crates `made` (their directories, from the root) added to the members of its
/// workspace that do not name them, and [`GLOB`] replaced when it would match a directory with no manifest. `files` is
/// every file of the project, `made`'s too, which tells the directories there are and which of them hold a manifest.
/// `None` when nothing changes: a manifest that is not TOML or has no workspace with members is not Rotproof's to
/// change.
pub fn extended(text: &str, made: &[String], files: &[String]) -> Option<Edited> {
    let mut doc: DocumentMut = text.parse().ok()?;
    let members = doc
        .get_mut("workspace")?
        .get_mut("members")?
        .as_array_mut()?;
    let listed = |members: &toml_edit::Array| -> Vec<String> {
        members
            .iter()
            .filter_map(Value::as_str)
            .map(String::from)
            .collect()
    };
    // The multi-line array Rotproof writes keeps a line to a member, whatever is put in
    let lines = members.to_string().contains('\n');
    let put = |members: &mut toml_edit::Array, at: usize, path: &str| {
        let mut value = Value::from(path);
        if lines {
            value.decor_mut().set_prefix("\n    ");
        }
        members.insert_formatted(at, value);
    };
    let manifests: BTreeSet<&str> = files
        .iter()
        .filter(|path| file_name(path) == "Cargo.toml")
        .map(|path| parent(path))
        .collect();
    let mut dirs = BTreeSet::new();
    for path in files {
        let mut dir = parent(path);
        while !dir.is_empty() && dirs.insert(dir) {
            dir = parent(dir);
        }
    }
    let mut said = Vec::new();
    if let Some(at) = listed(members).iter().position(|m| m == GLOB) {
        let hit: Vec<&str> = dirs
            .iter()
            .copied()
            .filter(|dir| matches(GLOB, dir))
            .collect();
        let bare: Vec<&str> = hit
            .iter()
            .copied()
            .filter(|dir| !manifests.contains(dir))
            .collect();
        if !bare.is_empty() {
            members.remove(at);
            let kept: Vec<&str> = hit
                .iter()
                .copied()
                .filter(|dir| manifests.contains(dir))
                .collect();
            for (i, dir) in kept.iter().enumerate() {
                put(members, at + i, dir);
            }
            said.push(format!(
                "replaced \"{GLOB}\" in members with the crates it matched, since it would match {}, which has no Cargo.toml",
                bare.join(", ")
            ));
        }
    }
    let now = listed(members);
    let added: Vec<&str> = made
        .iter()
        .map(String::as_str)
        .filter(|dir| !now.iter().any(|member| matches(member, dir)))
        .collect();
    for dir in &added {
        put(members, members.len(), dir);
    }
    if !added.is_empty() {
        said.push(format!("added to members: {}", added.join(", ")));
    }
    (!said.is_empty()).then(|| Edited {
        text: doc.to_string(),
        said,
    })
}

/// Whether the member `pattern` names the directory `dir`, as Cargo reads it: part by part, where `*` is any run of
/// characters and `?` any one, within the part. A part with `[` or `{` is read as it is written, which is a directory
/// to Cargo too only when there is one of that very name.
fn matches(pattern: &str, dir: &str) -> bool {
    let pattern = pattern.trim_end_matches('/');
    let pattern = pattern.strip_prefix("./").unwrap_or(pattern);
    let (parts, names): (Vec<&str>, Vec<&str>) =
        (pattern.split('/').collect(), dir.split('/').collect());
    parts.len() == names.len()
        && parts.iter().zip(&names).all(|(part, name)| {
            let (part, name): (Vec<char>, Vec<char>) =
                (part.chars().collect(), name.chars().collect());
            part_matches(&part, &name)
        })
}

fn part_matches(pattern: &[char], name: &[char]) -> bool {
    match pattern.split_first() {
        None => name.is_empty(),
        Some(('*', rest)) => (0..=name.len()).any(|skipped| part_matches(rest, &name[skipped..])),
        Some(('?', rest)) => name
            .split_first()
            .is_some_and(|(_, tail)| part_matches(rest, tail)),
        Some((c, rest)) => name
            .split_first()
            .is_some_and(|(n, tail)| n == c && part_matches(rest, tail)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(list: &[&str]) -> Vec<String> {
        list.iter().map(|p| p.to_string()).collect()
    }

    /// A project of two units at `crates/`, whose crates were just made.
    fn two_units() -> (Vec<String>, Vec<String>) {
        let made = paths(&[
            "crates/billing/domain",
            "crates/billing/protocol",
            "crates/core/domain",
        ]);
        let files = made
            .iter()
            .flat_map(|dir| [format!("{dir}/Cargo.toml"), format!("{dir}/src/lib.rs")])
            .collect();
        (made, files)
    }

    #[test]
    fn the_glob_rotproof_wrote_becomes_the_crates_it_matched_once_it_would_match_a_unit() {
        let (made, mut files) = two_units();
        files.extend(paths(&[
            "crates/composition/Cargo.toml",
            "crates/composition/src/lib.rs",
            "crates/domain/Cargo.toml",
        ]));
        let text =
            "[package]\nname = \"x\"\n\n[workspace]\nmembers = [\"crates/*\"]\nresolver = \"3\"\n";
        let edited = extended(text, &made, &files).unwrap();
        assert_eq!(
            edited.text,
            "[package]\nname = \"x\"\n\n[workspace]\nmembers = [\"crates/composition\", \"crates/domain\", \
             \"crates/billing/domain\", \"crates/billing/protocol\", \"crates/core/domain\"]\nresolver = \"3\"\n"
        );
        assert_eq!(
            edited.said,
            [
                "replaced \"crates/*\" in members with the crates it matched, since it would match crates/billing, \
                 crates/core, which has no Cargo.toml",
                "added to members: crates/billing/domain, crates/billing/protocol, crates/core/domain"
            ]
        );
        // Written again, it changes nothing
        let again = edited.text.clone();
        assert_eq!(extended(&again, &made, &files), None);
    }

    #[test]
    fn a_list_of_lines_gets_a_line_for_each_crate_added() {
        let (made, files) = two_units();
        let text = "[workspace]\nmembers = [\n    \"crates/billing/domain\",\n    \"xtask\",\n]\nresolver = \"3\"\n";
        let edited = extended(text, &made, &files).unwrap();
        assert_eq!(
            edited.text,
            "[workspace]\nmembers = [\n    \"crates/billing/domain\",\n    \"xtask\",\n    \"crates/billing/protocol\",\n    \
             \"crates/core/domain\",\n]\nresolver = \"3\"\n"
        );
        assert_eq!(
            edited.said,
            ["added to members: crates/billing/protocol, crates/core/domain"]
        );
    }

    #[test]
    fn a_glob_that_matches_no_directory_without_a_manifest_stays_and_covers_what_it_matches() {
        // A project that had no unit before: every directory of `crates/` is a crate
        let made = paths(&["crates/domain"]);
        let files = paths(&["crates/domain/Cargo.toml", "crates/app/Cargo.toml"]);
        let text = "[workspace]\nmembers = [\"crates/*\"]\n";
        assert_eq!(extended(text, &made, &files), None);
        // Units a level deeper are matched by a glob that reaches them
        let (made, files) = two_units();
        let text = "[workspace]\nmembers = [\"crates/*/*\"]\n";
        assert_eq!(extended(text, &made, &files), None);
    }

    #[test]
    fn only_the_glob_rotproof_wrote_is_replaced() {
        let (made, files) = two_units();
        // The project's own glob is the project's: crates are added beside it
        let text = "[workspace]\nmembers = [\"crates/b*\"]\n";
        let edited = extended(text, &made, &files).unwrap();
        assert_eq!(
            edited.text,
            "[workspace]\nmembers = [\"crates/b*\", \"crates/billing/domain\", \"crates/billing/protocol\", \
             \"crates/core/domain\"]\n"
        );
        assert!(edited.said.iter().all(|said| !said.starts_with("replaced")));
    }

    #[test]
    fn a_manifest_without_members_is_not_changed() {
        let (made, files) = two_units();
        for text in [
            "[package]\nname = \"x\"\n",
            "[workspace]\nresolver = \"3\"\n",
            "[workspace\n",
            "members = [\"a\"]\n",
        ] {
            assert_eq!(extended(text, &made, &files), None, "{text}");
        }
    }

    #[test]
    fn a_member_is_read_as_cargo_reads_it() {
        assert!(matches("crates/*", "crates/a") && !matches("crates/*", "crates/a/b"));
        assert!(matches("crates/*/*", "crates/a/b") && matches("./crates/a/", "crates/a"));
        assert!(matches("crates/a?c", "crates/abc") && !matches("crates/a?c", "crates/ac"));
        assert!(
            matches("crates/pre*post", "crates/prepost")
                && matches("crates/pre*post", "crates/pre_x_post")
        );
        assert!(!matches("crates/a", "crates/ab") && !matches("crates", "crates/a"));
    }

    fn composition_of(unit: &str, path: &str) -> Place {
        use crate::layers::{UnitTag, layout, table};
        let tag = UnitTag {
            name: unit.into(),
            ..UnitTag::default()
        };
        layout("rust", "none")
            .unwrap()
            .places_in(&table(), path, &tag)
            .into_iter()
            .find(|place| place.name == "composition")
            .unwrap()
    }

    #[test]
    fn the_package_of_an_entry_file_sits_above_its_src_and_depends_on_the_composition_of_its_unit()
    {
        let layout = crate::layers::layout("rust", "none").unwrap();
        let composition = composition_of("cli", "apps/cli/crates");
        let package = entry_package(
            &layout,
            "cli",
            0,
            "apps/cli/src/main.rs",
            Some(&composition),
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            (package.dir.as_str(), package.manifest.as_str()),
            ("apps/cli", "apps/cli/Cargo.toml")
        );
        assert_eq!(
            package.text,
            "[package]\nname = \"cli\"\nversion = \"0.1.0\"\nedition = \"2024\"\npublish = false\n\n[dependencies]\n\
             cli_composition = { path = \"crates/composition\" }\n"
        );
        // A second entry of the unit is named for its directory too, and a unit with no `composition` depends on none
        let second = entry_package(&layout, "cli", 1, "apps/cli-web/src/main.rs", None)
            .unwrap()
            .unwrap();
        assert!(
            second.text.contains("name = \"cli_cli-web\"\n")
                && !second.text.contains("[dependencies]")
        );
        // Another stack has no package for an entry file
        let python = crate::layers::layout("python", "none").unwrap();
        assert_eq!(
            entry_package(&python, "cli", 0, "bin/run.py", None),
            Ok(None)
        );
    }

    #[test]
    fn an_entry_file_that_is_no_src_main_rs_has_no_place_for_its_package_and_is_said() {
        let layout = crate::layers::layout("rust", "none").unwrap();
        for (entry, said) in [
            ("src/main.rs", "at the root, where the workspace is"),
            (
                "apps/cli/src/bin/run.rs",
                "a Rust entry file is <dir>/src/main.rs",
            ),
            (
                "apps/cli/main.rs",
                "so that its package is <dir>/Cargo.toml",
            ),
        ] {
            let why = entry_package(&layout, "cli", 0, entry, None).unwrap_err();
            assert!(
                why.starts_with(&format!("unit \"cli\" declares entry {entry:?}")),
                "{why}"
            );
            assert!(why.contains(said), "{why}");
        }
    }
}
