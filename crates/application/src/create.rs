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
use crate::layers::declaration;
use crate::tree::{exactly, read_text, read_written};
use domain::bundle::{LOG, in_docs, record_tags as tags_in};
use domain::hook::SETTINGS;
use domain::layers::{
    DECLARATION, Declared, MISSING, Place, completed, lacking, parse_declaration, unreadable,
};
use domain::project::{GUIDE, guide, project_files};
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
    let dir = in_docs("knowledge");
    out.make_dir(&dir).map_err(|e| format!("{dir}: {e}"))?;
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
    let (text, was) = read_written(tree, &path).map_err(|e| unreadable(&e.to_string()))?;
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
    // As the project wrote it: the text was read with every line ending `\n`, and a declaration checked out with CRLF
    // would otherwise change on every line, not only where a field was added
    let (text, added) = completed(&text, &values);
    Ok(Some((was.restore(&text), added)))
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
