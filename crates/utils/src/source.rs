//! The paths and lines in messages, and the one rule on paths every check shares.

use std::path::Path;

/// The line (from 1) at a byte offset.
pub fn line_of(source: &str, at: usize) -> usize {
    source.as_bytes()[..at.min(source.len())]
        .iter()
        .filter(|&&b| b == b'\n')
        .count()
        + 1
}

/// Whether `path` is `prefix` or inside it. Both are from the root, with `/`.
pub fn within(path: &str, prefix: &str) -> bool {
    path == prefix || path.starts_with(&format!("{prefix}/"))
}

/// The path from the root, for messages: with `/` on every platform, so the output is the same on Windows.
pub fn relative_path(path: &Path, root: &Path) -> String {
    let path = path.strip_prefix(root).unwrap_or(path);
    path.components()
        .map(|part| part.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn within_compares_whole_names() {
        assert!(within("domain", "domain"));
        assert!(within("domain/x.py", "domain"));
        assert!(!within("domains/x.py", "domain"));
        assert!(!within("domain", "domain/x.py"));
    }

    #[test]
    fn a_relative_path_uses_slashes() {
        let root = Path::new("repo");
        assert_eq!(
            relative_path(&root.join("docs").join("backlog").join("index.md"), root),
            "docs/backlog/index.md"
        );
    }
}
