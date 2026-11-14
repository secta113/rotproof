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

/// The version whose table splits the UI into the view (`ui`) and the Presentation Model (`presentation`), and whose
/// declaration says which UI a project has (`ui`). Crossing it, `rotproof init` says what moves, and moves nothing:
/// which part of a page is the state of the screen and which is its view, no machine can tell.
pub const PRESENTATION: &str = "1.0.0-rc.3";

/// What crossing [`PRESENTATION`] asks of a project whose `ui` is there, which the upgrade declares `ui = "atomic"`:
/// the new layer is missing until it is made, and `rotproof check` fails until then.
pub const PRESENTATION_ADDED: &str = "the layer presentation is new, and comes with ui = \"atomic\": what a screen \
                                      holds and decides goes in presentation.models, and the plain data it hands the \
                                      view in presentation.display. Run `rotproof create --yes` to make it";

/// What crossing [`PRESENTATION`] asks of a project that declared `ui` absent, which the upgrade declares
/// `ui = "none"`: the entry in absent names a layer the declaration no longer has.
pub const UI_NONE: &str = "ui = \"none\" now says that this project has no screens, so neither ui nor presentation \
                           is made or checked: take ui out of absent in .config/rotproof.toml";

/// What moved at [`PRESENTATION`] inside `ui`, said to a project that has it.
pub const UI_SPLIT: &str = "ui imports neither application nor domain any more: the state of a page and its calls \
                            of the use cases move to a model in presentation.models, which the page uses, and the \
                            domain values that organisms and templates show become plain values in \
                            presentation.display";

/// Whether an upgrade from `files` to `running` crosses the version `at`.
pub fn crosses(at: &str, files: &str, running: &str) -> bool {
    let at = version(at);
    version(files) < at && at <= version(running)
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
    fn a_retired_layer_is_where_the_layout_would_hold_it() {
        let rust = layout("rust", "none").unwrap();
        let found = retired(&rust);
        let names: Vec<&str> = found.iter().map(|(name, _, _)| *name).collect();
        assert_eq!(names, ["handler", "utils"]);
        assert_eq!(found[0].1, "crates/handler");
        assert_eq!(found[1].2, RETIRED[1].1);
    }
}
