//! The direction check of a Python project, read from the tree.

use std::io;

use super::{Direction, entry_files, layer_files, misread};
use crate::tree::{read_code, resolved};
use domain::code::module_parts;
use domain::direction::{judged, place_of};
use domain::layers::{Layout, Place};
use protocol::outbound::Parsers;
use protocol::outbound::Tree;

/// The direction check of a Python project: every import of a `.py` file in a layer, and of the entry file, as the
/// module it names.
pub(super) fn python(
    tree: &dyn Tree,
    parsers: &dyn Parsers,
    layout: &Layout,
    places: &[&Place],
    entries: &[Place],
) -> io::Result<Direction> {
    let mut found = Vec::new();
    let mut forbidden = Vec::new();
    let mut files = layer_files(tree, layout, places, |path| module_parts(path).join("/"))?;
    files.extend(entry_files(tree, entries));
    for (path, from) in files {
        let Some(source) = read_code(tree, &path, "imports", &mut found)? else {
            continue;
        };
        let read = parsers.python(&source, &path);
        if let Some((line, why)) = read.error {
            found.push(misread(&path, "Python", line, &why));
        }
        for (line, module) in read.imports {
            let Some(to) = place_of(&resolved(tree, &module.join("/")), places) else {
                continue;
            };
            let dotted = module.join(".");
            forbidden.extend(judged(
                &path,
                line,
                &dotted,
                &format!("imports {dotted}"),
                from,
                to,
            ));
        }
    }
    Ok(Direction {
        problems: found,
        forbidden,
    })
}
