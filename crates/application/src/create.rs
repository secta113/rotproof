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
use crate::tree::read_text;
use domain::bundle::{LOG, in_docs, record_tags as tags_in};
use domain::declaration::{
    DECLARATION, Declared, First, MISSING, completed, fields, lacking, parse_declaration,
    unreadable,
};
use domain::guide::{GUIDE, guide};
use domain::hook::SETTINGS;
use domain::layers::{NO_UI, Place, layout, table, uis};
use domain::project::project_files;
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
/// records skeleton, the guide, the project's files written once, and the index files. Nothing is written before the
/// declaration reads and fits its stack, and before the layers to make are known to be wanted.
fn make(tree: &dyn Tree, out: &dyn Writer, name: &str, mode: Layers) -> Result<Made, String> {
    let mut made = Made::default();
    let (declared, completed) = read_declared(tree)?;
    let layers = to_make(tree, &declared, mode)?;
    if let Some((text, added)) = completed {
        write(out, DECLARATION, &text, &mut made)?;
        made.added = added;
    }
    write_layers(out, &layers, &mut made)?;
    write_records(tree, out, &mut made)?;
    write_guide(tree, out, &declared, &mut made)?;
    write_project(tree, out, &declared, name, &mut made)?;
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

/// The project's files written once, outside `docs/`, each where it is not there yet; and what the stack does not have
/// Rotproof write yet.
fn write_project(
    tree: &dyn Tree,
    out: &dyn Writer,
    declared: &Declared,
    name: &str,
    made: &mut Made,
) -> Result<(), String> {
    let (files, not_written) = project_files(declared, name);
    let once = SETTINGS
        .iter()
        .map(|(path, text)| (*path, text.to_string()))
        .chain(files);
    for (path, text) in once {
        if tree.found(path).is_none() {
            write(out, path, &text, made)?;
        }
    }
    made.not_written = not_written;
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
}
