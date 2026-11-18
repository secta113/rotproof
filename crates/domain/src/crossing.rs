//! What `rotproof init` says to a project when an upgrade crosses a version that changed what the layers are: which
//! versions those are, what each moved, and whether an upgrade crosses one.
//!
//! Rotproof moves none of it. A layer that was split or renamed has code in it that no machine can place, so an
//! upgrade says where each part goes and leaves it to the project, and `rotproof check` fails until it is done. The
//! updates that change a file are in `upgrade.rs`.

use crate::layers::Layout;
use crate::version::version;

/// The version whose table is built on the onion architecture. Crossing it, `rotproof init` says how to move a
/// project's layers, and moves nothing: an entry point and its wiring may sit in one file of `handler`, and each part
/// of `utils` goes where its concept is, which no machine can tell.
pub const ONION: &str = "0.4.0";

/// The layers of the table before [`ONION`], each with where its code goes now.
pub const RETIRED: [(&str, &str); 2] = [
    (
        "handler",
        "its entry points go in infrastructure.inbound, each calling an input port of protocol.inbound; the wiring \
         in composition; and the start of the program in the entry file, which calls composition alone",
    ),
    (
        "utils",
        "each part goes in the layer whose concept it speaks, and a general procedure comes from the standard library \
         or an external one",
    ),
];

/// What moved at [`ONION`] inside layers that are still there, which no directory shows.
pub const PORTS_MOVED: &str = "the ports (the interfaces to the outside) move from domain to protocol.outbound, and \
                               the use cases the entry points call get interfaces in protocol.inbound, with plain data";

/// The first version that reads no record of work. Crossing it, a project that has a `docs/work/` is told what that
/// directory is now, and nothing in it moves: whether its open items become issues or stay as history is the project's.
pub const WORK_STOPPED: &str = "1.0.0-rc.1";

/// What crossing [`WORK_STOPPED`] means for a project whose `docs/work/` is there.
pub const WORK_LEFT: &str = "Rotproof kept open work in docs/work/ before 1.0 and reads none of it now: it checks \
                             nothing there and writes no index.md, closed.md or rules.md, so those stay as they \
                             were, and docs/index.md no longer links to it. Open work is kept in issues: its open \
                             items need a home you choose (the project's issues, or kept as frozen history where \
                             they are). Nothing there is moved or deleted";

/// The version whose table splits the UI into the view (`ui`) and the Presentation Model (`presentation`), and whose
/// declaration says which UI a project has (`ui`). Crossing it, `rotproof init` says what moves, and moves nothing:
/// which part of a page is the state of the screen and which is its view, no machine can tell.
pub const PRESENTATION: &str = "1.0.0-rc.3";

/// What crossing [`PRESENTATION`] asks of a project whose `ui` is there, which the upgrade declares `ui_design =
/// "atomic"`: the new layer is missing until it is made, and `rotproof check` fails until then.
pub const PRESENTATION_ADDED: &str = "the layer presentation is new, and comes with ui_design = \"atomic\": what a screen \
                                      holds and decides goes in presentation.models, and the plain data it hands the \
                                      view in presentation.display. Run `rotproof create --yes` to make it";

/// What crossing [`PRESENTATION`] asks of a project that declared `ui` absent, which the upgrade declares
/// `ui_design = "none"`: the entry in absent names a layer the declaration no longer has.
pub const UI_NONE: &str = "ui_design = \"none\" now says that this project has no screens, so neither ui nor presentation \
                           is made or checked: take ui out of absent in .config/rotproof.toml";

/// What moved at [`PRESENTATION`] inside `ui`, said to a project that has it.
pub const UI_SPLIT: &str = "ui imports neither application nor domain any more: the state of a page and its calls \
                            of the use cases move to a model in presentation.models, which the page uses, and the \
                            domain values that organisms and templates show become plain values in \
                            presentation.display";

/// The version from which a Python project is locked: its dependencies are in `pyproject.toml`, a lock file is
/// committed, and Rotproof is pinned in the `dev` group of `[dependency-groups]` instead of `requirements-dev.txt`.
/// Crossing it, `rotproof init` says what the project does, and edits none of its files: which tool locks the project
/// is the project's to choose.
pub const LOCKED: &str = "2.0.0-rc.2";

/// What crossing [`LOCKED`] asks of a Python project, in the order to do it.
pub const LOCKED_MOVES: &str = "a Python project lists its dependencies in pyproject.toml and commits a lock file \
                                (uv.lock, poetry.lock, pdm.lock or pylock.toml), which `rotproof check` fails \
                                without: list them in pyproject.toml, lock them with the tool of the project, and keep \
                                the virtual environment in .venv, the one the tools make, which the .gitignore \
                                excludes (delete venv/ and any other)";

/// What crossing [`LOCKED`] asks of a project whose `requirements-dev.txt` pins Rotproof, and whose CI installs it.
pub const LOCKED_REQUIREMENTS: &str = "Rotproof is pinned in dev of [dependency-groups] in pyproject.toml now, and \
                                       requirements-dev.txt, like any requirements file, is no declaration: move \
                                       the pin there, take the file out, and change the install step of \
                                       .github/workflows/ci.yml to the lock (a new project's README says it for each \
                                       tool, as `rotproof create` writes it)";

/// Whether an upgrade from `files` to `running` crosses the version `at`.
pub fn crosses(at: &str, files: &str, running: &str) -> bool {
    let at = version(at);
    version(files) < at && at <= version(running)
}

/// What a project does after it has moved the code of the layers that are gone, said with the directories `paths` of
/// those it still had: the files no check reads (documents, scripts, configuration, comments) keep naming them, and
/// nothing fails when they do.
pub fn old_paths(paths: &[String]) -> String {
    let named = match paths {
        [] => String::new(),
        [only] => format!("{only}/"),
        [rest @ .., last] => format!(
            "{} and {last}/",
            rest.iter()
                .map(|path| format!("{path}/"))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    };
    format!(
        "once the code has moved, search the files Rotproof does not read (documents, scripts, configuration, \
         comments) for the paths of the layers that are gone ({named}): no check reads them, and they keep naming \
         directories that are not there"
    )
}

/// Where the layout would hold each retired layer, with where its code goes now.
pub fn retired(layout: &Layout) -> Vec<(&'static str, String, &'static str)> {
    RETIRED
        .iter()
        .map(|(name, goes)| (*name, layout.layer_path(name), *goes))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layers::layout;

    #[test]
    fn an_upgrade_crosses_a_version_it_starts_before_and_ends_at_or_after() {
        assert!(crosses(ONION, "0.3.0", "0.4.0"));
        assert!(crosses(ONION, "0.1.0", "1.0.0"));
        // Already at it, or short of it
        assert!(!crosses(ONION, "0.4.0", "1.0.0"));
        assert!(!crosses(ONION, "0.2.0", "0.3.9"));
        // A pre-release is ordered as SemVer orders it
        assert!(crosses(PRESENTATION, "1.0.0-rc.2", "1.0.0-rc.3"));
        assert!(!crosses(PRESENTATION, "1.0.0-rc.3", "1.0.0"));
    }

    #[test]
    fn the_old_paths_are_named_as_the_directories_that_were_found() {
        let said =
            |paths: &[&str]| old_paths(&paths.iter().map(|p| p.to_string()).collect::<Vec<_>>());
        assert!(said(&["crates/handler"]).contains("(crates/handler/)"));
        assert!(said(&["handler", "utils"]).contains("(handler/ and utils/)"));
        assert!(said(&["a", "b", "c"]).contains("(a/, b/ and c/)"));
        assert!(said(&["handler"]).contains("search the files Rotproof does not read"));
    }

    #[test]
    fn a_retired_layer_is_where_the_layout_would_hold_it() {
        let rust = layout("rust", "none").unwrap();
        let found = retired(&rust);
        let names: Vec<&str> = found.iter().map(|(name, _, _)| *name).collect();
        assert_eq!(names, ["handler", "utils"]);
        assert_eq!(found[0].1, "crates/handler");
        assert_eq!(found[1].2, RETIRED[1].1);
    }
}
