//! `rotproof new`: a new knowledge document written with its frontmatter, read from the declaration (the areas) and
//! the tree (the names taken, and the code it follows, hashed as `rotproof follows` hashes it).

use domain::bundle::in_docs;
use domain::follows::added;
use domain::scaffold::{document, pick_area, slug_problem};
use domain::tree::as_windows_reads;
use protocol::inbound::{NewDocument, Started};
use protocol::outbound::{Parsers, Tree, Writer};

use crate::follows::current;

/// Write the document `new` describes to `docs/knowledge/<slug>.md` through `out`, and pin each key it follows at its
/// hash now, as `rotproof follows` would. All of it or none: nothing is written when the slug is not one, a file of
/// that name is there already (by Windows' reading of names too, as two names that read alike open one file there and
/// two on Linux), the area is not one the declaration lists, a key is not a form `follows` takes, names what is not
/// there, or cannot be read, or the alarm's strings cannot be written. The index is not written: the document is not
/// read until its description is, so it would be listed as left out now and rewritten again then.
pub fn start(
    tree: &dyn Tree,
    out: &dyn Writer,
    parsers: &dyn Parsers,
    new: &NewDocument,
) -> Result<Started, String> {
    slug_problem(&new.slug)?;
    let areas = crate::layers::areas(tree).map_err(|e| e.to_string())??;
    let area = pick_area(&areas, new.area.as_deref())?;
    let path = in_docs(&format!("knowledge/{}.md", new.slug));
    let names = tree
        .entries(&in_docs("knowledge"))
        .map_err(|_| "docs/knowledge/ is not there: `rotproof create` makes it".to_string())?;
    let file = format!("{}.md", new.slug);
    if let Some((taken, _)) = names
        .iter()
        .find(|(name, _)| as_windows_reads(name) == as_windows_reads(&file))
    {
        return Err(format!(
            "{path} is not written: docs/knowledge/{taken} is there already. A document is edited in place, and its slug \
             never changes: edit it, or choose another slug"
        ));
    }
    let alarm = new.alarm.then_some(new.matches.as_slice());
    if !new.alarm && !new.matches.is_empty() {
        return Err("--match is an alarm's: add --alarm, or take the strings out".into());
    }
    let mut text = document(&new.slug, &area, alarm)?;
    let mut keys: Vec<&str> = Vec::new();
    for key in &new.follows {
        if !keys.contains(&key.as_str()) {
            keys.push(key);
        }
    }
    let mut pinned: Vec<(String, String)> = Vec::new();
    let mut failed = Vec::new();
    for key in keys {
        match current(tree, parsers, key) {
            Ok(Some(now)) => pinned.push((key.to_string(), now)),
            Ok(None) => failed.push(format!("{key}: not there")),
            Err(why) => failed.push(format!("{key}: {why}")),
        }
    }
    if !failed.is_empty() {
        return Err(format!("{path}: nothing written\n{}", failed.join("\n")));
    }
    if !pinned.is_empty() {
        let adds: Vec<(&str, &str)> = pinned
            .iter()
            .map(|(key, hash)| (key.as_str(), hash.as_str()))
            .collect();
        text = added(&text, &adds).map_err(|why| format!("{path}: {why}"))?;
    }
    out.write(&path, &text)
        .map_err(|e| format!("{path}: {e}"))?;
    Ok(Started {
        path,
        area,
        follows: pinned,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::follows::pin;
    use crate::tree::fake::Fake;
    use domain::schema::{KnowledgeDoc, knowledge_doc};
    use infrastructure::outbound::readers::Readers;

    fn project(areas: &str) -> Fake {
        Fake::new(&[
            (
                ".config/rotproof.toml",
                &format!("stack = \"none\"\nareas = {areas}\n"),
            ),
            ("domain/model.py", "def play():\n    return 1\n"),
            ("docs/knowledge/old.md", "---\n---\n"),
        ])
    }

    fn new(slug: &str) -> NewDocument {
        NewDocument {
            slug: slug.into(),
            ..NewDocument::default()
        }
    }

    fn follows(slug: &str, keys: &[&str]) -> NewDocument {
        NewDocument {
            follows: keys.iter().map(|key| key.to_string()).collect(),
            ..new(slug)
        }
    }

    fn written(tree: &Fake, slug: &str) -> String {
        tree.text(&format!("docs/knowledge/{slug}.md")).unwrap()
    }

    #[test]
    fn a_document_is_written_with_its_frontmatter_and_the_index_is_left_alone() {
        let tree = project("[\"ops\"]");
        let started = start(&tree, &tree, &Readers, &new("rates-are-cached")).unwrap();
        assert_eq!(started.path, "docs/knowledge/rates-are-cached.md");
        assert_eq!(started.area, "ops");
        assert!(started.follows.is_empty());
        assert_eq!(
            written(&tree, "rates-are-cached"),
            "---\ntype: Knowledge\ntitle: Rates are cached\ndescription: \"\"\ntags: [ops]\nstatus: stable\n---\n"
        );
        assert_eq!(tree.text("docs/knowledge/index.md"), None);
        assert_eq!(tree.text("docs/index.md"), None);
    }

    #[test]
    fn what_it_follows_is_pinned_as_rotproof_follows_pins_it_and_nothing_is_left_to_pin() {
        let tree = project("[\"ops\"]");
        tree.write("domain/model.ts", "export function a() { return 1; }\n")
            .unwrap();
        let keys = [
            "domain/model.py",
            "domain/model.py::play",
            "domain/model.ts::a",
            "domain/",
            "domain/model.py",
        ];
        let started = start(&tree, &tree, &Readers, &follows("model", &keys)).unwrap();
        let pinned: Vec<&str> = started
            .follows
            .iter()
            .map(|(key, _)| key.as_str())
            .collect();
        assert_eq!(pinned, &keys[..4], "a key named twice is pinned once");
        for (key, hash) in &started.follows {
            assert_eq!(current(&tree, &Readers, key).unwrap().as_ref(), Some(hash));
        }
        let text = written(&tree, "model");
        let filled = text.replace("description: \"\"", "description: What it tells.");
        let Ok(KnowledgeDoc::Knowledge(doc, _)) = knowledge_doc(&filled) else {
            panic!("{filled}");
        };
        assert_eq!(doc.follows, started.follows);
        // A pin by `rotproof follows` has nothing to change in what `rotproof new` wrote
        tree.write("docs/knowledge/model.md", &filled).unwrap();
        let keys: Vec<String> = keys.iter().map(|key| key.to_string()).collect();
        let again = pin(&tree, &tree, &Readers, "knowledge/model.md", &keys).unwrap();
        assert!(!again.written, "{again:?}");
    }

    #[test]
    fn a_key_that_is_not_there_writes_nothing_and_every_such_key_is_named() {
        let tree = project("[\"ops\"]");
        let missing = follows(
            "model",
            &["domain/model.py", "domain/gone.py", "domain/model.py::nope"],
        );
        let why = start(&tree, &tree, &Readers, &missing).unwrap_err();
        assert!(why.contains("domain/gone.py: not there"), "{why}");
        assert!(why.contains("domain/model.py::nope: not there"), "{why}");
        assert!(!why.contains("domain/model.py: "), "{why}");
        assert_eq!(tree.text("docs/knowledge/model.md"), None);
        let form = follows("model", &["domain/model.py::"]);
        let why = start(&tree, &tree, &Readers, &form).unwrap_err();
        assert!(why.contains("domain/model.py::"), "{why}");
        assert_eq!(tree.text("docs/knowledge/model.md"), None);
    }

    #[test]
    fn an_alarm_has_its_strings_and_its_headings_and_a_match_without_one_is_refused() {
        let tree = project("[\"ops\"]");
        let alarm = NewDocument {
            alarm: true,
            matches: vec!["403".into(), "trusted publisher".into()],
            ..new("pypi-403")
        };
        start(&tree, &tree, &Readers, &alarm).unwrap();
        assert!(
            written(&tree, "pypi-403")
                .ends_with("match: [\"403\", \"trusted publisher\"]\n---\n\n# When\n\n# Do\n")
        );
        let stray = NewDocument {
            matches: vec!["403".into()],
            ..new("stray")
        };
        let why = start(&tree, &tree, &Readers, &stray).unwrap_err();
        assert!(why.contains("--alarm"), "{why}");
        assert_eq!(tree.text("docs/knowledge/stray.md"), None);
        // An alarm that follows code has both: `follows` is added to the frontmatter, before the body
        let both = NewDocument {
            slug: "both".into(),
            follows: vec!["domain/model.py".into()],
            ..alarm
        };
        start(&tree, &tree, &Readers, &both).unwrap();
        let text = written(&tree, "both");
        let (front, body) = text.split_once("\n---\n").unwrap();
        assert!(
            front.contains("\nfollows:\n  domain/model.py: \""),
            "{text}"
        );
        assert_eq!(body, "\n# When\n\n# Do\n");
    }

    #[test]
    fn a_document_that_is_there_is_never_written_over() {
        // A name that Windows reads as the same (case, a dot at the end) is there too
        for existing in ["old.md", "OLD.md", "old.md."] {
            let tree = project("[\"ops\"]");
            tree.remove("docs/knowledge/old.md").unwrap();
            tree.write(&format!("docs/knowledge/{existing}"), "x")
                .unwrap();
            let why = start(&tree, &tree, &Readers, &new("old")).unwrap_err();
            assert!(
                why.contains("is there already") && why.contains(existing),
                "{existing}: {why}"
            );
            assert_eq!(
                tree.text(&format!("docs/knowledge/{existing}")).as_deref(),
                Some("x")
            );
        }
        let tree = project("[\"ops\"]");
        let why = start(&tree, &tree, &Readers, &new("old")).unwrap_err();
        assert!(why.contains("is there already"), "{why}");
        assert_eq!(
            tree.text("docs/knowledge/old.md").as_deref(),
            Some("---\n---\n")
        );
        // A directory of that name is there too
        tree.make_dir("docs/knowledge/dir.md").unwrap();
        assert!(start(&tree, &tree, &Readers, &new("dir")).is_err());
    }

    #[test]
    fn a_slug_an_area_or_a_missing_directory_is_refused_before_anything_is_written() {
        let tree = project("[\"ops\", \"billing\"]");
        let why = start(&tree, &tree, &Readers, &new("a")).unwrap_err();
        assert!(
            why.contains("--area") && why.contains("ops, billing"),
            "{why}"
        );
        let other = NewDocument {
            area: Some("sales".into()),
            ..new("a")
        };
        let why = start(&tree, &tree, &Readers, &other).unwrap_err();
        assert!(why.contains("\"sales\""), "{why}");
        let billing = NewDocument {
            area: Some("billing".into()),
            ..new("a")
        };
        assert_eq!(
            start(&tree, &tree, &Readers, &billing).unwrap().area,
            "billing"
        );
        assert!(written(&tree, "a").contains("tags: [billing]\n"));
        for bad in ["Rates", "rates.md", "index", "a/b", ""] {
            assert!(start(&tree, &tree, &Readers, &new(bad)).is_err(), "{bad}");
        }
        let bare = Fake::new(&[(
            ".config/rotproof.toml",
            "stack = \"none\"\nareas = [\"ops\"]\n",
        )]);
        let why = start(&bare, &bare, &Readers, &new("a")).unwrap_err();
        assert!(why.contains("docs/knowledge/ is not there"), "{why}");
        let none = Fake::new(&[("docs/knowledge/old.md", "")]);
        assert!(start(&none, &none, &Readers, &new("a")).is_err());
    }
}
