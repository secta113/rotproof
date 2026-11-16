//! Reading what a `pyproject.toml` lists as dependencies, with toml_edit, and the name of its package.
//!
//! - **Runtime:** `[project] dependencies`, each a PEP 508 string whose distribution is cut out
//!   ([`domain::dependencies::python::requirement_name`]), and, for a project that uses Poetry, the keys of
//!   `[tool.poetry.dependencies]` but `python`.
//! - **Optional:** each list of `[project.optional-dependencies]`, the extras the package offers.
//! - **Development:** each group of `[dependency-groups]` (PEP 735; an entry that is a table,
//!   `{ include-group = ... }`, names no distribution), `[tool.uv] dev-dependencies`, each group of
//!   `[tool.pdm.dev-dependencies]`, and Poetry's `[tool.poetry.dev-dependencies]` and
//!   `[tool.poetry.group.<name>.dependencies]`.
//! - **The name:** `[project] name`, or `[tool.poetry] name`.
//!
//! A requirement that names no distribution (a path, a URL) is left out. Names are compared as PEP 503 says, and the
//! line is the one the requirement is written on.

use toml_edit::{Document, Item, TableLike};

use domain::dependencies::python::{normalize, requirement_name};
use domain::dependencies::{Dep, Kind, Listing};
use domain::text::line_of;

fn table(item: Option<&Item>) -> Option<&dyn TableLike> {
    item.and_then(Item::as_table_like)
}

/// Read one `pyproject.toml`, or `Err` with the line of the first error and what the parser says.
pub fn read(source: &str) -> Result<Listing, (usize, String)> {
    let document = Document::parse(source).map_err(|e| {
        (
            line_of(source, e.span().map_or(0, |span| span.start)),
            e.message().trim().to_string(),
        )
    })?;
    let top = document.as_table();
    let at = |path: &[&str]| -> Option<&Item> {
        let mut item = top.get(path[0])?;
        for key in &path[1..] {
            item = item.as_table_like()?.get(key)?;
        }
        Some(item)
    };
    let mut reader = Reader {
        source,
        deps: Vec::new(),
    };
    let project = table(at(&["project"]));
    let poetry = table(at(&["tool", "poetry"]));
    let name = project
        .and_then(|p| p.get("name"))
        .or_else(|| poetry.and_then(|p| p.get("name")))
        .and_then(Item::as_str)
        .map(normalize);
    if let Some(list) = project.and_then(|p| p.get("dependencies")) {
        reader.requirements(list, Kind::Runtime, "");
    }
    if let Some(extras) = table(project.and_then(|p| p.get("optional-dependencies"))) {
        for (extra, list) in extras.iter() {
            reader.requirements(list, Kind::Optional, extra);
        }
    }
    if let Some(groups) = table(at(&["dependency-groups"])) {
        for (group, list) in groups.iter() {
            reader.requirements(list, Kind::Development, group);
        }
    }
    if let Some(list) = at(&["tool", "uv", "dev-dependencies"]) {
        reader.requirements(list, Kind::Development, "dev");
    }
    if let Some(groups) = table(at(&["tool", "pdm", "dev-dependencies"])) {
        for (group, list) in groups.iter() {
            reader.requirements(list, Kind::Development, group);
        }
    }
    if let Some(poetry) = poetry {
        if let Some(keys) = table(poetry.get("dependencies")) {
            reader.keys(keys, Kind::Runtime, "");
        }
        if let Some(keys) = table(poetry.get("dev-dependencies")) {
            reader.keys(keys, Kind::Development, "dev");
        }
        if let Some(groups) = table(poetry.get("group")) {
            for (group, body) in groups.iter() {
                if let Some(keys) = table(body.as_table_like().and_then(|b| b.get("dependencies")))
                {
                    reader.keys(keys, Kind::Development, group);
                }
            }
        }
    }
    Ok(Listing {
        name,
        deps: reader.deps,
    })
}

struct Reader<'s> {
    source: &'s str,
    deps: Vec<Dep>,
}

impl Reader<'_> {
    /// The distributions of a list of PEP 508 strings.
    fn requirements(&mut self, list: &Item, kind: Kind, group: &str) {
        let Some(array) = list.as_array() else {
            return;
        };
        for value in array {
            let Some(text) = value.as_str() else { continue };
            let Some(shown) = requirement_name(text) else {
                continue;
            };
            let line = value
                .span()
                .map_or(0, |span| line_of(self.source, span.start));
            self.push(shown, line, kind, group);
        }
    }

    /// The keys of a table of Poetry's, but `python`, which is the interpreter.
    fn keys(&mut self, keys: &dyn TableLike, kind: Kind, group: &str) {
        for (name, item) in keys.iter() {
            if name == "python" {
                continue;
            }
            let at = keys
                .get_key_value(name)
                .and_then(|(key, _)| key.span())
                .or_else(|| item.span())
                .map_or(0, |span| span.start);
            self.push(name.to_string(), line_of(self.source, at), kind, group);
        }
    }

    fn push(&mut self, shown: String, line: usize, kind: Kind, group: &str) {
        self.deps.push(Dep {
            name: normalize(&shown),
            shown,
            line,
            kind,
            group: group.to_string(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(source: &str) -> Vec<(String, usize, Kind, String)> {
        read(source)
            .unwrap()
            .deps
            .into_iter()
            .map(|d| (d.name, d.line, d.kind, d.group))
            .collect()
    }

    #[test]
    fn the_dependencies_of_a_project_table_and_its_groups_are_read_with_their_lines() {
        let source = "\
[project]
name = \"Shop_Core\"
dependencies = [
    \"Requests[socks]>=2.31\",
    \"python-dotenv\",
    \"git+https://example.com/x.git\",
]

[project.optional-dependencies]
plot = [\"matplotlib\"]

[dependency-groups]
dev = [\"pytest>=8\", { include-group = \"lint\" }]
lint = [\"ruff\"]
";
        let listing = read(source).unwrap();
        assert_eq!(listing.name.as_deref(), Some("shop-core"));
        let dep = |n: &str, l, k, g: &str| (n.to_string(), l, k, g.to_string());
        assert_eq!(
            names(source),
            [
                dep("requests", 4, Kind::Runtime, ""),
                dep("python-dotenv", 5, Kind::Runtime, ""),
                dep("matplotlib", 10, Kind::Optional, "plot"),
                dep("pytest", 13, Kind::Development, "dev"),
                dep("ruff", 14, Kind::Development, "lint"),
            ]
        );
        // The name is kept as written for a message
        assert_eq!(listing.deps[0].shown, "Requests");
    }

    #[test]
    fn the_tools_that_keep_their_own_tables_are_read_too() {
        let source = "\
[tool.poetry]
name = \"app\"

[tool.poetry.dependencies]
python = \"^3.12\"
httpx = \"^0.27\"

[tool.poetry.group.test.dependencies]
pytest = \"*\"

[tool.uv]
dev-dependencies = [\"mypy\"]

[tool.pdm.dev-dependencies]
docs = [\"mkdocs\"]
";
        let kinds: Vec<(String, Kind)> = names(source)
            .into_iter()
            .map(|(name, _, kind, _)| (name, kind))
            .collect();
        assert_eq!(
            kinds,
            [
                ("mypy".to_string(), Kind::Development),
                ("mkdocs".to_string(), Kind::Development),
                ("httpx".to_string(), Kind::Runtime),
                ("pytest".to_string(), Kind::Development),
            ]
        );
    }

    #[test]
    fn a_file_that_is_not_toml_is_an_error_with_its_line() {
        let (line, _) = read("[project]\nname = \n").unwrap_err();
        assert_eq!(line, 2);
    }

    #[test]
    fn a_file_with_no_dependencies_lists_none() {
        let listing = read("[build-system]\nrequires = [\"hatchling\"]\n").unwrap();
        assert_eq!(listing, Listing::default());
    }
}
