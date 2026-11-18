//! The direction check of the templates of a Python project that declares `ui_design = "rendered"`: every reference one
//! template makes to another, judged by the levels of `ui` the two sit in as an import is.
//!
//! A reference is a path in the directory of the unit's `ui`, where the loader of the template language looks. It
//! lands in the level that holds the file it names. A reference that names no file there (a template of a package, a
//! name the loader finds elsewhere) is not judged, so is one that is not a literal, or climbs out of the directory.
//! The references are read as the template languages the layout names, at the versions it names, except where the
//! unit's `pyproject.toml` asks for another version of a language's package: `rotproof check` says which.

use std::io;

use crate::tree::{code_files, is_file, read_code, resolved};
use domain::direction::{Forbidden, Way, judged_landing, landing_of, place_of};
use domain::layers::{Layout, Place};
use domain::paths::join;
use domain::templates::{Premise, path_of, read_as, reading, references};
use domain::units::Unit;
use protocol::outbound::Tree;

/// The references between the templates of every unit that has a `ui`, judged into `forbidden`, and what the check
/// says of how they were read. `None` for a layout whose views are not templates.
pub(super) fn templates(
    tree: &dyn Tree,
    layout: &Layout,
    places: &[&Place],
    units: &[Unit],
    problems: &mut Vec<String>,
    forbidden: &mut Vec<Forbidden>,
) -> io::Result<Option<Premise>> {
    let Some(config) = &layout.templates else {
        return Ok(None);
    };
    let mut premise = Premise {
        read_as: read_as(config),
        not_judged: Vec::new(),
    };
    for unit in units {
        let Some(ui) = places
            .iter()
            .find(|place| place.name == "ui" && place.unit.name == unit.name)
        else {
            continue;
        };
        let manifest = join(&unit.root, &layout.manifest);
        let text = if is_file(tree, &manifest) {
            read_code(tree, &manifest, "versions", problems)?
        } else {
            None
        };
        let read = reading(config, &manifest, text.as_deref());
        premise.not_judged.extend(read.not_judged(&unit.name));
        let tags = read.tags();
        if tags.is_empty() {
            continue;
        }
        for path in code_files(tree, &ui.path, |name| layout.is_template(name))? {
            let Some(from) = place_of(&path, places) else {
                continue;
            };
            let Some(source) = read_code(tree, &path, "references", problems)? else {
                continue;
            };
            for reference in references(&source, &tags) {
                let Some(name) = path_of(&reference.target) else {
                    continue;
                };
                let at = resolved(tree, &join(&ui.path, &name));
                if !is_file(tree, &at) {
                    continue;
                }
                let Some(to) = landing_of(&at, places, units) else {
                    continue;
                };
                forbidden.extend(judged_landing(
                    &path,
                    reference.line,
                    &reference.target,
                    &format!("{} {}", reference.verb(), reference.target),
                    from,
                    to,
                    &Way::default(),
                ));
            }
        }
    }
    Ok(Some(premise))
}

#[cfg(test)]
mod tests {
    use super::super::{Direction, problems};
    use crate::tree::fake::Fake;
    use domain::declaration::{Declared, parse_declaration};
    use infrastructure::outbound::readers::Readers;

    /// A project that renders on the server and keeps no `presentation`.
    const DECLARATION: &str = "stack = \"python\"\nareas = []\nui_design = \"rendered\"\nabsent = [\"presentation\"]\nfiles = \"2.0.0\"\n";

    fn run(declaration: &str, files: &[(&str, &str)]) -> Direction {
        let declared = Declared::new(parse_declaration(declaration).unwrap())
            .unwrap_or_else(|e| panic!("{e:#?}"));
        problems(&Fake::new(files), &Readers, &declared).unwrap()
    }

    fn forbidden(direction: &Direction) -> Vec<&str> {
        direction
            .forbidden
            .iter()
            .map(|f| f.detail.as_str())
            .collect()
    }

    #[test]
    fn a_template_may_refer_to_itself_and_to_the_levels_below_its_own_and_to_no_other() {
        let direction = run(
            DECLARATION,
            &[
                (
                    "ui/pages/home.html",
                    "{% extends \"templates/base.html\" %}\n{% include \"pages/part.html\" %}",
                ),
                ("ui/pages/part.html", ""),
                (
                    "ui/templates/base.html",
                    "{% include \"organisms/nav.html\" %}\n{% import \"atoms/forms.html\" as forms %}",
                ),
                (
                    "ui/organisms/nav.html",
                    "{% include 'molecules/search.html' %}",
                ),
                (
                    "ui/molecules/search.html",
                    "{% from \"atoms/forms.html\" import field %}",
                ),
                (
                    "ui/atoms/forms.html",
                    "{% include \"atoms/forms.html\" %}\n{% extends \"templates/base.html\" %}\n{% include \"pages/home.html\" %}",
                ),
            ],
        );
        assert!(direction.problems.is_empty(), "{:?}", direction.problems);
        assert_eq!(
            forbidden(&direction),
            [
                "ui/atoms/forms.html:2: extends templates/base.html, in ui.templates; ui.atoms imports no other layer",
                "ui/atoms/forms.html:3: includes pages/home.html, in ui.pages; ui.atoms imports no other layer",
            ]
        );
        // Each is a reference by file and name, which an approval keys
        assert_eq!(direction.forbidden[0].file, "ui/atoms/forms.html");
        assert_eq!(direction.forbidden[0].import, "templates/base.html");
    }

    #[test]
    fn a_reference_that_names_no_file_of_the_project_is_not_judged() {
        let direction = run(
            DECLARATION,
            &[
                (
                    "ui/atoms/a.html",
                    "{% extends \"admin/base.html\" %}{% include \"pages/missing.html\" %}\n{% include \"../pages/home.html\" %}{% include \"/pages/home.html\" %}\n{% include name %}{% include \"pages\" %}{% include \"ui/pages/home.html\" %}",
                ),
                ("ui/pages/home.html", ""),
            ],
        );
        assert!(direction.problems.is_empty(), "{:?}", direction.problems);
        assert!(
            direction.forbidden.is_empty(),
            "{:?}",
            forbidden(&direction)
        );
    }

    #[test]
    fn the_check_says_how_the_references_were_read_on_every_run() {
        let direction = run(DECLARATION, &[("ui/atoms/a.html", "")]);
        let premise = direction.templates.unwrap();
        assert_eq!(premise.read_as, "Jinja2 3 and Django 5/6");
        assert!(premise.not_judged.is_empty());
        // A project that is not rendered says nothing of templates, and reads none
        let direction = run(
            "stack = \"python\"\nareas = []\nui_design = \"atomic\"\nfiles = \"2.0.0\"\n",
            &[
                ("ui/atoms/a.html", "{% include \"pages/x.html\" %}"),
                ("ui/pages/x.html", ""),
            ],
        );
        assert!(direction.templates.is_none() && direction.forbidden.is_empty());
    }

    #[test]
    fn a_manifest_that_asks_for_another_version_leaves_the_references_unjudged_and_says_so() {
        let files = |manifest: &'static str| {
            vec![
                ("pyproject.toml", manifest),
                (
                    "ui/atoms/a.html",
                    "{% include \"pages/x.html\" %}{% from \"molecules/m.html\" import m %}",
                ),
                ("ui/pages/x.html", ""),
                ("ui/molecules/m.html", ""),
            ]
        };
        // Jinja2 left out: only the tags Django has are read, so `from` is not
        let direction = run(
            DECLARATION,
            &files("[project]\ndependencies = [\"jinja2>=4\", \"django>=5\"]\n"),
        );
        assert_eq!(
            forbidden(&direction),
            [
                "ui/atoms/a.html:1: includes pages/x.html, in ui.pages; ui.atoms imports no other layer"
            ]
        );
        assert_eq!(
            direction.templates.unwrap().not_judged,
            [
                "references are not read as Jinja2 3: pyproject.toml asks for jinja2>=4, which is not Jinja2 3"
            ]
        );
        // Both left out: nothing is judged
        let direction = run(
            DECLARATION,
            &files("[project]\ndependencies = [\"jinja2>=4\", \"django>=7\"]\n"),
        );
        assert!(direction.forbidden.is_empty());
        assert_eq!(
            direction.templates.unwrap().not_judged,
            [
                "references are not judged: pyproject.toml asks for jinja2>=4, which is not Jinja2 3; \
                 pyproject.toml asks for django>=7, which is not Django 5/6"
            ]
        );
        // The manifest cannot be read: not judged either, and said
        let direction = run(DECLARATION, &files("[project\n"));
        assert!(direction.forbidden.is_empty());
        assert!(
            direction.templates.unwrap().not_judged[0]
                .starts_with("references are not judged: pyproject.toml cannot be read (")
        );
    }

    #[test]
    fn each_unit_is_read_through_the_manifest_beside_its_own_layers() {
        let declaration = format!(
            "{}\n[[unit]]\nname = \"billing\"\npath = \"packages/billing/src/billing\"\nroot = \"packages/billing\"\nabsent = [\"presentation\"]\n\n[[unit]]\nname = \"web\"\npath = \"web\"\nabsent = [\"presentation\"]\n",
            DECLARATION.replace("absent = [\"presentation\"]\n", "")
        );
        let direction = run(
            &declaration,
            &[
                (
                    "packages/billing/pyproject.toml",
                    "[project]\ndependencies = [\"jinja2<3\", \"django>=7\"]\n",
                ),
                (
                    "packages/billing/src/billing/ui/atoms/a.html",
                    "{% include \"pages/x.html\" %}",
                ),
                ("packages/billing/src/billing/ui/pages/x.html", ""),
                ("web/ui/atoms/a.html", "{% include \"pages/x.html\" %}"),
                ("web/ui/pages/x.html", ""),
            ],
        );
        assert_eq!(
            forbidden(&direction),
            [
                "web/ui/atoms/a.html:1: includes pages/x.html, in ui.pages; ui.atoms imports no other layer"
            ]
        );
        assert_eq!(
            direction.templates.unwrap().not_judged,
            [
                "unit billing: references are not judged: packages/billing/pyproject.toml asks for jinja2<3, which \
                 is not Jinja2 3; packages/billing/pyproject.toml asks for django>=7, which is not Django 5/6"
            ]
        );
    }
}
