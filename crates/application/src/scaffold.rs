//! `rotproof new`: a new knowledge document written with its frontmatter, read from the declaration (the areas) and
//! the tree (the names taken).

use domain::bundle::in_docs;
use domain::scaffold::{document, pick_area, slug_problem};
use domain::tree::as_windows_reads;
use protocol::inbound::{NewDocument, Started};
use protocol::outbound::{Tree, Writer};

/// Write the document `new` describes to `docs/knowledge/<slug>.md` through `out`, with no `follows`: a pin says the
/// document was reviewed against its code, and there is no body yet to review, so `rotproof follows` pins it once
/// there is. Nothing is written when the slug is not one, a file of that name is there already (by Windows' reading
/// of names too, as two names that read alike open one file there and two on Linux), the area is not one the
/// declaration lists, or the alarm's strings cannot be written. The index is not written: the document is not read
/// until its description is, so it would be listed as left out now and rewritten again then.
pub fn start(tree: &dyn Tree, out: &dyn Writer, new: &NewDocument) -> Result<Started, String> {
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
    let text = document(&new.slug, &area, alarm)?;
    out.write(&path, &text)
        .map_err(|e| format!("{path}: {e}"))?;
    Ok(Started { path, area })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::fake::Fake;

    fn project(areas: &str) -> Fake {
        Fake::new(&[
            (
                ".config/rotproof.toml",
                &format!("stack = \"none\"\nareas = {areas}\n"),
            ),
            ("docs/knowledge/old.md", "---\n---\n"),
        ])
    }

    fn new(slug: &str) -> NewDocument {
        NewDocument {
            slug: slug.into(),
            ..NewDocument::default()
        }
    }

    fn written(tree: &Fake, slug: &str) -> String {
        tree.text(&format!("docs/knowledge/{slug}.md")).unwrap()
    }

    #[test]
    fn a_document_is_written_with_its_frontmatter_and_the_index_is_left_alone() {
        let tree = project("[\"ops\"]");
        let started = start(&tree, &tree, &new("rates-are-cached")).unwrap();
        assert_eq!(started.path, "docs/knowledge/rates-are-cached.md");
        assert_eq!(started.area, "ops");
        assert_eq!(
            written(&tree, "rates-are-cached"),
            "---\ntype: Knowledge\ntitle: Rates are cached\ndescription: \"\"\ntags: [ops]\nstatus: stable\n---\n"
        );
        assert_eq!(tree.text("docs/knowledge/index.md"), None);
        assert_eq!(tree.text("docs/index.md"), None);
    }

    #[test]
    fn an_alarm_has_its_strings_and_its_headings_and_a_match_without_one_is_refused() {
        let tree = project("[\"ops\"]");
        let alarm = NewDocument {
            alarm: true,
            matches: vec!["403".into(), "trusted publisher".into()],
            ..new("pypi-403")
        };
        start(&tree, &tree, &alarm).unwrap();
        assert!(
            written(&tree, "pypi-403")
                .ends_with("match: [\"403\", \"trusted publisher\"]\n---\n\n# When\n\n# Do\n")
        );
        let stray = NewDocument {
            matches: vec!["403".into()],
            ..new("stray")
        };
        let why = start(&tree, &tree, &stray).unwrap_err();
        assert!(why.contains("--alarm"), "{why}");
        assert_eq!(tree.text("docs/knowledge/stray.md"), None);
    }

    #[test]
    fn a_document_that_is_there_is_never_written_over() {
        // A name that Windows reads as the same (case, a dot at the end) is there too
        for existing in ["old.md", "OLD.md", "old.md."] {
            let tree = project("[\"ops\"]");
            tree.remove("docs/knowledge/old.md").unwrap();
            tree.write(&format!("docs/knowledge/{existing}"), "x")
                .unwrap();
            let why = start(&tree, &tree, &new("old")).unwrap_err();
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
        let why = start(&tree, &tree, &new("old")).unwrap_err();
        assert!(why.contains("is there already"), "{why}");
        assert_eq!(
            tree.text("docs/knowledge/old.md").as_deref(),
            Some("---\n---\n")
        );
        // A directory of that name is there too
        tree.make_dir("docs/knowledge/dir.md").unwrap();
        assert!(start(&tree, &tree, &new("dir")).is_err());
    }

    #[test]
    fn a_slug_an_area_or_a_missing_directory_is_refused_before_anything_is_written() {
        let tree = project("[\"ops\", \"billing\"]");
        let why = start(&tree, &tree, &new("a")).unwrap_err();
        assert!(
            why.contains("--area") && why.contains("ops, billing"),
            "{why}"
        );
        let other = NewDocument {
            area: Some("sales".into()),
            ..new("a")
        };
        let why = start(&tree, &tree, &other).unwrap_err();
        assert!(why.contains("\"sales\""), "{why}");
        let billing = NewDocument {
            area: Some("billing".into()),
            ..new("a")
        };
        assert_eq!(start(&tree, &tree, &billing).unwrap().area, "billing");
        assert!(written(&tree, "a").contains("tags: [billing]\n"));
        for bad in ["Rates", "rates.md", "index", "a/b", ""] {
            assert!(start(&tree, &tree, &new(bad)).is_err(), "{bad}");
        }
        let bare = Fake::new(&[(
            ".config/rotproof.toml",
            "stack = \"none\"\nareas = [\"ops\"]\n",
        )]);
        let why = start(&bare, &bare, &new("a")).unwrap_err();
        assert!(why.contains("docs/knowledge/ is not there"), "{why}");
        let none = Fake::new(&[("docs/knowledge/old.md", "")]);
        assert!(start(&none, &none, &new("a")).is_err());
    }
}
