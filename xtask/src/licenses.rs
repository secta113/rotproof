//! Checks that `THIRD-PARTY-LICENSES.txt` is complete. The wheel and the binary link every crate Rotproof depends on,
//! and their licenses (MIT, Apache-2.0 and others) require their notices to go with the binary. The file is generated
//! by cargo-about, which takes minutes to build, so the checks come in two parts:
//!
//! - **The list**, in every run of `cargo xtask ci`: the file names every crate `cargo tree` says the binary links, and
//!   no other. cargo alone answers it. A new or bumped dependency fails here, and the only way to pass is to run
//!   cargo-about, which fails on a license outside `accepted` in `about.toml`.
//! - **The text**, in `cargo xtask licenses --check`: the file is what the pinned cargo-about writes now. It catches
//!   what the list cannot (a file edited by hand, a change to `about.toml` or `about.hbs`), and runs only when those
//!   files change (`.github/workflows/licenses.yml`) and before a release.

use std::collections::BTreeSet;

pub const FILE: &str = "THIRD-PARTY-LICENSES.txt";

/// The workflow that installs cargo-about and checks the text
pub const WORKFLOW: &str = ".github/workflows/licenses.yml";

/// Fewer crates than this means the output of `cargo tree` was not read, not that Rotproof lost its dependencies
pub const MIN_CRATES: usize = 10;

/// `name version` of every crate in the output of `cargo tree --prefix none --format {p}`, except Rotproof's own: the
/// first line, which is the root crate itself, and its layers, which come from a path on this machine
/// (`utils v0.0.0 (/work/crates/utils)`). A crate from git is not Rotproof's and counts. A crate shown again (`(*)`)
/// counts once.
pub fn tree_crates(cargo_tree: &str) -> BTreeSet<String> {
    cargo_tree
        .lines()
        .skip(1)
        .filter_map(|l| {
            let mut words = l.split_whitespace();
            let name = words.next()?;
            let version = words.next()?.strip_prefix('v')?;
            let source = words.next().unwrap_or_default();
            (!is_local_path(source)).then(|| format!("{name} {version}"))
        })
        .collect()
}

/// Whether the source `cargo tree` writes after a version is a path on this machine: `(/work/...)` or `(D:\...)`.
fn is_local_path(source: &str) -> bool {
    let Some(path) = source.strip_prefix('(') else {
        return false;
    };
    let bytes = path.as_bytes();
    path.starts_with('/') || (bytes.len() > 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':')
}

/// `name version` of every crate the notices list: the `- name version (repository)` lines under each `Used by:`,
/// up to the blank line before the license text. Lines in the license texts are never read.
pub fn listed_crates(notices: &str) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    let mut in_list = false;
    for line in notices.lines() {
        if line == "Used by:" {
            in_list = true;
        } else if line.is_empty() {
            in_list = false;
        } else if in_list && let Some(item) = line.strip_prefix("- ") {
            let mut words = item.split_whitespace();
            if let (Some(name), Some(version)) = (words.next(), words.next()) {
                found.insert(format!("{name} {version}"));
            }
        }
    }
    found
}

/// The version of cargo-about the Dockerfile and the licenses workflow install (`cargo-about@<version>`). Both have to
/// name it, and the same one: a check that compares nothing would pass.
pub fn pinned_version(dockerfile: &str, workflow: &str) -> Result<String, Vec<String>> {
    let versions = |text: &str| -> BTreeSet<String> {
        text.split_whitespace()
            .filter_map(|w| w.strip_prefix("cargo-about@"))
            .map(str::to_string)
            .collect()
    };
    let (docker, workflow) = (versions(dockerfile), versions(workflow));
    let mut found = Vec::new();
    for (file, v) in [("Dockerfile", &docker), (WORKFLOW, &workflow)] {
        if v.len() != 1 {
            found.push(format!(
                "{file} should install one version of cargo-about (cargo-about@<version>), and names {v:?}"
            ));
        }
    }
    if found.is_empty() && docker != workflow {
        found.push(format!(
            "the Dockerfile installs cargo-about {docker:?}, but {WORKFLOW} installs {workflow:?}"
        ));
    }
    match docker.into_iter().next() {
        Some(v) if found.is_empty() => Ok(v),
        _ => Err(found),
    }
}

/// Where the crates the file lists and the crates `cargo tree` lists disagree
pub fn list_problems(committed: &str, tree: &BTreeSet<String>) -> Vec<String> {
    let mut found = Vec::new();
    if tree.len() < MIN_CRATES {
        found.push(format!(
            "cargo tree lists {} crates, fewer than {MIN_CRATES}: did its output change?",
            tree.len()
        ));
    }
    let listed = listed_crates(committed);
    for c in tree.difference(&listed) {
        found.push(format!(
            "{c} is linked into the binary, but {FILE} does not list it"
        ));
    }
    for c in listed.difference(tree) {
        found.push(format!(
            "{c} is listed in {FILE}, but cargo tree does not link it"
        ));
    }
    if tree.len() >= MIN_CRATES && !found.is_empty() {
        found.push(format!(
            "after a change to the dependencies, run `cargo xtask licenses` and commit {FILE}"
        ));
    }
    found
}

/// Where the file differs from what cargo-about writes now. `generated` has its line endings made `\n`.
pub fn text_problems(committed: &str, generated: &str) -> Vec<String> {
    if committed.replace("\r\n", "\n") == generated {
        Vec::new()
    } else {
        vec![format!(
            "{FILE} is not what cargo-about writes now: run `cargo xtask licenses` and commit the result"
        )]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TREE: &str = "\
rotproof v0.1.0 (/work)
chrono v0.4.45
libc v0.2.189
clap v4.6.7
libc v0.2.189 (*)
utils v0.0.0 (/work/crates/utils)
domain v0.0.0 (D:\\work\\crates\\domain)
";

    #[test]
    fn a_crate_from_git_counts_and_one_of_rotproofs_layers_does_not() {
        let tree = "rotproof v0.1.0 (/work)\nforked v1.0.0 (https://github.com/x/forked#abc)\nutils v0.0.0 (/work/crates/utils)\n";
        assert_eq!(tree_crates(tree), set(&["forked 1.0.0"]));
    }

    const NOTICES: &str = "\
Third-party licenses of Rotproof

================================================================================
MIT License (MIT)

Used by:
- chrono 0.4.45 (https://github.com/chronotope/chrono)
- libc 0.2.189

Permission is hereby granted, free of charge,
- not a crate, a line of the license text

================================================================================
Apache License 2.0 (Apache-2.0)

Used by:
- clap 4.6.7 (https://github.com/clap-rs/clap)

   Apache License
";

    const HINT: &str = "after a change to the dependencies, run `cargo xtask licenses` and commit THIRD-PARTY-LICENSES.txt";

    fn set(items: &[&str]) -> BTreeSet<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    /// Crates added to both sides, so that the tree is above the floor and only the difference under test shows
    fn pad() -> Vec<String> {
        (0..MIN_CRATES).map(|i| format!("pad{i} 1.0.0")).collect()
    }

    fn tree() -> BTreeSet<String> {
        tree_crates(TREE).into_iter().chain(pad()).collect()
    }

    fn padded(notices: &str) -> String {
        let items: String = pad().iter().map(|c| format!("- {c}\n")).collect();
        notices.replacen("Used by:\n", &format!("Used by:\n{items}"), 1)
    }

    #[test]
    fn reads_the_crates_of_cargo_tree_without_the_root() {
        assert_eq!(
            tree_crates(TREE),
            set(&["chrono 0.4.45", "clap 4.6.7", "libc 0.2.189"])
        );
    }

    #[test]
    fn reads_only_the_lists_under_used_by() {
        assert_eq!(
            listed_crates(NOTICES),
            set(&["chrono 0.4.45", "clap 4.6.7", "libc 0.2.189"])
        );
    }

    #[test]
    fn a_list_that_matches_the_tree_passes() {
        assert_eq!(
            list_problems(&padded(NOTICES), &tree()),
            Vec::<String>::new()
        );
    }

    #[test]
    fn a_crate_the_file_misses_or_adds_fails() {
        let notices = padded(NOTICES).replace("- libc 0.2.189\n", "- regex 1.13.1\n");
        assert_eq!(
            list_problems(&notices, &tree()),
            [
                "libc 0.2.189 is linked into the binary, but THIRD-PARTY-LICENSES.txt does not list it",
                "regex 1.13.1 is listed in THIRD-PARTY-LICENSES.txt, but cargo tree does not link it",
                HINT,
            ]
        );
    }

    #[test]
    fn a_bumped_crate_fails() {
        let notices = padded(NOTICES).replace("- chrono 0.4.45 ", "- chrono 0.4.44 ");
        assert_eq!(
            list_problems(&notices, &tree()),
            [
                "chrono 0.4.45 is linked into the binary, but THIRD-PARTY-LICENSES.txt does not list it",
                "chrono 0.4.44 is listed in THIRD-PARTY-LICENSES.txt, but cargo tree does not link it",
                HINT,
            ]
        );
    }

    #[test]
    fn a_tree_read_as_empty_fails_by_its_floor() {
        assert_eq!(
            list_problems("", &BTreeSet::new()),
            ["cargo tree lists 0 crates, fewer than 10: did its output change?"]
        );
    }

    #[test]
    fn the_generated_text_passes_with_any_line_endings() {
        let notices = padded(NOTICES);
        assert_eq!(text_problems(&notices, &notices), Vec::<String>::new());
        let crlf = notices.replace('\n', "\r\n");
        assert_eq!(text_problems(&crlf, &notices), Vec::<String>::new());
    }

    #[test]
    fn a_file_edited_by_hand_fails() {
        let notices = padded(NOTICES);
        let edited = notices.replace("Permission is hereby granted", "Permission granted");
        assert_eq!(
            text_problems(&edited, &notices),
            [
                "THIRD-PARTY-LICENSES.txt is not what cargo-about writes now: run `cargo xtask licenses` and commit the result"
            ]
        );
    }

    #[test]
    fn the_pinned_version_is_the_one_both_files_install() {
        let docker = "RUN cargo install --locked --features cli cargo-about@0.9.2 \\\n";
        let workflow = "      - run: cargo install --locked --features cli cargo-about@0.9.2\n";
        assert_eq!(pinned_version(docker, workflow), Ok("0.9.2".into()));
    }

    #[test]
    fn a_version_missing_or_different_fails() {
        let docker = "RUN cargo install cargo-about@0.9.2\n";
        assert_eq!(
            pinned_version(docker, "run: cargo install cargo-about@0.9.1\n"),
            Err(vec![
                "the Dockerfile installs cargo-about {\"0.9.2\"}, but .github/workflows/licenses.yml installs {\"0.9.1\"}"
                    .into()
            ])
        );
        assert_eq!(
            pinned_version(docker, "run: cargo install cargo-about\n"),
            Err(vec![
                ".github/workflows/licenses.yml should install one version of cargo-about (cargo-about@<version>), and names {}"
                    .into()
            ])
        );
    }
}
