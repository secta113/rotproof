//! What a knowledge document follows, read from the tree and hashed as it is now, against the hashes the document
//! pinned (`follows.rs` in `domain`).

use std::collections::BTreeMap;

use crate::bundle::Bundle;
use crate::tree::{exactly, read_text};
use domain::bundle::in_docs;
use domain::follows::{
    Followed, added, directory_hash, document_name, drifted, followed, repinned,
};
use domain::records::content_hash;
use domain::schema::{Knowledge, KnowledgeDoc, Status, knowledge_doc};
use protocol::inbound::{Drift, Pinned};
use protocol::outbound::Parsers;
use protocol::outbound::{Tree, Writer};

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

/// Pin each of `keys` in the knowledge document `document` names (`knowledge/api.md` or `docs/knowledge/api.md`) at
/// its hash now, through `out`: a pinned key gets its new hash, a key not in `follows` is added. All of them or none:
/// when a key is not there or cannot be read, nothing is written. A deprecated document no longer holds, so it is not
/// pinned.
pub fn pin(
    tree: &dyn Tree,
    out: &dyn Writer,
    parsers: &dyn Parsers,
    document: &str,
    keys: &[String],
) -> Result<Pinned, String> {
    let name = document_name(document)?;
    let path = in_docs(&format!("knowledge/{name}"));
    let areas = crate::layers::areas(tree).map_err(|e| e.to_string())??;
    let folder = Bundle::new(tree, areas)
        .read_knowledge()
        .map_err(|e| e.to_string())?;
    let doc = match folder.documents.get(&name) {
        Some((doc, _)) => doc,
        None => {
            return Err(match folder.problems.get(&name) {
                Some(why) => format!("{path} cannot be pinned until it is fixed: {why}"),
                None if folder.guides.contains_key(&name) => {
                    format!("{path} is a guide, which follows nothing")
                }
                None => format!("{path} is not there"),
            });
        }
    };
    if doc.status == Status::Deprecated {
        return Err(format!(
            "{path} is deprecated: it no longer holds, so what it followed is not read"
        ));
    }
    let mut unique: Vec<&str> = Vec::new();
    for key in keys {
        if !unique.contains(&key.as_str()) {
            unique.push(key);
        }
    }
    let mut pinned = Vec::new();
    let mut failed = Vec::new();
    for key in unique {
        match current(tree, parsers, key) {
            Ok(Some(now)) => {
                let before = doc
                    .follows
                    .iter()
                    .find(|(k, _)| k == key)
                    .map(|(_, h)| h.clone());
                pinned.push((key.to_string(), before, now));
            }
            Ok(None) => failed.push(format!("{key}: not there")),
            Err(why) => failed.push(format!("{key}: {why}")),
        }
    }
    if !failed.is_empty() {
        return Err(format!("{path}: nothing written\n{}", failed.join("\n")));
    }
    let text = read_text(tree, &path).map_err(|e| format!("{path}: {e}"))?;
    let repins: Vec<(&str, &str, &str)> = pinned
        .iter()
        .filter_map(|(key, before, now)| {
            before
                .as_deref()
                .filter(|before| before != now)
                .map(|before| (key.as_str(), before, now.as_str()))
        })
        .collect();
    let adds: Vec<(&str, &str)> = pinned
        .iter()
        .filter(|(_, before, _)| before.is_none())
        .map(|(key, _, now)| (key.as_str(), now.as_str()))
        .collect();
    let mut written = None;
    if !repins.is_empty() || !adds.is_empty() {
        let new = repinned(&text, &repins)
            .and_then(|text| added(&text, &adds))
            .map_err(|why| format!("{path}: {why}"))?;
        // Read back as the check reads it, so a key written is a key pinned
        let reads_back = match knowledge_doc(&new) {
            Ok(KnowledgeDoc::Knowledge(doc, _)) => pinned
                .iter()
                .all(|(key, _, now)| doc.follows.iter().any(|(k, h)| k == key && h == now)),
            _ => false,
        };
        if !reads_back {
            return Err(format!(
                "{path}: nothing written, as the document would not read back with these keys pinned: edit follows by \
                 hand"
            ));
        }
        out.write(&path, &new).map_err(|e| format!("{path}: {e}"))?;
        written = Some(content_hash(&new));
    }
    Ok(Pinned {
        doc: name,
        keys: pinned,
        written,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::fake::Fake;
    use infrastructure::outbound::readers::Readers;

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

    #[test]
    fn a_pin_writes_every_key_or_none() {
        let tree = project(&[("domain/model.py::play", "00000000")]);
        let keys = |keys: &[&str]| keys.iter().map(|k| k.to_string()).collect::<Vec<_>>();
        let before = tree.text(DOC).unwrap();
        // A key not there leaves the document as it was, the key that could be pinned too
        let why = pin(
            &tree,
            &tree,
            &Readers,
            "knowledge/model.md",
            &keys(&["domain/model.py::play", "domain/gone.py"]),
        )
        .unwrap_err();
        assert!(why.contains("domain/gone.py: not there"), "{why}");
        assert_eq!(tree.text(DOC).unwrap(), before);

        // A pinned key is re-pinned and a new one added, named from the root, once though named twice
        let done = pin(
            &tree,
            &tree,
            &Readers,
            "docs/knowledge/model.md",
            &keys(&["domain/model.py::play", "domain/", "domain/"]),
        )
        .unwrap();
        assert_eq!(done.doc, "model.md");
        assert_eq!(done.keys.len(), 2, "{done:?}");
        assert_eq!(done.keys[0].1.as_deref(), Some("00000000"));
        assert_eq!(done.keys[1].1, None);
        let text = tree.text(DOC).unwrap();
        assert_eq!(done.written, Some(content_hash(&text)));
        assert!(
            text.contains(&format!("  domain/: \"{}\"\n", done.keys[1].2)),
            "{text}"
        );
        assert!(changed(&tree, &Readers).unwrap().is_empty());

        // Pinned as it is now: nothing written, and no log line to write
        let again = pin(
            &tree,
            &tree,
            &Readers,
            "knowledge/model.md",
            &keys(&["domain/"]),
        )
        .unwrap();
        assert_eq!(again.written, None);
        assert_eq!(tree.text(DOC).unwrap(), text);
    }

    #[test]
    fn a_pin_refuses_what_is_no_knowledge_document_that_holds() {
        let tree = project(&[]);
        let play = vec!["domain/model.py::play".to_string()];
        for (document, said) in [
            ("knowledge/gone.md", "is not there"),
            ("knowledge/rules.md", "is a guide"),
            (
                "domain/model.py",
                "rotproof follows takes the document first",
            ),
        ] {
            if document.contains("rules") {
                tree.write(
                    "docs/knowledge/rules.md",
                    "---\ntype: Guide\ntitle: R\ndescription: D.\n---\n\n# Rules\n",
                )
                .unwrap();
            }
            let why = pin(&tree, &tree, &Readers, document, &play).unwrap_err();
            assert!(why.contains(said), "{document}: {why}");
        }
        tree.write(
            DOC,
            "---\ntype: Knowledge\ntitle: M\ndescription: D.\ntags: [a]\nstatus: deprecated\n---\n\n# Resolution\n\n\
             Replaced.\n",
        )
        .unwrap();
        let why = pin(&tree, &tree, &Readers, "knowledge/model.md", &play).unwrap_err();
        assert!(why.contains("is deprecated"), "{why}");
        assert!(!tree.text(DOC).unwrap().contains("follows"));
    }
}
