//! Where a TypeScript import lands, the tree asked whether a module is under a `baseUrl`.

use crate::domain::code::{Aliases, Landing, module_files};
use crate::domain::tree::Tree;

/// Where `specifier`, imported by the file at `file` (from the root), lands: a path from the root, or `None` for a
/// package, or a path above the root.
pub fn lands(tree: &dyn Tree, aliases: &Aliases, file: &str, specifier: &str) -> Option<String> {
    match aliases.resolve(file, specifier) {
        Landing::At(path) => path,
        Landing::UnderBaseUrl(paths) => paths.into_iter().find(|path| {
            module_files(path)
                .iter()
                .any(|file| tree.found(file) == Some(false))
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::tree::fake::Fake;

    #[test]
    fn under_a_base_url_a_specifier_lands_only_where_a_module_is() {
        let aliases = Aliases {
            base_urls: ["src".to_string()].into(),
            ..Aliases::default()
        };
        let tree = Fake::new(&[("src/domain/song.ts", ""), ("src/ui/index.tsx", "")]);
        let at = |s: &str| lands(&tree, &aliases, "src/ui/pages/home.tsx", s);
        assert_eq!(at("domain/song"), Some("src/domain/song".into()));
        assert_eq!(at("ui"), Some("src/ui".into()));
        // A package of the same name otherwise
        assert_eq!(at("domain/missing"), None);
        assert_eq!(at("react"), None);
        assert_eq!(at("../atoms/x"), Some("src/ui/atoms/x".into()));
    }
}
