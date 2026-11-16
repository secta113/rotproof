//! What a `package.json` lists as dependencies, and the name of its package.
//!
//! - **Runtime:** `dependencies`.
//! - **Optional:** `peerDependencies` and `optionalDependencies`, which the package may use and need not list as its
//!   own.
//! - **Development:** `devDependencies`.
//!
//! The names are npm's, compared as written. JSON has no lines in `serde_json`, so the line of a dependency is the
//! first line that holds its quoted name after the line of the table it is in: a message points at the right place
//! unless the same name is written twice in the file before it.

use serde_json::Value;

use domain::dependencies::{Dep, Kind, Listing};
use domain::tree::with_lf;

/// The tables of `package.json` that list dependencies, with the kind each lists.
const TABLES: [(&str, Kind); 4] = [
    ("dependencies", Kind::Runtime),
    ("peerDependencies", Kind::Optional),
    ("optionalDependencies", Kind::Optional),
    ("devDependencies", Kind::Development),
];

/// Read one `package.json`, or `Err` with the line of the error and what the parser says.
pub fn read(source: &str) -> Result<Listing, (usize, String)> {
    let text = with_lf(source);
    let json: Value = serde_json::from_str(&text).map_err(|e| (e.line(), e.to_string()))?;
    let mut listing = Listing {
        name: json.get("name").and_then(Value::as_str).map(String::from),
        deps: Vec::new(),
    };
    for (table, kind) in TABLES {
        let Some(entries) = json.get(table).and_then(Value::as_object) else {
            continue;
        };
        let from = line_of_key(&text, table, 0).unwrap_or(1);
        for name in entries.keys() {
            listing.deps.push(Dep {
                name: name.clone(),
                shown: name.clone(),
                line: line_of_key(&text, name, from.saturating_sub(1)).unwrap_or(from),
                kind,
                group: String::new(),
            });
        }
    }
    Ok(listing)
}

/// The line (from 1) of the first `"key"` followed by `:` on a line after the `skip` first lines.
fn line_of_key(text: &str, key: &str, skip: usize) -> Option<usize> {
    let quoted = format!("\"{key}\"");
    text.lines()
        .enumerate()
        .skip(skip)
        .find(|(_, line)| {
            line.find(&quoted)
                .is_some_and(|at| line[at + quoted.len()..].trim_start().starts_with(':'))
        })
        .map(|(i, _)| i + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_table_is_read_with_the_kind_it_lists_and_the_line_of_each_name() {
        let source = "{\n  \"name\": \"@shop/billing\",\n  \"dependencies\": {\n    \"@shop/kernel\": \"workspace:*\",\n    \
                      \"react\": \"^18\"\n  },\n  \"devDependencies\": {\n    \"react\": \"^18\",\n    \"vitest\": \"^2\"\n  },\n  \
                      \"peerDependencies\": { \"vite\": \"*\" }\n}\n";
        let listing = read(source).unwrap();
        assert_eq!(listing.name.as_deref(), Some("@shop/billing"));
        let found: Vec<(&str, usize, Kind)> = listing
            .deps
            .iter()
            .map(|d| (d.name.as_str(), d.line, d.kind))
            .collect();
        assert_eq!(
            found,
            [
                ("@shop/kernel", 4, Kind::Runtime),
                ("react", 5, Kind::Runtime),
                ("vite", 11, Kind::Optional),
                ("react", 8, Kind::Development),
                ("vitest", 9, Kind::Development),
            ]
        );
    }

    #[test]
    fn a_file_that_is_not_json_is_an_error_with_its_line() {
        let (line, _) = read("{\n  \"name\": \n}\n").unwrap_err();
        assert_eq!(line, 3);
    }

    #[test]
    fn a_package_with_no_tables_lists_nothing() {
        assert_eq!(read("{}").unwrap(), Listing::default());
    }
}
