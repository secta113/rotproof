//! `rotproof create`: make what `.config/rotproof.toml` declares and the tree does not have yet. What it makes and
//! rewrites is listed for a person in the help of `rotproof create` (`cli.rs`).
//!
//! A present layer (its path exists) is the project's, and nothing in it is touched. Layers are made only with `--yes`:
//! a project that ran `rotproof create` straight after `rotproof init`, never listing in absent what it lacks, would
//! otherwise get every layer of the stack (`ui` and its five levels in a command-line tool), and `rotproof create`
//! never deletes them again. A run that makes no layer, as after an upgrade, needs no `--yes`.
//!
//! It never runs by itself, so a layer removed by mistake fails the check instead of coming back.

use crate::bundle::Bundle;
use crate::layers::{Source, source};
use crate::tree::{read_text, read_written};
use domain::bundle::{LOG, in_docs, record_tags as tags_in};
use domain::declaration::{
    DECLARATION, Declared, First, MISSING, completed, fields, lacking, parse_declaration,
    unreadable,
};
use domain::guide::{GUIDE, guide};
use domain::layers::{Language, NO_UI, Place, layout, table, uis};
use domain::members::{Package, entry_package, extended};
use domain::project::project_files;
use domain::skill::{PATHS as SKILL_PATHS, SKILL};
use domain::tree::{Written, with_lf};
use protocol::inbound::Made;
use protocol::outbound::{Tree, Writer};

/// What a run does with the layers that are neither present nor declared absent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Layers {
    /// Make them (`--yes`)
    Make,
    /// Stop before anything is written when there is one (no `--yes`)
    Stop,
    /// Leave them, and make the rest (`rotproof init` on an upgrade)
    Leave,
}

/// The text of a declaration completed with the fields it lacked, and the fields added as `name = value`.
type Completed = (String, Vec<String>);

/// Make what is missing in `tree`, writing through `out`, for the project named `name` (its root directory's). Layers
/// are made only when `yes` says so. `Err` is a declaration that cannot be read, a file that cannot be written, or
/// layers to make without `yes`: then nothing is written.
pub fn create(tree: &dyn Tree, out: &dyn Writer, name: &str, yes: bool) -> Result<Made, String> {
    make(
        tree,
        out,
        name,
        if yes { Layers::Make } else { Layers::Stop },
    )
}

/// What `rotproof create` does after an upgrade, never making a layer: the fields the declaration lacks, the files
/// Rotproof generates, and the records skeleton and the project's files where they are missing. A layer that is
/// missing stays missing, for `rotproof check` to name.
pub fn refresh(tree: &dyn Tree, out: &dyn Writer, name: &str) -> Result<Made, String> {
    make(tree, out, name, Layers::Leave)
}

/// Make what is missing, in the order a project is built: the declaration completed and the layers it declares, the
/// records skeleton, the guide and the skill, the project's files written once, and the index files. Nothing is
/// written before the declaration reads and fits its stack, and before the layers to make are known to be wanted.
fn make(tree: &dyn Tree, out: &dyn Writer, name: &str, mode: Layers) -> Result<Made, String> {
    let mut made = Made::default();
    let (declared, completed) = read_declared(tree)?;
    let layers = to_make(tree, &declared, mode)?;
    // Known before anything is written: an entry file with no place for its package stops the run
    let packages = if mode == Layers::Leave {
        Vec::new()
    } else {
        entry_packages(tree, &declared)?
    };
    if let Some((text, added)) = completed {
        write(out, DECLARATION, &text, &mut made)?;
        made.added = added;
    }
    write_layers(out, &layers, &mut made)?;
    if mode != Layers::Leave {
        write_entries(tree, out, &declared, &mut made)?;
    }
    let new_packages = write_packages(tree, out, &packages, &mut made)?;
    write_records(tree, out, &mut made)?;
    write_guide(tree, out, &declared, &mut made)?;
    write_skill(tree, out, &mut made)?;
    let crates = Crates {
        layers: &layers,
        packages: &packages,
    };
    write_project(tree, out, &declared, &crates, name, &mut made)?;
    let crates = Crates {
        layers: &layers,
        packages: &new_packages,
    };
    write_members(tree, out, &declared, &crates, &mut made)?;
    write_indexes(tree, out, &declared, &mut made)?;
    Ok(made)
}

/// The declaration as `rotproof create` goes on from it, and the completed text with the fields added when it lacked
/// any (`complete`): not written yet. The file is read once; what it holds is parsed as it is, and completed as
/// [`with_lf`] reads it.
fn read_declared(tree: &dyn Tree) -> Result<(Declared, Option<Completed>), String> {
    match source(tree).map_err(|e| e.to_string())? {
        Source::Missing => Err(MISSING.into()),
        Source::Spelled(why) => Err(unreadable(&why)),
        // A declaration that lacks `ui` reads, and fails only once the stack is known: it is completed all the same
        Source::Text(raw) => match complete(tree, &raw)? {
            // Written only once the completed declaration reads and fits its stack, so a declaration that fails for
            // another reason is left as it was
            Some((text, added)) => {
                let declaration = parse_declaration(&text).map_err(|why| unreadable(&why))?;
                let declared = Declared::new(declaration).map_err(|found| found.join("\n"))?;
                Ok((declared, Some((text, added))))
            }
            None => Ok((
                Declared::new(parse_declaration(&raw).map_err(|why| unreadable(&why))?)
                    .map_err(|found| found.join("\n"))?,
                None,
            )),
        },
    }
}

/// The layers this run makes: those neither present nor declared absent, or none in a run that leaves them. `Err` when
/// there are some and the run was not told to make them.
fn to_make<'a>(
    tree: &dyn Tree,
    declared: &'a Declared,
    mode: Layers,
) -> Result<Vec<&'a Place>, String> {
    // A level whose layer is declared absent is absent too. A level of a layer about to be made is not there either
    let mut layers: Vec<&Place> = declared
        .places
        .iter()
        .filter(|place| !declared.is_absent(place) && tree.found(&place.path).is_none())
        .collect();
    // Before anything is written: a project that never listed in absent what it lacks would get every layer
    match mode {
        Layers::Stop if !layers.is_empty() => return Err(unconfirmed(&layers)),
        Layers::Leave => layers.clear(),
        _ => {}
    }
    Ok(layers)
}

/// The files of each layer in `layers`, as the layout writes them.
fn write_layers(out: &dyn Writer, layers: &[&Place], made: &mut Made) -> Result<(), String> {
    for place in layers {
        for (path, text) in &place.files {
            write(out, path, text, made)?;
        }
    }
    Ok(())
}

/// The entry files the declared units name that are not there, each with the layout's text for an entry file: an
/// upgrade, which makes no layer, makes no code either. The implicit unit's entry is not made here: it is the root
/// package's file in Rust, and optional elsewhere.
fn write_entries(
    tree: &dyn Tree,
    out: &dyn Writer,
    declared: &Declared,
    made: &mut Made,
) -> Result<(), String> {
    let Some(text) = declared
        .layout
        .as_ref()
        .and_then(|layout| layout.entry_file())
    else {
        return Ok(());
    };
    let entries = declared
        .units
        .iter()
        .filter(|unit| unit.declared)
        .flat_map(|unit| &unit.entries);
    for entry in entries.filter(|entry| tree.found(entry).is_none()) {
        write(out, entry, text, made)?;
    }
    Ok(())
}

/// The crates a run makes or has: the layers it makes, and the packages that hold the entry files of the units.
struct Crates<'a> {
    layers: &'a [&'a Place],
    packages: &'a [Package],
}

/// The package of each entry file the declared units name, in a stack whose entry file needs one: where its manifest
/// sits is read from the entry's path. An entry file that is not there and has no place for its package fails, before
/// anything is written; one that is there, whatever its path, is the project's.
fn entry_packages(tree: &dyn Tree, declared: &Declared) -> Result<Vec<Package>, String> {
    let Some(layout) = &declared.layout else {
        return Ok(Vec::new());
    };
    let mut packages = Vec::new();
    for unit in declared.units.iter().filter(|unit| unit.declared) {
        let composition = declared.places.iter().find(|p| {
            p.unit.name == unit.name && p.name == "composition" && !declared.is_absent(p)
        });
        for (nth, entry) in unit.entries.iter().enumerate() {
            match entry_package(layout, &unit.name, nth, entry, composition) {
                Ok(Some(package)) => packages.push(package),
                Ok(None) => {}
                Err(why) if tree.found(entry).is_none() => {
                    return Err(format!("{DECLARATION}: {why}"));
                }
                Err(_) => {}
            }
        }
    }
    Ok(packages)
}

/// The manifest of each package in `packages` that is not there, which the entry file needs to be a crate: never
/// over one that is. The packages written, for the workspace to name.
fn write_packages(
    tree: &dyn Tree,
    out: &dyn Writer,
    packages: &[Package],
    made: &mut Made,
) -> Result<Vec<Package>, String> {
    let mut written = Vec::new();
    for package in packages
        .iter()
        .filter(|p| tree.found(&p.manifest).is_none())
    {
        write(out, &package.manifest, &package.text, made)?;
        written.push(package.clone());
    }
    Ok(written)
}

/// The records skeleton: the directory of the knowledge documents, and the log when there is none.
fn write_records(tree: &dyn Tree, out: &dyn Writer, made: &mut Made) -> Result<(), String> {
    let dir = in_docs("knowledge");
    out.make_dir(&dir).map_err(|e| format!("{dir}: {e}"))?;
    if tree.found(&in_docs("log.md")).is_none() {
        write(out, "docs/log.md", LOG, made)?;
    }
    Ok(())
}

/// The guide, when it is not what Rotproof writes for the stack.
fn write_guide(
    tree: &dyn Tree,
    out: &dyn Writer,
    declared: &Declared,
    made: &mut Made,
) -> Result<(), String> {
    let guide = guide(&declared.declaration.stack, declared.layout.as_ref());
    if read_text(tree, GUIDE).ok().as_ref() != Some(&guide) {
        write(out, GUIDE, &guide, made)?;
    }
    Ok(())
}

/// The skill, in each place it is written, where it is not what Rotproof writes. It is the same for every project.
fn write_skill(tree: &dyn Tree, out: &dyn Writer, made: &mut Made) -> Result<(), String> {
    for path in SKILL_PATHS {
        if read_text(tree, path).ok().as_deref() != Some(SKILL) {
            write(out, path, SKILL, made)?;
        }
    }
    Ok(())
}

/// The project's files written once, outside `docs/`, each where it is not there yet; and what the stack does not have
/// Rotproof write yet. The crates a workspace lists are the layers that are there and those this run makes, and the
/// packages of the entry files.
fn write_project(
    tree: &dyn Tree,
    out: &dyn Writer,
    declared: &Declared,
    made_crates: &Crates,
    name: &str,
    made: &mut Made,
) -> Result<(), String> {
    let mut crates: Vec<String> = declared
        .places
        .iter()
        .filter(|place| place.is_layer() && !declared.is_absent(place))
        .filter(|place| {
            tree.found(&place.path).is_some()
                || made_crates
                    .layers
                    .iter()
                    .any(|made| made.path == place.path)
        })
        .map(|place| place.path.clone())
        .collect();
    crates.extend(made_crates.packages.iter().map(|p| p.dir.clone()));
    let (files, not_written) = project_files(declared, name, &crates);
    for (path, text) in files {
        if tree.found(path).is_none() {
            write(out, path, &text, made)?;
        }
    }
    made.not_written = not_written;
    Ok(())
}

/// there gets the crates of the layers and the entry packages this run made among its members, and loses a glob that
/// would match a unit's directory, which has no `Cargo.toml` (`domain::members`). It is said, in `made.changed`. A
/// workspace whose members already match every crate made, as the glob an older Rotproof wrote does in a project with
/// no unit, is left as it is.
/// is.
fn write_members(
    tree: &dyn Tree,
    out: &dyn Writer,
    declared: &Declared,
    made_crates: &Crates,
    made: &mut Made,
) -> Result<(), String> {
    let Some(layout) = &declared.layout else {
        return Ok(());
    };
    // The manifest at the root, which the layout names as what its code is
    let manifest = layout.code.as_str();
    if layout.language != Language::Rust
        || (made_crates.layers.is_empty() && made_crates.packages.is_empty())
        || made.written.iter().any(|path| path == manifest)
        || tree.found(manifest) != Some(false)
    {
        return Ok(());
    }
    // A manifest that is not text is the project's, and no crate of a layer is the less made for it
    let Ok((text, written)) = read_written(tree, manifest) else {
        return Ok(());
    };
    let crates: Vec<String> = made_crates
        .layers
        .iter()
        .filter(|place| place.is_layer())
        .map(|place| place.path.clone())
        .chain(made_crates.packages.iter().map(|p| p.dir.clone()))
        .collect();
    let mut files = tree.files("").map_err(|e| e.to_string())?;
    files.extend(
        made_crates
            .layers
            .iter()
            .flat_map(|place| place.files.iter().map(|(path, _)| path.clone())),
    );
    files.extend(made_crates.packages.iter().map(|p| p.manifest.clone()));
    if let Some(edited) = extended(&text, &crates, &files) {
        out.write(manifest, &written.restore(&edited.text))
            .map_err(|e| format!("{manifest}: {e}"))?;
        made.changed.extend(
            edited
                .said
                .into_iter()
                .map(|said| (manifest.to_string(), said)),
        );
    }
    Ok(())
}

/// The index files and the rules of the knowledge documents, each where it is not what Rotproof writes from the
/// documents as they are now; and the documents left out of them.
fn write_indexes(
    tree: &dyn Tree,
    out: &dyn Writer,
    declared: &Declared,
    made: &mut Made,
) -> Result<(), String> {
    let bundle = Bundle::new(tree, declared.declaration.areas.clone());
    let (files, problems) = bundle.expected().map_err(|e| e.to_string())?;
    for (path, text) in files {
        if read_text(tree, &path).ok().as_ref() != Some(&text) {
            write(out, &path, &text, made)?;
        }
    }
    made.left_out = problems.into_iter().collect();
    Ok(())
}

/// The declaration with every field of [`ADDED`](domain::declaration::ADDED) it lacks, each under its comment, and
/// the fields added as `name = value`. `raw` is the text of the file as it is, which also gives how it was written.
/// `None` when it lacks none, or is not TOML: then its own error stands.
fn complete(tree: &dyn Tree, raw: &str) -> Result<Option<Completed>, String> {
    let text = with_lf(raw);
    let was = Written::of(raw);
    let Some(lacking) = lacking(&text) else {
        return Ok(None);
    };
    if lacking.is_empty() {
        return Ok(None);
    }
    let mut values = Vec::new();
    for name in lacking {
        let value = match name {
            "areas" => First::List(record_tags(tree)?),
            "ui" => {
                let stack = fields(&text)?.stack.unwrap_or_default();
                First::Name(ui_in_tree(tree, &stack))
            }
            name => {
                unreachable!("every field Rotproof adds has a rule for its first value: {name}")
            }
        };
        values.push((name, value));
    }
    // As the project wrote it: the text was read with every line ending `\n`, and a declaration checked out with CRLF
    // would otherwise change on every line, not only where a field was added
    let (text, added) = completed(&text, &values);
    Ok(Some((was.restore(&text), added)))
}

/// The UI whose layers `tree` holds: the first UI `stack` offers with a layer of the table there, or
/// [`NO_UI`] when none is. The first value of `ui`, so a project keeps the layers it made: one with `ui/` keeps it,
/// and one without, which had declared it absent or never made it, is checked for no screens.
pub fn ui_in_tree(tree: &dyn Tree, stack: &str) -> String {
    let Some(base) = layout(stack, NO_UI) else {
        return NO_UI.to_string();
    };
    let table = table();
    uis(stack)
        .unwrap_or_default()
        .into_iter()
        .find(|ui| {
            table
                .layers
                .iter()
                .filter(|l| l.ui.as_ref() == Some(ui))
                .any(|l| tree.found(&base.layer_path(&l.name)) == Some(true))
        })
        .unwrap_or_else(|| NO_UI.to_string())
}

/// Every tag the knowledge documents use, sorted by name: the first value of `areas`, so the documents that fit their
/// one-area rule keep fitting once the field exists.
fn record_tags(tree: &dyn Tree) -> Result<Vec<String>, String> {
    if tree.found(&in_docs("knowledge")) != Some(true) {
        return Ok(Vec::new());
    }
    let docs = Bundle::new(tree, Vec::new())
        .read_folder("knowledge")
        .map_err(|e| e.to_string())?;
    Ok(tags_in(&docs).into_iter().collect())
}

/// What `rotproof create` says when it would make `layers` and was not told to: each with its path, and how to go on.
fn unconfirmed(layers: &[&Place]) -> String {
    let listed: Vec<String> = layers
        .iter()
        .map(|place| format!("  {}/ ({})", place.path, place.name))
        .collect();
    format!(
        "rotproof create would make these layers, and makes them only with --yes:\n{}\nDeclare in absent in \
         {DECLARATION} the ones this project does not have, then run `rotproof create --yes`. Nothing was written.",
        listed.join("\n")
    )
}

fn write(out: &dyn Writer, path: &str, text: &str, made: &mut Made) -> Result<(), String> {
    out.write(path, text).map_err(|e| format!("{path}: {e}"))?;
    made.written.push(path.to_string());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::fake::Fake;

    #[test]
    fn the_declaration_is_read_once_whether_or_not_it_lacks_a_field() {
        for (declaration, added) in [
            ("stack = \"none\"\nareas = [\"a\"]\n", 0),
            ("stack = \"python\"\nareas = [\"a\"]\n", 1),
            ("stack = \"python\"\n", 2),
        ] {
            let tree = Fake::new(&[(DECLARATION, declaration)]);
            let made = create(&tree, &tree, "project", true).unwrap();
            assert_eq!(made.added.len(), added, "{declaration}");
            assert_eq!(tree.reads_of(DECLARATION), 1, "{declaration}");
        }
    }

    #[test]
    fn the_entry_files_a_unit_declares_are_made_from_the_layouts_text_and_never_over_one_that_is_there()
     {
        for (stack, path, unit_path) in [
            ("python", "bin/run.py", "app"),
            ("typescript", "apps/web/src/main.tsx", "packages/app/src"),
            ("rust", "apps/cli/src/main.rs", "crates/app"),
        ] {
            let declaration = format!(
                "stack = \"{stack}\"\nareas = [\"a\"]\nui = \"none\"\n\n[[unit]]\nname = \"app\"\npath = \"{unit_path}\"\n\
                 entry = [\"{path}\"]\nabsent = [\"application\", \"protocol\", \"domain\"]\n\n[[unit]]\nname = \"lib\"\n\
                 path = \"{}\"\n",
                unit_path.replace("app", "lib")
            );
            let text = layout(stack, "none")
                .unwrap()
                .entry_file()
                .unwrap()
                .to_string();
            // Made once, with the layout's text whatever the entry is named, and by `create` alone: an upgrade makes
            // no code
            let tree = Fake::new(&[(DECLARATION, &declaration)]);
            refresh(&tree, &tree, "project").unwrap();
            assert!(tree.text(path).is_none(), "{stack}");
            let made = create(&tree, &tree, "project", true).unwrap();
            assert_eq!(tree.text(path).as_deref(), Some(text.as_str()), "{stack}");
            assert!(made.written.iter().any(|w| w == path), "{stack}");
            let tree = Fake::new(&[(DECLARATION, &declaration), (path, "mine\n")]);
            create(&tree, &tree, "project", true).unwrap();
            assert_eq!(tree.text(path).as_deref(), Some("mine\n"), "{stack}");
        }
    }

    const UNITS: &str = "stack = \"rust\"\nareas = [\"a\"]\nui = \"none\"\n\n[[unit]]\nname = \"billing\"\n\
                         path = \"crates/billing\"\nabsent = [\"application\", \"infrastructure\", \"protocol\"]\n";

    #[test]
    fn the_workspace_of_a_project_that_declares_units_lists_the_crates_it_has() {
        let tree = Fake::new(&[(DECLARATION, UNITS)]);
        let made = create(&tree, &tree, "project", true).unwrap();
        assert!(made.changed.is_empty());
        // A workspace and no package: the entry files are the units'
        assert_eq!(
            tree.text("Cargo.toml").as_deref(),
            Some(
                "[workspace]\nmembers = [\n    \"crates/billing/composition\",\n    \"crates/billing/domain\",\n]\n\
                 resolver = \"3\"\n"
            )
        );
        assert!(tree.text("src/main.rs").is_none());
        // A crate that is not there is not listed, and one that is, is, whoever made it
        let tree = Fake::new(&[
            (DECLARATION, UNITS),
            ("crates/billing/domain/Cargo.toml", "[package]\n"),
        ]);
        refresh(&tree, &tree, "project").unwrap();
        assert!(
            tree.text("Cargo.toml")
                .unwrap()
                .contains("members = [\n    \"crates/billing/domain\",\n]")
        );
    }

    #[test]
    fn a_workspace_that_is_there_is_changed_only_for_a_crate_it_does_not_name_and_it_is_said() {
        let older = "[workspace]\nmembers = [\"crates/*\"]\n";
        let with =
            |declaration: &str| Fake::new(&[(DECLARATION, declaration), ("Cargo.toml", older)]);
        // No unit: the glob matches what is made
        let tree = with("stack = \"rust\"\nareas = [\"a\"]\nui = \"none\"\n");
        let made = create(&tree, &tree, "project", true).unwrap();
        assert!(made.changed.is_empty() && tree.text("Cargo.toml").as_deref() == Some(older));
        // No unit, and the workspace Rotproof writes now, which names its crates: a layer made later is added to it
        let tree = Fake::new(&[(
            DECLARATION,
            "stack = \"rust\"\nareas = [\"a\"]\nui = \"none\"\nabsent = [\"application\"]\n",
        )]);
        create(&tree, &tree, "project", true).unwrap();
        assert!(
            !tree
                .text("Cargo.toml")
                .unwrap()
                .contains("\"crates/application\"")
        );
        let declaration = "stack = \"rust\"\nareas = [\"a\"]\nui = \"none\"\n";
        tree.write(DECLARATION, declaration).unwrap();
        let made = create(&tree, &tree, "project", true).unwrap();
        assert_eq!(
            made.changed,
            [(
                "Cargo.toml".to_string(),
                "added to members: crates/application".to_string()
            )]
        );
        assert!(
            tree.text("Cargo.toml")
                .unwrap()
                .contains("    \"crates/application\",\n]")
        );
        // Units, and a run that makes no crate: nothing to add
        let tree = with(UNITS);
        let made = refresh(&tree, &tree, "project").unwrap();
        assert!(made.changed.is_empty() && tree.text("Cargo.toml").as_deref() == Some(older));
        // Units, and crates made: the glob would match `crates/billing`
        let made = create(&tree, &tree, "project", true).unwrap();
        assert_eq!(made.changed.len(), 2, "{:?}", made.changed);
        assert!(made.changed.iter().all(|(path, _)| path == "Cargo.toml"));
        assert!(!made.written.iter().any(|path| path == "Cargo.toml"));
        let text = tree.text("Cargo.toml").unwrap();
        assert!(
            !text.contains('*') && text.contains("crates/billing/domain"),
            "{text}"
        );
        // The crates are listed now: the next run changes nothing
        let again = create(&tree, &tree, "project", true).unwrap();
        assert!(
            again.changed.is_empty() && tree.text("Cargo.toml").as_deref() == Some(text.as_str())
        );
    }

    /// A shared unit, and an app whose entry file is `entry`, in a package at `apps/cli`
    fn with_app(entry: &str) -> String {
        format!(
            "stack = \"rust\"\nareas = [\"a\"]\nui = \"none\"\n\n[[unit]]\nname = \"core\"\npath = \"crates/core\"\n\
             shared = true\n\n[[unit]]\nname = \"cli\"\npath = \"apps/cli/crates\"\nroot = \"apps/cli\"\n\
             entry = [\"{entry}\"]\nabsent = [\"application\", \"protocol\", \"domain\"]\n"
        )
    }

    #[test]
    fn the_entry_file_of_a_rust_unit_is_made_with_the_package_that_holds_it_and_the_workspace_names_it()
     {
        let tree = Fake::new(&[(DECLARATION, &with_app("apps/cli/src/main.rs"))]);
        let made = create(&tree, &tree, "project", true).unwrap();
        assert!(made.written.iter().any(|w| w == "apps/cli/src/main.rs"));
        assert_eq!(
            tree.text("apps/cli/Cargo.toml").as_deref(),
            Some(
                "[package]\nname = \"cli\"\nversion = \"0.1.0\"\nedition = \"2024\"\npublish = false\n\n[dependencies]\n\
                 cli_composition = { path = \"crates/composition\" }\n"
            )
        );
        let workspace = tree.text("Cargo.toml").unwrap();
        assert!(workspace.contains("    \"apps/cli\",\n"), "{workspace}");
        assert!(
            workspace.contains("    \"apps/cli/crates/composition\",\n"),
            "{workspace}"
        );
        assert!(!workspace.contains("[package]"), "{workspace}");
        // The project's own package and entry file stay as they are
        let tree = Fake::new(&[
            (DECLARATION, &with_app("apps/cli/src/main.rs")),
            ("apps/cli/src/main.rs", "fn main() {}\n"),
            ("apps/cli/Cargo.toml", "[package]\nname = \"mine\"\n"),
        ]);
        create(&tree, &tree, "project", true).unwrap();
        assert_eq!(
            tree.text("apps/cli/Cargo.toml").as_deref(),
            Some("[package]\nname = \"mine\"\n")
        );
        // An upgrade makes no code, and no package for it
        let tree = Fake::new(&[(DECLARATION, &with_app("apps/cli/src/main.rs"))]);
        refresh(&tree, &tree, "project").unwrap();
        assert!(tree.text("apps/cli/Cargo.toml").is_none());
    }

    #[test]
    fn an_entry_file_with_no_place_for_its_package_stops_a_run_before_it_writes_anything() {
        let declaration = with_app("apps/cli/main.rs");
        let tree = Fake::new(&[(DECLARATION, &declaration)]);
        let why = create(&tree, &tree, "project", true).unwrap_err();
        assert!(
            why.starts_with(&format!(
                "{DECLARATION}: unit \"cli\" declares entry \"apps/cli/main.rs\": a Rust entry file is <dir>/src/main.rs"
            )),
            "{why}"
        );
        assert!(tree.text("Cargo.toml").is_none() && tree.text("docs/log.md").is_none());
        // An entry file that is there is the project's, wherever it is
        let tree = Fake::new(&[
            (DECLARATION, &declaration),
            ("apps/cli/main.rs", "fn main() {}\n"),
        ]);
        create(&tree, &tree, "project", true).unwrap();
        assert!(tree.text("apps/cli/Cargo.toml").is_none());
    }
}
