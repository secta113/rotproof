//! Server-side templates, the views of a Python project with screens that the server renders: the references between
//! template files, and the template languages they are read as.
//!
//! - **A reference is a tag of a template that names another template by a string**: `{% extends %}`,
//!   `{% include %}`, `{% import %}` and `{% from %}`. Only a tag the language has is read, and only when it names the
//!   template with a literal; a name built at run time (a variable, a concatenation, a list) is not seen, as an
//!   import built at run time is not. What is inside `{# #}`, `{% raw %}`, `{% verbatim %}` and `{% comment %}` is no
//!   tag.
//! - **The libraries' syntax is known here, and a version is named for it.** A template says nothing of the language
//!   it is written in, so Rotproof reads the references as the languages its layout names (`[templates]` in
//!   `layers/python.toml`), at the major versions named, and says so on every run. A project whose `pyproject.toml`
//!   asks for a package outside those versions is not judged, and `rotproof check` says that too. Nothing else about
//!   the libraries is known: not the lock file, not the settings of the loader.
//! - **The versions a manifest asks for** are read from its requirements (PEP 508, a form every tool writes): the
//!   dependencies, the optional dependencies and the dependency groups. A package it does not name is read, since a
//!   framework brings it in; one it names with a version range is read when the range holds a version of the one the
//!   layout names, and left out when it holds none.

use std::cmp::Ordering;

use crate::layers::{TemplateLanguage, Templates};

/// The tags a layout may name: every tag Rotproof reads a reference from.
pub const TAGS: [&str; 4] = ["extends", "include", "import", "from"];

/// A template's reference to another template.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reference {
    /// Counted from 1: the line the tag begins on
    pub line: usize,
    /// The tag, one of [`TAGS`]
    pub tag: String,
    /// The name of the template as written, a path in the loader's directory
    pub target: String,
}

impl Reference {
    /// What the reference does, as a finding says it: "extends", "includes", "imports".
    pub fn verb(&self) -> &'static str {
        match self.tag.as_str() {
            "extends" => "extends",
            "include" => "includes",
            _ => "imports",
        }
    }
}

/// The references in the text of a template, in the order they are written, of the tags `tags`.
pub fn references(text: &str, tags: &[&str]) -> Vec<Reference> {
    let mut found = Vec::new();
    let mut at = 0;
    while let Some(rel) = text[at..].find('{') {
        let open = at + rel;
        match text.as_bytes().get(open + 1) {
            Some(b'#') => {
                at = text[open + 2..]
                    .find("#}")
                    .map_or(text.len(), |end| open + 2 + end + 2);
            }
            Some(b'%') => {
                let Some(end) = text[open + 2..].find("%}") else {
                    break;
                };
                let (name, rest) = split_tag(&text[open + 2..open + 2 + end]);
                at = open + 2 + end + 2;
                if matches!(name, "raw" | "verbatim" | "comment") {
                    at = after_block(text, at, &format!("end{name}"));
                } else if tags.contains(&name)
                    && let Some(target) = target_of(name, rest)
                {
                    found.push(Reference {
                        line: 1 + text[..open].matches('\n').count(),
                        tag: name.to_string(),
                        target,
                    });
                }
            }
            _ => at = open + 1,
        }
    }
    found
}

/// The offset after the tag named `end` that closes a block opened before `from`, or the end of the text.
fn after_block(text: &str, from: usize, end: &str) -> usize {
    let mut at = from;
    while let Some(rel) = text[at..].find("{%") {
        let open = at + rel;
        let Some(close) = text[open + 2..].find("%}") else {
            return text.len();
        };
        at = open + 2 + close + 2;
        if split_tag(&text[open + 2..open + 2 + close]).0 == end {
            return at;
        }
    }
    text.len()
}

/// The name of the tag a `{% ... %}` holds and what follows it, without the signs that trim white space (`-`, `+`).
fn split_tag(body: &str) -> (&str, &str) {
    let body = body.strip_prefix(['-', '+']).unwrap_or(body);
    let body = body.strip_suffix('-').unwrap_or(body).trim();
    let end = body
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .unwrap_or(body.len());
    let (name, rest) = body.split_at(end);
    (name, rest.trim_start())
}

/// The name of the template a tag refers to, when it is a literal and nothing else names a template: what may follow
/// is what the tag takes of its own (`ignore missing`, `with`, `only`, `as`, `import`).
fn target_of(tag: &str, rest: &str) -> Option<String> {
    let quote = rest.chars().next().filter(|c| matches!(c, '"' | '\''))?;
    let inner = &rest[1..];
    let end = inner.find(quote)?;
    let (name, after) = (&inner[..end], &inner[end + 1..]);
    if name.is_empty()
        || name.contains('\\')
        || !(after.is_empty() || after.starts_with(char::is_whitespace))
    {
        return None;
    }
    let word = after.split_whitespace().next();
    let follows = match tag {
        "extends" => word.is_none(),
        "include" => word.is_none_or(|w| matches!(w, "ignore" | "with" | "without" | "only")),
        "import" => word == Some("as"),
        "from" => word == Some("import"),
        _ => false,
    };
    follows.then(|| name.to_string())
}

/// The path of a reference from the loader's directory: `None` when it climbs out of it (`..`, which neither Jinja
/// nor Django allow), starts with `/` or holds a `\`, so it names no template of the project. `.` and empty parts are
/// dropped.
pub fn path_of(target: &str) -> Option<String> {
    if target.starts_with('/') || target.contains('\\') || target.split('/').any(|p| p == "..") {
        return None;
    }
    let parts: Vec<&str> = target
        .split('/')
        .filter(|part| !matches!(*part, "" | "."))
        .collect();
    (!parts.is_empty()).then(|| parts.join("/"))
}

/// What the template languages of a layout say of the references: how they are read, as the check says it on every
/// run ("Jinja2 3 and Django 5/6").
pub fn read_as(templates: &Templates) -> String {
    let named: Vec<String> = templates.languages.iter().map(named).collect();
    named.join(" and ")
}

/// A language with its versions: "Django 5/6".
fn named(language: &TemplateLanguage) -> String {
    let versions: Vec<String> = language.versions.iter().map(u64::to_string).collect();
    format!("{} {}", language.name, versions.join("/"))
}

/// The languages the references of one unit are read as, from the manifest of its package.
#[derive(Debug)]
pub struct Reading<'a> {
    /// The languages the references are read as
    pub read: Vec<&'a TemplateLanguage>,
    /// The languages that are not, as named with their versions, each with why: what the manifest asks for
    pub left: Vec<(String, String)>,
}

/// What `rotproof check` says of the templates on every run: how the references are read, and for which unit they are
/// not, in whole or for a language.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Premise {
    /// The languages and versions the references are read as: [`read_as`]
    pub read_as: String,
    /// One line for each unit that is not read as every language, from [`Reading::not_judged`]
    pub not_judged: Vec<String>,
}

impl Reading<'_> {
    /// What to say of the unit named `unit` ("" for the implicit unit) when some language is left: that its references
    /// are not judged when none is read, and as which language they are not read otherwise, with why. `None` when
    /// every language is read.
    pub fn not_judged(&self, unit: &str) -> Option<String> {
        if self.left.is_empty() {
            return None;
        }
        let of = if unit.is_empty() {
            String::new()
        } else {
            format!("unit {unit}: ")
        };
        let names: Vec<&str> = self.left.iter().map(|(name, _)| name.as_str()).collect();
        let why: Vec<&str> = self.left.iter().map(|(_, why)| why.as_str()).collect();
        let what = if self.read.is_empty() {
            "are not judged".to_string()
        } else {
            format!("are not read as {}", names.join(" and "))
        };
        Some(format!("{of}references {what}: {}", why.join("; ")))
    }

    /// The tags of every language read, without repeats, in the order the layout names them.
    pub fn tags(&self) -> Vec<&str> {
        let mut tags: Vec<&str> = Vec::new();
        for tag in self.read.iter().flat_map(|language| &language.tags) {
            if !tags.contains(&tag.as_str()) {
                tags.push(tag);
            }
        }
        tags
    }
}

/// The languages of `templates` a project's references are read as, given the text of the manifest of its package
/// (`None` when it has none), which is named `manifest` in the reasons.
pub fn reading<'a>(templates: &'a Templates, manifest: &str, text: Option<&str>) -> Reading<'a> {
    let requirements = match text.map(requirements) {
        None => Ok(Vec::new()),
        Some(read) => read,
    };
    let mut reading = Reading {
        read: Vec::new(),
        left: Vec::new(),
    };
    for language in &templates.languages {
        let asked = match &requirements {
            Ok(requirements) => asked_of(language, requirements),
            Err(why) => Asked::Broken(why.clone()),
        };
        let why = match asked {
            Asked::Read => {
                reading.read.push(language);
                continue;
            }
            Asked::Outside(asks) => format!(
                "{manifest} asks for {}, which is not {}",
                asks.join(", "),
                named(language)
            ),
            Asked::Unreadable(asks) => format!(
                "{manifest} asks for {}, which Rotproof cannot read as a version of {}",
                asks.join(", "),
                language.name
            ),
            Asked::Broken(why) => format!(
                "{manifest} cannot be read ({why}), so the version of {} it asks for is not known",
                language.name
            ),
        };
        reading.left.push((named(language), why));
    }
    reading
}

/// What a manifest says of the package of a language.
enum Asked {
    /// It asks for no version outside the language's, or does not name the package
    Read,
    /// It asks only for versions outside it: each requirement as written
    Outside(Vec<String>),
    /// It asks in a form that cannot be read as a version
    Unreadable(Vec<String>),
    /// The manifest cannot be read at all
    Broken(String),
}

fn asked_of(language: &TemplateLanguage, requirements: &[Requirement]) -> Asked {
    let package = normalized(&language.package);
    let mut outside = Vec::new();
    let mut unreadable = Vec::new();
    for requirement in requirements.iter().filter(|r| r.name == package) {
        match requirement.holds_any(&language.versions) {
            Some(true) => return Asked::Read,
            Some(false) => outside.push(requirement.text.clone()),
            None => unreadable.push(requirement.text.clone()),
        }
    }
    outside.dedup();
    unreadable.dedup();
    if !unreadable.is_empty() {
        Asked::Unreadable(unreadable)
    } else if !outside.is_empty() {
        Asked::Outside(outside)
    } else {
        Asked::Read
    }
}

/// A package name as PEP 503 normalizes it: lower case, a run of `-`, `_` and `.` as one `-`.
fn normalized(name: &str) -> String {
    let mut out = String::new();
    let mut dash = false;
    for c in name.chars() {
        if matches!(c, '-' | '_' | '.') {
            dash = true;
        } else {
            if dash && !out.is_empty() {
                out.push('-');
            }
            dash = false;
            out.push(c.to_ascii_lowercase());
        }
    }
    out
}

/// One requirement of a manifest, such as `jinja2>=3,<4`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Requirement {
    /// The package, as [`normalized`]
    pub name: String,
    /// As written, without its marker and spaces around it
    pub text: String,
    /// What it asks of the version: `None` when it is in a form that cannot be read as one
    range: Option<Range>,
}

impl Requirement {
    /// Whether the range holds a version of the major version of any of `majors`; `None` when it cannot be read.
    fn holds_any(&self, majors: &[u64]) -> Option<bool> {
        let range = self.range.as_ref()?;
        Some(majors.iter().any(|major| range.holds_major(*major)))
    }
}

/// The requirements of a manifest, `pyproject.toml`: `project.dependencies`, each list of
/// `project.optional-dependencies` and each group of `dependency-groups`. `Err` is why the text is no TOML.
pub fn requirements(text: &str) -> Result<Vec<Requirement>, String> {
    let table: toml::Table = text
        .parse()
        .map_err(|e: toml::de::Error| e.message().to_string())?;
    let mut lists: Vec<&toml::value::Array> = Vec::new();
    let project = table.get("project").and_then(toml::Value::as_table);
    if let Some(array) = project
        .and_then(|p| p.get("dependencies"))
        .and_then(toml::Value::as_array)
    {
        lists.push(array);
    }
    for key in [
        ("project", "optional-dependencies"),
        ("dependency-groups", ""),
    ] {
        let group = if key.1.is_empty() {
            table.get(key.0)
        } else {
            project.and_then(|p| p.get(key.1))
        };
        lists.extend(
            group
                .and_then(toml::Value::as_table)
                .into_iter()
                .flat_map(|t| t.values())
                .filter_map(toml::Value::as_array),
        );
    }
    Ok(lists
        .into_iter()
        .flatten()
        .filter_map(toml::Value::as_str)
        .filter_map(requirement)
        .collect())
}

/// A requirement as PEP 508 writes it: a name, extras, a version range, and a marker after `;`.
fn requirement(text: &str) -> Option<Requirement> {
    let written = text.split(';').next().unwrap_or_default().trim();
    let end = written
        .find(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')))
        .unwrap_or(written.len());
    let (name, rest) = written.split_at(end);
    if name.is_empty() {
        return None;
    }
    let mut rest = rest.trim_start();
    if rest.starts_with('[') {
        rest = rest
            .find(']')
            .map_or("", |close| rest[close + 1..].trim_start());
    }
    let rest = rest
        .strip_prefix('(')
        .and_then(|inner| inner.strip_suffix(')'))
        .unwrap_or(rest);
    Some(Requirement {
        name: normalized(name),
        text: written.to_string(),
        range: range_of(rest),
    })
}

/// A version as its release segments: `3.1.2` is `[3, 1, 2]`. A suffix (`rc1`, `.post1`, `+local`) is dropped.
type Release = Vec<u64>;

/// The release of a version, and whether it ends in `.*`. `None` when it has no number.
fn release(text: &str) -> Option<(Release, bool)> {
    let text = text.trim();
    let text = text.strip_prefix(['v', 'V']).unwrap_or(text);
    // An epoch (`1!`) is dropped, a local version (`+x`) too
    let text = text.split_once('!').map_or(text, |(_, rest)| rest);
    let text = text.split('+').next().unwrap_or_default();
    let mut parts = Vec::new();
    let mut wildcard = false;
    for part in text.split('.') {
        if part == "*" {
            wildcard = true;
            break;
        }
        let digits: String = part.chars().take_while(char::is_ascii_digit).collect();
        if digits.is_empty() {
            break;
        }
        parts.push(digits.parse().ok()?);
        if digits.len() != part.len() {
            break;
        }
    }
    (!parts.is_empty()).then_some((parts, wildcard))
}

/// A version compared as numbers, a missing segment being 0.
fn compare(a: &[u64], b: &[u64]) -> Ordering {
    (0..a.len().max(b.len()))
        .map(|i| {
            a.get(i)
                .copied()
                .unwrap_or(0)
                .cmp(&b.get(i).copied().unwrap_or(0))
        })
        .find(|order| order.is_ne())
        .unwrap_or(Ordering::Equal)
}

/// One end of a range of versions.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Bound {
    version: Release,
    inclusive: bool,
}

/// The versions a requirement asks for, as the interval its clauses leave: `!=` leaves a hole in it that no major
/// version falls into, so it is not read.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct Range {
    lower: Option<Bound>,
    upper: Option<Bound>,
}

impl Range {
    fn at_least(&mut self, version: Release, inclusive: bool) {
        let tighter =
            self.lower
                .as_ref()
                .is_none_or(|lower| match compare(&version, &lower.version) {
                    Ordering::Greater => true,
                    Ordering::Equal => !inclusive && lower.inclusive,
                    Ordering::Less => false,
                });
        if tighter {
            self.lower = Some(Bound { version, inclusive });
        }
    }

    fn at_most(&mut self, version: Release, inclusive: bool) {
        let tighter =
            self.upper
                .as_ref()
                .is_none_or(|upper| match compare(&version, &upper.version) {
                    Ordering::Less => true,
                    Ordering::Equal => !inclusive && upper.inclusive,
                    Ordering::Greater => false,
                });
        if tighter {
            self.upper = Some(Bound { version, inclusive });
        }
    }

    /// Whether the range holds a version that begins with `major`: it meets `[major, major + 1)`.
    fn holds_major(&self, major: u64) -> bool {
        let mut meets = self.clone();
        meets.at_least(vec![major], true);
        meets.at_most(vec![major + 1], false);
        match (&meets.lower, &meets.upper) {
            (Some(lower), Some(upper)) => match compare(&lower.version, &upper.version) {
                Ordering::Less => true,
                Ordering::Equal => lower.inclusive && upper.inclusive,
                Ordering::Greater => false,
            },
            _ => true,
        }
    }
}

/// The range a list of clauses (`>=3,<4`) leaves, or `None` when one cannot be read: an operator Rotproof does not
/// know (`===`), a version with no number, a wildcard where it has no meaning, or a requirement of a URL (`@`).
fn range_of(clauses: &str) -> Option<Range> {
    let mut range = Range::default();
    if clauses.trim().is_empty() {
        return Some(range);
    }
    for clause in clauses.split(',') {
        let clause = clause.trim();
        let at = clause.find(|c: char| !matches!(c, '<' | '>' | '=' | '!' | '~'))?;
        let (operator, version) = clause.split_at(at);
        let (version, wildcard) = release(version)?;
        match (operator, wildcard) {
            (">=", false) => range.at_least(version, true),
            (">", false) => range.at_least(version, false),
            ("<=", false) => range.at_most(version, true),
            ("<", false) => range.at_most(version, false),
            ("==", false) => {
                range.at_least(version.clone(), true);
                range.at_most(version, true);
            }
            ("==", true) => {
                let mut next = version.clone();
                *next.last_mut()? += 1;
                range.at_least(version, true);
                range.at_most(next, false);
            }
            ("!=", _) => {}
            ("~=", false) if version.len() >= 2 => {
                let mut next = version[..version.len() - 1].to_vec();
                *next.last_mut()? += 1;
                range.at_least(version, true);
                range.at_most(next, false);
            }
            _ => return None,
        }
    }
    Some(range)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layers::layout;

    fn read(text: &str) -> Vec<(String, String, usize)> {
        references(text, &TAGS)
            .into_iter()
            .map(|r| (r.tag, r.target, r.line))
            .collect()
    }

    fn one(tag: &str, target: &str, line: usize) -> (String, String, usize) {
        (tag.to_string(), target.to_string(), line)
    }

    #[test]
    fn a_tag_that_names_a_template_with_a_literal_is_a_reference() {
        let text = "{% extends \"templates/base.html\" %}\n\
                    {%- include 'organisms/nav.html' ignore missing with context -%}\n\
                    {% import \"atoms/forms.html\" as forms %}\n\
                    {%+ from \"atoms/macros.html\" import button, link %}\n\
                    {% include \"atoms/x.html\" only %}\n";
        assert_eq!(
            read(text),
            [
                one("extends", "templates/base.html", 1),
                one("include", "organisms/nav.html", 2),
                one("import", "atoms/forms.html", 3),
                one("from", "atoms/macros.html", 4),
                one("include", "atoms/x.html", 5),
            ]
        );
    }

    #[test]
    fn a_name_built_at_run_time_is_not_seen() {
        let text = "{% extends layout %}{% include \"a\" ~ b %}{% include [\"a.html\", \"b.html\"] %}\n\
                    {% extends \"a.html\" if wide else \"b.html\" %}{% include name %}\n\
                    {% from \"x.html\" %}{% include \"\" %}{% include \"a\\b.html\" %}{% load static %}\n\
                    {% include \"a.html\"x %}";
        assert_eq!(read(text), []);
    }

    #[test]
    fn what_is_written_inside_a_comment_or_a_block_that_is_not_read_is_no_tag() {
        let text = "{# {% include \"a.html\" %} #}\n\
                    {% raw %}{% include \"b.html\" %}{% endraw %}\n\
                    {% verbatim %}{% extends \"c.html\" %}{% endverbatim %}\n\
                    {% comment \"why\" %}{% include \"d.html\" %}{% endcomment %}\n\
                    {% include \"e.html\" %}\n";
        assert_eq!(read(text), [one("include", "e.html", 5)]);
        // A comment or a block that is never closed ends the text
        assert_eq!(read("{# {% include \"a.html\" %}"), []);
        assert_eq!(read("{% raw %}{% include \"a.html\" %}"), []);
        assert_eq!(read("{% include \"a.html\" "), []);
    }

    #[test]
    fn only_the_tags_a_language_has_are_read() {
        let text = "{% import \"a.html\" as a %}{% include \"b.html\" %}";
        let tags = ["extends", "include"];
        let found: Vec<String> = references(text, &tags)
            .into_iter()
            .map(|r| r.target)
            .collect();
        assert_eq!(found, ["b.html"]);
    }

    #[test]
    fn a_reference_is_a_path_from_the_loaders_directory() {
        assert_eq!(path_of("atoms/a.html").as_deref(), Some("atoms/a.html"));
        assert_eq!(path_of("./atoms//a.html").as_deref(), Some("atoms/a.html"));
        for name in [
            "../a.html",
            "atoms/../../a.html",
            "/a.html",
            "a\\b.html",
            "",
            ".",
        ] {
            assert_eq!(path_of(name), None, "{name}");
        }
    }

    fn languages() -> Templates {
        layout("python", "atomic").unwrap().templates.unwrap()
    }

    #[test]
    fn the_languages_are_named_with_their_versions() {
        assert_eq!(read_as(&languages()), "Jinja2 3 and Django 5/6");
    }

    fn outcome(manifest: &str) -> (Vec<String>, Vec<String>) {
        let templates = languages();
        let reading = reading(&templates, "pyproject.toml", Some(manifest));
        (
            reading.read.iter().map(|l| l.name.clone()).collect(),
            reading.left.into_iter().map(|(_, why)| why).collect(),
        )
    }

    #[test]
    fn a_manifest_that_names_no_package_or_a_version_that_holds_the_languages_leaves_them_read() {
        let templates = languages();
        let read = |r: Reading| r.read.iter().map(|l| l.name.clone()).collect::<Vec<_>>();
        for text in [None, Some(""), Some("[project]\nname = \"x\"\n")] {
            let reading = reading(&templates, "pyproject.toml", text);
            assert!(reading.left.is_empty(), "{text:?}");
            assert_eq!(read(reading), ["Jinja2", "Django"]);
        }
        let (read, left) = outcome(
            "[project]\ndependencies = [\"Jinja2>=3\", \"django >= 5.2, < 7 ; python_version >= '3.10'\"]\n",
        );
        assert_eq!(
            (read, left),
            (vec!["Jinja2".into(), "Django".into()], vec![])
        );
        // No upper bound, an extra and a name written otherwise are all the same package
        let (read, _) = outcome("[project]\ndependencies = [\"jinja2[i18n]\", \"Django\"]\n");
        assert_eq!(read, ["Jinja2", "Django"]);
    }

    #[test]
    fn a_manifest_that_asks_for_another_version_leaves_the_language_out_and_says_what_it_asks() {
        let (read, left) = outcome("[project]\ndependencies = [\"jinja2>=4\", \"django\"]\n");
        assert_eq!(read, ["Django"]);
        assert_eq!(
            left,
            ["pyproject.toml asks for jinja2>=4, which is not Jinja2 3"]
        );
        let (read, left) = outcome(
            "[project]\ndependencies = [\"Jinja2<3\"]\n[project.optional-dependencies]\nweb = [\"django>=7\"]\n",
        );
        assert!(read.is_empty());
        assert_eq!(
            left,
            [
                "pyproject.toml asks for Jinja2<3, which is not Jinja2 3",
                "pyproject.toml asks for django>=7, which is not Django 5/6",
            ]
        );
        // One requirement that holds the version is enough, wherever it is: the project ships it
        let (read, left) = outcome(
            "[project]\ndependencies = [\"jinja2>=3\"]\n[dependency-groups]\ndev = [\"jinja2>=4\", { include-group = \"x\" }]\n",
        );
        assert_eq!(read, ["Jinja2", "Django"]);
        assert!(left.is_empty());
    }

    #[test]
    fn a_unit_that_is_not_read_as_every_language_is_named_with_why() {
        let templates = languages();
        let said = |manifest: &str, unit: &str| {
            reading(&templates, "pyproject.toml", Some(manifest)).not_judged(unit)
        };
        assert_eq!(said("[project]\n", ""), None);
        assert_eq!(
            said("[project]\ndependencies = [\"jinja2>=4\"]\n", "").unwrap(),
            "references are not read as Jinja2 3: pyproject.toml asks for jinja2>=4, which is not Jinja2 3"
        );
        assert_eq!(
            said(
                "[project]\ndependencies = [\"jinja2>=4\", \"django>=7\"]\n",
                "billing"
            )
            .unwrap(),
            "unit billing: references are not judged: pyproject.toml asks for jinja2>=4, which is not Jinja2 3; \
             pyproject.toml asks for django>=7, which is not Django 5/6"
        );
    }

    #[test]
    fn what_cannot_be_read_as_a_version_is_not_judged_and_said() {
        let (read, left) =
            outcome("[project]\ndependencies = [\"jinja2===3.1\", \"django @ https://x/d.zip\"]\n");
        assert!(read.is_empty());
        assert_eq!(
            left,
            [
                "pyproject.toml asks for jinja2===3.1, which Rotproof cannot read as a version of Jinja2",
                "pyproject.toml asks for django @ https://x/d.zip, which Rotproof cannot read as a version of Django",
            ]
        );
        let (read, left) = outcome("[project\n");
        assert!(read.is_empty());
        assert_eq!(left.len(), 2);
        assert!(
            left[0].starts_with("pyproject.toml cannot be read ("),
            "{left:?}"
        );
    }

    #[test]
    fn a_range_holds_a_major_version_when_one_of_its_versions_begins_with_it() {
        let holds = |clauses: &str, major: u64| range_of(clauses).unwrap().holds_major(major);
        for (clauses, major, holds_it) in [
            ("", 3, true),
            (">=3", 3, true),
            (">=4", 3, false),
            (">3", 3, true),
            (">3.9", 3, true),
            (">=3,<4", 3, true),
            ("<4", 3, true),
            ("<3", 3, false),
            ("<=3", 3, true),
            ("<3.0.1", 3, true),
            ("<=2.11", 3, false),
            ("==3.1.2", 3, true),
            ("==3.1.*", 3, true),
            ("==3.*", 3, true),
            ("==4.0", 3, false),
            ("==4.*", 3, false),
            ("~=3.1", 3, true),
            ("~=3.1", 4, false),
            ("~=2.1", 3, false),
            ("~=3.1.2", 3, true),
            (">=3.1,!=3.2", 3, true),
            (">=5.2,<6", 5, true),
            (">=5.2,<6", 6, false),
            (">=5.2,<7", 6, true),
            (">= 3.0.0rc1", 3, true),
        ] {
            assert_eq!(holds(clauses, major), holds_it, "{clauses:?} and {major}");
        }
        for clauses in ["===3", ">=x", "==3.*,>=2.*", "~=3", ">=3.*"] {
            assert_eq!(range_of(clauses), None, "{clauses}");
        }
    }

    #[test]
    fn a_requirement_is_read_by_its_name_extras_range_and_marker() {
        let name = |text: &str| requirement(text).map(|r| (r.name, r.text));
        assert_eq!(
            name("Jinja2_Extra[i18n] (>=3) ; python_version > '3.8'"),
            Some(("jinja2-extra".into(), "Jinja2_Extra[i18n] (>=3)".into()))
        );
        assert_eq!(name("  "), None);
        assert_eq!(normalized("Flask.Cors__x"), "flask-cors-x");
    }

    #[test]
    fn every_layout_that_reads_templates_names_the_tags_it_knows() {
        let templates = languages();
        assert!(!templates.extensions.is_empty() && !templates.languages.is_empty());
        for language in &templates.languages {
            assert!(!language.versions.is_empty(), "{}", language.name);
            for tag in &language.tags {
                assert!(TAGS.contains(&tag.as_str()), "{}: {tag}", language.name);
            }
        }
        // The tags of the languages together are every tag there is, so none is read by nothing
        for tag in TAGS {
            assert!(
                templates
                    .languages
                    .iter()
                    .any(|l| l.tags.iter().any(|t| t == tag)),
                "{tag}"
            );
        }
    }
}
