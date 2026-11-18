//! The structure check, read from the tree: the declaration, the paths the rules name and the code files of the
//! directories they list, judged by the rules in `domain`.

use std::collections::BTreeSet;
use std::io;

use crate::layers::read;
use crate::tree::{code_files, exactly};
use domain::code::is_source;
use domain::declaration::{DECLARATION, Declared};
use domain::paths::file_name;
use domain::structure::{Seen, declares_units, listed, named, problems as judged};
use protocol::inbound::TestFiles;
use protocol::outbound::Tree;

/// What the structure check found.
#[derive(Debug, Default)]
pub struct Structure {
    /// Every way the tree differs from its declaration
    pub found: Vec<String>,
    /// What was not checked, and why. Printed on every run, so a skipped check is never silent
    pub skipped: Option<String>,
    /// The declaration with its layout, when it can be read and fits, for the checks that follow
    pub declared: Option<Declared>,
    /// The areas it declares, when it can be read: the check of the records takes them from here, so the declaration
    /// is read once
    pub areas: Option<Vec<String>>,
    /// How many source files were read as tests, and by which patterns: `None` for a stack that tells no test by its
    /// name
    pub tests: Option<TestFiles>,
}

/// Every way `tree` differs from its declaration. An error is a file or directory that could not be read.
pub fn problems(tree: &dyn Tree) -> io::Result<Structure> {
    let declaration = match read(tree)? {
        Ok(declaration) => declaration,
        Err(why) => {
            return Ok(Structure {
                found: vec![why],
                ..Structure::default()
            });
        }
    };
    // The records are grouped by the areas even when the stack does not fit: they are checked all the same
    let areas = Some(declaration.areas.clone());
    let declared = match Declared::new(declaration) {
        Ok(declared) => declared,
        Err(found) => {
            return Ok(Structure {
                found,
                areas,
                ..Structure::default()
            });
        }
    };
    let Some(layout) = &declared.layout else {
        return Ok(Structure {
            found: Vec::new(),
            skipped: Some(format!(
                "the layers are not checked: {DECLARATION} declares stack = \"none\" (records only)"
            )),
            declared: Some(declared),
            areas,
            tests: None,
        });
    };
    let mut seen = Seen::default();
    for path in named(&declared) {
        let exact = exactly(tree, &path).map(|_| ());
        seen.named.insert(path, exact);
    }
    // A test is no code of a layer, as the files of Python's `tests/` are none: the rules on code outside the layers
    // and on views never see it. The source files among them are counted, once each, for what `check` says of them
    let mut tests = BTreeSet::new();
    for dir in listed(&declared, layout, &seen) {
        let mut code = code_files(tree, &dir, |name| layout.is_code_in(&dir, name))?;
        code.retain(|path| {
            if !layout.is_test_file(path) {
                return true;
            }
            if is_source(path) && !declared.is_not_checked(path) {
                tests.insert(path.clone());
            }
            false
        });
        seen.code.insert(dir, code);
    }
    let tests = layout.tests.as_ref().map(|names| TestFiles {
        count: tests.len(),
        patterns: names.patterns(),
    });
    // Every manifest of the tree, which only a project that declares units is judged by: it is where a package that no
    // unit says it is shows
    if declares_units(&declared) {
        seen.manifests = tree
            .files("")?
            .into_iter()
            .filter(|path| layout.is_manifest(file_name(path)))
            .collect();
    }
    Ok(Structure {
        found: judged(&declared, layout, &seen),
        skipped: None,
        declared: Some(declared),
        areas,
        tests,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::fake::Fake;

    #[test]
    fn the_manifests_of_the_whole_tree_are_read_when_units_are_declared_and_only_then() {
        let layers = [
            "domain/__init__.py",
            "infrastructure/__init__.py",
            "infrastructure/inbound/__init__.py",
            "infrastructure/outbound/__init__.py",
        ];
        let absent = "absent = [\"composition\", \"application\", \"protocol\"]\n";
        let declaration = format!(
            "stack = \"python\"\nareas = [\"a\"]\nui_design = \"none\"\n\n[[unit]]\nname = \"a\"\npath = \"a\"\n{absent}"
        );
        let mut files: Vec<(String, &str)> = layers
            .iter()
            .map(|layer| (format!("a/{layer}"), ""))
            .collect();
        files.extend([
            ("pyproject.toml".to_string(), ""),
            ("a/pyproject.toml".to_string(), ""),
            ("tools/pyproject.toml".to_string(), ""),
            (".config/rotproof.toml".to_string(), &declaration),
        ]);
        let files: Vec<(&str, &str)> = files.iter().map(|(p, t)| (p.as_str(), *t)).collect();
        let found = problems(&Fake::new(&files)).unwrap().found;
        assert_eq!(
            found,
            [
                "tools/pyproject.toml is a manifest outside every unit's root: declare the unit whose root is \
                 tools (root = \"tools\"), or list tools in unchecked in .config/rotproof.toml"
            ]
        );
        // The same stray manifest in a project of one unit is no finding
        let one = format!("stack = \"python\"\nareas = [\"a\"]\nui_design = \"none\"\n{absent}");
        let mut files: Vec<(String, &str)> =
            layers.iter().map(|layer| (layer.to_string(), "")).collect();
        files.extend([
            ("tools/pyproject.toml".to_string(), ""),
            (".config/rotproof.toml".to_string(), &one),
        ]);
        let files: Vec<(&str, &str)> = files.iter().map(|(p, t)| (p.as_str(), *t)).collect();
        let found = problems(&Fake::new(&files)).unwrap().found;
        // `tools/` holds code of no layer only through its manifest, which is no code
        assert!(found.is_empty(), "{found:#?}");
    }

    #[test]
    fn a_test_beside_the_code_is_no_code_outside_the_layers_and_no_view_and_is_counted_once() {
        let declaration = "stack = \"typescript\"\nareas = [\"a\"]\nui_design = \"atomic\"\nunchecked = [\"src/legacy\"]\n";
        let layers = [
            "composition",
            "domain",
            "application",
            "protocol/inbound",
            "protocol/outbound",
            "infrastructure/inbound",
            "infrastructure/outbound",
            "ui/atoms",
            "ui/molecules",
            "ui/organisms",
            "ui/templates",
            "ui/pages",
            "presentation/models",
            "presentation/display",
        ];
        let mut files: Vec<(String, &str)> =
            vec![(".config/rotproof.toml".to_string(), declaration)];
        for layer in layers {
            files.push((format!("src/{layer}/index.ts"), ""));
        }
        for layer in ["protocol", "infrastructure", "ui", "presentation"] {
            files.push((format!("src/{layer}/index.ts"), ""));
        }
        files.extend(
            [
                // In a layer, beside its levels, in a level that holds no view, in no layer, and in a directory of
                // tests
                "src/domain/a.test.ts",
                "src/ui/a.spec.ts",
                "src/presentation/models/useA.test.tsx",
                "src/a.test.ts",
                "src/__tests__/setup.ts",
                // Not source, so not counted: a file in a directory of tests is a test, and a JSON file with the name
                // of one is not
                "src/__tests__/data.json",
                // In a path `unchecked` keeps
                "src/legacy/old.test.ts",
            ]
            .map(|path| (path.to_string(), "")),
        );
        let found = |files: &[(String, &str)]| {
            let files: Vec<(&str, &str)> = files.iter().map(|(p, t)| (p.as_str(), *t)).collect();
            problems(&Fake::new(&files)).unwrap()
        };
        let passes = found(&files);
        assert!(passes.found.is_empty(), "{:#?}", passes.found);
        // The same name that is no test's is code beside the levels of `ui`, as it is anywhere
        files.push(("src/ui/b.test.json".to_string(), ""));
        let fails = found(&files);
        assert_eq!(fails.found.len(), 1, "{:#?}", fails.found);
        assert!(
            fails.found[0].starts_with("code in ui outside its levels: src/ui/b.test.json ("),
            "{:#?}",
            fails.found
        );
        let tests = passes.tests.unwrap();
        assert_eq!(tests.count, 5, "{tests:?}");
        assert_eq!(tests.patterns, ["*.test.*", "*.spec.*", "__tests__/"]);
    }

    #[test]
    fn the_tree_is_read_through_the_port_alone() {
        // No file system: a declaration, two layers, one in another case, and code outside the layers
        let tree = Fake::new(&[
            (
                ".config/rotproof.toml",
                "stack = \"python\"\nareas = [\"a\"]\nui_design = \"none\"\nabsent = [\"composition\", \"application\", \
                 \"protocol\"]\n",
            ),
            ("domain/__init__.py", ""),
            ("Infrastructure/__init__.py", ""),
            ("scripts/run.py", ""),
        ]);
        let found = problems(&tree).unwrap().found;
        assert_eq!(
            found,
            [
                "infrastructure is missing: infrastructure (Infrastructure is there: names are compared exactly, as \
                 Linux and GitHub compare them)",
                "code outside the layers: Infrastructure (move it into a layer, or list it in unchecked in \
                 .config/rotproof.toml)",
                "code outside the layers: scripts (move it into a layer, or list it in unchecked in \
                 .config/rotproof.toml)",
            ]
        );
    }
}
