//! A knowledge document that follows code: it names what it describes in its frontmatter, each with a hash of it when
//! the document was last reviewed, and `rotproof check` fails when the code changed since.
//!
//! ```yaml
//! follows:
//!   src/api/routes.py: "3e1f0a9c"
//!   src/api/routes.py::create_user: "0b7d21e4"
//!   src/api/routes.py::Router.add: "9a01c3ff"
//!   src/api/: "51c0d2aa"
//! ```
//!
//! A key is a file, in any stack, a function or class of a Python file (`::create_user`), a method (`::Router.add`),
//! or a directory, for every `.py` file under it. The frontmatter holds no comment (`frontmatter.rs`).
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
        Some(_) => Some(format!(
            "knowledge/{doc} follows {key}, which changed since the document was last reviewed: review the \
             document against it, then run `rotproof follows knowledge/{doc} {key}`"
        )),
    }
}

/// The file name in `docs/knowledge/` of the document `arg` names: as `rotproof check` and the log name it
/// (`knowledge/api.md`), or from the root (`docs/knowledge/api.md`, also with `\`, as a Windows shell completes it).
/// `Err` when it names no document of `docs/knowledge/`, such as a key passed first.
pub fn document_name(arg: &str) -> Result<String, String> {
    let path = arg.replace('\\', "/");
    path.strip_prefix("docs/")
        .unwrap_or(&path)
        .strip_prefix("knowledge/")
        .filter(|name| name.ends_with(".md") && name.len() > 3 && !name.contains('/'))
        .map(str::to_string)
        .ok_or_else(|| {
            format!(
                "{arg:?} is not a document of docs/knowledge/: rotproof follows takes the document first, then the \
                 keys (rotproof follows knowledge/api.md src/api.py)"
            )
        })
}

/// The line after the frontmatter of `lines`, which ends at the second `---`, as the frontmatter is read in the check
/// (`frontmatter.rs`): a `---` on a line of its own, with no trailing character.
fn frontmatter_end(lines: &[String]) -> Result<usize, String> {
    lines
        .iter()
        .skip(1)
        .position(|line| line == "---")
        .map(|i| i + 1)
        .filter(|_| lines.first().map(String::as_str) == Some("---"))
        .ok_or_else(|| "no frontmatter".to_string())
}

/// The key `key` as a YAML key: as it is, unless YAML would read it otherwise, then in double quotes.
fn yaml_key(key: &str) -> String {
    let plain = key
        .chars()
        .next()
        .is_some_and(|c| !c.is_whitespace() && !"-?:,[]{}#&*!|>'\"%@`".contains(c))
        && !key.contains(": ")
        && !key.contains(" #")
        && !key.ends_with([':', ' ']);
    if plain {
        key.to_string()
    } else {
        format!("\"{}\"", key.replace('\\', "\\\\").replace('"', "\\\""))
    }
}

/// The knowledge document `text` with each of `added` (a key of `follows`, its hash) added at the end of `follows`,
/// the field made at the end of the frontmatter when there is none, and everything else as it was. A hash is quoted,
/// as YAML reads `12345678` as a number and `1e100000` as a float. `Err` when `follows` is not one key per line, for a
/// person to edit. `text` is taken as the check reads it: every line ending `\n` and no byte order mark
/// (`tree::with_lf`), which `tree::Written` puts back when the document is written.
pub fn added(text: &str, added: &[(&str, &str)]) -> Result<String, String> {
    let mut lines: Vec<String> = text.split('\n').map(str::to_string).collect();
    let end = frontmatter_end(&lines)?;
    let field = (1..end).find(|&i| lines[i].starts_with("follows:"));
    let (at, indent) = match field {
        Some(i) => {
            if !lines[i]["follows:".len()..].trim().is_empty() {
                return Err("follows is not one key per line: edit follows by hand".into());
            }
            let block = (i + 1..end)
                .take_while(|&j| lines[j].starts_with([' ', '\t']))
                .count();
            let indent = match block {
                0 => "  ".to_string(),
                _ => {
                    let first = &lines[i + 1];
                    first[..first.len() - first.trim_start().len()].to_string()
                }
            };
            (i + 1 + block, indent)
        }
        None => {
            lines.insert(end, "follows:".to_string());
            (end + 1, "  ".to_string())
        }
    };
    for (n, (key, hash)) in added.iter().enumerate() {
        lines.insert(at + n, format!("{indent}{}: \"{hash}\"", yaml_key(key)));
    }
    Ok(lines.join("\n"))
}

/// The knowledge document `text` with each pin in `pins` (a key of `follows`, its hash as written, the hash to write)
/// set to its new hash, and everything else as it was: only the hash on the key's line in the frontmatter changes,
/// quoted as it was. `Err` when a key is not on one line of the frontmatter with its hash, for a person to edit. `text`
/// is taken as the check reads it: every line ending `\n` and no byte order mark (`tree::with_lf`).
pub fn repinned(text: &str, pins: &[(&str, &str, &str)]) -> Result<String, String> {
    let mut lines: Vec<String> = text.split('\n').map(str::to_string).collect();
    let end = frontmatter_end(&lines)?;
    for (key, old, new) in pins {
        let mut found = 0;
        for line in &mut lines[1..end] {
            let trimmed = line.trim_start();
            let Some(rest) = [
                format!("{key}:"),
                format!("\"{key}\":"),
                format!("'{key}':"),
            ]
            .iter()
            .find_map(|head| trimmed.strip_prefix(head.as_str())) else {
                continue;
            };
            // A comment after the value is YAML's, not the value's
            let value = rest
                .split(" #")
                .next()
                .unwrap_or_default()
                .trim()
                .trim_matches(['"', '\'']);
            if value == *old {
                *line = line.replacen(old, new, 1);
                found += 1;
            }
        }
        if found != 1 {
            return Err(format!(
                "{key} is not on one line of the frontmatter with its hash {old}: edit follows by hand"
            ));
        }
    }
    Ok(lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pin_changes_only_its_hash() {
        let text = "---\ntype: Knowledge\nfollows:\n  src/a.py: \"00000000\"\n  'src/b.py::f': 11111111  # ours\n\
                    ---\n\n# Body\n\nsrc/a.py: 00000000\n";
        let pinned = repinned(
            text,
            &[
                ("src/a.py", "00000000", "aaaaaaaa"),
                ("src/b.py::f", "11111111", "bbbbbbbb"),
            ],
        )
        .unwrap();
        assert_eq!(
            pinned,
            text.replacen("\"00000000\"", "\"aaaaaaaa\"", 1)
                .replace("11111111", "bbbbbbbb")
        );
        // The body is not the frontmatter
        assert!(pinned.ends_with("src/a.py: 00000000\n"));
        assert!(repinned(text, &[("src/c.py", "00000000", "cccccccc")]).is_err());
        assert!(repinned(text, &[("src/a.py", "99999999", "cccccccc")]).is_err());
        assert!(repinned("no frontmatter", &[]).is_err());
    }

    #[test]
    fn a_key_is_added_at_the_end_of_follows_with_its_hash_quoted() {
        let text = "---\ntype: Knowledge\nfollows:\n    src/a.py: \"00000000\"\nstatus: stable\n---\n\n# Body\n";
        assert_eq!(
            added(text, &[("src/b.py::f", "12345678"), ("src/c/", "1e100000")]).unwrap(),
            "---\ntype: Knowledge\nfollows:\n    src/a.py: \"00000000\"\n    src/b.py::f: \"12345678\"\n    \
             src/c/: \"1e100000\"\nstatus: stable\n---\n\n# Body\n"
        );
        // Without follows, the field is made at the end of the frontmatter
        let bare = "---\ntype: Knowledge\nstatus: stable\n---\n\n# Body\n";
        assert_eq!(
            added(bare, &[("src/a.py", "aaaaaaaa")]).unwrap(),
            "---\ntype: Knowledge\nstatus: stable\nfollows:\n  src/a.py: \"aaaaaaaa\"\n---\n\n# Body\n"
        );
        // A key YAML would read otherwise is quoted
        assert!(
            added(bare, &[("my api.py", "aaaaaaaa"), ("@x.py", "bbbbbbbb")])
                .unwrap()
                .contains("  my api.py: \"aaaaaaaa\"\n  \"@x.py\": \"bbbbbbbb\"\n")
        );
        assert!(added("---\nfollows: {}\n---\n", &[("a.py", "aaaaaaaa")]).is_err());
        assert!(added("no frontmatter", &[]).is_err());
    }

    #[test]
    fn a_document_is_named_as_the_check_names_it_or_from_the_root() {
        for arg in [
            "knowledge/api.md",
            "docs/knowledge/api.md",
            "docs\\knowledge\\api.md",
        ] {
            assert_eq!(document_name(arg), Ok("api.md".to_string()), "{arg}");
        }
        for bad in [
            "src/api.py",
            "api.md",
            "knowledge/.md",
            "knowledge/a/b.md",
            "docs/work/a.md",
            "knowledge/api.txt",
        ] {
            assert!(document_name(bad).is_err(), "{bad}");
        }
    }

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
            changed.contains("then run `rotproof follows knowledge/api.md a.py`"),
            "{changed}"
        );
        let gone = drifted("api.md", "a.py::f", "00000000", None).unwrap();
        assert!(gone.contains("which is not there"), "{gone}");
    }
}
