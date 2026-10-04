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
use std::fs;
use std::path::Path;

use toml_edit::{Array, DocumentMut, Item, Value};
use yaml_rust2::Yaml;

use crate::bundle::{Bundle, LOG};
use crate::frontmatter::split;
use crate::hook::SETTINGS;
use crate::layers::{ADDED, DECLARATION, Declaration, Declared, MISSING, declaration};
use crate::project::{GUIDE, guide, project_files};
use crate::source::{exactly, read_source, relative_path};

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

/// Make what is missing at `root`. `Err` is a declaration that cannot be read, or a file that cannot be written.
pub fn create(root: &Path) -> Result<Made, String> {
    let mut made = Made::default();
    let declared = match declaration(root).map_err(|e| e.to_string())? {
        None => return Err(MISSING.into()),
        Some(Err(why)) => match complete(root)? {
            // Written only once the completed declaration reads and fits its stack, so a declaration that fails for
            // another reason is left as it was
            Some((text, added)) => {
                let declaration: Declaration =
                    toml::from_str(&text).map_err(|e| format!("{DECLARATION}: {}", e.message()))?;
                let declared = Declared::new(declaration).map_err(|found| found.join("\n"))?;
                write(root, DECLARATION, &text, &mut made)?;
                made.added = added;
                declared
            }
            None => return Err(format!("{DECLARATION}: {why}")),
        },
        Some(Ok(declaration)) => Declared::new(declaration).map_err(|found| found.join("\n"))?,
    };
    for place in &declared.places {
        // A level whose layer is declared absent is absent too. Otherwise its layer was made just before it
        if declared.is_absent(place) || root.join(&place.path).exists() {
            continue;
        }
        for (path, text) in &place.files {
            write(root, path, text, &mut made)?;
        }
    }

    let bundle = Bundle::new(root, declared.declaration.areas.clone());
    for folder in ["backlog", "specs", "knowledge"] {
        let dir = bundle.docs.join(folder);
        fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    if !bundle.docs.join("log.md").exists() {
        write(root, "docs/log.md", LOG, &mut made)?;
    }
    let guide = guide(&declared.declaration.stack, declared.layout.as_ref());
    if read_source(&root.join(GUIDE)).ok().as_ref() != Some(&guide) {
        write(root, GUIDE, &guide, &mut made)?;
    }
    let name = project_name(root)?;
    let (files, not_written) = project_files(&declared, &name);
    let once = SETTINGS
        .iter()
        .map(|(path, text)| (*path, text.to_string()))
        .chain(files);
    for (path, text) in once {
        if !root.join(path).exists() {
            write(root, path, &text, &mut made)?;
        }
    }
    made.not_written = not_written;
    let (files, problems) = bundle.expected().map_err(|e| e.to_string())?;
    for (path, text) in files {
        if read_source(&path).ok().as_ref() != Some(&text) {
            write(root, &relative_path(&path, root), &text, &mut made)?;
        }
    }
    made.left_out = problems.into_iter().collect();
    Ok(made)
}

/// The declaration with every field of [`ADDED`] it lacks, each under its comment, and the fields added as
/// `name = value`. `None` when it lacks none, or is not TOML: then its own error stands.
///
/// The comments and the values already there are kept. A value that is present is never changed.
fn complete(root: &Path) -> Result<Option<(String, Vec<String>)>, String> {
    let Ok(path) = exactly(root, DECLARATION) else {
        return Ok(None);
    };
    let text = read_source(&path).map_err(|e| format!("{DECLARATION}: {e}"))?;
    let Ok(mut document) = text.parse::<DocumentMut>() else {
        return Ok(None);
    };
    let mut added = Vec::new();
    for field in &ADDED {
        if document.contains_key(field.name) {
            continue;
        }
        let value = match field.name {
            "areas" => record_tags(root)?,
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
    // In the line endings the project wrote: `read_source` gave every line `\n`, and a declaration checked out with
    // CRLF would otherwise change on every line, not only where a field was added
    let raw = fs::read_to_string(&path).map_err(|e| format!("{DECLARATION}: {e}"))?;
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
fn record_tags(root: &Path) -> Result<Vec<String>, String> {
    let bundle = Bundle::new(root, Vec::new());
    let mut tags = BTreeSet::new();
    for folder in ["backlog", "specs", "knowledge"] {
        if !bundle.docs.join(folder).is_dir() {
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

/// The project's name: the name of its root directory, which `--root .` gives only once resolved.
fn project_name(root: &Path) -> Result<String, String> {
    let full = root
        .canonicalize()
        .map_err(|e| format!("{}: {e}", root.display()))?;
    Ok(full.file_name().map_or_else(
        // The root of a drive has no name of its own
        || "project".to_string(),
        |name| name.to_string_lossy().into_owned(),
    ))
}

fn write(root: &Path, path: &str, text: &str, made: &mut Made) -> Result<(), String> {
    let full = root.join(path);
    if let Some(dir) = full.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    fs::write(&full, text).map_err(|e| format!("{path}: {e}"))?;
    made.written.push(path.to_string());
    Ok(())
}
