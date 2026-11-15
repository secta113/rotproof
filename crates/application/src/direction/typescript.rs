//! The direction check of a TypeScript project, read from the tree.

use std::io;

use super::{Direction, entry_files, layer_files, misread};
use crate::code::lands;
use crate::tree::{read_code, resolved};
use domain::code::is_source;
use domain::direction::{edge, judged, place_of};
use domain::layers::{Layout, Place};
use protocol::outbound::Parsers;
use protocol::outbound::Tree;

/// The direction check of a TypeScript project: every import of a source file in a layer, resolved to a path
/// (`typescript.rs`), judged by the place that path sits in.
pub(super) fn typescript(
    tree: &dyn Tree,
    parsers: &dyn Parsers,
    layout: &Layout,
    places: &[&Place],
    entries: &[Place],
) -> io::Result<Direction> {
    let mut forbidden = Vec::new();
    let mut edges = Vec::new();
    let aliases = parsers.typescript_aliases(tree)?;
    let mut found: Vec<String> = aliases
        .problems
        .iter()
        .map(|why| format!("{why}; imports through it are not checked"))
        .collect();
    let mut files = layer_files(tree, layout, places, str::to_string)?;
    files.extend(entry_files(tree, entries));
    for (path, from) in files {
        if !is_source(&path) {
            continue;
        }
        let Some(source) = read_code(tree, &path, "imports", &mut found)? else {
            continue;
        };
        let read = parsers.typescript(&source, &path);
        if let Some((line, why)) = read.error {
            found.push(misread(&path, "TypeScript", line, &why));
        }
        for (line, specifier) in read.imports {
            let Some(target) = lands(tree, &aliases, &path, &specifier) else {
                continue;
            };
            let target = resolved(tree, &target);
            let Some(to) = place_of(&target, places) else {
                continue;
            };
            forbidden.extend(judged(
                &path,
                line,
                &specifier,
                &format!("imports {specifier} ({target})"),
                from,
                to,
            ));
            edges.extend(edge(&path, line, &specifier, from, to));
        }
    }
    Ok(Direction {
        problems: found,
        forbidden,
        edges,
    })
}
