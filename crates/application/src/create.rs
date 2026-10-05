//! `rotproof create`: make what `.config/rotproof.toml` declares and the tree does not have yet.
//!
//! - Each layer of the stack's layout that is neither declared absent nor present (its path exists) is made, with its
//!   files. A present layer is the project's, and nothing in it is touched.
//!   A repository that keeps records only (`stack = "none"`) has no layers to make.
//!   Layers are made only with `--yes`. Without it, a run that would make one lists them and writes nothing: a project
//!   that ran `rotproof create` straight after `rotproof init`, never listing in absent what it lacks, would otherwise
//!   get every layer of the stack (`ui` and its five levels in a command-line tool), and `rotproof create` never
//!   deletes them again. A run that makes no layer, as after an upgrade, needs no `--yes`.
//! - The records skeleton: the directories of `docs/`, `docs/log.md` with its title when it does not exist, and the
//!   generated files (the index files and the rules of `docs/backlog/`, `docs/specs/` and `docs/knowledge/`), which
//!   Rotproof rewrites.
//! - Rotproof's guide, `.rotproof/AGENTS.md` (`project.rs`), which Rotproof rewrites: the rules it keeps in the stack,
//!   from the version that runs.
//! - `.claude/settings.json` with the hook that runs `rotproof stop-hook` when the agent stops (`hook.rs`), when it
//!   does not exist. A project that has one already adds the hook to it by hand.
//! - The project's files (`project.rs`: `AGENTS.md`, `README.md`, the pin of Rotproof, the CI workflow, a Rust
//!   project's workspace and others), each when it does not exist. The project's name in them is its root directory's.
//!
//! - The fields the declaration lacks that Rotproof requires (`ADDED` in `layers.rs`): an upgrade of Rotproof that adds
//!   a field fails `rotproof check` until `rotproof create` runs, and then only on what the new rules find. The
//!   comments and the values already in the declaration are kept, and a value that is present is never changed.
//!
//! It never overwrites a file it does not generate, apart from adding those fields, and never moves or deletes one. It
//! runs when a project starts, and again when its declaration is changed on purpose; it never runs by itself, so a
//! layer removed by mistake fails the check instead of coming back.

use std::collections::BTreeSet;

use crate::bundle::Bundle;
use crate::layers::declaration;
use crate::tree::{exactly, read_text};
use domain::bundle::{LOG, in_docs, record_tags as tags_in};
use domain::hook::SETTINGS;
use domain::layers::{
    DECLARATION, Declared, MISSING, Place, completed, lacking, parse_declaration, unreadable,
};
use domain::project::{GUIDE, guide, project_files};
use domain::tree::{Tree, Writer};

/// What `rotproof create` did.
#[derive(Debug, Default)]
pub struct Made {
    /// The files written, from the root, with `/`: made new or rewritten with a change
    pub written: Vec<String>,
    /// Documents left out of the index files: name -> why
    pub left_out: Vec<(String, String)>,
    /// The fields added to the declaration, as `name = value`
    pub added: Vec<String>,
    /// What the stack does not have Rotproof write yet, and why
    pub not_written: Option<String>,
}

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

fn make(tree: &dyn Tree, out: &dyn Writer, name: &str, mode: Layers) -> Result<Made, String> {
    let mut made = Made::default();
    let (declared, completed) = match declaration(tree).map_err(|e| e.to_string())? {
        None => return Err(MISSING.into()),
        Some(Err(why)) => match complete(tree)? {
            // Written only once the completed declaration reads and fits its stack, so a declaration that fails for
            // another reason is left as it was
            Some((text, added)) => {
                let declaration = parse_declaration(&text).map_err(|why| unreadable(&why))?;
                let declared = Declared::new(declaration).map_err(|found| found.join("\n"))?;
                (declared, Some((text, added)))
            }
            None => return Err(unreadable(&why)),
        },
        Some(Ok(declaration)) => (
            Declared::new(declaration).map_err(|found| found.join("\n"))?,
            None,
        ),
    };
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
    if let Some((text, added)) = completed {
        write(out, DECLARATION, &text, &mut made)?;
        made.added = added;
    }
    for place in layers {
        for (path, text) in &place.files {
            write(out, path, text, &mut made)?;
        }
    }

    let bundle = Bundle::new(tree, declared.declaration.areas.clone());
    for folder in ["backlog", "specs", "knowledge"] {
        let dir = in_docs(folder);
        out.make_dir(&dir).map_err(|e| format!("{dir}: {e}"))?;
    }
    if tree.found(&in_docs("log.md")).is_none() {
        write(out, "docs/log.md", LOG, &mut made)?;
    }
    let guide = guide(&declared.declaration.stack, declared.layout.as_ref());
    if read_text(tree, GUIDE).ok().as_ref() != Some(&guide) {
        write(out, GUIDE, &guide, &mut made)?;
    }
    let (files, not_written) = project_files(&declared, name);
    let once = SETTINGS
        .iter()
        .map(|(path, text)| (*path, text.to_string()))
        .chain(files);
    for (path, text) in once {
        if tree.found(path).is_none() {
            write(out, path, &text, &mut made)?;
        }
    }
    made.not_written = not_written;
    let (files, problems) = bundle.expected().map_err(|e| e.to_string())?;
    for (path, text) in files {
        if read_text(tree, &path).ok().as_ref() != Some(&text) {
            write(out, &path, &text, &mut made)?;
        }
    }
    made.left_out = problems.into_iter().collect();
    Ok(made)
}

/// The declaration with every field of [`ADDED`](domain::layers::ADDED) it lacks, each under its comment, and
/// the fields added as `name = value`. `None` when it lacks none, or is not TOML: then its own error stands.
fn complete(tree: &dyn Tree) -> Result<Option<(String, Vec<String>)>, String> {
    let Ok((path, _)) = exactly(tree, DECLARATION) else {
        return Ok(None);
    };
    let text = read_text(tree, &path).map_err(|e| unreadable(&e.to_string()))?;
    let Some(lacking) = lacking(&text) else {
        return Ok(None);
    };
    if lacking.is_empty() {
        return Ok(None);
    }
    let mut values = Vec::new();
    for name in lacking {
        let value = match name {
            "areas" => record_tags(tree)?,
            name => {
                unreachable!("every field Rotproof adds has a rule for its first value: {name}")
            }
        };
        values.push((name, value));
    }
    // In the line endings the project wrote: `read_text` gave every line `\n`, and a declaration checked out with
    // CRLF would otherwise change on every line, not only where a field was added
    let raw = tree.read(&path).map_err(|e| unreadable(&e.to_string()))?;
    Ok(Some(completed(&text, &values, raw.contains("\r\n"))))
}

/// Every tag the backlog items, specs and knowledge documents use, sorted by name: the first value of `areas`, so the
/// records that fit their one-area rule keep fitting once the field exists.
fn record_tags(tree: &dyn Tree) -> Result<Vec<String>, String> {
    let bundle = Bundle::new(tree, Vec::new());
    let mut tags = BTreeSet::new();
    for folder in ["backlog", "specs", "knowledge"] {
        if tree.found(&in_docs(folder)) != Some(true) {
            continue;
        }
        let docs = bundle.read_folder(folder).map_err(|e| e.to_string())?;
        tags.extend(tags_in(&docs));
    }
    Ok(tags.into_iter().collect())
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
