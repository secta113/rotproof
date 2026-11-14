//! The aliases of a TypeScript project: `compilerOptions.paths` and `baseUrl` of the `tsconfig*.json` files at the
//! root and the local files they extend. Where an import lands through them is the rule of `code.rs`
//! ([`Aliases::resolve`](domain::code::Aliases::resolve)); this reads the files that say it.
//!
//! No parser of oxc is in it: a config is JSON with comments, made into JSON by [`strip_jsonc`] and read with
//! `serde_json`. What cannot be read is a problem of the [`Aliases`] and not an error, so the check can name it; an
//! error is a config the tree cannot read at all.

use std::io;

use serde_json::Value;

use domain::code::{Alias, Aliases};
use domain::paths::{join, parent};
use domain::tree::with_lf;
use protocol::outbound::Tree;

/// What one config says, its `extends` followed.
#[derive(Debug, Default, Clone)]
struct Options {
    /// From the root
    base_url: Option<String>,
    /// Pattern -> target, and the directory (from the root) the targets resolve against when there is no `baseUrl`
    paths: Option<(Vec<(String, String)>, String)>,
}

/// The aliases of the project in `tree`. An error is a config that cannot be read at all.
pub fn aliases(tree: &dyn Tree) -> io::Result<Aliases> {
    let mut names: Vec<String> = Vec::new();
    for (name, is_dir) in tree.entries("")? {
        if name.starts_with("tsconfig") && name.ends_with(".json") && !is_dir {
            names.push(name);
        }
    }
    names.sort();
    let mut aliases = Aliases::default();
    for name in &names {
        let mut seen = Vec::new();
        let Some(options) = load(tree, name, &mut seen, &mut aliases.problems)? else {
            continue;
        };
        if let Some(base) = &options.base_url {
            aliases.base_urls.insert(base.clone());
        }
        let Some((paths, dir)) = options.paths else {
            continue;
        };
        let base = options.base_url.clone().unwrap_or(dir);
        for (pattern, target) in paths {
            let stars = pattern.matches('*').count();
            if stars > 1 || target.matches('*').count() > stars {
                aliases.problems.push(format!(
                    "{name}: paths maps {pattern:?} to {target:?}, which Rotproof cannot read: one * at most, on both \
                     sides"
                ));
                continue;
            }
            let (prefix, suffix) = match pattern.split_once('*') {
                Some((prefix, suffix)) => (prefix.to_string(), Some(suffix.to_string())),
                None => (pattern.clone(), None),
            };
            let target = join(&base, &target);
            let alias = Alias {
                prefix,
                suffix,
                target,
                config: name.clone(),
            };
            match aliases
                .paths
                .iter()
                .find(|a| a.prefix == alias.prefix && a.suffix == alias.suffix)
            {
                Some(other) if other.target != alias.target => aliases.problems.push(format!(
                    "{} and {name} map {pattern:?} to different paths, so Rotproof does not know which a file uses",
                    other.config
                )),
                Some(_) => {}
                None => aliases.paths.push(alias),
            }
        }
    }
    Ok(aliases)
}

/// The options of the config at `name` (from the root), its local `extends` followed: a field it sets wins over the
/// configs it extends, and a later one in `extends` over an earlier one. `None`, with a problem, when it cannot be
/// read.
fn load(
    tree: &dyn Tree,
    name: &str,
    seen: &mut Vec<String>,
    problems: &mut Vec<String>,
) -> io::Result<Option<Options>> {
    if seen.contains(&name.to_string()) {
        problems.push(format!(
            "{name}: extends itself through {}",
            seen.join(", ")
        ));
        return Ok(None);
    }
    seen.push(name.to_string());
    let options = load_one(tree, name, seen, problems);
    seen.pop();
    options
}

/// [`load`] for one config, while `seen` holds it and the configs that extend it.
fn load_one(
    tree: &dyn Tree,
    name: &str,
    seen: &mut Vec<String>,
    problems: &mut Vec<String>,
) -> io::Result<Option<Options>> {
    let text = match tree.read(name).map(|text| with_lf(&text)) {
        Ok(text) => text,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            problems.push(format!("{name}: extended by {}, and not there", seen[0]));
            return Ok(None);
        }
        Err(e) => return Err(io::Error::new(e.kind(), format!("{name}: {e}"))),
    };
    let json: Value = match serde_json::from_str(&strip_jsonc(&text)) {
        Ok(json) => json,
        Err(e) => {
            problems.push(format!(
                "{name}: cannot be read as JSON with comments ({e}), so its paths are not known"
            ));
            return Ok(None);
        }
    };
    let dir = parent(name).to_string();
    let mut options = Options::default();
    let extends: Vec<&str> = match json.get("extends") {
        Some(Value::String(one)) => vec![one.as_str()],
        Some(Value::Array(many)) => many.iter().filter_map(Value::as_str).collect(),
        _ => Vec::new(),
    };
    // A package's config (`@tsconfig/strictest`) resolves its paths inside the package: no layer is there
    for extended in extends.iter().filter(|e| e.starts_with('.')) {
        let mut path = join(&dir, extended);
        if !path.ends_with(".json") {
            path.push_str(".json");
        }
        if let Some(parent) = load(tree, &path, seen, problems)? {
            options.base_url = parent.base_url.or(options.base_url);
            options.paths = parent.paths.or(options.paths);
        }
    }
    let compiler = json.get("compilerOptions");
    if let Some(base) = compiler.and_then(|c| c.get("baseUrl")) {
        match base.as_str() {
            Some(base) => options.base_url = Some(join(&dir, base)),
            None => problems.push(format!("{name}: baseUrl is not a string")),
        }
    }
    if let Some(paths) = compiler.and_then(|c| c.get("paths")) {
        let Some(map) = paths.as_object() else {
            problems.push(format!("{name}: paths is not an object"));
            return Ok(Some(options));
        };
        let mut entries = Vec::new();
        for (pattern, targets) in map {
            match targets
                .as_array()
                .and_then(|t| t.first())
                .and_then(Value::as_str)
            {
                Some(target) => entries.push((pattern.clone(), target.to_string())),
                None => problems.push(format!(
                    "{name}: paths maps {pattern:?} to {targets}, not to a list of paths"
                )),
            }
        }
        options.paths = Some((entries, dir));
    }
    Ok(Some(options))
}

/// JSON with comments, as tsconfig files are written, made into JSON: `//` and `/* */` comments and the commas before
/// a closing bracket go, and strings are kept as they are.
fn strip_jsonc(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '"' {
            out.push(c);
            i += 1;
            while i < chars.len() {
                out.push(chars[i]);
                if chars[i] == '\\' && i + 1 < chars.len() {
                    out.push(chars[i + 1]);
                    i += 2;
                    continue;
                }
                i += 1;
                if chars[i - 1] == '"' {
                    break;
                }
            }
        } else if c == '/' && chars.get(i + 1) == Some(&'/') {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
        } else if c == '/' && chars.get(i + 1) == Some(&'*') {
            i += 2;
            while i < chars.len() && !(chars[i] == '*' && chars.get(i + 1) == Some(&'/')) {
                i += 1;
            }
            i += 2;
        } else if c == ',' {
            if !matches!(next_token(&chars, i + 1), Some('}' | ']')) {
                out.push(c);
            }
            i += 1;
        } else {
            out.push(c);
            i += 1;
        }
    }
    out
}

/// The first character from `i` on that is neither white space nor in a comment.
fn next_token(chars: &[char], mut i: usize) -> Option<char> {
    while i < chars.len() {
        match (chars[i], chars.get(i + 1)) {
            (c, _) if c.is_whitespace() => i += 1,
            ('/', Some('/')) => {
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
            }
            ('/', Some('*')) => {
                i += 2;
                while i < chars.len() && !(chars[i] == '*' && chars.get(i + 1) == Some(&'/')) {
                    i += 1;
                }
                i += 2;
            }
            (c, _) => return Some(c),
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::outbound::disk::Disk;
    use domain::code::Landing;

    fn project(files: &[(&str, &str)]) -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        for (path, text) in files {
            let full = root.path().join(path);
            fs::create_dir_all(full.parent().unwrap()).unwrap();
            fs::write(full, text).unwrap();
        }
        root
    }

    #[test]
    fn paths_and_base_url_resolve_through_every_config_and_what_they_extend() {
        let root = project(&[
            (
                "tsconfig.json",
                "{\n  // the root config\n  \"files\": [],\n  \"extends\": \"./configs/base.json\",\n}\n",
            ),
            (
                "configs/base.json",
                "{ \"compilerOptions\": { /* from here */ \"baseUrl\": \"../src\", \"paths\": { \"@/*\": [\"./*\"], \"#theme\": [\"ui/atoms/theme\"], } } }",
            ),
            (
                "tsconfig.app.json",
                "{ \"compilerOptions\": { \"paths\": { \"~/*\": [\"src/*\"] } } }",
            ),
            ("src/domain/song.ts", ""),
        ]);
        let tree = Disk::new(root.path());
        let aliases = aliases(&tree).unwrap();
        assert_eq!(aliases.problems, Vec::<String>::new());
        let at = |s: &str| aliases.resolve("src/ui/pages/home.tsx", s);
        let at_path = |path: &str| Landing::At(Some(path.into()));
        assert_eq!(at("@/domain/song"), at_path("src/domain/song"));
        assert_eq!(at("#theme"), at_path("src/ui/atoms/theme"));
        assert_eq!(at("~/application/play"), at_path("src/application/play"));
        // Through baseUrl when a module is there, which the caller asks the tree
        assert_eq!(
            at("domain/song"),
            Landing::UnderBaseUrl(vec!["src/domain/song".into()])
        );
    }

    #[test]
    fn what_cannot_be_read_is_a_problem() {
        let root = project(&[
            (
                "tsconfig.json",
                "{ \"compilerOptions\": { \"paths\": { \"*/*\": [\"src/*\"], \"a\": \"src/a\" } } }",
            ),
            ("tsconfig.node.json", "{ not json"),
            ("tsconfig.app.json", "{ \"extends\": \"./missing.json\" }"),
        ]);
        let problems = aliases(&Disk::new(root.path())).unwrap().problems;
        assert_eq!(problems.len(), 4, "{problems:#?}");
        assert!(problems.iter().any(|p| p.contains("\"*/*\"")));
        assert!(
            problems
                .iter()
                .any(|p| p.contains("not to a list of paths"))
        );
        assert!(
            problems
                .iter()
                .any(|p| p.starts_with("tsconfig.node.json: cannot be read"))
        );
        assert!(
            problems
                .iter()
                .any(|p| p.contains("missing.json: extended by tsconfig.app.json"))
        );
    }

    #[test]
    fn jsonc_loses_its_comments_and_trailing_commas_and_keeps_its_strings() {
        let text =
            "{ \"a\": \"// not a comment, /* nor this */\", // gone\n \"b\": [1, 2,], /* gone */ }";
        let json: Value = serde_json::from_str(&strip_jsonc(text)).unwrap();
        assert_eq!(json["a"], "// not a comment, /* nor this */");
        assert_eq!(json["b"], serde_json::json!([1, 2]));
    }
}
