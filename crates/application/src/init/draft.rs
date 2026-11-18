//! The knowledge document of the units, `docs/knowledge/units.md`, which `rotproof init` drafts once for a project
//! that declares units, from the imports the direction check reads between them (`domain::units_doc` writes the text).

use crate::bundle::Bundle;
use crate::direction::problems as direction;
use crate::follows::current;
use crate::layers::read;
use domain::declaration::{DECLARATION, Declared};
use domain::units_doc::{self, DOCUMENT, FILE, KEY};
use protocol::inbound::Upgraded;
use protocol::outbound::{Parsers, Tree, Writer};

/// Draft the knowledge document of the units into `upgraded`, once: the project declares units, no document of
/// `docs/knowledge/` follows them (a deprecated one counts, as the person deprecated it on purpose) and none is named
/// `units.md` in any case, so a person's picture is never written over and a second run changes nothing. The draft is
/// tagged with the first of the declared areas and pinned at the declaration's units as they are now; its edges are the
/// imports the direction check reads between the units. The index of `docs/knowledge/` is written again with it, so
/// that `rotproof check` passes right after. Nothing for a project that declares no unit, or whose declaration cannot
/// be read (the other checks say why); with no area to tag it with, or with units that cannot be followed, nothing is
/// written and `not_drafted` says why.
pub fn draft(
    tree: &dyn Tree,
    parsers: &dyn Parsers,
    out: &dyn Writer,
    upgraded: &mut Upgraded,
) -> Result<(), String> {
    let Ok(declaration) = read(tree).map_err(|e| e.to_string())? else {
        return Ok(());
    };
    let areas = declaration.areas.clone();
    let Ok(declared) = Declared::new(declaration) else {
        return Ok(());
    };
    let units: Vec<_> = declared
        .units
        .iter()
        .filter(|unit| unit.declared)
        .cloned()
        .collect();
    if units.is_empty() || declared.layout.is_none() || tree.found(FILE).is_some() {
        return Ok(());
    }
    let folder = Bundle::new(tree, areas.clone())
        .read_knowledge()
        .map_err(|e| e.to_string())?;
    let named = |name: &String| name.eq_ignore_ascii_case(DOCUMENT);
    let followed = folder
        .documents
        .values()
        .any(|(doc, _)| doc.follows.iter().any(|(key, _)| key == KEY));
    if followed
        || folder.documents.keys().any(named)
        || folder.guides.keys().any(named)
        || folder.problems.keys().any(named)
    {
        return Ok(());
    }
    let Some(area) = areas.first() else {
        upgraded.not_drafted = Some(format!(
            "{FILE} is not drafted: {DECLARATION} declares no areas, and a knowledge document is tagged with one: add \
             an area to areas, then run `rotproof init` again"
        ));
        return Ok(());
    };
    let Some(hash) = current(tree, parsers, KEY)? else {
        upgraded.not_drafted = Some(format!(
            "{FILE} is not drafted: {DECLARATION} has no `[[unit]]` table for a document to follow: write the units as \
             tables"
        ));
        return Ok(());
    };
    let found = direction(tree, parsers, &declared).map_err(|e| e.to_string())?;
    out.write(FILE, &units_doc::draft(&units, &found.links, area, &hash))
        .map_err(|e| format!("{FILE}: {e}"))?;
    // The document is in the index now
    upgraded
        .made
        .written
        .extend(crate::index::index(tree, out)?.written);
    upgraded.drafted = Some(FILE.to_string());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::init::init;
    use crate::tree::fake::Fake;
    use infrastructure::outbound::readers::Readers;
    use protocol::inbound::Initialized;

    /// The declaration of a Python project of three units, each in a directory of its name: `core` (shared),
    /// `billing`, and the app `cli`, with `areas` as given (a list in TOML) and, when `files` is up to the running
    /// version, nothing to upgrade.
    fn units_declaration(areas: &str) -> String {
        format!(
            "stack = \"python\"\nareas = {areas}\nui_design = \"none\"\nfiles = \"{}\"\ndeclined = []\n\n\
             [[unit]]\nname = \"core\"\npath = \"core\"\nshared = true\n\n\
             [[unit]]\nname = \"billing\"\npath = \"billing\"\n\n\
             [[unit]]\nname = \"cli\"\npath = \"cli\"\nentry = [\"cli/main.py\"]\n",
            domain::version::VERSION
        )
    }

    /// The code of the units: `billing` imports `core`, and `cli` wires `billing` in its composition.
    const UNIT_CODE: [(&str, &str); 4] = [
        ("core/domain/score.py", ""),
        (
            "billing/application/play.py",
            "from core.domain import score\n",
        ),
        (
            "cli/composition/wire.py",
            "from billing.application import play\n",
        ),
        ("cli/main.py", ""),
    ];

    fn units_project(areas: &str) -> Fake {
        let tree = Fake::new(&UNIT_CODE);
        tree.write(DECLARATION, &units_declaration(areas)).unwrap();
        tree
    }

    /// `rotproof init` on `tree` as the command runs it: the upgrade, then the draft.
    fn upgrade_and_draft(tree: &Fake) -> Upgraded {
        let Ok(Initialized::Upgraded(mut upgraded)) =
            init(tree, tree, None, None, "x", domain::version::VERSION)
        else {
            panic!("upgraded");
        };
        draft(tree, &Readers, tree, &mut upgraded).unwrap();
        *upgraded
    }

    #[test]
    fn the_units_are_drafted_once_with_the_imports_between_them_and_a_second_run_writes_nothing() {
        let tree = units_project("[\"design\", \"ops\"]");
        let upgraded = upgrade_and_draft(&tree);
        assert_eq!(upgraded.drafted.as_deref(), Some(FILE));
        assert_eq!(upgraded.not_drafted, None);
        let text = tree.text(FILE).unwrap();
        // Tagged with the first area, pinned at the units as the declaration has them now, the edges the imports found
        assert!(text.contains("\ntags: [\"design\"]\n"), "{text}");
        let hash = current(&tree, &Readers, KEY).unwrap().unwrap();
        assert!(
            text.contains(&format!("follows:\n  {KEY}: \"{hash}\"\n")),
            "{text}"
        );
        for edge in ["billing --> core", "cli --> billing"] {
            assert!(text.contains(&format!("\n    {edge}\n")), "{text}");
        }
        assert!(!text.contains("cli --> core"), "{text}");
        for node in [
            "core([\"core (shared)\"])",
            "billing[\"billing\"]",
            "cli[\"cli (app)\"]",
        ] {
            assert!(text.contains(&format!("\n    {node}\n")), "{text}");
        }
        // The index of the knowledge documents is written with it, and lists it
        let index = tree.text("docs/knowledge/index.md").unwrap();
        assert!(index.contains("units.md"), "{index}");
        assert!(
            upgraded
                .made
                .written
                .iter()
                .any(|p| p == "docs/knowledge/index.md"),
            "{:?}",
            upgraded.made.written
        );

        // A second run writes nothing, and a document a person rewrote is left as it is
        let before = tree.text(FILE).unwrap();
        let again = upgrade_and_draft(&tree);
        assert_eq!((again.drafted, again.not_drafted), (None, None));
        assert!(again.made.written.is_empty(), "{:?}", again.made.written);
        assert_eq!(tree.text(FILE).unwrap(), before);
        let mine = text.replace("_Not written yet", "Written, and not");
        tree.write(FILE, &mine).unwrap();
        let again = upgrade_and_draft(&tree);
        assert_eq!(again.drafted, None);
        assert_eq!(tree.text(FILE).unwrap(), mine);
    }

    #[test]
    fn the_draft_keeps_the_rules_of_the_records_it_is_checked_by() {
        use chrono::DateTime;
        let tree = units_project("[\"design\"]");
        upgrade_and_draft(&tree);
        let now = DateTime::parse_from_rfc3339("2026-11-15T12:00:00+09:00").unwrap();
        let report = crate::check::check(&tree, &Readers, now).unwrap();
        // The layers are not made in this tree, which the structure check says; of the document, not a word
        let of_it: Vec<&str> = report
            .findings
            .iter()
            .map(|f| f.detail.as_str())
            .filter(|detail| detail.contains("units.md") || detail.contains("index.md"))
            .collect();
        assert!(of_it.is_empty(), "{of_it:#?}");
        assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    }

    #[test]
    fn a_document_that_follows_the_units_is_not_written_beside() {
        for status in ["stable", "deprecated"] {
            let tree = units_project("[\"design\"]");
            let closed = if status == "deprecated" {
                "\n# Resolution\n\nReplaced by the code.\n"
            } else {
                ""
            };
            tree.write(
                "docs/knowledge/picture.md",
                &format!(
                    "---\ntype: Knowledge\ntitle: The picture\ndescription: Why the units are as they are.\n\
                     tags: [design]\nstatus: {status}\nfollows:\n  {KEY}: \"0a1b2c3d\"\n---\n{closed}\n# Why\n\nThey are.\n"
                ),
            )
            .unwrap();
            let upgraded = upgrade_and_draft(&tree);
            assert_eq!(
                (upgraded.drafted, upgraded.not_drafted),
                (None, None),
                "{status}"
            );
            assert_eq!(tree.text(FILE), None, "{status}");
        }
        // A document that follows something else does not stand in for it
        let tree = units_project("[\"design\"]");
        tree.write(
            "docs/knowledge/other.md",
            "---\ntype: Knowledge\ntitle: Other\ndescription: Another.\ntags: [design]\nstatus: stable\n\
             follows:\n  core/domain/score.py: \"0a1b2c3d\"\n---\n\n# Why\n\nIt is.\n",
        )
        .unwrap();
        assert_eq!(upgrade_and_draft(&tree).drafted.as_deref(), Some(FILE));
    }

    #[test]
    fn a_document_of_that_name_is_never_written_over_in_any_case() {
        for name in ["units.md", "Units.md"] {
            let tree = units_project("[\"design\"]");
            let path = format!("docs/knowledge/{name}");
            // Not a document that follows the units, nor a valid one: it is a person's file
            tree.write(&path, "not a document\n").unwrap();
            let upgraded = upgrade_and_draft(&tree);
            assert_eq!(upgraded.drafted, None, "{name}");
            assert_eq!(tree.text(&path).as_deref(), Some("not a document\n"));
            if name != "units.md" {
                // A file of the same name in another case would not check out on every system
                assert_eq!(tree.text(FILE), None, "{name}");
            }
        }
    }

    #[test]
    fn with_no_area_nothing_is_written_and_the_reason_is_said() {
        let tree = units_project("[]");
        let upgraded = upgrade_and_draft(&tree);
        assert_eq!(upgraded.drafted, None);
        assert_eq!(tree.text(FILE), None);
        let why = upgraded.not_drafted.unwrap();
        assert!(
            why.contains("declares no areas") && why.contains("add an area"),
            "{why}"
        );
        // Said again on the next run, until there is an area; then it is drafted
        assert!(upgrade_and_draft(&tree).not_drafted.is_some());
        let text = tree.text(DECLARATION).unwrap();
        tree.write(
            DECLARATION,
            &text.replace("areas = []", "areas = [\"design\"]"),
        )
        .unwrap();
        let upgraded = upgrade_and_draft(&tree);
        assert_eq!(upgraded.drafted.as_deref(), Some(FILE));
        assert_eq!(upgraded.not_drafted, None);
    }

    #[test]
    fn units_written_inline_cannot_be_followed_and_that_is_said() {
        let tree = units_project("[\"design\"]");
        tree.write(
            DECLARATION,
            &format!(
                "stack = \"python\"\nareas = [\"design\"]\nui_design = \"none\"\nfiles = \"{}\"\ndeclined = []\n\
                 unit = [{{ name = \"core\", path = \"core\" }}]\n",
                domain::version::VERSION
            ),
        )
        .unwrap();
        let upgraded = upgrade_and_draft(&tree);
        assert_eq!(upgraded.drafted, None);
        assert_eq!(tree.text(FILE), None);
        assert!(
            upgraded
                .not_drafted
                .unwrap()
                .contains("no `[[unit]]` table"),
        );
    }

    #[test]
    fn a_project_that_declares_no_unit_has_no_draft_and_no_word_about_it() {
        let tree = Fake::new(&UNIT_CODE);
        tree.write(
            DECLARATION,
            &format!(
                "stack = \"python\"\nareas = [\"design\"]\nui_design = \"none\"\nfiles = \"{}\"\ndeclined = []\n",
                domain::version::VERSION
            ),
        )
        .unwrap();
        let upgraded = upgrade_and_draft(&tree);
        assert_eq!((upgraded.drafted, upgraded.not_drafted), (None, None));
        assert_eq!(tree.text(FILE), None);
        // Nor one whose declaration cannot be read: the checks say why
        tree.write(DECLARATION, "stack = ").unwrap();
        let mut upgraded = Upgraded::default();
        draft(&tree, &Readers, &tree, &mut upgraded).unwrap();
        assert!(upgraded.drafted.is_none() && upgraded.not_drafted.is_none());
    }
}
