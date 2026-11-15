//! Where a TypeScript import lands, the tree asked whether a module is under a `baseUrl`.

use domain::code::{Aliases, Landing, Reach, module_files};
use protocol::outbound::Tree;

/// The declaration and the TypeScript source an import of a `.js` file stands for, which a project that writes `.js` in
/// its imports (as `nodenext` asks) has the source of beside it
const SOURCE_OF: [(&str, &[&str]); 4] = [
    (".js", &[".ts", ".tsx", ".d.ts"]),
    (".jsx", &[".tsx"]),
    (".mjs", &[".mts", ".d.mts"]),
    (".cjs", &[".cts", ".d.cts"]),
];

/// Where `specifier`, imported by the file at `file` (from the root), lands: through `paths`, then a `baseUrl` where a
/// module is, then the package of a unit, as tsc tries them. A path from the root, or `At(None)` for a package that is
/// no unit's, or a path above the root; `Unlisted` for a package of a unit that does not export what is imported; and
/// `Missing` for one that exports it as a path where no file is.
pub fn lands(tree: &dyn Tree, aliases: &Aliases, file: &str, specifier: &str) -> Reach {
    match aliases.resolve(file, specifier) {
        Landing::At(path) => Reach::At(path),
        Landing::UnderBaseUrl(paths) => {
            let found = paths.into_iter().find(|path| {
                module_files(path)
                    .iter()
                    .any(|file| tree.found(file) == Some(false))
            });
            match found {
                Some(path) => Reach::At(Some(path)),
                None => match aliases.package_of(specifier) {
                    Some((package, sub)) => match package.reach(sub) {
                        Reach::At(Some(path)) if !is_there(tree, &path) => Reach::Missing {
                            config: package.config.clone(),
                            path,
                        },
                        reached => reached,
                    },
                    None => Reach::At(None),
                },
            }
        }
    }
}

/// Whether a module is at `path`: a file of that name, one with an extension of source (`.ts`, `.d.ts`, ...), an
/// `index` in a directory of that name, or, for a path written with `.js`, the source it is built from.
fn is_there(tree: &dyn Tree, path: &str) -> bool {
    let file = |path: &str| tree.found(path) == Some(false);
    if module_files(path).iter().any(|path| file(path))
        || file(&format!("{path}.d.ts"))
        || file(&format!("{path}/index.d.ts"))
    {
        return true;
    }
    SOURCE_OF.iter().any(|(extension, sources)| {
        path.strip_suffix(extension).is_some_and(|stem| {
            sources
                .iter()
                .any(|source| file(&format!("{stem}{source}")))
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::fake::Fake;
    use domain::code::{Alias, Package, Target};

    #[test]
    fn under_a_base_url_a_specifier_lands_only_where_a_module_is() {
        let aliases = Aliases {
            base_urls: ["src".to_string()].into(),
            ..Aliases::default()
        };
        let tree = Fake::new(&[("src/domain/song.ts", ""), ("src/ui/index.tsx", "")]);
        let at = |s: &str| lands(&tree, &aliases, "src/ui/pages/home.tsx", s);
        assert_eq!(at("domain/song"), Reach::At(Some("src/domain/song".into())));
        assert_eq!(at("ui"), Reach::At(Some("src/ui".into())));
        // A package of the same name otherwise
        assert_eq!(at("domain/missing"), Reach::At(None));
        assert_eq!(at("react"), Reach::At(None));
        assert_eq!(at("../atoms/x"), Reach::At(Some("src/ui/atoms/x".into())));
    }

    #[test]
    fn a_units_package_is_tried_after_paths_and_a_base_url() {
        let package = |name: &str, root: &str| Package {
            name: name.into(),
            root: root.into(),
            config: format!("{root}/package.json"),
            exports: Some(vec![("./*".into(), Target::Path("./src/*".into()))]),
            types: None,
            main: None,
        };
        let aliases = Aliases {
            paths: vec![Alias::from_entry("@acme/mapped/*", "lib/*", "", "tsconfig.json").unwrap()],
            base_urls: ["src".to_string()].into(),
            packages: vec![
                package("@acme/billing", "packages/billing"),
                package("@acme/mapped", "packages/mapped"),
                package("@acme/billing-extra", "packages/extra"),
            ],
            ..Aliases::default()
        };
        let tree = Fake::new(&[
            ("src/@acme/billing/x.ts", ""),
            ("packages/billing/src/domain/index.ts", ""),
            ("packages/extra/src/x.ts", ""),
        ]);
        let at = |s: &str| lands(&tree, &aliases, "src/a.ts", s);
        let path = |p: &str| Reach::At(Some(p.into()));
        assert_eq!(
            at("@acme/billing/domain"),
            path("packages/billing/src/domain")
        );
        // `paths` first, then a module under the base URL, and only then the package
        assert_eq!(at("@acme/mapped/x"), path("lib/x"));
        assert_eq!(at("@acme/billing/x"), path("src/@acme/billing/x"));
        // The name ends at a `/`: `@acme/billing-extra` is another package
        assert_eq!(at("@acme/billing-extra/x"), path("packages/extra/src/x"));
        assert_eq!(at("@acme/other/x"), Reach::At(None));
    }

    #[test]
    fn what_a_package_exports_as_a_path_with_no_file_is_missing_and_a_file_of_another_extension_is_one()
     {
        let aliases = Aliases {
            packages: vec![Package {
                name: "@acme/db".into(),
                root: "packages/db".into(),
                config: "packages/db/package.json".into(),
                exports: Some(vec![
                    ("./*".into(), Target::Path("./src/*/index.ts".into())),
                    ("./esm/*".into(), Target::Path("./src/esm/*".into())),
                ]),
                types: None,
                main: None,
            }],
            ..Aliases::default()
        };
        let tree = Fake::new(&[
            ("packages/db/src/domain/index.ts", ""),
            ("packages/db/src/esm/a.ts", ""),
            ("packages/db/src/esm/b.d.ts", ""),
            ("packages/db/src/esm/c/index.tsx", ""),
        ]);
        let at = |s: &str| lands(&tree, &aliases, "x.ts", s);
        let path = |p: &str| Reach::At(Some(p.into()));
        assert_eq!(
            at("@acme/db/domain"),
            path("packages/db/src/domain/index.ts")
        );
        // The `.js` a file is built to stands for its source, and a module is a file with a type declaration, or a
        // directory with an `index`
        for specifier in ["a.js", "a", "b", "c"] {
            assert!(
                matches!(at(&format!("@acme/db/esm/{specifier}")), Reach::At(Some(_))),
                "{specifier}"
            );
        }
        assert_eq!(
            at("@acme/db/domain/invoice.ts"),
            Reach::Missing {
                config: "packages/db/package.json".into(),
                path: "packages/db/src/domain/invoice.ts/index.ts".into()
            }
        );
        assert!(matches!(at("@acme/db/esm/d.js"), Reach::Missing { .. }));
    }
}
