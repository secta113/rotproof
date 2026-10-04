//! `rotproof create`: make what `.config/rotproof.toml` declares and the tree does not have yet.
//!
//! - Each layer of the stack's layout that is neither declared absent nor present (its path exists) is made, with its
//!   files. A present layer is the project's, and nothing in it is touched.
//!   A repository that keeps records only (`stack = "none"`) has no layers to make.
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
//! It never overwrites a file it does not generate, apart from adding those fields, and never moves or deletes one. It runs when a project starts, and
//! again when its declaration is changed on purpose; it never runs by itself, so a layer removed by mistake fails the
//! check instead of coming back.

use std::collections::BTreeSet;

use toml_edit::{Array, DocumentMut, Item, Value};
use yaml_rust2::Yaml;

use crate::bundle::{Bundle, LOG, in_docs};

use crate::application::layers::declaration;
use crate::application::tree::{exactly, read_text};
use crate::domain::hook::SETTINGS;
use crate::domain::layers::{ADDED, DECLARATION, Declaration, Declared, MISSING};
use crate::domain::project::{GUIDE, guide, project_files};
use crate::domain::tree::{Tree, Writer};
use utils::frontmatter::split;

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

/// Make what is missing in `tree`, writing through `out`, for the project named `name` (its root directory's). `Err`
/// is a declaration that cannot be read, or a file that cannot be written.
pub fn create(tree: &dyn Tree, out: &dyn Writer, name: &str) -> Result<Made, String> {
    let mut made = Made::default();
    let declared = match declaration(tree).map_err(|e| e.to_string())? {
        None => return Err(MISSING.into()),
        Some(Err(why)) => match complete(tree)? {
            // Written only once the completed declaration reads and fits its stack, so a declaration that fails for
            // another reason is left as it was
            Some((text, added)) => {
                let declaration: Declaration =
                    toml::from_str(&text).map_err(|e| format!("{DECLARATION}: {}", e.message()))?;
                let declared = Declared::new(declaration).map_err(|found| found.join("\n"))?;
                write(out, DECLARATION, &text, &mut made)?;
                made.added = added;
                declared
            }
            None => return Err(format!("{DECLARATION}: {why}")),
        },
        Some(Ok(declaration)) => Declared::new(declaration).map_err(|found| found.join("\n"))?,
    };
    for place in &declared.places {
        // A level whose layer is declared absent is absent too. Otherwise its layer was made just before it
        if declared.is_absent(place) || tree.found(&place.path).is_some() {
            continue;
        }
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

/// The declaration with every field of [`ADDED`] it lacks, each under its comment, and the fields added as
/// `name = value`. `None` when it lacks none, or is not TOML: then its own error stands.
///
/// The comments and the values already there are kept. A value that is present is never changed.
fn complete(tree: &dyn Tree) -> Result<Option<(String, Vec<String>)>, String> {
    let Ok((path, _)) = exactly(tree, DECLARATION) else {
        return Ok(None);
    };
    let text = read_text(tree, &path).map_err(|e| format!("{DECLARATION}: {e}"))?;
    let Ok(mut document) = text.parse::<DocumentMut>() else {
        return Ok(None);
    };
    let mut added = Vec::new();
    for field in &ADDED {
        if document.contains_key(field.name) {
            continue;
        }
        let value = match field.name {
            "areas" => record_tags(tree)?,
            name => {
                unreachable!("every field Rotproof adds has a rule for its first value: {name}")
            }
        };
        let mut array = Array::new();
        array.extend(value.iter().map(String::as_str));
        document.insert(field.name, Item::Value(Value::Array(array)));
        let mut key = document
            .key_mut(field.name)
            .expect("the field was inserted just before");
        key.leaf_decor_mut()
            .set_prefix(format!("\n{}{}", field.comment, field.first_value));
        added.push(format!("{} = {}", field.name, document[field.name]));
    }
    if added.is_empty() {
        return Ok(None);
    }
    // In the line endings the project wrote: `read_text` gave every line `\n`, and a declaration checked out with
    // CRLF would otherwise change on every line, not only where a field was added
    let raw = tree
        .read(&path)
        .map_err(|e| format!("{DECLARATION}: {e}"))?;
    let text = document.to_string();
    Ok(Some((
        if raw.contains("\r\n") {
            text.replace('\n', "\r\n")
        } else {
            text
        },
        added,
    )))
}

/// Every tag the backlog items and specs use, sorted by name: the first value of `areas`, so the records that fit their
/// one-area rule keep fitting once the field exists. A document that cannot be read is left to `rotproof check`.
fn record_tags(tree: &dyn Tree) -> Result<Vec<String>, String> {
    let bundle = Bundle::new(tree, Vec::new());
    let mut tags = BTreeSet::new();
    for folder in ["backlog", "specs", "knowledge"] {
        if tree.found(&in_docs(folder)) != Some(true) {
            continue;
        }
        let docs = bundle.read_folder(folder).map_err(|e| e.to_string())?;
        for text in docs.values() {
            let Ok((meta, _)) = split(text) else {
                continue;
            };
            let kind = meta
                .get(&Yaml::String("type".into()))
                .and_then(Yaml::as_str);
            if !matches!(kind, Some("Backlog Item" | "Spec" | "Knowledge")) {
                continue;
            }
            match meta.get(&Yaml::String("tags".into())) {
                Some(Yaml::Array(list)) => {
                    tags.extend(list.iter().filter_map(Yaml::as_str).map(String::from))
                }
                Some(Yaml::String(tag)) => {
                    tags.insert(tag.clone());
                }
                _ => {}
            }
        }
    }
    Ok(tags.into_iter().collect())
}

fn write(out: &dyn Writer, path: &str, text: &str, made: &mut Made) -> Result<(), String> {
    out.write(path, text).map_err(|e| format!("{path}: {e}"))?;
    made.written.push(path.to_string());
    Ok(())
}
