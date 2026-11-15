//! The names of Python's packages: how two names compare (PEP 503), the name a requirement string (PEP 508) gives, and
//! which distribution an import is of.
//!
//! An import names a module and a manifest lists a distribution, and the two differ (`dotenv` is `python-dotenv`,
//! `PIL` is `pillow`, `bs4` is `beautifulsoup4`). What the project's declaration maps (`imports` under
//! `[dependencies]`) is the answer; an import it does not map is taken to be of the distribution of the same name,
//! which is what most are, and a finding for one that is not names the line to add.

use std::collections::BTreeMap;

/// A distribution's name as PEP 503 compares names: lowercase, with every run of `-`, `_` and `.` one `-`.
pub fn normalize(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut dash = false;
    for c in name.trim().chars() {
        if matches!(c, '-' | '_' | '.') {
            dash = true;
            continue;
        }
        if dash && !out.is_empty() {
            out.push('-');
        }
        dash = false;
        out.extend(c.to_lowercase());
    }
    out
}

/// The distribution a PEP 508 requirement names (`requests[socks]>=2.0; python_version >= "3.10"` is `requests`), as
/// the string writes it. `None` for a requirement that names none: a path or a URL (`-e .`, `git+https://...`), an
/// option, or an empty string.
pub fn requirement_name(requirement: &str) -> Option<String> {
    let text = requirement.trim();
    let end = text
        .find(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')))
        .unwrap_or(text.len());
    let name = &text[..end];
    let first = name.chars().next()?;
    let last = name.chars().last()?;
    // PEP 508: a letter or digit at each end, so `-e` and `.` are no name
    if !first.is_ascii_alphanumeric() || !last.is_ascii_alphanumeric() {
        return None;
    }
    // `pkg @ url` and `pkg[extra]` and `pkg>=1` end the name; `pkg:` or `pkg/` is a path
    match text[end..].chars().next() {
        None | Some(' ' | '\t' | '[' | '(' | '<' | '>' | '=' | '!' | '~' | ';' | '@' | ',') => {
            Some(name.to_string())
        }
        Some(_) => None,
    }
}

/// The distribution a module of an import is of, normalized, and whether the declaration says so. `module` is the
/// dotted name, `mapped` the declaration's `imports` as it writes them (import name, distribution): the longest entry
/// that is the module or a package above it wins, so `google.cloud.storage` can be mapped without `google` being. An
/// import that no entry covers is of the distribution named like its first part.
pub fn distribution_of(module: &[String], mapped: &BTreeMap<String, String>) -> (String, bool) {
    let dotted = module.join(".");
    let found = mapped
        .iter()
        .filter(|(name, _)| {
            dotted == **name
                || dotted
                    .strip_prefix(name.as_str())
                    .is_some_and(|rest| rest.starts_with('.'))
        })
        .max_by_key(|(name, _)| name.len());
    match found {
        Some((_, distribution)) => (normalize(distribution), true),
        None => (normalize(module.first().map_or("", String::as_str)), false),
    }
}

/// The distribution a directory of an environment's `site-packages` is the metadata of, normalized
/// (`python_dotenv-1.0.1.dist-info` is `python-dotenv`). `None` for any other name.
pub fn dist_info_name(directory: &str) -> Option<String> {
    let stem = directory.strip_suffix(".dist-info")?;
    let (name, _version) = stem.split_once('-')?;
    (!name.is_empty()).then(|| normalize(name))
}

/// The names a distribution installed into `site-packages` is imported by, from its `RECORD`: the first part of each
/// path it installed there (`dotenv/main.py` is `dotenv`, `six.py` is `six`, `_cffi_backend.cpython-312.so` is
/// `_cffi_backend`). A path that leaves `site-packages` (`../../bin/x`, the scripts), the metadata, `__pycache__`, a
/// `.pth` file and a file of no module are no name. This is a hint for a message: the verdict never reads it.
pub fn record_names(record: &str) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for line in record.lines() {
        let path = line
            .split(',')
            .next()
            .unwrap_or("")
            .trim()
            .trim_matches('"');
        let Some(first) = path.split('/').next().filter(|first| !first.is_empty()) else {
            continue;
        };
        if first == ".." || first == "__pycache__" || first.ends_with(".dist-info") {
            continue;
        }
        let name = if path.contains('/') {
            first
        } else {
            // A module in a file of its own: `six.py`, `_cffi_backend.cpython-312-x86_64-linux-gnu.so`
            let Some((stem, extension)) = first.split_once('.') else {
                continue;
            };
            if !(extension.ends_with("py")
                || extension.ends_with("so")
                || extension.ends_with("pyd"))
            {
                continue;
            }
            stem
        };
        if !name.is_empty() && !names.iter().any(|known| known == name) {
            names.push(name.to_string());
        }
    }
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_an_environment_installed_is_read_from_the_metadata_names_and_the_record() {
        assert_eq!(
            dist_info_name("python_dotenv-1.0.1.dist-info").as_deref(),
            Some("python-dotenv")
        );
        assert_eq!(dist_info_name("pip-24.0.dist-info").as_deref(), Some("pip"));
        assert_eq!(dist_info_name("six.py"), None);
        let record = "dotenv/__init__.py,sha256=x,10\ndotenv/main.py,sha256=y,20\n\
                      dotenv/__pycache__/main.cpython-312.pyc,,\npython_dotenv-1.0.1.dist-info/METADATA,sha256=z,1\n\
                      ../../Scripts/dotenv.exe,sha256=q,3\nsix.py,sha256=w,4\n\
                      _cffi_backend.cpython-312-x86_64-linux-gnu.so,sha256=v,5\nnspkg.pth,sha256=u,6\n";
        assert_eq!(record_names(record), ["dotenv", "six", "_cffi_backend"]);
    }

    #[test]
    fn names_compare_as_pep_503_says() {
        assert_eq!(normalize("Python_Dotenv"), "python-dotenv");
        assert_eq!(normalize("zope.interface"), "zope-interface");
        assert_eq!(normalize("a--b__c..d-_.e"), "a-b-c-d-e");
        assert_eq!(normalize("PyYAML"), "pyyaml");
    }

    #[test]
    fn a_requirement_gives_its_distribution_whatever_follows() {
        for (text, name) in [
            ("requests", "requests"),
            ("requests>=2.31", "requests"),
            ("requests[socks] >= 2.0", "requests"),
            ("requests (>=2.0)", "requests"),
            ("Django~=4.2; python_version >= \"3.10\"", "Django"),
            ("pkg @ https://example.com/pkg.zip", "pkg"),
            ("  pkg==1.0 ", "pkg"),
            ("a.b-c_d!=1", "a.b-c_d"),
            ("pytest<9,>=8", "pytest"),
        ] {
            assert_eq!(requirement_name(text).as_deref(), Some(name), "{text}");
        }
        for text in [
            "",
            "-e .",
            "./local",
            "git+https://x/y.git",
            "https://x/y.whl",
            "-r base.txt",
        ] {
            assert_eq!(requirement_name(text), None, "{text}");
        }
    }

    #[test]
    fn an_import_is_of_the_distribution_the_declaration_maps_it_to_or_of_the_one_named_like_it() {
        let mapped: BTreeMap<String, String> = [
            ("dotenv", "python-dotenv"),
            ("google.cloud.storage", "google-cloud-storage"),
            ("google", "protobuf"),
        ]
        .iter()
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect();
        let of = |module: &str| {
            let parts: Vec<String> = module.split('.').map(String::from).collect();
            distribution_of(&parts, &mapped)
        };
        assert_eq!(of("dotenv"), ("python-dotenv".to_string(), true));
        assert_eq!(of("dotenv.main"), ("python-dotenv".to_string(), true));
        assert_eq!(
            of("google.cloud.storage.blob"),
            ("google-cloud-storage".to_string(), true)
        );
        // The longest entry wins, and a name that only starts like one is not it
        assert_eq!(of("google.auth"), ("protobuf".to_string(), true));
        assert_eq!(of("dotenvx"), ("dotenvx".to_string(), false));
        assert_eq!(of("Flask_Cors"), ("flask-cors".to_string(), false));
    }
}
