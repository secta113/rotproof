//! The project's files Rotproof writes once ([`project_files`]): `AGENTS.md`, `CLAUDE.md`, `README.md`, `.gitignore`,
//! `.gitattributes`, a GitHub Actions workflow that installs the pinned Rotproof and runs `rotproof check` (with
//! `requirements-dev.txt` where the stack installs it from PyPI; for `typescript` the pin goes in `package.json`, the
//! project's own file, which Rotproof does not write), and the files the stack's layout lists for its toolchain (the
//! workspace's `Cargo.toml` for `rust`). `rotproof create` writes each once, when it does not exist, and never again:
//! from then on it is the project's. Rotproof's guide, which it rewrites, is `guide.rs`.
//!
//! The texts are in `project/`, built into the binary. Each is named after the file it becomes, without a leading dot
//! and with `.in` added, so no agent working in Rotproof reads a project's `AGENTS.md` as its own.

use crate::declaration::{DECLARATION, Declared};

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
/// the root -> text, and what was not written for the stack, and why. `crates` are the directories of the crates the
/// project has, which the workspace of a Rust project lists as its members.
pub fn project_files<'a>(
    declared: &'a Declared,
    name: &str,
    crates: &[String],
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

/// The rows of the map for the layers present in the declaration, each with the first sentence of its role, which the
/// project rewrites into what the layer holds there. A project that declares units has a row for the layers of each
/// unit, with the unit's name in front of the role: the same layer in two units is two rows that would read alike.
fn map(declared: &Declared) -> String {
    let units = declared.units.iter().any(|unit| unit.declared);
    declared
        .places
        .iter()
        .filter(|p| p.is_layer() && !declared.is_absent(p))
        .map(|place| {
            let role = first_sentence(&place.role);
            let role = match place.unit.name.as_str() {
                name if units && !name.is_empty() => format!("{name}: {role}"),
                _ => role,
            };
            format!("| `{}/` | {} |\n", place.path, role.replace('|', "\\|"))
        })
        .collect()
}

/// The first sentence of a role, with the lines of its paragraph joined: up to the first full stop that ends a word
/// (not the one of `e.g.` or `i.e.`), or the whole paragraph when it has none.
fn first_sentence(role: &str) -> String {
    let paragraph = role
        .trim()
        .lines()
        .take_while(|line| !line.trim().is_empty())
        .map(str::trim)
        .collect::<Vec<_>>()
        .join(" ");
    let end = paragraph
        .char_indices()
        .filter(|(at, c)| {
            *c == '.' && !paragraph[..*at].ends_with("e.g") && !paragraph[..*at].ends_with("i.e")
        })
        .map(|(at, _)| at + 1)
        .find(|end| {
            paragraph[*end..]
                .chars()
                .next()
                .is_none_or(char::is_whitespace)
        });
    paragraph[..end.unwrap_or(paragraph.len())].to_string()
}

/// What `rotproof check` says, as a warning, when the map is out of date
pub const MAP_HEADING: &str =
    "the map in AGENTS.md lists the layers .config/rotproof.toml declares";

/// How many findings of a stale map are listed before the rest is counted
const LISTED: usize = 6;

/// What is out of date in the map of the project's `AGENTS.md`, `agents` being its text, against the layers `declared`
/// has: a layer present with no row (a layer that was declared absent and then made, or a unit added after the file was
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
    use crate::layers::{NO_UI, RECORDS_ONLY, known_stacks, uis};

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
            let (files, not_written) = project_files(&declared, "x", &[]);
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
    fn the_workspace_of_a_rust_project_names_its_crates_and_has_no_package_with_units() {
        use crate::declaration::parse_declaration;
        let crates = [
            "crates/a/domain".to_string(),
            "crates/a/protocol".to_string(),
        ];
        let workspace = |declaration: &str| {
            let declared = Declared::new(parse_declaration(declaration).unwrap()).unwrap();
            let (files, _) = project_files(&declared, "my-app", &crates);
            files
                .into_iter()
                .map(|(path, text)| (path.to_string(), text))
                .filter(|(path, _)| path == "Cargo.toml" || path == "src/main.rs")
                .collect::<Vec<_>>()
        };
        let one = workspace(
            "stack = \"rust\"\nareas = []
ui = \"none\"\n",
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
ui = \"none\"\n[[unit]]\nname = \"a\"\npath = \"crates/a\"\n",
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
            "stack = \"python\"\nareas = []\nui = \"none\"\n\
             [[unit]]\nname = \"core\"\npath = \"packages/core/src/core\"\nshared = true\n\
             absent = [\"composition\", \"infrastructure\", \"application\", \"protocol\"]\n\
             [[unit]]\nname = \"cli\"\npath = \"apps/cli/src/cli\"\nentry = [\"apps/cli/src/cli/__main__.py\"]\n\
             absent = [{cli_absent}]\n"
        );
        Declared::new(crate::declaration::parse_declaration(&text).unwrap()).unwrap()
    }

    #[test]
    fn the_first_sentence_of_a_role_is_whole_and_ends_at_a_full_stop_that_ends_a_word() {
        assert_eq!(
            first_sentence(
                "The view: what the user sees (e.g. a button). `composition` registers it.\nMore."
            ),
            "The view: what the user sees (e.g. a button)."
        );
        assert_eq!(
            first_sentence(
                "The concepts and rules of the business: types, calculations and decisions. No I/O,\nand no ports."
            ),
            "The concepts and rules of the business: types, calculations and decisions."
        );
        // Over the lines of a paragraph, and no further than it
        assert_eq!(
            first_sentence("One sentence that\nruns on\n\nThe next paragraph."),
            "One sentence that runs on"
        );
        assert_eq!(first_sentence("`a.b` is a name. It."), "`a.b` is a name.");
    }

    #[test]
    fn every_role_of_every_layer_has_a_first_sentence_that_ends_at_a_full_stop_and_fits_a_cell() {
        use crate::declaration::Declaration;
        for stack in known_stacks()
            .into_iter()
            .filter(|stack| *stack != RECORDS_ONLY)
        {
            for ui in uis(stack).unwrap() {
                let declared = Declared::new(Declaration {
                    stack: stack.to_string(),
                    areas: Vec::new(),
                    ui: Some(ui.clone()),
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
                for place in declared.places.iter().filter(|p| p.is_layer()) {
                    let sentence = first_sentence(&place.role);
                    assert!(
                        sentence.ends_with('.'),
                        "{stack} {ui} {}: {sentence}",
                        place.name
                    );
                    assert!(!sentence.contains('\n'), "{stack} {ui} {}", place.name);
                }
            }
        }
    }

    #[test]
    fn a_project_that_declares_units_has_a_row_for_each_layer_of_each_unit_with_the_unit_named() {
        let declared =
            two_units("\"domain\", \"application\", \"infrastructure.outbound\", \"protocol\"");
        let rows = map(&declared);
        let cell = |row: &str| {
            row.split(" | ")
                .nth(1)
                .unwrap()
                .trim_end_matches(" |")
                .to_string()
        };
        let lines: Vec<&str> = rows.lines().collect();
        assert_eq!(
            lines
                .iter()
                .map(|line| line.split(" | ").next().unwrap())
                .collect::<Vec<_>>(),
            [
                "| `packages/core/src/core/domain/`",
                "| `apps/cli/src/cli/composition/`",
                "| `apps/cli/src/cli/infrastructure/`",
            ]
        );
        assert_eq!(
            cell(lines[0]),
            "core: The concepts and rules of the business: types, calculations and decisions."
        );
        assert!(
            cell(lines[1])
                .starts_with("cli: The composition root: the one place that wires the program."),
            "{rows}"
        );
        // A project of one unit names none
        let one = Declared::new(
            crate::declaration::parse_declaration(
                "stack = \"python\"\nareas = []\nui = \"none\"\n",
            )
            .unwrap(),
        )
        .unwrap();
        assert!(
            map(&one)
                .lines()
                .all(|line| line.starts_with("| `") && !line.contains("core: "))
        );
        assert!(map(&one).contains(
            "| `composition/` | The composition root: the one place that wires the program. |\n"
        ));
    }

    #[test]
    fn a_map_is_stale_for_a_layer_with_no_row_and_a_row_of_a_layer_declared_absent() {
        let absent = "\"domain\", \"application\", \"protocol\", \"infrastructure.outbound\"";
        let declared = two_units(absent);
        let written = AGENTS_TEXT.replace("{map}", &map(&declared));
        assert_eq!(stale_map(&declared, &written), Vec::<String>::new());
        // The words are the project's: what a person rewrote in a row is never judged
        let rewritten = written.replace("core: The concepts and rules", "Mine, the rules");
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
            stale_map(&declared, &AGENTS_TEXT.replace("{map}", &map(&later))),
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
ui = \"none\"
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
}
