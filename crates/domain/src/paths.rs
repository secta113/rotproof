//! Paths from the root, with `/` on every platform: their parts, the one rule on paths every check shares, and the
//! paths in messages.

use std::path::Path;

/// Whether `path` is `prefix` or inside it. Both are from the root, with `/`.
pub fn within(path: &str, prefix: &str) -> bool {
    path == prefix || path.starts_with(&format!("{prefix}/"))
}

/// Whether `path` is `dir` or inside it, the root ("") holding every path.
pub fn inside(path: &str, dir: &str) -> bool {
    dir.is_empty() || within(path, dir)
}

/// The directory of a path from the root: "" for one at the root.
pub fn parent(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(dir, _)| dir)
}

/// The name a path ends with.
pub fn file_name(path: &str) -> &str {
    path.rsplit_once('/').map_or(path, |(_, name)| name)
}

/// `path` in the directory `dir` (from the root, "" for the root), not normalized.
pub fn join(dir: &str, path: &str) -> String {
    if dir.is_empty() {
        path.to_string()
    } else {
        format!("{dir}/{path}")
    }
}

/// Why a path written in the declaration is not fit to be read as it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Miswritten {
    /// It holds a `\`
    Backslash,
    /// It is not from the root with `/`: it starts with `/`, holds a `:`, or has an empty, `.` or `..` part, so it
    /// would never equal the paths Rotproof compares it with
    NotFromRoot,
}

/// Whether `path`, as the declaration writes it (`unchecked`, a unit's `path`), is fit to be compared with the paths
/// Rotproof reads, and if not, why.
pub fn miswritten(path: &str) -> Option<Miswritten> {
    if path.contains('\\') {
        Some(Miswritten::Backslash)
    } else if path.starts_with('/')
        || path.contains(':')
        || path.split('/').any(|part| matches!(part, "" | "." | ".."))
    {
        Some(Miswritten::NotFromRoot)
    } else {
        None
    }
}

/// A path from the root without `.`, `..` or empty parts. `None` when it climbs above the root.
pub fn normalize(path: &str) -> Option<String> {
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            part => parts.push(part),
        }
    }
    Some(parts.join("/"))
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
    fn the_root_holds_every_path() {
        assert!(inside("a/b.py", ""));
        assert!(inside("", ""));
        assert!(inside("a/b.py", "a"));
        assert!(!inside("ab/c.py", "a"));
        assert!(!inside("", "a"));
    }

    #[test]
    fn a_path_at_the_root_has_the_root_as_its_parent_and_itself_as_its_name() {
        assert_eq!(parent("src/domain/x.py"), "src/domain");
        assert_eq!(parent("x.py"), "");
        assert_eq!(file_name("src/domain/x.py"), "x.py");
        assert_eq!(file_name("x.py"), "x.py");
    }

    #[test]
    fn a_path_joined_to_the_root_is_itself() {
        assert_eq!(join("src", "x.py"), "src/x.py");
        assert_eq!(join("", "x.py"), "x.py");
        assert_eq!(join("src", "../x.py"), "src/../x.py");
    }

    #[test]
    fn a_path_is_written_from_the_root_with_slashes() {
        assert_eq!(miswritten("packages/billing/src"), None);
        assert_eq!(miswritten("a\\b"), Some(Miswritten::Backslash));
        for path in ["./a", "a/./b", "../a", "/a", "a//b", "C:/a", "", "a/.."] {
            assert_eq!(miswritten(path), Some(Miswritten::NotFromRoot), "{path:?}");
        }
    }

    #[test]
    fn a_normalized_path_has_no_dots_and_never_climbs_above_the_root() {
        assert_eq!(normalize("src/./a/../b.ts").as_deref(), Some("src/b.ts"));
        assert_eq!(normalize("/src//b.ts").as_deref(), Some("src/b.ts"));
        assert_eq!(normalize("src/../.."), None);
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
