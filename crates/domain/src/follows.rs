//! A knowledge document that follows code: it names what it describes in its frontmatter, each with a hash of it when
//! the document was last reviewed, and `rotproof check` fails when the code changed since.
//!
//! ```yaml
//! follows:
//!   src/api/routes.py: 3e1f0a9c          # a file, in any stack
//!   src/api/routes.py::create_user: 0b7d21e4   # a function or class of a Python file
//!   src/api/routes.py::Router.add: 9a01c3ff    # a method
//!   src/api/: 51c0d2aa                   # every .py file under a directory
//! ```
//!
//! - **The review is pinned in the document.** Writing the new hash changes the document, so the log entry that names
//!   it with its new hash (`records.rs`) records the review: no rule of its own.
//! - **A file is the unit in every stack;** Python goes further, since Rotproof reads its definitions: a definition is
//!   hashed on its own source, from its first decorator to its end, so a change elsewhere in its file does not fail
//!   it, and a directory on the `.py` files under it.
//! - **What a document follows must exist.** A file, directory or definition that is gone fails, as a dangling link
//!   does.
//!
//! The hash is the knowledge documents' own (`records::content_hash`): the first 8 hex digits of SHA-256 of the text,
//! with every line ending as `\n`.

use crate::records::content_hash;

/// What a key of `follows` names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Followed {
    /// A file, from the root
    File(String),
    /// Every `.py` file under a directory, from the root, written with a `/` at its end
    Directory(String),
    /// A function or class of a Python file: the file, and its dotted name (`f`, `Class`, `Class.method`)
    Definition { file: String, name: String },
}

/// What the key `key` of `follows` names, or why it names nothing Rotproof can follow.
pub fn followed(key: &str) -> Result<Followed, String> {
    let (path, name) = match key.split_once("::") {
        Some((path, name)) => (path, Some(name)),
        None => (key, None),
    };
    let bare = path.strip_suffix('/').unwrap_or(path);
    if bare.is_empty()
        || path.starts_with('/')
        || path.contains('\\')
        || bare
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err(format!(
            "{key:?} is not a path from the root, written with / (src/api/routes.py)"
        ));
    }
    match name {
        None if path.ends_with('/') => Ok(Followed::Directory(bare.to_string())),
        None => Ok(Followed::File(path.to_string())),
        Some(_) if path.ends_with('/') || !path.ends_with(".py") => Err(format!(
            "{key:?} names a definition outside a .py file: only Python's definitions are read"
        )),
        Some(name) => {
            let identifier = |part: &str| {
                part.chars()
                    .next()
                    .is_some_and(|c| c.is_alphabetic() || c == '_')
                    && part.chars().all(|c| c.is_alphanumeric() || c == '_')
            };
            if !name.split('.').all(identifier) {
                return Err(format!(
                    "{key:?} names no definition after ::, which is a function or class, or Class.method"
                ));
            }
            Ok(Followed::Definition {
                file: path.to_string(),
                name: name.to_string(),
            })
        }
    }
}

/// The hash of a directory: of its `.py` files, each as its path from the root and its text, in the order of their
/// paths. A file added, removed, renamed or changed changes it.
pub fn directory_hash(files: &[(String, String)]) -> String {
    let mut sorted: Vec<&(String, String)> = files.iter().collect();
    sorted.sort();
    let joined: String = sorted
        .iter()
        .map(|(path, text)| format!("{path}\0{text}\0"))
        .collect();
    content_hash(&joined)
}

/// The finding for a document `doc` that follows `key`, pinned at `pinned`, which is now `now` (`None` when it is
/// gone). `None` when it is unchanged.
pub fn drifted(doc: &str, key: &str, pinned: &str, now: Option<&str>) -> Option<String> {
    match now {
        None => Some(format!(
            "knowledge/{doc} follows {key}, which is not there: follow what replaced it, or drop it from follows"
        )),
        Some(now) if now == pinned => None,
        Some(now) => Some(format!(
            "knowledge/{doc} follows {key}, which changed since the document was last reviewed: review the \
             document against it, then write `{key}: {now}` in follows"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_names_a_file_a_directory_or_a_python_definition() {
        assert_eq!(
            followed("src/api.py"),
            Ok(Followed::File("src/api.py".into()))
        );
        assert_eq!(
            followed("src/main.rs"),
            Ok(Followed::File("src/main.rs".into()))
        );
        assert_eq!(
            followed("src/api/"),
            Ok(Followed::Directory("src/api".into()))
        );
        assert_eq!(
            followed("src/api.py::Router.add"),
            Ok(Followed::Definition {
                file: "src/api.py".into(),
                name: "Router.add".into()
            })
        );
        for bad in [
            "",
            "/src/api.py",
            "src\\api.py",
            "src/../api.py",
            "./api.py",
            "src//api.py",
            "/",
            "src/api.rs::f",
            "src/::f",
            "src/api.py::",
            "src/api.py::f()",
            "src/api.py::1f",
            "src/api.py::a..b",
        ] {
            assert!(followed(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn a_directory_changes_with_any_of_its_files() {
        let one = vec![("a/x.py".to_string(), "x = 1\n".to_string())];
        let mut two = one.clone();
        two.push(("a/y.py".into(), "y = 1\n".into()));
        let renamed = vec![("a/z.py".to_string(), "x = 1\n".to_string())];
        assert_ne!(directory_hash(&one), directory_hash(&two));
        assert_ne!(directory_hash(&one), directory_hash(&renamed));
        // The order the files were read in does not count
        let reversed: Vec<(String, String)> = two.iter().rev().cloned().collect();
        assert_eq!(directory_hash(&two), directory_hash(&reversed));
    }

    #[test]
    fn a_change_or_a_gone_target_is_said_with_what_to_write() {
        assert_eq!(
            drifted("api.md", "a.py", "00000000", Some("00000000")),
            None
        );
        let changed = drifted("api.md", "a.py", "00000000", Some("12345678")).unwrap();
        assert!(
            changed.contains("write `a.py: 12345678` in follows"),
            "{changed}"
        );
        let gone = drifted("api.md", "a.py::f", "00000000", None).unwrap();
        assert!(gone.contains("which is not there"), "{gone}");
    }
}
