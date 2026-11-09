//! `rotproof index`: every index file written from the frontmatter.

use crate::bundle::Bundle;
use crate::layers::areas;
use protocol::inbound::Indexed;
use protocol::outbound::{Tree, Writer};

/// Write every index file of the bundle in `tree` through `out`. `Err` is a declaration that cannot be read, or a file
/// that cannot be read or written.
pub fn index(tree: &dyn Tree, out: &dyn Writer) -> Result<Indexed, String> {
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
            // The directory of the bundle, with rules that are about to be rewritten
            ("docs/knowledge/rules.md", ""),
            // A project's own records, which Rotproof no longer writes into
            ("docs/work/rules.md", "ours"),
        ]);
        let indexed = index(&tree, &tree).unwrap();
        assert_eq!(
            indexed.written,
            [
                "docs/index.md",
                "docs/knowledge/rules.md",
                "docs/knowledge/index.md"
            ]
        );
        for path in &indexed.written {
            assert!(tree.text(path).is_some(), "{path}");
        }
        assert_eq!(tree.text("docs/work/rules.md").as_deref(), Some("ours"));
        assert_eq!(tree.found("docs/work/index.md"), None);
        assert!(indexed.left_out.is_empty());
        let without = Fake::new(&[("docs/log.md", "# Log\n")]);
        assert!(index(&without, &without).is_err());
    }
}
