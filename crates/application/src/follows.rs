//! What a knowledge document follows, read from the tree and hashed as it is now, against the hashes the document
//! pinned (`follows.rs` in `domain`).

use std::collections::BTreeMap;

use crate::bundle::Bundle;
use crate::tree::{exactly, read_text, read_written};
use domain::bundle::in_docs;
use domain::definitions::selected;
use domain::follows::content_hash;
use domain::follows::{
    Followed, added, directory_hash, document_name, drifted, followed, language_of, repinned,
};
use domain::schema::{Knowledge, KnowledgeDoc, Status, knowledge_doc};
use protocol::inbound::{Drift, Pinned};
use protocol::outbound::Parsers;
use protocol::outbound::{Tree, Writer};

/// The hash of what `key` names as it is now: `Ok(None)` when it is not there, `Err` when it cannot be read. Every
/// file is read as `read_text` reads it, so a checkout with CRLF or a byte order mark hashes as one with LF, a
/// definition too, as it is cut out of that text.
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
        Followed::Definition {
            file,
            language,
            name,
        } => {
            let Some(source) = file_text(&file)? else {
                return Ok(None);
            };
            let outline = parsers
                .outline(language, &source, &file)
                .map_err(|(line, why)| {
                    format!(
                        "{file} cannot be parsed, so its definitions cannot be read: line {line}: {why}"
                    )
                })?;
            Ok(selected(&source, &outline, &name).map(|text| content_hash(&text)))
        }
        Followed::Directory(dir) => {
            if !matches!(exactly(tree, &dir), Ok((_, true))) {
                return Ok(None);
            }
            let paths = tree
                .files(&dir)
                .map_err(|e| format!("{dir}/ cannot be read: {e}"))?;
            let mut files = Vec::new();
            // Every file Rotproof reads as code, whatever the stack of the project: a root that holds Rust under
            // `stack = "none"` follows a directory of it
            for path in paths.into_iter().filter(|p| language_of(p).is_some()) {
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
/// be read is left for a person. A document keeps its line endings and its byte order mark
/// ([`Written`](domain::tree::Written)). The file name of each document written.
pub fn repin(tree: &dyn Tree, out: &dyn Writer, drifts: &[Drift]) -> Result<Vec<String>, String> {
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
        let (text, was) = read_written(tree, &path).map_err(|e| format!("{path}: {e}"))?;
        let pinned = repinned(&text, &pins).map_err(|why| format!("{path}: {why}"))?;
        out.write(&path, &was.restore(&pinned))
            .map_err(|e| format!("{path}: {e}"))?;
        written.push(doc.to_string());
    }
    Ok(written)
}

/// Pin each of `keys` in the knowledge document `document` names (`knowledge/api.md` or `docs/knowledge/api.md`) at
/// its hash now, through `out`: a pinned key gets its new hash, a key not in `follows` is added. All of them or none:
/// when a key is not there or cannot be read, nothing is written. A deprecated document no longer holds, so it is not
/// pinned. The document keeps its line endings and its byte order mark, as in [`repin`].
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
    let (text, was) = read_written(tree, &path).map_err(|e| format!("{path}: {e}"))?;
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
    let mut written = false;
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
        out.write(&path, &was.restore(&new))
            .map_err(|e| format!("{path}: {e}"))?;
        written = true;
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
        assert_eq!(written, ["model.md"]);
        let text = tree.text(DOC).unwrap();
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
    fn a_file_written_with_crlf_and_a_byte_order_mark_hashes_as_its_lf() {
        let tree = project(&[]);
        let keys = ["domain/model.py", "domain/model.py::play", "domain/"];
        let hashes = |tree: &Fake| -> Vec<Option<String>> {
            keys.iter()
                .map(|key| current(tree, &Readers, key).unwrap())
                .collect()
        };
        let lf = hashes(&tree);
        assert!(lf.iter().all(Option::is_some), "{lf:?}");
        tree.write("domain/model.py", "\u{feff}def play():\r\n    return 1\r\n")
            .unwrap();
        assert_eq!(hashes(&tree), lf);
    }

    #[test]
    fn a_definition_in_a_typescript_file_hashes_on_its_own_text() {
        let tree = project(&[]);
        let source = "export function a(x: string): void;\nexport function a(x: number): void;\n\
                      export function a(x: any) { return 1; }\nexport function b() { return 1; }\n";
        tree.write("domain/api.ts", source).unwrap();
        let now = |tree: &Fake, key: &str| current(tree, &Readers, key).unwrap();
        let (a, b, file) = (
            now(&tree, "domain/api.ts::a"),
            now(&tree, "domain/api.ts::b"),
            now(&tree, "domain/api.ts"),
        );
        assert!(a.is_some() && b.is_some() && a != b);
        assert_eq!(now(&tree, "domain/api.ts::c"), None);

        // A change in another definition fails the file, not this definition
        tree.write(
            "domain/api.ts",
            &source.replace("b() { return 1", "b() { return 2"),
        )
        .unwrap();
        assert_eq!(now(&tree, "domain/api.ts::a"), a);
        assert_ne!(now(&tree, "domain/api.ts::b"), b);
        assert_ne!(now(&tree, "domain/api.ts"), file);

        // Every piece of a name counts: the implementation after the overloads is part of its definition
        tree.write("domain/api.ts", &source.replace("x: any", "x: unknown"))
            .unwrap();
        assert_ne!(now(&tree, "domain/api.ts::a"), a);
        assert_eq!(now(&tree, "domain/api.ts::b"), b);
    }

    #[test]
    fn a_typescript_file_the_parser_gives_up_on_cannot_be_read_and_is_not_a_definition_gone() {
        let tree = project(&[]);
        // The definition above the error is not read either: oxc gives an empty program
        tree.write("domain/api.ts", "export function a() {}\nconst = ;\n")
            .unwrap();
        let why = current(&tree, &Readers, "domain/api.ts::a").unwrap_err();
        assert!(
            why.starts_with(
                "domain/api.ts cannot be parsed, so its definitions cannot be read: line 2:"
            ),
            "{why}"
        );
        // The file as a whole is still followed
        assert!(current(&tree, &Readers, "domain/api.ts").unwrap().is_some());
    }

    #[test]
    fn a_directory_is_hashed_on_every_code_file_under_it_whatever_the_stack() {
        // The project declares `stack = "none"`: its directories still cover the code of every language
        let tree = project(&[]);
        tree.write("domain/lib.rs", "pub fn a() {}\n").unwrap();
        tree.write("domain/view.TSX", "export const V = 1;\n")
            .unwrap();
        tree.write("domain/Old.PY", "x = 1\n").unwrap();
        tree.write("domain/notes.md", "notes\n").unwrap();
        let now = |tree: &Fake| current(tree, &Readers, "domain/").unwrap().unwrap();
        let before = now(&tree);

        // What is not code is not read
        tree.write("domain/notes.md", "other notes\n").unwrap();
        tree.write("domain/data.json", "{}\n").unwrap();
        assert_eq!(now(&tree), before);
        // Each language is
        for (path, text) in [
            ("domain/lib.rs", "pub fn a() { 1; }\n"),
            ("domain/view.TSX", "export const V = 2;\n"),
            ("domain/Old.PY", "x = 2\n"),
            ("domain/model.py", "def play():\n    return 2\n"),
            ("domain/new.mjs", "export {};\n"),
        ] {
            let was = tree.text(path);
            tree.write(path, text).unwrap();
            assert_ne!(now(&tree), before, "{path}");
            match was {
                Some(was) => tree.write(path, &was).unwrap(),
                None => tree.remove(path).unwrap(),
            }
            assert_eq!(now(&tree), before, "{path}");
        }
    }

    #[test]
    fn a_rust_type_is_hashed_with_every_impl_block_of_it() {
        let tree = project(&[]);
        let source = "/// A table.\n#[derive(Debug)]\npub struct Table {\n    rows: u8,\n}\n\nimpl Table {\n    \
                      pub fn add(&self) -> u8 {\n        1\n    }\n}\n\nstruct Other;\n\nimpl Default for Table \
                      {\n    fn default() -> Self {\n        Table { rows: 0 }\n    }\n}\n";
        tree.write("domain/table.rs", source).unwrap();
        let now = |tree: &Fake, key: &str| current(tree, &Readers, key).unwrap();
        let (table, add, other) = (
            now(&tree, "domain/table.rs::Table"),
            now(&tree, "domain/table.rs::Table.add"),
            now(&tree, "domain/table.rs::Other"),
        );
        assert!(table.is_some() && add.is_some() && other.is_some());
        assert_eq!(now(&tree, "domain/table.rs::Missing"), None);

        // A change in the trait impl fails the type, not the method of the other block or the other type
        tree.write("domain/table.rs", &source.replace("rows: 0", "rows: 9"))
            .unwrap();
        assert_ne!(now(&tree, "domain/table.rs::Table"), table);
        assert_eq!(now(&tree, "domain/table.rs::Table.add"), add);
        assert_eq!(now(&tree, "domain/table.rs::Other"), other);

        // A change in the doc comment, which is an attribute, fails the type
        tree.write("domain/table.rs", &source.replace("A table.", "A grid."))
            .unwrap();
        assert_ne!(now(&tree, "domain/table.rs::Table"), table);
        assert_eq!(now(&tree, "domain/table.rs::Table.add"), add);

        // A comment above it is not part of it
        tree.write("domain/table.rs", &format!("// Note.\n{source}"))
            .unwrap();
        assert_eq!(now(&tree, "domain/table.rs::Table"), table);
    }

    #[test]
    fn a_python_name_defined_twice_is_hashed_on_all_its_pieces() {
        let tree = project(&[]);
        let getter = "@property\n    def size(self):\n        return 1";
        let setter = "@size.setter\n    def size(self, value):\n        pass";
        let source = |setter: &str| {
            format!("class C:\n    {getter}\n\n    {setter}\n\n\ndef other():\n    pass\n")
        };
        tree.write("domain/model.py", &source(setter)).unwrap();
        let size = || current(&tree, &Readers, "domain/model.py::C.size").unwrap();
        // The pieces in the order they appear, joined with a line break
        assert_eq!(size(), Some(content_hash(&format!("{getter}\n{setter}"))));

        // A change in the setter fails the key, which the getter alone missed
        let was = size();
        tree.write(
            "domain/model.py",
            &source("@size.setter\n    def size(self, value):\n        self._v = value"),
        )
        .unwrap();
        assert_ne!(size(), was);
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
        assert!(done.written);
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
        assert!(!again.written);
        assert_eq!(tree.text(DOC).unwrap(), text);
    }

    #[test]
    fn a_pin_and_a_re_pin_keep_the_line_endings_and_the_byte_order_mark() {
        let keys = |keys: &[&str]| keys.iter().map(|k| k.to_string()).collect::<Vec<_>>();
        for (crlf, bom) in [(true, true), (true, false), (false, true), (false, false)] {
            let tree = project(&[("domain/model.py::play", "00000000")]);
            let lf = tree.text(DOC).unwrap();
            let as_written = |text: &str| {
                let text = if crlf {
                    text.replace('\n', "\r\n")
                } else {
                    text.to_string()
                };
                if bom { format!("\u{feff}{text}") } else { text }
            };
            tree.write(DOC, &as_written(&lf)).unwrap();
            let done = pin(
                &tree,
                &tree,
                &Readers,
                "knowledge/model.md",
                &keys(&["domain/model.py::play", "domain/"]),
            )
            .unwrap();
            // Only the hash changed and the key was added, and every line ends as before
            let pinned = lf.replace("00000000", &done.keys[0].2).replace(
                "---\n\n#",
                &format!("  domain/: \"{}\"\n---\n\n#", done.keys[1].2),
            );
            assert_eq!(
                tree.text(DOC).unwrap(),
                as_written(&pinned),
                "crlf {crlf}, bom {bom}"
            );
            // Written, as the text shows above
            assert!(done.written);

            tree.write("domain/model.py", "def play():\n    return 2\n")
                .unwrap();
            let drifts = changed(&tree, &Readers).unwrap();
            let written = repin(&tree, &tree, &drifts).unwrap();
            let mut repinned = pinned.clone();
            for d in &drifts {
                let Ok(Some(now)) = &d.now else {
                    panic!("{drifts:?}")
                };
                repinned = repinned.replace(&d.pinned, now);
            }
            assert_ne!(repinned, pinned);
            assert_eq!(
                tree.text(DOC).unwrap(),
                as_written(&repinned),
                "crlf {crlf}, bom {bom}"
            );
            assert_eq!(written, ["model.md"]);
        }
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
