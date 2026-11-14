//! The project's files Rotproof writes once ([`project_files`]): `AGENTS.md`, `CLAUDE.md`, `README.md`, `.gitignore`,
//! `.gitattributes`, a GitHub Actions workflow that installs the pinned Rotproof and runs `rotproof check` (with
//! `requirements-dev.txt` where the stack installs it from PyPI; for `typescript` the pin goes in `package.json`, the
//! project's own file, which Rotproof does not write), and the files the stack's layout lists for its toolchain (the
//! workspace's `Cargo.toml` for `rust`). `rotproof create` writes each once, when it does not exist, and never again:
//! from then on it is the project's. Rotproof's guide, which it rewrites, is `guide.rs`.
//!
//! The texts are in `project/`, built into the binary. Each is named after the file it becomes, without a leading dot
//! and with `.in` added, so no agent working in Rotproof reads a project's `AGENTS.md` as its own.

use crate::declaration::Declared;

const AGENTS_TEXT: &str = include_str!("../../../project/AGENTS.md.in");
const CLAUDE_TEXT: &str = include_str!("../../../project/CLAUDE.md.in");
const README_TEXT: &str = include_str!("../../../project/README.md.in");
const GITATTRIBUTES_TEXT: &str = include_str!("../../../project/gitattributes.in");
/// Stack -> its `.gitignore`
const GITIGNORE: [(&str, &str); 4] = [
    (
        "python",
        include_str!("../../../project/gitignore/python.in"),
    ),
    (
        "typescript",
        include_str!("../../../project/gitignore/typescript.in"),
    ),
    ("rust", include_str!("../../../project/gitignore/rust.in")),
    ("none", include_str!("../../../project/gitignore/none.in")),
];
/// Where a project installs Rotproof from, which decides how `rotproof create` pins it
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Channel {
    /// PyPI, pinned in `requirements-dev.txt`, which a Python project installs anyway
    Pypi,
    /// The archive of the GitHub Release, pinned by its version in the workflow, which checks it against the
    /// release's `SHA256SUMS`. The releases are immutable, so the version fixes the binary; the binary cannot know the
    /// hash of the archive it is in, so no hash is written
    Release,
    /// npm, pinned in `package.json`, which exists before Rotproof runs (Vite writes it) and is the project's, so
    /// `rotproof create` writes the workflow and says how to add the pin, and never writes the pin itself
    Npm,
}
/// Stack -> where it installs Rotproof from. A project that does not write Python needs no Python for it
const CHANNEL: [(&str, Channel); 4] = [
    ("python", Channel::Pypi),
    ("typescript", Channel::Npm),
    ("rust", Channel::Release),
    ("none", Channel::Release),
];
const PYPI_REQUIREMENTS: &str = include_str!("../../../project/pypi/requirements-dev.txt.in");
const PYPI_WORKFLOW: &str = include_str!("../../../project/pypi/ci.yml.in");
const PYPI_DEVELOPMENT: &str = include_str!("../../../project/pypi/development.md.in");
const RELEASE_WORKFLOW: &str = include_str!("../../../project/release/ci.yml.in");
const RELEASE_DEVELOPMENT: &str = include_str!("../../../project/release/development.md.in");
const NPM_WORKFLOW: &str = include_str!("../../../project/npm/ci.yml.in");
const NPM_DEVELOPMENT: &str = include_str!("../../../project/npm/development.md.in");

fn channel(stack: &str) -> Channel {
    CHANNEL
        .iter()
        .find(|(s, _)| *s == stack)
        .map(|(_, channel)| *channel)
        .expect("every stack has a channel: a test reads every stack")
}

/// The project's files `rotproof create` writes once, for the project named `name` (its directory's name): path from
/// the root -> text, and what was not written for the stack, and why.
pub fn project_files<'a>(
    declared: &'a Declared,
    name: &str,
) -> (Vec<(&'a str, String)>, Option<String>) {
    let stack = declared.declaration.stack.as_str();
    let fill = |text: &str| {
        text.replace("{name}", name)
            .replace("{stack}", stack)
            .replace("{version}", env!("CARGO_PKG_VERSION"))
    };
    let channel = channel(stack);
    let development = match channel {
        Channel::Pypi => PYPI_DEVELOPMENT,
        Channel::Release => RELEASE_DEVELOPMENT,
        Channel::Npm => NPM_DEVELOPMENT,
    };
    let gitignore = GITIGNORE
        .iter()
        .find(|(s, _)| *s == stack)
        .map(|(_, text)| *text)
        .expect("every stack has a .gitignore: a test reads every stack");
    let mut files = vec![
        (
            "AGENTS.md",
            fill(&AGENTS_TEXT.replace("{map}", &map(declared))),
        ),
        ("CLAUDE.md", CLAUDE_TEXT.to_string()),
        (
            "README.md",
            fill(&README_TEXT.replace("{development}", development)),
        ),
        (".gitignore", gitignore.to_string()),
        (".gitattributes", GITATTRIBUTES_TEXT.to_string()),
    ];
    // What the stack's toolchain needs at the root: the workspace and the root package of a Rust project
    if let Some(layout) = &declared.layout {
        files.extend(layout.files.iter().map(|file| {
            (
                file.path.as_str(),
                file.text.trim_start().replace("{package}", &package(name)),
            )
        }));
    }
    match channel {
        Channel::Pypi => {
            files.push(("requirements-dev.txt", fill(PYPI_REQUIREMENTS)));
            files.push((".github/workflows/ci.yml", PYPI_WORKFLOW.to_string()));
        }
        Channel::Release => files.push((".github/workflows/ci.yml", fill(RELEASE_WORKFLOW))),
        Channel::Npm => {
            files.push((".github/workflows/ci.yml", NPM_WORKFLOW.to_string()));
            return (
                files,
                Some(format!(
                    "the pin in package.json, which is the project's: until it pins Rotproof \
                     (`npm install -D -E rotproof@{}`), the CI workflow fails",
                    env!("CARGO_PKG_VERSION")
                )),
            );
        }
    }
    (files, None)
}

/// The project's name as a package name, which Cargo takes only of letters, digits, `-` and `_`: every other
/// character becomes `_`, and a name that starts with no letter starts with `app_`.
fn package(name: &str) -> String {
    let name: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if name.starts_with(|c: char| c.is_ascii_alphabetic()) {
        name
    } else {
        format!("app_{name}")
    }
}

/// The rows of the map for the layers present in the declaration, each with the first line of its role, which the
/// project rewrites into what the layer holds there.
fn map(declared: &Declared) -> String {
    declared
        .places
        .iter()
        .filter(|p| p.parent.is_none() && !declared.is_absent(p))
        .map(|place| {
            let role = place
                .role
                .trim()
                .lines()
                .next()
                .expect("every role has text: a test reads the table");
            format!("| `{}/` | {role} |\n", place.path)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layers::{NO_UI, RECORDS_ONLY, known_stacks};

    #[test]
    fn every_file_in_project_is_built_in() {
        // What the modules of this crate include (this one, the guide, and the rules that say something in it), read
        // from their sources, against what `project/` holds: a text added there and never included would be edited in
        // vain
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut included = std::collections::BTreeSet::new();
        for entry in std::fs::read_dir(&src).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_none_or(|e| e != "rs") {
                continue;
            }
            let source = std::fs::read_to_string(&path).unwrap();
            included.extend(
                source
                    .match_indices("include_str!(\"../../../project/")
                    .map(|(at, prefix)| {
                        let rest = &source[at + prefix.len()..];
                        rest[..rest.find('"').unwrap()].to_string()
                    }),
            );
        }
        let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../project");
        let mut on_disk = std::collections::BTreeSet::new();
        let mut dirs = vec![base.clone()];
        while let Some(dir) = dirs.pop() {
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    dirs.push(path);
                } else {
                    let name = path.strip_prefix(&base).unwrap().to_string_lossy();
                    on_disk.insert(name.replace('\\', "/"));
                }
            }
        }
        assert!(on_disk.len() >= 10, "{on_disk:?}");
        assert_eq!(included, on_disk);
        assert!(on_disk.iter().all(|name| name.ends_with(".in")));
    }

    #[test]
    fn every_stack_has_its_project_files() {
        use crate::declaration::Declaration;
        for stack in known_stacks() {
            let declared = Declared::new(Declaration {
                stack: stack.to_string(),
                areas: Vec::new(),
                ui: (stack != RECORDS_ONLY).then(|| NO_UI.to_string()),
                absent: Vec::new(),
                unchecked: Vec::new(),
                files: None,
                declined: Vec::new(),
            })
            .unwrap();
            let (files, not_written) = project_files(&declared, "x");
            // Only the pin npm keeps in package.json, the project's own file, is left to the project
            assert_eq!(
                channel(stack) == Channel::Npm,
                not_written.is_some(),
                "{stack}"
            );
            assert!(
                files
                    .iter()
                    .any(|(path, _)| *path == ".github/workflows/ci.yml"),
                "{stack}"
            );
            assert!(files.iter().any(|(path, _)| *path == ".gitignore"));
        }
    }
}
