//! Where a TypeScript import lands, the tree asked whether a module is under a `baseUrl`.

use domain::code::{Aliases, Landing, Reach, module_files};
use protocol::outbound::Tree;

/// Where `specifier`, imported by the file at `file` (from the root), lands: through `paths`, then a `baseUrl` where a
/// module is, then the package of a unit, as tsc tries them. A path from the root, or `At(None)` for a package that is
/// no unit's, or a path above the root; and `Unlisted` for a package of a unit that does not export what is imported.
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
                None => aliases.package(specifier).unwrap_or(Reach::At(None)),
            }
        }
    }
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
        let tree = Fake::new(&[("src/@acme/billing/x.ts", "")]);
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
}
