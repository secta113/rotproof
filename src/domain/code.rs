//! What Rotproof reads from code, as plain values, and the port to the readers that give them.
//!
//! The readers wrap parsers (Ruff for Python, oxc for TypeScript, toml_edit for a `Cargo.toml`) and give what a file
//! imports, its comments and its first syntax error; the checks judge those values and never see a parser. The
//! rules on the values are here too: the module a Python file is, which files are TypeScript source, and where a
//! TypeScript import lands through the project's aliases.
//!
//! Where a TypeScript import lands is a path from the root, found without asking whether a file is there: the place of
//! an import is its directory. A relative specifier resolves against the file's directory, and one that starts with
//! `/` against the root, as Vite reads it. Another resolves through the aliases (`compilerOptions.paths`), then
//! through a `baseUrl` when a module is there; otherwise it names a package, which no layer is. A query
//! (`./logo.svg?url`) is not part of the path. Whether a module is under a `baseUrl` is the one question asked of the
//! tree: [`Aliases::resolve`] gives the paths to ask about, and the caller asks.

use std::collections::{BTreeMap, BTreeSet};
use std::io;

use crate::domain::tree::Tree;

/// The readers of code: the port the checks read code through.
pub trait Parsers {
    /// A Python file, whose path from the root names the package its relative imports start from: every module it
    /// imports, as parts.
    fn python(&self, source: &str, path: &str) -> Source<Vec<String>>;
    /// The names of the functions and classes a Python file defines, at any depth.
    fn python_definitions(&self, source: &str) -> BTreeSet<String>;
    /// A TypeScript or JavaScript file, whose path from the root says how to read it: every specifier it imports.
    fn typescript(&self, source: &str, path: &str) -> Source<String>;
    /// The aliases of the TypeScript project in `tree`, from its `tsconfig*.json` files. An error is a config that
    /// cannot be read at all.
    fn typescript_aliases(&self, tree: &dyn Tree) -> io::Result<Aliases>;
    /// A crate's `Cargo.toml`, or `Err` with the line of the first error and what the parser says.
    fn manifest(&self, source: &str) -> Result<Manifest, (usize, String)>;
}

/// What one file of code holds: what it imports, as `I`, its comments and its first syntax error.
#[derive(Debug)]
pub struct Source<I> {
    /// Everything the file imports, each with the line (from 1) it is imported on
    pub imports: Vec<(usize, I)>,
    /// Every comment: its byte offset in the source, and its text with its delimiters. A reader reads comments past a
    /// syntax error too
    pub comments: Vec<(usize, String)>,
    /// The first syntax error, if any: its line and what the parser says. The parser recovers, but what follows may be
    /// misread
    pub error: Option<(usize, String)>,
}

impl<I> Default for Source<I> {
    fn default() -> Self {
        Source {
            imports: Vec::new(),
            comments: Vec::new(),
            error: None,
        }
    }
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

impl Aliases {
    /// Where `specifier`, imported by the file at `file` (from the root), lands.
    pub fn resolve(&self, file: &str, specifier: &str) -> Landing {
        let specifier = specifier.split('?').next().unwrap_or(specifier);
        if specifier == "."
            || specifier == ".."
            || specifier.starts_with("./")
            || specifier.starts_with("../")
        {
            return Landing::At(normalize(&format!("{}/{specifier}", parent(file))));
        }
        if let Some(from_root) = specifier.strip_prefix('/') {
            return Landing::At(normalize(from_root));
        }
        let matched = self
            .paths
            .iter()
            .filter_map(|alias| {
                let rest = specifier.strip_prefix(&alias.prefix)?;
                match &alias.suffix {
                    None => rest.is_empty().then(|| alias.target.clone()),
                    Some(suffix) => rest
                        .strip_suffix(suffix.as_str())
                        .map(|star| alias.target.replacen('*', star, 1)),
                }
                .map(|target| (alias.prefix.len(), target))
            })
            // The longest prefix wins, as in TypeScript
            .max_by_key(|(length, _)| *length);
        if let Some((_, target)) = matched {
            return Landing::At(normalize(&target));
        }
        Landing::UnderBaseUrl(
            self.base_urls
                .iter()
                .filter_map(|base| normalize(&join(base, specifier)))
                .collect(),
        )
    }
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

/// The directory of a path from the root: "" for a file at the root.
pub fn parent(path: &str) -> String {
    path.rsplit_once('/')
        .map_or(String::new(), |(dir, _)| dir.to_string())
}

/// `path` from the directory `dir` (from the root), not yet normalized.
pub fn join(dir: &str, path: &str) -> String {
    if dir.is_empty() {
        path.to_string()
    } else {
        format!("{dir}/{path}")
    }
}

/// A path from the root without `.`, `..` or empty parts. `None` when it climbs above the root.
fn normalize(path: &str) -> Option<String> {
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            part => parts.push(part),
        }
    }
    Some(parts.join("/"))
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
