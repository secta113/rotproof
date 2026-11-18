//! The project's files Rotproof writes once ([`project_files`]): `README.md`, `.gitignore`, `.gitattributes`, a GitHub
//! Actions workflow that installs the pinned Rotproof and runs `rotproof check` (with
//! a `pyproject.toml` that pins it in `dev` of `[dependency-groups]` where the stack installs it from PyPI, and a
//! workflow for the tool of the lock file the project has (uv's when it has none); for `typescript` the pin goes in
//! `package.json`, the project's own file, which Rotproof does not write), and the files the stack's layout lists for
//! its toolchain (the workspace's `Cargo.toml` for `rust`). `rotproof create` writes each once, when it does not exist,
//! and never again: from then on it is the project's. Rotproof's guide, which it rewrites, is `guide.rs`.
//!
//! It writes no `AGENTS.md` or `CLAUDE.md`, nor anything else an agent reads by itself: what an agent is told is the
//! project's to decide, and the project points its agent at the guide with a line of its own. When the project keeps
//! a map of its layers in its `AGENTS.md`, [`stale_map`] says what that map has gone behind on.
//!
//! The texts are in `project/`, built into the binary. Each is named after the file it becomes, without a leading dot
//! and with `.in` added, so no agent working in Rotproof reads a text meant for a project as its own.

use crate::declaration::{DECLARATION, Declared};
use crate::locked::{LOCK_NAMES, Locked, Tool};

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
    /// PyPI, pinned in `dev` of the `[dependency-groups]` of `pyproject.toml`, which the project's lock file locks and
    /// its tool installs
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
const PYPI_PYPROJECT: &str = include_str!("../../../project/pypi/pyproject.toml.in");
/// Tool -> its workflow and the part of the README on development
const PYPI: [(Tool, &str, &str); 4] = [
    (
        Tool::Uv,
        include_str!("../../../project/pypi/uv/ci.yml.in"),
        include_str!("../../../project/pypi/uv/development.md.in"),
    ),
    (
        Tool::Poetry,
        include_str!("../../../project/pypi/poetry/ci.yml.in"),
        include_str!("../../../project/pypi/poetry/development.md.in"),
    ),
    (
        Tool::Pdm,
        include_str!("../../../project/pypi/pdm/ci.yml.in"),
        include_str!("../../../project/pypi/pdm/development.md.in"),
    ),
    (
        Tool::Pip,
        include_str!("../../../project/pypi/pip/ci.yml.in"),
        include_str!("../../../project/pypi/pip/development.md.in"),
    ),
];
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
/// the root -> text, and what was not written for the stack, and why. `crates` are the directories of the crates the
/// project has, which the workspace of a Rust project lists as its members. `python` is what the tree of a Python
/// project holds that decides what is written for it.
pub fn project_files<'a>(
    declared: &'a Declared,
    name: &str,
    crates: &[String],
    python: &PythonTree,
) -> (Vec<(&'a str, String)>, Option<String>) {
    let stack = declared.declaration.stack.as_str();
    let fill = |text: &str| {
        text.replace("{name}", name)
            .replace("{stack}", stack)
            .replace("{version}", env!("CARGO_PKG_VERSION"))
            .replace("{lock}", &python.locked.file)
    };
    let channel = channel(stack);
    let (workflow, python_development) = PYPI
        .iter()
        .find(|(tool, _, _)| *tool == python.locked.tool)
        .map(|(_, workflow, development)| (*workflow, *development))
        .expect("every tool has a workflow: a test reads every tool");
    let development = match channel {
        Channel::Pypi => python_development,
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
            "README.md",
            fill(&README_TEXT.replace("{development}", development)),
        ),
        (".gitignore", gitignore.to_string()),
        (".gitattributes", GITATTRIBUTES_TEXT.to_string()),
    ];
    // What the stack's toolchain needs at the root: the workspace and the root package of a Rust project, and with
    // units the workspace alone. The workspace lists the crates one to a line
    if let Some(layout) = &declared.layout {
        let members = members(crates);
        let units = !declared.declaration.units.is_empty();
        files.extend(layout.root_files(units).iter().map(|file| {
            (
                file.path.as_str(),
                file.text
                    .trim_start()
                    .replace("{package}", &package(name))
                    .replace("{members}", &members),
            )
        }));
    }
    match channel {
        Channel::Pypi => {
            files.push((
                "pyproject.toml",
                fill(&PYPI_PYPROJECT.replace("{package}", &distribution(name))),
            ));
            files.push((".github/workflows/ci.yml", fill(workflow)));
            return (files, python.not_written());
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

/// What the tree of a Python project holds that decides what `rotproof create` writes: which lock file it has, and
/// whether its `pyproject.toml` is there already without Rotproof in it (the project's own, which Rotproof writes no
/// pin into).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PythonTree {
    /// The lock file at the root and its tool, uv's when the root has none
    pub locked: Locked,
    /// Whether the root has a lock file at all
    pub has_lock: bool,
    /// Whether `pyproject.toml` is at the root and never names `rotproof`
    pub unpinned: bool,
}

impl Default for PythonTree {
    /// A tree with neither a manifest nor a lock file
    fn default() -> Self {
        PythonTree {
            locked: Locked::of_names(&[]),
            has_lock: false,
            unpinned: false,
        }
    }
}

impl PythonTree {
    /// What `rotproof create` leaves to the project: the pin in a `pyproject.toml` that was there before it, and the
    /// lock file, which only a tool can make.
    fn not_written(&self) -> Option<String> {
        let mut left = Vec::new();
        if self.unpinned {
            left.push(format!(
                "the pin in pyproject.toml, which is the project's: until `dev` of its [dependency-groups] pins \
                 Rotproof (rotproof=={}), the CI workflow fails",
                env!("CARGO_PKG_VERSION")
            ));
        }
        if !self.has_lock {
            left.push(format!(
                "the lock file ({LOCK_NAMES}), which a tool makes from pyproject.toml (`uv lock`): until one is \
                 committed, `rotproof check` fails"
            ));
        }
        (!left.is_empty()).then(|| left.join("; and "))
    }
}

/// The project's name as the name of a distribution (PEP 508), which takes letters, digits, `-`, `_` and `.`, and
/// starts and ends with a letter or a digit: every other character becomes `-`, and the ends are trimmed; `app` when
/// nothing is left.
fn distribution(name: &str) -> String {
    let name: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c
            } else {
                '-'
            }
        })
        .collect();
    let name = name.trim_matches(|c: char| !c.is_ascii_alphanumeric());
    if name.is_empty() {
        "app".to_string()
    } else {
        name.to_string()
    }
}

/// The members of a workspace, as the array holds them: each crate's directory on a line of its own, and nothing for a
/// project with none.
fn members(crates: &[String]) -> String {
    let lines: String = crates
        .iter()
        .map(|dir| format!("\n    \"{dir}\","))
        .collect();
    if lines.is_empty() {
        lines
    } else {
        format!("{lines}\n")
    }
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

/// What `rotproof check` says, as a warning, when the map is out of date
pub const MAP_HEADING: &str =
    "the map in AGENTS.md lists the layers .config/rotproof.toml declares";

/// How many findings of a stale map are listed before the rest is counted
const LISTED: usize = 6;

/// What is out of date in the map of the project's `AGENTS.md`, `agents` being its text, against the layers `declared`
/// has: a layer present with no row (a layer that was declared absent and then made, or a unit added after the map was
/// written), and a row for a layer the declaration says is absent. The rows are found by the path in the first cell,
/// under the heading `## Map`; their words are the project's, as it rewrote them, and are never judged. Nothing when
/// there is no such heading and table: a project may keep its map as it likes, or not at all.
pub fn stale_map(declared: &Declared, agents: &str) -> Vec<String> {
    let mut lines = agents
        .lines()
        .skip_while(|line| line.trim_end() != "## Map");
    if lines.next().is_none() {
        return Vec::new();
    }
    let rows: Vec<&str> = lines
        .take_while(|line| !line.starts_with("## "))
        .filter_map(|line| line.strip_prefix("| `")?.split_once('`'))
        .map(|(path, _)| path.trim_end_matches('/'))
        .collect();
    if rows.is_empty() {
        return Vec::new();
    }
    let named = declared.units.iter().any(|unit| unit.declared);
    let whose = |place: &crate::layers::Place| match place.unit.name.as_str() {
        name if named && !name.is_empty() => {
            format!("the layer {} of the unit {name}", place.name)
        }
        _ => format!("the layer {}", place.name),
    };
    let mut found = Vec::new();
    for place in declared.places.iter().filter(|p| p.is_layer()) {
        let listed = rows.contains(&place.path.as_str());
        if declared.is_absent(place) {
            if listed {
                found.push(format!(
                    "AGENTS.md has a row in its map for {}/, {}, which {DECLARATION} declares absent: take the row out",
                    place.path,
                    whose(place)
                ));
            }
        } else if !listed {
            found.push(format!(
                "AGENTS.md has no row in its map for {}/, {}: add it, or declare the layer absent in {DECLARATION}",
                place.path,
                whose(place)
            ));
        }
    }
    if found.len() > LISTED {
        let more = found.len() - LISTED;
        found.truncate(LISTED);
        found.push(format!("and {more} more"));
    }
    found
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
                moving: Vec::new(),
                todo_link: None,
                approved_max: None,
                files: None,
                declined: Vec::new(),
                dependencies: Default::default(),
                units: Vec::new(),
            })
            .unwrap();
            let (files, not_written) = project_files(&declared, "x", &[], &PythonTree::default());
            // Only the pin npm keeps in package.json, the project's own file, and the lock file a Python project's
            // tool makes are left to the project
            assert_eq!(
                matches!(channel(stack), Channel::Npm | Channel::Pypi),
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
    fn the_workspace_of_a_rust_project_names_its_crates_and_has_no_package_with_units() {
        use crate::declaration::parse_declaration;
        let crates = [
            "crates/a/domain".to_string(),
            "crates/a/protocol".to_string(),
        ];
        let workspace = |declaration: &str| {
            let declared = Declared::new(parse_declaration(declaration).unwrap()).unwrap();
            let (files, _) = project_files(&declared, "my-app", &crates, &PythonTree::default());
            files
                .into_iter()
                .map(|(path, text)| (path.to_string(), text))
                .filter(|(path, _)| path == "Cargo.toml" || path == "src/main.rs")
                .collect::<Vec<_>>()
        };
        let one = workspace(
            "stack = \"rust\"\nareas = []
ui_design = \"none\"\n",
        );
        assert_eq!(one.len(), 2);
        assert!(one[0].1.contains("name = \"my-app\"") && !one[0].1.contains("[dependencies]"));
        assert!(
            one[0]
                .1
                .contains("members = [\n    \"crates/a/domain\",\n    \"crates/a/protocol\",\n]\n")
        );
        let units = workspace(
            "stack = \"rust\"\nareas = []
ui_design = \"none\"\n[[unit]]\nname = \"a\"\npath = \"crates/a\"\n",
        );
        assert_eq!(
            units,
            [(
                "Cargo.toml".to_string(),
                "[workspace]\nmembers = [\n    \"crates/a/domain\",\n    \"crates/a/protocol\",\n]\nresolver = \"3\"\n"
                    .to_string()
            )]
        );
        // No crate, no member
        assert_eq!(members(&[]), "");
    }

    /// The declaration of a Python project of two units: `core` (shared, with only `domain`) and `cli` (an app, with
    /// `infrastructure.inbound` and `composition`, and `protocol` when `protocol` says so).
    fn two_units(cli_absent: &str) -> Declared {
        let text = format!(
            "stack = \"python\"\nareas = []\nui_design = \"none\"\n\
             [[unit]]\nname = \"core\"\npath = \"packages/core/src/core\"\nshared = true\n\
             absent = [\"composition\", \"infrastructure\", \"application\", \"protocol\"]\n\
             [[unit]]\nname = \"cli\"\npath = \"apps/cli/src/cli\"\nentry = [\"apps/cli/src/cli/__main__.py\"]\n\
             absent = [{cli_absent}]\n"
        );
        Declared::new(crate::declaration::parse_declaration(&text).unwrap()).unwrap()
    }

    /// A map as a project might keep it in its `AGENTS.md`: a row for each layer `declared` has present, with words of
    /// the project's own.
    fn kept_map(declared: &Declared) -> String {
        let rows: String = declared
            .places
            .iter()
            .filter(|p| p.is_layer() && !declared.is_absent(p))
            .map(|place| format!("| `{}/` | What {} holds here |\n", place.path, place.name))
            .collect();
        format!(
            "# AGENTS.md\n\n## Map\n\n| Path | Content |\n|---|---|\n{rows}\n## Rules\n\n- Mine.\n"
        )
    }

    #[test]
    fn a_map_is_stale_for_a_layer_with_no_row_and_a_row_of_a_layer_declared_absent() {
        let absent = "\"domain\", \"application\", \"protocol\", \"infrastructure.outbound\"";
        let declared = two_units(absent);
        let written = kept_map(&declared);
        assert_eq!(stale_map(&declared, &written), Vec::<String>::new());
        // The words are the project's: what a person rewrote in a row is never judged
        let rewritten = written.replace("What domain holds here", "Mine, the rules");
        assert_ne!(rewritten, written);
        assert_eq!(stale_map(&declared, &rewritten), Vec::<String>::new());
        // `protocol` declared absent when the map was made, and made after
        let later = two_units("\"domain\", \"application\", \"infrastructure.outbound\"");
        assert_eq!(
            stale_map(&later, &written),
            [
                "AGENTS.md has no row in its map for apps/cli/src/cli/protocol/, the layer protocol of the unit cli: add it, or declare the layer absent in .config/rotproof.toml"
            ]
        );
        // The other way: the row stays after the layer is declared absent
        assert_eq!(
            stale_map(&declared, &kept_map(&later)),
            [
                "AGENTS.md has a row in its map for apps/cli/src/cli/protocol/, the layer protocol of the unit cli, which .config/rotproof.toml declares absent: take the row out"
            ]
        );
        // No map, no judgement: the file is the project's
        assert!(stale_map(&declared, "# Mine\n\nNo table here.\n").is_empty());
        assert!(
            stale_map(
                &declared,
                "## Map\n\nIn prose.\n\n## Rules\n\n| `core/` | x |\n"
            )
            .is_empty()
        );
        // Only the table under `## Map` is read
        let elsewhere = written.replace("## Map", "## Layers");
        assert!(stale_map(&later, &elsewhere).is_empty());
    }

    #[test]
    fn a_map_that_is_far_behind_lists_the_first_rows_and_counts_the_rest() {
        let units: String = (0..8)
            .map(|n| {
                format!(
                    "[[unit]]\nname = \"u{n}\"\npath = \"packages/u{n}/src/u{n}\"\n\
                     absent = [\"composition\", \"infrastructure\", \"application\", \"protocol\"]\n"
                )
            })
            .collect();
        let declared = Declared::new(
            crate::declaration::parse_declaration(&format!(
                "stack = \"python\"
areas = []
ui_design = \"none\"
{units}"
            ))
            .unwrap(),
        )
        .unwrap();
        let found = stale_map(
            &declared,
            "## Map\n\n| Path | Content |\n|---|---|\n| `docs/` | Records |\n",
        );
        assert_eq!(found.len(), LISTED + 1, "{found:?}");
        assert!(
            found[..LISTED]
                .iter()
                .all(|line| line.starts_with("AGENTS.md has no row in its map for "))
        );
        assert!(found[LISTED].starts_with("and "), "{found:?}");
    }

    /// The files of a Python project made as `name` over a root that holds `names`, with a `pyproject.toml` that does
    /// not pin Rotproof when `unpinned`.
    fn python_files(
        name: &str,
        names: &[&str],
        unpinned: bool,
    ) -> (Vec<(String, String)>, Option<String>) {
        let declared = Declared::new(
            crate::declaration::parse_declaration(
                "stack = \"python\"\nareas = []\nui_design = \"none\"\n",
            )
            .unwrap(),
        )
        .unwrap();
        let owned: Vec<String> = names.iter().map(|name| name.to_string()).collect();
        let python = PythonTree {
            locked: Locked::of_names(&owned),
            has_lock: names.iter().any(|name| crate::locked::is_lock(name)),
            unpinned,
        };
        let (files, not_written) = project_files(&declared, name, &[], &python);
        (
            files
                .into_iter()
                .map(|(path, text)| (path.to_string(), text))
                .collect(),
            not_written,
        )
    }

    #[test]
    fn a_python_project_pins_rotproof_in_its_manifest_and_its_workflow_installs_the_lock_of_its_tool()
     {
        let text = |files: &[(String, String)], path: &str| {
            files
                .iter()
                .find(|(file, _)| file == path)
                .map(|(_, text)| text.clone())
        };
        let version = env!("CARGO_PKG_VERSION");
        let (files, _) = python_files("my app", &[], false);
        // No requirements file: the pin is in the dev group of the manifest, which names the project as PEP 508 does
        assert_eq!(text(&files, "requirements-dev.txt"), None);
        let manifest = text(&files, "pyproject.toml").unwrap();
        assert!(manifest.contains("name = \"my-app\""), "{manifest}");
        assert!(
            manifest.contains(&format!(
                "[dependency-groups]\ndev = [\"rotproof=={version}\"]"
            )),
            "{manifest}"
        );
        let ignore = text(&files, ".gitignore").unwrap();
        assert!(ignore.lines().any(|line| line == ".venv/"), "{ignore}");
        // The workflow is written for the tool of the lock file that is there, uv's when none is
        for (names, install) in [
            (&[][..], "uv sync --locked"),
            (&["uv.lock"][..], "uv sync --locked"),
            (&["poetry.lock"][..], "poetry install"),
            (&["pdm.lock"][..], "pdm install --check"),
            (&["pylock.dev.toml"][..], "pip install -r pylock.dev.toml"),
        ] {
            let (files, _) = python_files("x", names, false);
            let workflow = text(&files, ".github/workflows/ci.yml").unwrap();
            assert!(workflow.contains(install), "{names:?}: {workflow}");
            assert!(workflow.contains("rotproof check"), "{names:?}");
            assert!(!workflow.contains("requirements-dev.txt"), "{names:?}");
            assert!(!workflow.contains('{'), "{names:?}: {workflow}");
            let readme = text(&files, "README.md").unwrap();
            assert!(
                !readme.contains("{lock}") && !readme.contains("{version}"),
                "{readme}"
            );
        }
    }

    #[test]
    fn a_python_project_is_told_what_rotproof_leaves_to_it_only_while_it_is_left() {
        let (_, none) = python_files("x", &[], false);
        let none = none.unwrap();
        assert!(
            none.contains("the lock file (uv.lock, poetry.lock") && !none.contains("the pin"),
            "{none}"
        );
        // A manifest of the project's own that does not pin Rotproof, with a lock file
        let (_, pin) = python_files("x", &["uv.lock"], true);
        let pin = pin.unwrap();
        assert!(
            pin.starts_with("the pin in pyproject.toml") && !pin.contains("lock file"),
            "{pin}"
        );
        // Both, and neither
        let (_, both) = python_files("x", &[], true);
        assert!(both.unwrap().contains("; and the lock file"));
        assert_eq!(python_files("x", &["pdm.lock"], false).1, None);
    }

    #[test]
    fn the_name_of_a_distribution_is_made_of_what_a_name_may_hold() {
        assert_eq!(distribution("my app"), "my-app");
        assert_eq!(distribution("shop_2.0"), "shop_2.0");
        assert_eq!(distribution("-x-"), "x");
        assert_eq!(distribution("日本"), "app");
    }
}
