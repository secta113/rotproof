//! The packages of a TypeScript project's units: the `package.json` at each unit's `root`, with the `name` an import of
//! the package starts with, its `exports` (a string, a map of subpaths, a pattern with one `*`, conditions), `types`
//! and `main`. Where an import lands through them is the rule of `code.rs`
//! ([`Aliases::package`](domain::code::Aliases::package)); this reads the files that say it.
//!
//! As the aliases are, a file that cannot be read is a problem of the [`Aliases`](domain::code::Aliases), not an error,
//! so the check can name it; an error is a file the tree cannot read at all. A `package.json` that is not there is no
//! problem, and one with no `name` is not a package an import names.

use std::io;

use serde_json::Value;

use domain::code::{Package, Target};
use domain::paths::join;
use domain::tree::with_lf;
use protocol::outbound::Tree;

/// The packages of the units whose roots are `roots` (from the root of the project, each read once), and what could
/// not be read, each with the file it is in.
pub fn packages(tree: &dyn Tree, roots: &[String]) -> io::Result<(Vec<Package>, Vec<String>)> {
    let mut found: Vec<Package> = Vec::new();
    let mut problems = Vec::new();
    let mut seen: Vec<&String> = Vec::new();
    for root in roots {
        if seen.contains(&root) {
            continue;
        }
        seen.push(root);
        let config = join(root, "package.json");
        let text = match tree.read(&config) {
            Ok(text) => with_lf(&text),
            Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
            Err(e) if e.kind() == io::ErrorKind::InvalidData => {
                problems.push(format!(
                    "{config}: cannot be read as UTF-8, so the package it names is not known"
                ));
                continue;
            }
            Err(e) => return Err(io::Error::new(e.kind(), format!("{config}: {e}"))),
        };
        match serde_json::from_str::<Value>(&text) {
            Ok(json) => match package(&json, root, &config) {
                Ok(Some(package)) => found.push(package),
                Ok(None) => {}
                Err(problem) => problems.push(problem),
            },
            Err(e) => problems.push(format!(
                "{config}: cannot be read as JSON ({e}), so the package it names is not known"
            )),
        }
    }
    Ok((found, problems))
}

/// The package one `package.json` says, `None` when it names none, or the problem when Rotproof cannot read how it
/// exports.
fn package(json: &Value, root: &str, config: &str) -> Result<Option<Package>, String> {
    let Some(name) = json.get("name").and_then(Value::as_str) else {
        return Ok(None);
    };
    let text = |key: &str| json.get(key).and_then(Value::as_str).map(str::to_string);
    Ok(Some(Package {
        name: name.to_string(),
        root: root.to_string(),
        config: config.to_string(),
        exports: match json.get("exports") {
            None | Some(Value::Null) => None,
            Some(exports) => Some(entries(exports).map_err(|why| {
                format!("{config}: exports {why}, so the package {name} is not known")
            })?),
        },
        types: text("types").or_else(|| text("typings")),
        main: text("main"),
    }))
}

/// The entries of `exports`: a map whose keys all start with `.` is the subpaths, and anything else (a string, a list,
/// a map of conditions) is the entry `.`. The reason when it is none of these.
fn entries(exports: &Value) -> Result<Vec<(String, Target)>, String> {
    let Value::Object(map) = exports else {
        return Ok(vec![(".".to_string(), target(exports)?)]);
    };
    let dotted = map.keys().filter(|key| key.starts_with('.')).count();
    if dotted == 0 {
        return Ok(vec![(".".to_string(), target(exports)?)]);
    }
    if dotted != map.len() {
        return Err("mixes subpaths (keys that start with `.`) and conditions".to_string());
    }
    map.iter()
        .map(|(key, value)| Ok((key.clone(), target(value)?)))
        .collect()
}

/// What one entry leads to.
fn target(value: &Value) -> Result<Target, String> {
    match value {
        Value::String(path) => Ok(Target::Path(path.clone())),
        Value::Null => Ok(Target::Nothing),
        Value::Array(list) => list
            .iter()
            .map(target)
            .collect::<Result<_, _>>()
            .map(Target::Any),
        Value::Object(map) => map
            .iter()
            .map(|(condition, value)| Ok((condition.clone(), target(value)?)))
            .collect::<Result<_, _>>()
            .map(Target::Conditions),
        other => Err(format!(
            "holds {other}, which is no path, list or conditions"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::code::Reach;

    fn read(files: &[(&str, &str)], roots: &[&str]) -> (Vec<Package>, Vec<String>) {
        let root = tempfile::tempdir().unwrap();
        for (path, text) in files {
            let full = root.path().join(path);
            std::fs::create_dir_all(full.parent().unwrap()).unwrap();
            std::fs::write(full, text).unwrap();
        }
        let roots: Vec<String> = roots.iter().map(|r| r.to_string()).collect();
        packages(&crate::outbound::disk::Disk::new(root.path()), &roots).unwrap()
    }

    fn only(exports: &str) -> Package {
        let text = format!("{{ \"name\": \"@acme/billing\", \"exports\": {exports} }}");
        let (mut found, problems) = read(
            &[("packages/billing/package.json", &text)],
            &["packages/billing"],
        );
        assert_eq!(problems, Vec::<String>::new());
        found.pop().unwrap()
    }

    fn at(package: &Package, sub: &str) -> Option<String> {
        match package.reach(sub) {
            Reach::At(path) => path,
            Reach::Unlisted { .. } | Reach::Missing { .. } => None,
        }
    }

    #[test]
    fn a_package_json_with_a_name_makes_a_package_at_each_root_once() {
        let (found, problems) = read(
            &[
                (
                    "packages/billing/package.json",
                    "{ \"name\": \"@acme/billing\", \"types\": \"./src/index.ts\", \"main\": \"dist/index.js\" }",
                ),
                ("packages/nameless/package.json", "{ \"private\": true }"),
            ],
            &[
                "packages/billing",
                "packages/billing",
                "packages/nameless",
                "packages/none",
            ],
        );
        assert_eq!(problems, Vec::<String>::new());
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "@acme/billing");
        assert_eq!(found[0].config, "packages/billing/package.json");
        assert_eq!(found[0].exports, None);
        assert_eq!(found[0].types.as_deref(), Some("./src/index.ts"));
        assert_eq!(found[0].main.as_deref(), Some("dist/index.js"));
    }

    #[test]
    fn what_cannot_be_read_is_a_problem_once_for_each_file() {
        let (found, problems) = read(
            &[
                ("a/package.json", "{ not json"),
                ("b/package.json", "{ \"name\": \"b\", \"exports\": 3 }"),
                (
                    "c/package.json",
                    "{ \"name\": \"c\", \"exports\": { \".\": \"./x.ts\", \"import\": \"./y.ts\" } }",
                ),
                (
                    "d/package.json",
                    "{ \"name\": \"d\", \"exports\": { \"./x\": { \"import\": 4 } } }",
                ),
            ],
            &["a", "a", "b", "c", "d"],
        );
        assert!(found.is_empty(), "{found:?}");
        assert_eq!(problems.len(), 4, "{problems:#?}");
        assert!(problems[0].starts_with("a/package.json: cannot be read as JSON ("));
        assert!(
            problems[1].starts_with(
                "b/package.json: exports holds 3, which is no path, list or conditions"
            )
        );
        assert!(problems[1].ends_with("so the package b is not known"));
        assert_eq!(
            problems[2],
            "c/package.json: exports mixes subpaths (keys that start with `.`) and conditions, so the package c is not known"
        );
        assert!(problems[3].contains("holds 4"));
    }

    #[test]
    fn a_package_at_the_root_of_the_project_is_read_too() {
        let (found, _) = read(&[("package.json", "{ \"name\": \"acme\" }")], &[""]);
        assert_eq!(found[0].config, "package.json");
        assert_eq!(found[0].reach("src/x"), Reach::At(Some("src/x".into())));
    }

    #[test]
    fn exports_as_a_string_or_conditions_is_the_package_itself() {
        let string = only("\"./src/index.ts\"");
        assert_eq!(
            at(&string, ""),
            Some("packages/billing/src/index.ts".into())
        );
        assert!(matches!(string.reach("domain"), Reach::Unlisted { .. }));
        let conditions =
            only("{ \"node\": \"./n.js\", \"import\": \"./src/i.ts\", \"default\": \"./d.js\" }");
        assert_eq!(
            at(&conditions, ""),
            Some("packages/billing/src/i.ts".into())
        );
        let list = only("[\"./a.ts\", \"./b.ts\"]");
        assert_eq!(at(&list, ""), Some("packages/billing/a.ts".into()));
    }

    #[test]
    fn exports_as_subpaths_lists_exactly_the_subpaths_it_names() {
        let package = only(
            "{ \".\": \"./src/index.ts\", \"./protocol/inbound\": \"./src/protocol/inbound/index.ts\", \
             \"./package.json\": \"./package.json\" }",
        );
        assert_eq!(
            at(&package, ""),
            Some("packages/billing/src/index.ts".into())
        );
        assert_eq!(
            at(&package, "protocol/inbound"),
            Some("packages/billing/src/protocol/inbound/index.ts".into())
        );
        assert_eq!(
            at(&package, "package.json"),
            Some("packages/billing/package.json".into())
        );
        // A subpath that is a directory of one it lists is not listed
        assert_eq!(
            package.reach("protocol/inbound/x"),
            Reach::Unlisted {
                config: "packages/billing/package.json".into()
            }
        );
        assert!(matches!(package.reach("domain"), Reach::Unlisted { .. }));
    }

    #[test]
    fn a_pattern_with_one_star_takes_the_longest_prefix_and_what_the_star_matched() {
        let package = only(
            "{ \"./*\": \"./src/*.ts\", \"./protocol/*\": { \"types\": \"./src/protocol/*/index.ts\", \"default\": \"./x.js\" }, \
             \"./protocol/internal/*\": null, \"./css/*.css\": \"./styles/*.css\" }",
        );
        assert_eq!(
            at(&package, "domain"),
            Some("packages/billing/src/domain.ts".into())
        );
        assert_eq!(
            at(&package, "protocol/inbound"),
            Some("packages/billing/src/protocol/inbound/index.ts".into())
        );
        assert_eq!(
            at(&package, "css/a.css"),
            Some("packages/billing/styles/a.css".into())
        );
        // `null` takes a subpath out, and a star matches at least a character
        assert!(matches!(
            package.reach("protocol/internal/db"),
            Reach::Unlisted { .. }
        ));
        let only_protocol = only("{ \"./protocol/*\": \"./src/protocol/*.ts\" }");
        assert!(matches!(
            only_protocol.reach("protocol/"),
            Reach::Unlisted { .. }
        ));
        assert!(matches!(
            only_protocol.reach("protocol"),
            Reach::Unlisted { .. }
        ));
    }

    #[test]
    fn the_first_of_types_import_and_default_that_leads_to_a_path_is_taken() {
        let package = only(
            "{ \"./a\": { \"default\": \"./a-default.ts\", \"import\": \"./a-import.ts\", \"types\": \"./a-types.ts\" }, \
             \"./b\": { \"require\": \"./b.cjs\", \"import\": { \"node\": \"./n.js\", \"default\": \"./b-import.ts\" } }, \
             \"./c\": { \"require\": \"./c.cjs\", \"node\": \"./c.js\" }, \"./d\": { \"types\": null, \"default\": \"./d.ts\" }, \
             \"./e\": \"src/e.ts\" }",
        );
        assert_eq!(
            at(&package, "a"),
            Some("packages/billing/a-types.ts".into())
        );
        assert_eq!(
            at(&package, "b"),
            Some("packages/billing/b-import.ts".into())
        );
        // Only conditions Rotproof does not take, or a path not written `./...`: no path
        assert!(matches!(package.reach("c"), Reach::Unlisted { .. }));
        assert_eq!(at(&package, "d"), Some("packages/billing/d.ts".into()));
        assert!(matches!(package.reach("e"), Reach::Unlisted { .. }));
    }

    #[test]
    fn without_exports_a_subpath_is_a_path_and_the_package_is_its_types_or_main() {
        let make = |fields: &str| {
            let text = format!("{{ \"name\": \"@acme/billing\"{fields} }}");
            let (mut found, _) = read(
                &[("packages/billing/package.json", &text)],
                &["packages/billing"],
            );
            found.pop().unwrap()
        };
        let plain = make("");
        assert_eq!(
            at(&plain, "src/protocol/inbound"),
            Some("packages/billing/src/protocol/inbound".into())
        );
        assert_eq!(at(&plain, ""), Some("packages/billing".into()));
        assert_eq!(
            at(&make(", \"main\": \"./src/index.js\""), ""),
            Some("packages/billing/src/index.js".into())
        );
        assert_eq!(
            at(
                &make(", \"main\": \"./a.js\", \"types\": \"src/index.d.ts\""),
                ""
            ),
            Some("packages/billing/src/index.d.ts".into())
        );
        assert_eq!(
            at(&make(", \"typings\": \"t.d.ts\""), ""),
            Some("packages/billing/t.d.ts".into())
        );
        // Above the root
        assert_eq!(at(&plain, "../../../x"), None);
    }
}
