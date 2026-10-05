//! What a knowledge document follows, read from the tree and hashed as it is now, against the hashes the document
//! pinned (`follows.rs` in `domain`).

use std::collections::BTreeMap;

use crate::bundle::Bundle;
use crate::tree::{exactly, read_text};
use domain::bundle::in_docs;
use domain::code::Parsers;
use domain::follows::{Followed, directory_hash, drifted, followed, repinned};
use domain::records::content_hash;
use domain::schema::{Knowledge, Status};
use domain::tree::{Tree, Writer};

/// The hash of what `key` names as it is now: `Ok(None)` when it is not there, `Err` when it cannot be read.
pub fn current(
    tree: &dyn Tree,
    parsers: &dyn Parsers,
    key: &str,
) -> Result<Option<String>, String> {
    let file_text = |path: &str| -> Result<Option<String>, String> {
        match exactly(tree, path) {
            Ok((found, false)) => read_text(tree, &found)
                .map(Some)
                .map_err(|e| format!("{path} cannot be read: {e}")),
            _ => Ok(None),
        }
    };
    match followed(key)? {
        Followed::File(path) => Ok(file_text(&path)?.map(|text| content_hash(&text))),
        Followed::Definition { file, name } => Ok(file_text(&file)?
            .and_then(|source| parsers.python_definition(&source, &name))
            .map(|text| content_hash(&text.replace("\r\n", "\n")))),
        Followed::Directory(dir) => {
            if !matches!(exactly(tree, &dir), Ok((_, true))) {
                return Ok(None);
            }
            let paths = tree
                .files(&dir)
                .map_err(|e| format!("{dir}/ cannot be read: {e}"))?;
            let mut files = Vec::new();
            for path in paths.into_iter().filter(|p| p.ends_with(".py")) {
                let text =
                    read_text(tree, &path).map_err(|e| format!("{path} cannot be read: {e}"))?;
                files.push((path, text));
            }
            Ok(Some(directory_hash(&files)))
        }
    }
}

/// One key of a document's `follows` whose code is not as pinned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Drift {
    /// The document's file name in `docs/knowledge/`
    pub doc: String,
    pub key: String,
    pub pinned: String,
    /// Its hash now: `None` when it is gone. `Err` when it cannot be read
    pub now: Result<Option<String>, String>,
}

/// Every key of the knowledge documents that hold (`documents`: file name -> document) whose code is not as pinned. A
/// deprecated document no longer holds, so what it followed is not read.
fn drifts(
    tree: &dyn Tree,
    parsers: &dyn Parsers,
    documents: &BTreeMap<String, Knowledge>,
) -> Vec<Drift> {
    let mut found = Vec::new();
    for (name, doc) in documents {
        if doc.status == Status::Deprecated {
            continue;
        }
        for (key, pinned) in &doc.follows {
            let now = current(tree, parsers, key);
            if now.as_ref().ok().and_then(|n| n.as_deref()) != Some(pinned.as_str()) {
                found.push(Drift {
                    doc: name.clone(),
                    key: key.clone(),
                    pinned: pinned.clone(),
                    now,
                });
            }
        }
    }
    found
}

/// Every way the knowledge documents that hold drifted from what they follow, for `rotproof check`.
pub fn drift(
    tree: &dyn Tree,
    parsers: &dyn Parsers,
    documents: &BTreeMap<String, Knowledge>,
) -> Vec<String> {
    drifts(tree, parsers, documents)
        .into_iter()
        .filter_map(|d| match &d.now {
            Ok(now) => drifted(&d.doc, &d.key, &d.pinned, now.as_deref()),
            Err(why) => Some(format!("knowledge/{} follows {}: {why}", d.doc, d.key)),
        })
        .collect()
}

/// Every key whose code is not as pinned, in the knowledge documents of the project `tree` holds, for the re-pin.
pub fn changed(tree: &dyn Tree, parsers: &dyn Parsers) -> Result<Vec<Drift>, String> {
    let areas = crate::layers::areas(tree).map_err(|e| e.to_string())??;
    let documents = Bundle::new(tree, areas)
        .read_knowledge()
        .map_err(|e| e.to_string())?
        .documents
        .into_iter()
        .map(|(name, (doc, _))| (name, doc))
        .collect();
    Ok(drifts(tree, parsers, &documents))
}

/// Write the new hash of every changed key in `drifts` into its document, through `out`. A key that is gone or cannot
/// be read is left for a person. Each document written, with its new hash for the log.
pub fn repin(
    tree: &dyn Tree,
    out: &dyn Writer,
    drifts: &[Drift],
) -> Result<Vec<(String, String)>, String> {
    let mut by_doc: BTreeMap<&str, Vec<(&str, &str, &str)>> = BTreeMap::new();
    for d in drifts {
        if let Ok(Some(now)) = &d.now {
            by_doc
                .entry(&d.doc)
                .or_default()
                .push((&d.key, &d.pinned, now));
        }
    }
    let mut written = Vec::new();
    for (doc, pins) in by_doc {
        let path = in_docs(&format!("knowledge/{doc}"));
        let text = read_text(tree, &path).map_err(|e| format!("{path}: {e}"))?;
        let pinned = repinned(&text, &pins).map_err(|why| format!("{path}: {why}"))?;
        out.write(&path, &pinned)
            .map_err(|e| format!("{path}: {e}"))?;
        written.push((doc.to_string(), content_hash(&pinned)));
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::fake::Fake;
    use infrastructure::readers::Readers;

    const DOC: &str = "docs/knowledge/model.md";

    fn project(pins: &[(&str, &str)]) -> Fake {
        let follows: String = pins
            .iter()
            .map(|(key, hash)| format!("  {key}: \"{hash}\"\n"))
            .collect();
        Fake::new(&[
            (
                ".config/rotproof.toml",
                "stack = \"none\"\nareas = [\"a\"]\n",
            ),
            ("domain/model.py", "def play():\n    return 1\n"),
            (
                DOC,
                &format!(
                    "---\ntype: Knowledge\ntitle: M\ndescription: D.\ntags: [a]\nstatus: stable\nfollows:\n\
                     {follows}---\n\n# Shape\n\nIt plays.\n"
                ),
            ),
        ])
    }

    #[test]
    fn every_changed_key_is_re_pinned_and_a_gone_one_is_left() {
        let tree = project(&[
            ("domain/model.py::play", "00000000"),
            ("domain/gone.py", "11111111"),
        ]);
        let drifts = changed(&tree, &Readers).unwrap();
        assert_eq!(drifts.len(), 2, "{drifts:?}");
        let written = repin(&tree, &tree, &drifts).unwrap();
        assert_eq!(written.len(), 1);
        assert_eq!(written[0].0, "model.md");
        let text = tree.text(DOC).unwrap();
        assert_eq!(content_hash(&text), written[0].1);
        assert!(
            !text.contains("00000000") && text.contains("11111111"),
            "{text}"
        );
        // What changed is pinned now; what is gone still drifts
        let left = changed(&tree, &Readers).unwrap();
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].key, "domain/gone.py");
        assert_eq!(left[0].now, Ok(None));
    }
}
