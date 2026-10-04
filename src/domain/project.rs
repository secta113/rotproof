//! The files Rotproof writes outside `docs/`.
//!
//! - **Rotproof's guide, `.rotproof/AGENTS.md`,** which every `rotproof create` rewrites and `rotproof check` compares,
//!   as it does the generated files in `docs/`. It says the rules Rotproof keeps in the project's stack: how to run
//!   Rotproof, the layers (their table is written from `layers/table.toml`, so the two cannot disagree) and the
//!   records. It names the version that wrote it, so an upgrade fails `rotproof check` until `rotproof create` has run.
//! - **The project's files** ([`project_files`]): `AGENTS.md`, `CLAUDE.md`, `README.md`, `.gitignore`,
//!   `.gitattributes`, where the stack installs Rotproof from PyPI, `requirements-dev.txt` with Rotproof pinned and a
//!   GitHub Actions workflow that runs `rotproof check`, and the files the stack's layout lists for its toolchain (the
//!   workspace's `Cargo.toml` for `rust`). `rotproof create` writes each once, when it does not exist, and never again:
//!   from then on it is the project's.
//!
//! The texts are in `project/`, built into the binary. Each is named after the file it becomes, without a leading dot
//! and with `.in` added, so no agent working in Rotproof reads a project's `AGENTS.md` as its own.

use crate::domain::layers::{
    Declared, Language, Layout, RECORDS_ONLY, known_stacks, layout, listed, table,
};

/// Rotproof's guide, from the root
pub const GUIDE: &str = ".rotproof/AGENTS.md";

const GUIDE_TEXT: &str = include_str!("../../project/rotproof/AGENTS.md.in");
const LAYERS_TEXT: &str = include_str!("../../project/rotproof/layers.md.in");
const UI_TEXT: &str = include_str!("../../project/rotproof/ui.md.in");
/// How the direction is read: the imports of a source file, or the dependencies of a crate
const DIRECTION_IMPORTS: &str = include_str!("../../project/rotproof/direction-imports.md.in");
const DIRECTION_CARGO: &str = include_str!("../../project/rotproof/direction-cargo.md.in");

const AGENTS_TEXT: &str = include_str!("../../project/AGENTS.md.in");
const CLAUDE_TEXT: &str = include_str!("../../project/CLAUDE.md.in");
const README_TEXT: &str = include_str!("../../project/README.md.in");
const GITATTRIBUTES_TEXT: &str = include_str!("../../project/gitattributes.in");
/// Stack -> its `.gitignore`
const GITIGNORE: [(&str, &str); 4] = [
    ("python", include_str!("../../project/gitignore/python.in")),
    (
        "typescript",
        include_str!("../../project/gitignore/typescript.in"),
    ),
    ("rust", include_str!("../../project/gitignore/rust.in")),
    ("none", include_str!("../../project/gitignore/none.in")),
];
/// The stacks that install Rotproof from PyPI, pinned in `requirements-dev.txt`. How the others pin it is decided
/// with each stack
const PYPI: [&str; 2] = ["python", "none"];
const PYPI_REQUIREMENTS: &str = include_str!("../../project/pypi/requirements-dev.txt.in");
const PYPI_WORKFLOW: &str = include_str!("../../project/pypi/ci.yml.in");
const PYPI_DEVELOPMENT: &str = include_str!("../../project/pypi/development.md.in");
const UNPINNED_DEVELOPMENT: &str = include_str!("../../project/unpinned/development.md.in");

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
    let pypi = PYPI.contains(&stack);
    let development = if pypi {
        PYPI_DEVELOPMENT
    } else {
        UNPINNED_DEVELOPMENT
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
    // What the stack's toolchain needs at the root: the workspace of a Rust project
    if let Some(layout) = &declared.layout {
        files.extend(
            layout
                .files
                .iter()
                .map(|file| (file.path.as_str(), file.text.trim_start().to_string())),
        );
    }
    if !pypi {
        return (
            files,
            Some(format!(
                "Rotproof is not pinned and no CI workflow is written: how a {stack} project installs Rotproof is \
                 not decided yet"
            )),
        );
    }
    files.push(("requirements-dev.txt", fill(PYPI_REQUIREMENTS)));
    files.push((".github/workflows/ci.yml", PYPI_WORKFLOW.to_string()));
    (files, None)
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

/// The guide for the stack named `stack`. `Err` when the stack is unknown.
pub fn guide_named(stack: &str) -> Result<String, String> {
    if stack == RECORDS_ONLY {
        return Ok(guide(stack, None));
    }
    match layout(stack) {
        Some(layout) => Ok(guide(stack, Some(&layout))),
        None => Err(format!(
            "unknown stack {stack:?} (known: {})",
            known_stacks().join(", ")
        )),
    }
}

/// The guide for a project of `stack`, whose layout is `layout` (`None` for a repository of records only).
pub fn guide(stack: &str, layout: Option<&Layout>) -> String {
    let (kept, checked, layers) = match layout {
        None => (
            format!(
                "Rotproof keeps the records in `docs/` in this project. It has no layers: its stack is \"{stack}\"."
            ),
            "the records",
            String::new(),
        ),
        Some(layout) => (
            "Rotproof keeps two things in this project: the layers (where code lives and what it may import) and the\n\
             records in `docs/`."
                .to_string(),
            "the layers and the records",
            layers_section(layout),
        ),
    };
    GUIDE_TEXT
        .replace("{version}", env!("CARGO_PKG_VERSION"))
        .replace("{stack}", stack)
        .replace("{kept}", &kept)
        .replace("{checked}", checked)
        .replace("{layers}", &layers)
}

/// The section on the layers: the rules, and the table of where each layer lives in this stack and what it may import.
fn layers_section(layout: &Layout) -> String {
    let has = |name: &String| !layout.without.contains(name);
    let mut rows = vec![
        "| Layer | Where | May import |".to_string(),
        "|---|---|---|".to_string(),
    ];
    for place in layout.places(&table()) {
        let imports = if place.parent.is_some() {
            let below = if place.below.is_empty() {
                ""
            } else {
                "The levels below it, and "
            };
            format!("{below}{}", listed(&place.imports))
        } else if place.name == "ui" {
            "Nothing: it holds only its levels".to_string()
        } else if place.imports.is_empty() {
            "No other layer".to_string()
        } else {
            listed(&place.imports)
        };
        rows.push(format!(
            "| `{}` | `{}/` | {imports} |",
            place.name, place.path
        ));
    }
    let not_layers = if layout.not_layers.is_empty() {
        String::new()
    } else {
        // Paths from the root, as `unchecked` writes them, on a line of their own in the list item
        format!(
            "\n  {} {}, and may hold code.",
            listed(&layout.not_layers),
            if layout.not_layers.len() == 1 {
                "is not a layer"
            } else {
                "are not layers"
            }
        )
    };
    let ui = if has(&"ui".to_string()) { UI_TEXT } else { "" };
    let direction = match layout.language {
        Language::Python | Language::TypeScript => DIRECTION_IMPORTS,
        Language::Rust => DIRECTION_CARGO,
    };
    LAYERS_TEXT
        .replace("{direction}\n", direction)
        .replace("{not_layers}", &not_layers)
        .replace("{table}", &format!("{}\n", rows.join("\n")))
        .replace("{ui}", ui)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::layers::STACKS;

    #[test]
    fn every_stack_has_a_guide_with_every_placeholder_filled() {
        let mut guides: Vec<String> = STACKS
            .iter()
            .map(|(stack, _)| guide(stack, layout(stack).as_ref()))
            .collect();
        guides.push(guide(RECORDS_ONLY, None));
        for text in &guides {
            for placeholder in [
                "{version}",
                "{stack}",
                "{kept}",
                "{checked}",
                "{layers}",
                "{not_layers}",
                "{table}",
                "{ui}",
                "{direction}",
            ] {
                assert!(
                    !text.contains(placeholder),
                    "{placeholder} left in:\n{text}"
                );
            }
            assert!(text.contains(env!("CARGO_PKG_VERSION")));
        }
    }

    #[test]
    fn the_table_lists_every_place_of_the_layout() {
        for (stack, _) in STACKS {
            let layout = layout(stack).unwrap();
            let text = guide(stack, Some(&layout));
            let places = layout.places(&table());
            assert!(!places.is_empty());
            for place in places {
                let row = format!("| `{}` | `{}/` |", place.name, place.path);
                assert!(text.contains(&row), "{stack}: no row {row}");
            }
            assert_eq!(
                text.contains("atomic design"),
                !layout.without.contains(&"ui".to_string()),
                "{stack}: the rule on ui goes with the layer"
            );
        }
    }

    #[test]
    fn every_file_in_project_is_built_in() {
        // What this module includes, read from its own source, against what `project/` holds: a text added there and
        // never included would be edited in vain
        let source = include_str!("project.rs");
        let included: std::collections::BTreeSet<String> = source
            .match_indices("include_str!(\"../../project/")
            .map(|(at, prefix)| {
                let rest = &source[at + prefix.len()..];
                rest[..rest.find('"').unwrap()].to_string()
            })
            .collect();
        let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("project");
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
        use crate::domain::layers::Declaration;
        for stack in known_stacks() {
            let declared = Declared::new(Declaration {
                stack: stack.to_string(),
                areas: Vec::new(),
                absent: Vec::new(),
                unchecked: Vec::new(),
            })
            .unwrap();
            let (files, not_written) = project_files(&declared, "x");
            assert_eq!(PYPI.contains(&stack), not_written.is_none(), "{stack}");
            assert!(files.iter().any(|(path, _)| *path == ".gitignore"));
        }
    }

    #[test]
    fn a_repository_of_records_only_has_no_section_on_layers() {
        let text = guide(RECORDS_ONLY, None);
        assert!(!text.contains("## The layers"));
        assert!(text.contains("## The records"));
    }
}
