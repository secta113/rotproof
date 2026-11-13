//! The files Rotproof writes outside `docs/`.
//!
//! - **Rotproof's guide, `.rotproof/AGENTS.md`,** which every `rotproof create` rewrites and `rotproof check` compares,
//!   as it does the generated files in `docs/`. It says the rules Rotproof keeps in the project's stack: how to run
//!   Rotproof, the layers (their table is written from `layers/table.toml`, so the two cannot disagree) and the
//!   records. It names the version that wrote it, so an upgrade fails `rotproof check` until `rotproof create` has run.
//! - **The project's files** ([`project_files`]): `AGENTS.md`, `CLAUDE.md`, `README.md`, `.gitignore`,
//!   `.gitattributes`, a GitHub Actions workflow that installs the pinned Rotproof and runs `rotproof check` (with
//!   `requirements-dev.txt` where the stack installs it from PyPI; for `typescript` the pin goes in `package.json`,
//!   the project's own file, which Rotproof does not write), and the files the
//!   stack's layout lists for its toolchain (the workspace's `Cargo.toml` for `rust`). `rotproof create` writes each
//!   once, when it does not exist, and never again: from then on it is the project's.
//!
//! The texts are in `project/`, built into the binary. Each is named after the file it becomes, without a leading dot
//! and with `.in` added, so no agent working in Rotproof reads a project's `AGENTS.md` as its own.

use crate::layers::{
    Declared, Language, Layout, RECORDS_ONLY, fullest_ui, importable, known_stacks, layout, listed,
    table,
};

/// Rotproof's guide, from the root
pub const GUIDE: &str = ".rotproof/AGENTS.md";

const GUIDE_TEXT: &str = include_str!("../../../project/rotproof/AGENTS.md.in");
const LAYERS_TEXT: &str = include_str!("../../../project/rotproof/layers.md.in");
const UI_TEXT: &str = include_str!("../../../project/rotproof/ui.md.in");
const ENVIRONMENT_TEXT: &str = include_str!("../../../project/rotproof/environment.md.in");
const TYPES_PYTHON: &str = include_str!("../../../project/rotproof/types-python.md.in");
const TYPES_TYPESCRIPT: &str = include_str!("../../../project/rotproof/types-typescript.md.in");
/// How the direction is read: the imports of a source file, or the dependencies of a crate
const DIRECTION_IMPORTS: &str = include_str!("../../../project/rotproof/direction-imports.md.in");
const DIRECTION_CARGO: &str = include_str!("../../../project/rotproof/direction-cargo.md.in");

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

/// The guide for the stack named `stack`. `Err` when the stack is unknown.
pub fn guide_named(stack: &str) -> Result<String, String> {
    if stack == RECORDS_ONLY {
        return Ok(guide(stack, None));
    }
    match layout(stack, &fullest_ui(stack)) {
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
    let places = layout.places(&table());
    for place in &places {
        let importable = importable(place);
        let has_levels = places
            .iter()
            .any(|p| p.parent.as_ref() == Some(&place.name));
        let imports = if has_levels {
            "Nothing: it holds only its levels".to_string()
        } else if importable.is_empty() {
            "No other layer".to_string()
        } else {
            let mut chars = importable.chars();
            chars
                .next()
                .into_iter()
                .flat_map(char::to_uppercase)
                .chain(chars)
                .collect()
        };
        rows.push(format!(
            "| `{}` | `{}/` | {imports} |",
            place.name, place.path
        ));
    }
    if let Some(entry) = layout.entry(&table()) {
        rows.push(format!(
            "| The entry file, no layer | `{}` | {} |",
            entry.path,
            listed(&entry.imports)
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
    let mut ui = if has(&"ui".to_string()) { UI_TEXT } else { "" }.to_string();
    let no_view = layout.no_view.as_ref().filter(|n| n.layers.iter().any(has));
    if let Some(no_view) = no_view {
        let names: Vec<String> = no_view.layers.iter().filter(|n| has(n)).cloned().collect();
        let files: Vec<String> = no_view
            .extensions
            .iter()
            .map(|e| format!("`.{e}`"))
            .collect();
        ui.push_str(&format!(
            "- **{} holds no view, and `rotproof check` fails on a {} file there.** Rotproof tells a view by the \
             extension alone, so a view written without JSX (`createElement` in a `.ts` file) passes: do not write \
             one. To fix a finding, {}.\n",
            listed(&names),
            files.join(" or "),
            no_view.goes
        ));
    }
    let direction = match layout.language {
        Language::Python | Language::TypeScript => DIRECTION_IMPORTS,
        Language::Rust => DIRECTION_CARGO,
    };
    // Rust's compiler has no comment that switches it off
    let types = match layout.language {
        Language::Python => TYPES_PYTHON,
        Language::TypeScript => TYPES_TYPESCRIPT,
        Language::Rust => "",
    };
    let reads = match layout.language {
        Language::Python => {
            "`os.environ` and `os.getenv` (and the other names of `os` that touch it)"
        }
        Language::TypeScript => {
            "`process.env` and `import.meta.env` (except what Vite sets itself, such as `import.meta.env.DEV`)"
        }
        Language::Rust => {
            "`std::env::var` and the other functions of `std::env` that read or set a variable"
        }
    };
    LAYERS_TEXT
        .replace("{direction}\n", direction)
        .replace("{environment}", &ENVIRONMENT_TEXT.replace("{reads}", reads))
        .replace("{types}", types)
        .replace("{not_layers}", &not_layers)
        .replace("{table}", &format!("{}\n", rows.join("\n")))
        .replace("{ui}", &ui)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layers::{NO_UI, STACKS, uis};

    #[test]
    fn every_stack_has_a_guide_with_every_placeholder_filled() {
        let mut guides: Vec<String> = STACKS
            .iter()
            .flat_map(|(stack, _)| {
                uis(stack)
                    .unwrap()
                    .into_iter()
                    .map(move |ui| guide(stack, layout(stack, &ui).as_ref()))
            })
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
    fn the_guide_says_where_a_view_is_not_allowed_only_in_the_stack_that_checks_it() {
        let said = "`presentation` holds no view, and `rotproof check` fails on a `.tsx` or `.jsx` file there";
        let ts = |ui: &str| guide("typescript", layout("typescript", ui).as_ref());
        assert!(ts("atomic").contains(said));
        assert!(ts("atomic").contains("move the component into src/ui"));
        assert!(!ts(NO_UI).contains("holds no view"));
        assert!(!guide("python", layout("python", "atomic").as_ref()).contains("holds no view"));
        assert!(!guide("rust", layout("rust", NO_UI).as_ref()).contains("holds no view"));
    }

    #[test]
    fn the_table_lists_every_place_of_the_layout() {
        let every = STACKS.iter().flat_map(|(stack, _)| {
            uis(stack)
                .unwrap()
                .into_iter()
                .map(move |ui| (*stack, layout(stack, &ui).unwrap()))
        });
        for (stack, layout) in every {
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
    fn the_table_says_what_each_place_may_import_as_its_documentation_does() {
        let layout = layout("python", "atomic").unwrap();
        let text = guide("python", Some(&layout));
        for row in [
            "| `ui` | `ui/` | Nothing: it holds only its levels |",
            "| `ui.pages` | `ui/pages/` | The levels below it, and `presentation.models`, `presentation.display` and \
             `protocol.inbound` |",
            "| `ui.organisms` | `ui/organisms/` | The levels below it, and `presentation.display` |",
            "| `ui.molecules` | `ui/molecules/` | The levels below it |",
            "| `ui.atoms` | `ui/atoms/` | No other layer |",
            "| `presentation` | `presentation/` | Nothing: it holds only its levels |",
            "| `presentation.models` | `presentation/models/` | The levels below it, and `protocol.inbound` |",
            "| `presentation.display` | `presentation/display/` | No other layer |",
            "| `infrastructure` | `infrastructure/` | Nothing: it holds only its levels |",
            "| `infrastructure.inbound` | `infrastructure/inbound/` | `protocol.inbound` |",
            "| `protocol.outbound` | `protocol/outbound/` | `domain` |",
            "| `domain` | `domain/` | No other layer |",
            "| The entry file, no layer | `__main__.py` | `composition` |",
        ] {
            assert!(text.contains(row), "no row {row}:\n{text}");
        }
    }

    #[test]
    fn every_file_in_project_is_built_in() {
        // What this module includes, read from its own source, against what `project/` holds: a text added there and
        // never included would be edited in vain
        let source = include_str!("project.rs");
        let included: std::collections::BTreeSet<String> = source
            .match_indices("include_str!(\"../../../project/")
            .map(|(at, prefix)| {
                let rest = &source[at + prefix.len()..];
                rest[..rest.find('"').unwrap()].to_string()
            })
            .collect();
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
        use crate::layers::Declaration;
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

    #[test]
    fn a_repository_of_records_only_has_no_section_on_layers() {
        let text = guide(RECORDS_ONLY, None);
        assert!(!text.contains("## The layers"));
        assert!(text.contains("## The records"));
    }
}
