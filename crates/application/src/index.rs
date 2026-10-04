//! `rotproof index`: every index file written from the frontmatter, and the open items to measure again.

use crate::bundle::Bundle;
use crate::layers::areas;
use domain::bundle::{backlog, stale};
use domain::schema::Time;
use domain::tree::{Tree, Writer};

/// What `rotproof index` did.
#[derive(Debug, Default)]
pub struct Indexed {
    /// The files written, from the root
    pub written: Vec<String>,
    /// Documents left out of the index files: name -> why
    pub left_out: Vec<(String, String)>,
    /// The open backlog items past their `stale_after`, each with it
    pub stale: Vec<(String, String)>,
}

/// Write every index file of the bundle in `tree` through `out`, and find the items stale at `now`. `Err` is a
/// declaration that cannot be read, or a file that cannot be read or written.
pub fn index(tree: &dyn Tree, out: &dyn Writer, now: Time) -> Result<Indexed, String> {
    let areas = areas(tree).map_err(|e| e.to_string())??;
    let bundle = Bundle::new(tree, areas);
    let (files, problems) = bundle.expected().map_err(|e| e.to_string())?;
    let mut indexed = Indexed::default();
    for (path, text) in files {
        out.write(&path, &text)
            .map_err(|e| format!("{path}: {e}"))?;
        indexed.written.push(path);
    }
    indexed.left_out = problems.into_iter().collect();
    let docs = bundle.read_folder("backlog").map_err(|e| e.to_string())?;
    let parsed = backlog(&docs, &bundle.areas);
    for name in stale(&parsed.items, now) {
        let at = parsed.items[&name]
            .0
            .stale_after
            .expect("an item is stale only past its stale_after");
        indexed.stale.push((name, at.to_string()));
    }
    Ok(indexed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::fake::Fake;

    #[test]
    fn every_index_file_is_written_through_the_port() {
        let tree = Fake::new(&[
            (
                ".config/rotproof.toml",
                "stack = \"none\"\nareas = [\"a\"]\n",
            ),
            ("docs/log.md", "# Log\n"),
            // The directories of the bundle, with rules that are about to be rewritten
            ("docs/backlog/rules.md", ""),
            ("docs/specs/rules.md", ""),
            ("docs/knowledge/rules.md", ""),
        ]);
        let now = chrono::DateTime::parse_from_rfc3339("2026-10-05T00:00:00+09:00").unwrap();
        let indexed = index(&tree, &tree, now).unwrap();
        assert!(
            indexed
                .written
                .contains(&"docs/backlog/index.md".to_string())
        );
        for path in &indexed.written {
            assert!(tree.text(path).is_some(), "{path}");
        }
        assert!(indexed.left_out.is_empty() && indexed.stale.is_empty());
        let without = Fake::new(&[("docs/log.md", "# Log\n")]);
        assert!(index(&without, &without, now).is_err());
    }
}
