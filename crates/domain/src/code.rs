//! What Rotproof reads from code, as plain values. The port to the readers that give them is `protocol::outbound`'s
//! `Parsers`.
//!
//! The readers wrap parsers (Ruff for Python, oxc for TypeScript, toml_edit for a `Cargo.toml`) and give what a file
//! imports, its comments and its first syntax error; the checks judge those values and never see a parser. The
//! rules on the values are here too: the module a Python file is, which files are TypeScript source, the alias an
//! entry of `compilerOptions.paths` makes, where a TypeScript import lands through the project's aliases, and where
//! one lands through the `package.json` of a unit's package.
//!
//! Where a TypeScript import lands is a path from the root, found without asking whether a file is there: the place of
//! an import is its directory. A relative specifier resolves against the file's directory, and one that starts with
//! `/` against the root, as Vite reads it. Another resolves through the aliases (`compilerOptions.paths`), then
//! through a `baseUrl` when a module is there, then through the name of a unit's package ([`Aliases::package`]);
//! otherwise it names a package that is no unit's, which no layer is. A query (`./logo.svg?url`) is not part of the
//! path. Whether a module is under a `baseUrl` is the one question asked of the tree: [`Aliases::resolve`] gives the
//! paths to ask about, and the caller asks.
//!
//! A unit's package is read from the `package.json` at the unit's `root`: `name` is what an import starts with, and
//! the rest of the specifier is a subpath of the package. With `exports`, only a subpath it lists is the package's,
//! each key an exact subpath or a pattern with one `*`, the longest prefix winning as in Node, and a target a path
//! from the package's root, or conditions among which the first of `types`, `import` and `default` that leads to a
//! path is taken. An import of a subpath it does not list names no file Node or tsc would find, so it is a finding of
//! its own, never a path ([`Reach::Unlisted`]); so does one it lists whose target holds no file, which the caller that
//! asks the tree finds ([`Reach::Missing`]). Without `exports`, a subpath is a path in the package, and the
//! package itself is its `types`, or its `main`, or its root. Not read: a `tsconfig` of the unit, `imports`
//! (`#name`), and a condition other than the three.

use std::collections::{BTreeMap, BTreeSet};

use crate::paths::{join, normalize, parent};

/// What one file of code holds: what it imports, as `I`, its comments and its first syntax error.
#[derive(Debug)]
pub struct Source<I> {
    /// Everything the file imports, each with the line (from 1) it is imported on
    pub imports: Vec<(usize, I)>,
    /// Every comment: its byte offset in the source, and its text with its delimiters. A reader reads comments past a
    /// syntax error too
    pub comments: Vec<(usize, String)>,
    /// Every read or write of the environment variables the reader sees (`environment.rs`): the line (from 1), and
    /// what it names (`os.environ`, `process.env`)
    pub environment: Vec<(usize, String)>,
    /// The first syntax error, if any: its line and what the parser says. The parser recovers, but what follows may be
    /// misread
    pub error: Option<(usize, String)>,
}

impl<I> Default for Source<I> {
    fn default() -> Self {
        Source {
            imports: Vec::new(),
            comments: Vec::new(),
            environment: Vec::new(),
            error: None,
        }
    }
}

/// What one import of a TypeScript or JavaScript file names: the module, and the names it takes from it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Import {
    /// The specifier, as written
    pub specifier: String,
    /// The names it takes, as the module exports them: `default`, `*` for the whole module (`import * as ns`,
    /// `export * from`), and each name of `import { a as b }` or `export { a as b } from` as `a`. Empty for an import
    /// that takes none (`import "x"`, `import("x")`, `require("x")`, `import x = require("x")`, `import("x")` in a
    /// type), which is read as the module
    pub names: Vec<String>,
}

/// The module a file is, as parts: `ui/pages/home.py` is `ui.pages.home`, and `ui/pages/__init__.py` is `ui.pages`.
pub fn module_parts(path: &str) -> Vec<String> {
    let mut parts: Vec<String> = path.split('/').map(String::from).collect();
    let last = parts.pop().unwrap_or_default();
    // `.PY` too: Windows runs it with Python
    let stem = &last[..last.len() - ".py".len()];
    if !stem.eq_ignore_ascii_case("__init__") {
        parts.push(stem.to_string());
    }
    parts
}

/// The extensions read as source, in lower case
pub const EXTENSIONS: [&str; 8] = ["ts", "tsx", "mts", "cts", "js", "jsx", "mjs", "cjs"];

/// Whether a file is TypeScript or JavaScript source, by its extension in any case.
pub fn is_source(path: &str) -> bool {
    path.rsplit_once('.')
        .is_some_and(|(_, extension)| EXTENSIONS.contains(&extension.to_ascii_lowercase().as_str()))
}

/// `path` without the extension that makes it source (`a/b.ts` is `a/b`, `a/b.d.ts` is `a/b`), which an import leaves
/// off as often as not: the name of the module an import lands in.
pub fn without_extension(path: &str) -> &str {
    if !is_source(path) {
        return path;
    }
    let stem = path.rsplit_once('.').map_or(path, |(stem, _)| stem);
    stem.strip_suffix(".d").unwrap_or(stem)
}

/// One entry of `compilerOptions.paths`: a pattern with at most one `*`, and the first of its targets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Alias {
    /// Before the `*`, or the whole pattern
    pub prefix: String,
    /// After the `*`; `None` for a pattern without one, which matches only itself
    pub suffix: Option<String>,
    /// The target, with `*` where the matched text goes, from the root
    pub target: String,
    /// The config that maps it, for the message
    pub config: String,
}

/// How a specifier that is not relative resolves in this project, from the `tsconfig*.json` files at the root.
#[derive(Debug, Default)]
pub struct Aliases {
    pub paths: Vec<Alias>,
    /// The `baseUrl` directories, from the root
    pub base_urls: BTreeSet<String>,
    /// The package of each unit that has a `package.json` with a name, once for each root
    pub packages: Vec<Package>,
    /// What could not be read, each with the config it is in: the aliases it would give are not known
    pub problems: Vec<String>,
}

/// Where a specifier lands, as far as the aliases say.
#[derive(Debug, PartialEq, Eq)]
pub enum Landing {
    /// A path from the root, or `None` for a package, or a path above the root
    At(Option<String>),
    /// The first of these paths (from the root, under each `baseUrl` in turn) where a module is, by
    /// [`module_files`]; a package when there is none
    UnderBaseUrl(Vec<String>),
}

/// What an `exports` entry leads to, as `package.json` writes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// A path from the package's root, written `./...`, with a `*` where a pattern's match goes
    Path(String),
    /// Conditions, each with what it leads to, in the order written
    Conditions(Vec<(String, Target)>),
    /// A list, of which the first that leads to a path is taken
    Any(Vec<Target>),
    /// `null`: the subpath is not exported
    Nothing,
}

/// The conditions Rotproof takes, in the order it tries them: `types` is what tsc reads, `import` what an import
/// takes, and `default` what is left. Any other (`node`, `require`, `browser`) is passed over.
const CONDITIONS: [&str; 3] = ["types", "import", "default"];

impl Target {
    /// The path this leads to, `star` put where the pattern's `*` was, from the package's root; `None` when it leads
    /// to none: `null`, a path that is not written `./...`, or none of the three conditions.
    fn path(&self, star: &str) -> Option<String> {
        match self {
            Target::Path(path) => path.strip_prefix("./").map(|rest| rest.replace('*', star)),
            Target::Conditions(conditions) => CONDITIONS.iter().find_map(|wanted| {
                conditions
                    .iter()
                    .find(|(name, _)| name == wanted)
                    .and_then(|(_, target)| target.path(star))
            }),
            Target::Any(targets) => targets.iter().find_map(|target| target.path(star)),
            Target::Nothing => None,
        }
    }
}

/// The package of a unit, from its `package.json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Package {
    /// `name`: what an import of the package starts with
    pub name: String,
    /// The directory the `package.json` is in, from the root
    pub root: String,
    /// The `package.json`, from the root, for the message
    pub config: String,
    /// `exports` as its entries, each a key (`.` or `./subpath`, with at most one `*`) and what it leads to: a string
    /// or a map of conditions alone is the entry `.`. `None` when the package has no `exports`
    pub exports: Option<Vec<(String, Target)>>,
    /// `types` (or `typings`) and `main`, as written, from the root of the package
    pub types: Option<String>,
    pub main: Option<String>,
}

/// Where an import through a unit's package lands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reach {
    /// A path from the root, or `None` for one above the root
    At(Option<String>),
    /// The package has `exports` and the subpath is not among them (or is `null`, or leads to no path by the three
    /// conditions): Node and tsc fail it, and where it would land is not known
    Unlisted {
        /// The `package.json`, from the root
        config: String,
    },
    /// The package exports the subpath, but the path it leads to holds no file: a pattern such as
    /// `"./*": "./src/*/index.ts"` takes `domain/invoice.ts` to `src/domain/invoice.ts/index.ts`. The package never
    /// gives this itself, as it does not look at the tree: the caller that does (`lands`) turns an `At` into it
    Missing {
        /// The `package.json`, from the root
        config: String,
        /// Where the export leads, from the root, where no file is
        path: String,
    },
}

impl Package {
    /// Where the subpath `sub` of this package (`protocol/inbound`; "" for the package itself) lands.
    pub fn reach(&self, sub: &str) -> Reach {
        let at = |path: &str| Reach::At(normalize(&join(&self.root, path)));
        let Some(exports) = &self.exports else {
            let main = self.types.as_deref().or(self.main.as_deref()).unwrap_or("");
            return at(if sub.is_empty() { main } else { sub });
        };
        let key = if sub.is_empty() {
            ".".to_string()
        } else {
            format!("./{sub}")
        };
        match export_for(exports, &key).and_then(|(target, star)| target.path(&star)) {
            Some(path) => at(&path),
            None => Reach::Unlisted {
                config: self.config.clone(),
            },
        }
    }
}

/// The entry of `exports` that `key` is, and the text a `*` of its key matched: an exact key first, then the pattern
/// with the longest part before its `*`, as Node chooses.
fn export_for<'a>(exports: &'a [(String, Target)], key: &str) -> Option<(&'a Target, String)> {
    if let Some((_, target)) = exports.iter().find(|(name, _)| name == key)
        && !key.contains('*')
    {
        return Some((target, String::new()));
    }
    exports
        .iter()
        .filter(|(name, _)| name.matches('*').count() == 1)
        .filter_map(|(name, target)| {
            let (before, after) = name.split_once('*')?;
            let star = key.strip_prefix(before)?.strip_suffix(after)?;
            // `*` matches at least one character
            (!star.is_empty()).then(|| (before.len(), name.len(), target, star.to_string()))
        })
        .max_by_key(|(before, length, _, _)| (*before, *length))
        .map(|(_, _, target, star)| (target, star))
}

impl Alias {
    /// The alias one entry of `paths`, `pattern` mapped to its first `target`, makes, the target resolved against the
    /// directory `base` (from the root) and the entry kept as the one of `config`. `None` when the entry cannot be
    /// read: a pattern with more than one `*`, or a target with more `*` than the pattern, since there would be no
    /// text to put in the extra one.
    pub fn from_entry(pattern: &str, target: &str, base: &str, config: &str) -> Option<Alias> {
        let stars = pattern.matches('*').count();
        if stars > 1 || target.matches('*').count() > stars {
            return None;
        }
        let (prefix, suffix) = match pattern.split_once('*') {
            Some((prefix, suffix)) => (prefix.to_string(), Some(suffix.to_string())),
            None => (pattern.to_string(), None),
        };
        Some(Alias {
            prefix,
            suffix,
            target: join(base, target),
            config: config.to_string(),
        })
    }

    /// The target `specifier` maps to, with what the `*` matched in place of the target's `*`; `None` when the
    /// pattern does not match it.
    pub fn mapped(&self, specifier: &str) -> Option<String> {
        let rest = specifier.strip_prefix(&self.prefix)?;
        match &self.suffix {
            None => rest.is_empty().then(|| self.target.clone()),
            Some(suffix) => rest
                .strip_suffix(suffix.as_str())
                .map(|star| self.target.replacen('*', star, 1)),
        }
    }
}

impl Aliases {
    /// Where `specifier`, imported by the file at `file` (from the root), lands, in the order TypeScript tries: a
    /// relative path, a path from the root, `paths`, then each `baseUrl`.
    pub fn resolve(&self, file: &str, specifier: &str) -> Landing {
        let specifier = specifier.split('?').next().unwrap_or(specifier);
        if is_relative(specifier) {
            return Landing::At(normalize(&join(parent(file), specifier)));
        }
        if let Some(from_root) = specifier.strip_prefix('/') {
            return Landing::At(normalize(from_root));
        }
        if let Some(target) = self.mapped(specifier) {
            return Landing::At(normalize(&target));
        }
        Landing::UnderBaseUrl(
            self.base_urls
                .iter()
                .filter_map(|base| normalize(&join(base, specifier)))
                .collect(),
        )
    }

    /// Where `specifier` lands through the package of a unit whose name it starts with (the longest, when two do), or
    /// `None` when it names no such package. It is tried after `paths` and `baseUrl`, as tsc does.
    pub fn package(&self, specifier: &str) -> Option<Reach> {
        self.package_of(specifier)
            .map(|(package, sub)| package.reach(sub))
    }

    /// The package of a unit whose name `specifier` starts with (the longest, when two do), and the subpath that is
    /// left of it (`protocol/inbound`; "" for the package itself).
    pub fn package_of<'a>(&'a self, specifier: &'a str) -> Option<(&'a Package, &'a str)> {
        let specifier = specifier.split('?').next().unwrap_or(specifier);
        self.packages
            .iter()
            .filter_map(|package| {
                let rest = specifier.strip_prefix(&package.name)?;
                let sub = if rest.is_empty() {
                    rest
                } else {
                    rest.strip_prefix('/')?
                };
                Some((package, sub))
            })
            .max_by_key(|(package, _)| package.name.len())
    }

    /// The target of the alias in `paths` that maps `specifier`: the one with the longest prefix, as in TypeScript.
    fn mapped(&self, specifier: &str) -> Option<String> {
        self.paths
            .iter()
            .filter_map(|alias| Some((alias.prefix.len(), alias.mapped(specifier)?)))
            .max_by_key(|(length, _)| *length)
            .map(|(_, target)| target)
    }
}

/// Whether `specifier` is relative to the file that imports it: `.`, `..`, or a path that starts with either.
fn is_relative(specifier: &str) -> bool {
    specifier == "."
        || specifier == ".."
        || specifier.starts_with("./")
        || specifier.starts_with("../")
}

/// The files that make a module at `path` (from the root), any one of them enough: the file itself, with one of
/// [`EXTENSIONS`], or an `index` in it.
pub fn module_files(path: &str) -> Vec<String> {
    let mut files = vec![path.to_string()];
    for extension in EXTENSIONS {
        files.push(format!("{path}.{extension}"));
        files.push(format!("{path}/index.{extension}"));
    }
    files
}

/// Where a dependency comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Origin {
    /// A path, from the manifest's directory, as written
    Path(String),
    /// The entry of the same name in its workspace's `[workspace.dependencies]`
    Workspace,
}

/// A dependency that can name a path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dependency {
    /// The line (from 1) of its name
    pub line: usize,
    /// Its name in the manifest (with `package`, the name the crate is used by)
    pub name: String,
    pub origin: Origin,
}

/// What one manifest holds.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Manifest {
    /// Every dependency read that comes from a path or from the workspace, in the order written
    pub dependencies: Vec<Dependency>,
    /// Whether the manifest declares a package (`[package]`): a manifest that declares only `[workspace]` builds no
    /// crate
    pub is_package: bool,
    /// `[package] workspace`: the workspace's directory, from the manifest's directory
    pub workspace: Option<String>,
    /// Whether the manifest declares a workspace
    pub is_workspace: bool,
    /// `[workspace.dependencies]`: name -> its path from the manifest's directory, or `None` for one from a registry
    /// or git
    pub workspace_dependencies: BTreeMap<String, Option<String>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_alias_with_the_longest_prefix_maps_and_a_suffix_must_match() {
        let alias = |pattern: &str, target: &str| {
            Alias::from_entry(pattern, target, "", "tsconfig.json").unwrap()
        };
        let aliases = Aliases {
            paths: vec![
                alias("@/*", "src/*"),
                alias("@/ui/*", "lib/ui/*"),
                alias("#styles/*.css", "src/styles/*.css"),
                alias("#theme", "src/theme"),
            ],
            ..Aliases::default()
        };
        let at = |s: &str| match aliases.resolve("src/a.ts", s) {
            Landing::At(path) => path,
            other => panic!("{s}: {other:?}"),
        };
        assert_eq!(at("@/domain/song"), Some("src/domain/song".into()));
        assert_eq!(at("@/ui/button"), Some("lib/ui/button".into()));
        assert_eq!(at("#styles/main.css"), Some("src/styles/main.css".into()));
        assert_eq!(at("#theme"), Some("src/theme".into()));
        // A pattern without `*` matches only itself, and one with a suffix only what ends in it
        assert_eq!(
            aliases.resolve("src/a.ts", "#theme/dark"),
            Landing::UnderBaseUrl(Vec::new())
        );
        assert_eq!(
            aliases.resolve("src/a.ts", "#styles/main.scss"),
            Landing::UnderBaseUrl(Vec::new())
        );
    }

    #[test]
    fn an_entry_of_paths_with_one_star_at_most_on_both_sides_is_an_alias() {
        let alias = Alias::from_entry("@/*", "*", "src", "tsconfig.app.json").unwrap();
        assert_eq!(alias.prefix, "@/");
        assert_eq!(alias.suffix.as_deref(), Some(""));
        assert_eq!(alias.target, "src/*");
        assert_eq!(alias.config, "tsconfig.app.json");
        // A pattern without `*` is itself, and so is its target, from the root when the base is
        let theme = Alias::from_entry("#theme", "ui/theme", "", "tsconfig.json").unwrap();
        assert_eq!((theme.prefix.as_str(), theme.suffix), ("#theme", None));
        assert_eq!(theme.target, "ui/theme");
        // A suffix is what follows the `*`
        let css = Alias::from_entry("#styles/*.css", "s/*.css", "src", "t.json").unwrap();
        assert_eq!(
            (css.prefix.as_str(), css.suffix.as_deref()),
            ("#styles/", Some(".css"))
        );
        // Two `*` in the pattern, or more in the target than in the pattern, cannot be read
        for (pattern, target) in [("*/*", "src/*"), ("a", "src/*"), ("a/*", "*/*")] {
            assert_eq!(
                Alias::from_entry(pattern, target, "", "t.json"),
                None,
                "{pattern} {target}"
            );
        }
    }

    #[test]
    fn a_relative_or_rooted_specifier_resolves_against_the_file() {
        let aliases = Aliases::default();
        let at = |s: &str| match aliases.resolve("src/ui/atoms/button.tsx", s) {
            Landing::At(path) => path,
            other => panic!("{s}: {other:?}"),
        };
        assert_eq!(at("../molecules/row"), Some("src/ui/molecules/row".into()));
        assert_eq!(
            at("./theme.css?inline"),
            Some("src/ui/atoms/theme.css".into())
        );
        assert_eq!(at("../../domain"), Some("src/domain".into()));
        assert_eq!(at("/src/utils/x"), Some("src/utils/x".into()));
        assert_eq!(at("../../../../../out"), None);
        // Without a baseUrl, nothing to ask about: a package
        assert_eq!(
            aliases.resolve("src/a.ts", "react"),
            Landing::UnderBaseUrl(Vec::new())
        );
    }

    #[test]
    fn under_a_base_url_a_specifier_is_each_path_it_names_in_turn() {
        let aliases = Aliases {
            base_urls: ["src".to_string(), "lib".to_string()].into(),
            ..Aliases::default()
        };
        assert_eq!(
            aliases.resolve("src/a.ts", "domain/song?raw"),
            Landing::UnderBaseUrl(vec!["lib/domain/song".into(), "src/domain/song".into()])
        );
        let files = module_files("src/domain/song");
        assert_eq!(files.len(), 1 + 2 * EXTENSIONS.len());
        assert!(files.contains(&"src/domain/song".into()));
        assert!(files.contains(&"src/domain/song.tsx".into()));
        assert!(files.contains(&"src/domain/song/index.mjs".into()));
    }

    #[test]
    fn a_python_file_is_the_module_its_path_names() {
        let parts = |path: &str| module_parts(path).join(".");
        assert_eq!(parts("ui/pages/home.py"), "ui.pages.home");
        assert_eq!(parts("ui/pages/__init__.py"), "ui.pages");
        assert_eq!(parts("domain/Model.PY"), "domain.Model");
    }

    #[test]
    fn typescript_source_is_named_by_its_extension_in_any_case() {
        assert!(is_source("src/a.ts") && is_source("src/A.TSX") && is_source("x.mjs"));
        assert!(!is_source("src/a.css") && !is_source("src/a.d") && !is_source("ts"));
    }
}
