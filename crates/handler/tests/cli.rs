//! The command line, run as a user runs it: the built binary in a separate process.

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rotproof"))
        .args(args)
        .output()
        .expect("the binary runs")
}

/// The comment `rotproof create` leaves in the milestone it writes, until a person writes what to look at
const UNWRITTEN: &str = "<!-- Written by `rotproof create`";

/// `rotproof create` with `args` (`--root <root> create ...`), then what a person does next: the milestone it wrote
/// gets a `# Condition` and the index is written again. Until then the check fails, as
/// `create_leaves_a_milestone_for_a_person_to_write` shows; the tests that go on from a made project start after it.
fn create(args: &[&str]) -> Output {
    let out = run(args);
    let root = args[1];
    let milestone = Path::new(root).join("docs/work/next-milestone.md");
    if let Ok(text) = fs::read_to_string(&milestone)
        && let Some(start) = text.find(UNWRITTEN)
    {
        let written = format!(
            "{}The release workflow of tag v0.1.0 passes every job.\n",
            &text[..start]
        );
        fs::write(&milestone, written).unwrap();
        run(&["--root", root, "index"]);
    }
    out
}

/// What a person does when `rotproof init` declared no area: declare `core`, put the milestone `rotproof create` wrote
/// in it, and write the index again.
fn declare_first_area(root: &Path) {
    let declaration = root.join(".config/rotproof.toml");
    let text = fs::read_to_string(&declaration).unwrap();
    assert!(text.contains("areas = []"), "{text}");
    fs::write(
        &declaration,
        text.replace("areas = []", "areas = [\"core\"]"),
    )
    .unwrap();
    let milestone = root.join("docs/work/next-milestone.md");
    let text = fs::read_to_string(&milestone).unwrap();
    assert!(text.contains("tags: []"), "{text}");
    fs::write(&milestone, text.replace("tags: []", "tags: [core]")).unwrap();
    assert!(run(&["--root", &root_arg(root), "index"]).status.success());
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// A repository of records only with an empty bundle: the directories of `docs/`, nothing in them.
fn repo() -> tempfile::TempDir {
    let root = declared("stack = \"none\"\nareas = [\"operations\"]\n");
    let docs = root.path().join("docs");
    for folder in ["work", "knowledge"] {
        fs::create_dir_all(docs.join(folder)).unwrap();
    }
    root
}

/// A repository with only `.config/rotproof.toml`, its files up to this version of Rotproof.
fn declared(declaration: &str) -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir_all(root.path().join(".config")).unwrap();
    fs::write(
        root.path().join(".config/rotproof.toml"),
        up_to_date(declaration),
    )
    .unwrap();
    root
}

/// `declaration` with `files` at this version of Rotproof, unless it names one: a declaration written by hand would
/// otherwise be up to the first release, and fail once Rotproof is newer.
fn up_to_date(declaration: &str) -> String {
    if declaration.contains("files =") {
        declaration.to_string()
    } else {
        format!("{declaration}files = \"{}\"\n", env!("CARGO_PKG_VERSION"))
    }
}

fn root_arg(root: &Path) -> String {
    root.to_string_lossy().into_owned()
}

#[test]
fn a_missing_root_fails() {
    let out = run(&["--root", "no/such/directory", "check"]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("not a directory"));
}

/// A repository that keeps every rule: what `rotproof create` makes for a Python project without `ui`, and a log entry.
fn clean_repo() -> tempfile::TempDir {
    let root = declared("stack = \"python\"\nareas = [\"a\"]\nabsent = [\"ui\"]\n");
    let out = create(&["--root", &root_arg(root.path()), "create", "--yes"]);
    assert!(out.status.success(), "{}", stdout(&out));
    fs::write(
        root.path().join("docs/log.md"),
        "# Log\n\n## 2026-10-02\n\n* Something\n",
    )
    .unwrap();
    root
}

#[test]
fn a_clean_repository_passes() {
    let root = clean_repo();
    let out = run(&["--root", &root_arg(root.path()), "check"]);
    assert!(out.status.success(), "{}", stdout(&out));
    assert!(stdout(&out).contains("the layers and every record keep the rules"));
}

#[test]
fn a_record_with_a_byte_order_mark_passes() {
    // As PowerShell 5.1 writes UTF-8 with `-Encoding UTF8`
    let root = clean_repo();
    let docs = root.path().join("docs");
    fs::write(
        docs.join("work/x.md"),
        "\u{feff}---\ntype: Work Item\ntitle: X\ndescription: Y.\ntags: [a]\nstatus: draft\nfiled: 2026-10-01\n\
         verified: {by: human:a, at: 2026-10-01T10:00:00+09:00}\ndeadline_kind: none\ndeadline: an alarm\n---\n\n\
         # Trigger\n\nX.\n\n# State\n\nY.\n\n# Details\n\n[log](/log.md)\n",
    )
    .unwrap();
    fs::write(
        docs.join("log.md"),
        "\u{feff}# Log\n\n## 2026-10-02\n\n* Something\n",
    )
    .unwrap();
    let out = run(&["--root", &root_arg(root.path()), "index"]);
    assert!(out.status.success(), "{}", stdout(&out));
    assert!(
        fs::read_to_string(docs.join("work/index.md"))
            .unwrap()
            .contains("[X](x.md)")
    );
    let out = run(&["--root", &root_arg(root.path()), "check"]);
    assert!(out.status.success(), "{}", stdout(&out));
}

#[test]
fn each_broken_rule_fails_under_its_check() {
    let item = |details: &str| {
        format!(
            "---\ntype: Work Item\ntitle: X\ndescription: Y.\ntags: [a]\nstatus: draft\nfiled: 2026-10-01\n\
             verified: {{by: human:a, at: 2026-10-01T10:00:00+09:00}}\ndeadline_kind: none\ndeadline: an alarm\n---\n\n\
             # Trigger\n\nX.\n\n# State\n\nY.\n\n# Details\n\n{details}\n"
        )
    };
    // Each case breaks one rule of a clean repository: (the check that must name it, what to write)
    let cases: [(&str, &str, String); 20] = [
        ("the bundle is seen", "docs/work/rules.md", String::new()),
        (
            "every document in docs/work/ keeps the format",
            "docs/work/x.md",
            "# no frontmatter\n".into(),
        ),
        (
            "every link in # Details resolves",
            "docs/work/x.md",
            item("[gone](/no_such.md)"),
        ),
        // The only link is inside a comment, so a reader sees none
        (
            "every link in # Details resolves",
            "docs/work/x.md",
            item("Nothing yet. <!-- [log](/log.md) -->"),
        ),
        (
            "the log points only at real work items",
            "docs/log.md",
            "# Log\n\n## 2026-10-02\n\n- docs/work/no-such-item.md\n".into(),
        ),
        (
            "the log keeps its structure",
            "docs/log.md",
            "# Log\n\n## 2026-10-01\n\n## 2026-10-02\n".into(),
        ),
        (
            "the log keeps its structure",
            "docs/log.md",
            "# Log\n\nNo headings, just text.\n".into(),
        ),
        (
            "every document is a known type in its place",
            "docs/other/x.md",
            "---\ntype: Spec\n---\n".into(),
        ),
        (
            "every document in docs/work/ keeps the format",
            "docs/work/y.md",
            "---\ntype: Spec\ntitle: A\ndescription: B.\nstatus: stable\n---\n".into(),
        ),
        (
            "every generated file is up to date",
            "docs/work/index.md",
            "edited by hand\n".into(),
        ),
        // The rules a project reads are the rules its Rotproof checks
        (
            "every generated file is up to date",
            "docs/work/rules.md",
            "---\ntype: Guide\ntitle: Work rules\ndescription: Our own.\n---\n".into(),
        ),
        // A record left where the records were before docs/work/
        (
            "every document is a known type in its place",
            "docs/backlog/x.md",
            item("[log](/log.md)"),
        ),
        (
            "the bundle is seen",
            "docs/knowledge/rules.md",
            String::new(),
        ),
        (
            "no spec sits at the repository root",
            "genre_spec.md",
            "# spec\n".into(),
        ),
        ("the log keeps its structure", "docs/log.md", String::new()),
        // The area of a record is declared, and the declared areas are distinct headings
        (
            "every document in docs/work/ keeps the format",
            "docs/work/x.md",
            item("[log](/log.md)").replace("tags: [a]", "tags: [b]"),
        ),
        (
            "every document in docs/work/ keeps the format",
            "docs/work/y.md",
            "---\ntype: Spec\ntitle: A\ndescription: B.\ntags: [b]\nstatus: stable\n---\n".into(),
        ),
        (
            "every document in docs/work/ keeps the format",
            "docs/work/y.md",
            "---\ntype: Spec\ntitle: A\ndescription: B.\nstatus: stable\n---\n".into(),
        ),
        (
            "the areas are distinct headings",
            ".config/rotproof.toml",
            "stack = \"python\"\nareas = [\"a\", \"A\"]\nabsent = [\"ui\"]\n".into(),
        ),
        // Without the areas, no record can be judged: the check says so instead of passing them
        (
            "the bundle is seen",
            ".config/rotproof.toml",
            "stack = \"python\"\nabsent = [\"ui\"]\n".into(),
        ),
    ];
    for (check, path, text) in cases {
        let root = clean_repo();
        let target = root.path().join(path);
        if text.is_empty() {
            fs::remove_file(&target).unwrap();
        } else {
            fs::create_dir_all(target.parent().unwrap()).unwrap();
            fs::write(&target, text).unwrap();
        }
        let out = run(&["--root", &root_arg(root.path()), "check"]);
        assert_eq!(out.status.code(), Some(1), "{check}: {}", stdout(&out));
        assert!(
            stdout(&out).contains(&format!("{check}:")),
            "{check} did not name it: {}",
            stdout(&out)
        );
    }
}

#[test]
fn a_name_in_another_case_is_missing() {
    // Windows opens each of these under the name Rotproof looks for; Linux and GitHub do not. Renamed in two steps, as
    // a case-only rename is not a rename on Windows
    let cases = [
        (
            "docs/work/rules.md",
            "docs/work/Rules.md",
            "the bundle is seen",
        ),
        ("docs/log.md", "docs/Log.md", "the log keeps its structure"),
        ("domain", "Domain", "the tree matches .config/rotproof.toml"),
        (
            ".config/rotproof.toml",
            ".config/Rotproof.toml",
            "the tree matches .config/rotproof.toml",
        ),
        // Not in the floor: only the comparison with what Rotproof writes sees it
        (
            "docs/work/index.md",
            "docs/work/Index.md",
            "every generated file is up to date",
        ),
    ];
    for (from, to, check) in cases {
        let root = clean_repo();
        let (from, to) = (root.path().join(from), root.path().join(to));
        let between = root.path().join("renaming");
        fs::rename(&from, &between).unwrap();
        fs::rename(&between, &to).unwrap();
        let out = run(&["--root", &root_arg(root.path()), "check"]);
        assert_eq!(out.status.code(), Some(1), "{to:?}: {}", stdout(&out));
        assert!(
            stdout(&out).contains(&format!("{check}:")),
            "{to:?}: {}",
            stdout(&out)
        );
        // Where Rotproof names the file itself, it says what the disk has instead
        if check != "every generated file is up to date" {
            assert!(
                stdout(&out).contains("is there"),
                "{to:?}: {}",
                stdout(&out)
            );
        }
    }
}

#[test]
fn a_parent_and_its_children_are_checked_together() {
    let spec = |parent: &str, status: &str| {
        format!(
            "---\ntype: Spec\ntitle: X\ndescription: Y.\ntags: [a]\nstatus: {status}\n{parent}---\n\n\
             # Resolution\n\nDone.\n"
        )
    };
    let item = |parent: &str| {
        format!(
            "---\ntype: Work Item\ntitle: X\ndescription: Y.\ntags: [a]\nstatus: stable\nparent: {parent}\n\
             filed: 2026-10-01\nverified: {{by: human:a, at: 2026-10-01T10:00:00+09:00}}\ndeadline_kind: none\n\
             deadline: an alarm\n---\n\n# Trigger\n\nX.\n\n# State\n\nY.\n\n# Details\n\n[log](/log.md)\n"
        )
    };
    // (the check that must name it, the parent's status, the child)
    let cases = [
        (
            "a parent closes after its children",
            "deprecated\nclosed_as: done",
            spec("parent: big\n", "stable"),
        ),
        (
            "a parent closes after its children",
            "deprecated\nclosed_as: done",
            item("big"),
        ),
        (
            "every document in docs/work/ keeps the format",
            "stable",
            spec("parent: no-such-spec\n", "stable"),
        ),
        (
            "every document in docs/work/ keeps the format",
            "stable",
            item("no-such-spec"),
        ),
    ];
    for (check, status, child) in cases {
        let root = clean_repo();
        let r = root.path();
        fs::write(r.join("docs/work/big.md"), spec("", status)).unwrap();
        fs::write(r.join("docs/work/child.md"), child).unwrap();
        // The index files are current, so only the relation can fail
        assert!(run(&["--root", &root_arg(r), "index"]).status.success());
        let out = run(&["--root", &root_arg(r), "check"]);
        assert_eq!(out.status.code(), Some(1), "{check}: {}", stdout(&out));
        assert!(
            stdout(&out).contains(&format!("{check}:")),
            "{check} did not name it: {}",
            stdout(&out)
        );
    }
}

#[test]
fn index_writes_every_index_file() {
    let root = repo();
    let out = run(&["--root", &root_arg(root.path()), "index"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    for path in [
        "docs/work/rules.md",
        "docs/index.md",
        "docs/work/index.md",
        "docs/knowledge/rules.md",
        "docs/knowledge/index.md",
    ] {
        assert!(root.path().join(path).is_file(), "{path} was not written");
        assert!(
            stdout(&out).contains(&format!("wrote {path}")),
            "{}",
            stdout(&out)
        );
    }
    let work = fs::read_to_string(root.path().join("docs/work/index.md")).unwrap();
    assert!(
        work.contains("# Guides\n\n* [Work rules](rules.md) - What goes in docs/work/"),
        "{work}"
    );
}

#[test]
fn index_names_what_it_left_out() {
    let root = repo();
    fs::write(
        root.path().join("docs/work/broken.md"),
        "# no frontmatter\n",
    )
    .unwrap();
    let out = run(&["--root", &root_arg(root.path()), "index"]);
    assert!(out.status.success());
    assert!(
        stdout(&out).contains("left out of the index, fix it: work/broken.md: no frontmatter"),
        "{}",
        stdout(&out)
    );
}

#[test]
fn create_leaves_a_milestone_for_a_person_to_write() {
    let root = declared("stack = \"none\"\nareas = [\"a\"]\n");
    let r = root.path();
    let arg = root_arg(r);
    let out = run(&["--root", &arg, "create"]);
    assert!(
        stdout(&out).contains("wrote docs/work/next-milestone.md"),
        "{}",
        stdout(&out)
    );
    // What the next moment is, the project says: until it does, the check fails on the milestone
    let check = run(&["--root", &arg, "check"]);
    assert_eq!(check.status.code(), Some(1), "{}", stdout(&check));
    for said in [
        "work/next-milestone.md: body headings missing or empty: [\"Condition\"]",
        "a milestone is open:\n  no open milestone in docs/work/",
    ] {
        assert!(stdout(&check).contains(said), "{said}: {}", stdout(&check));
    }
    let milestone = r.join("docs/work/next-milestone.md");
    let text = fs::read_to_string(&milestone).unwrap();
    let start = text.find(UNWRITTEN).unwrap();
    fs::write(
        &milestone,
        format!("{}The tag v0.1.0 is pushed.\n", &text[..start]),
    )
    .unwrap();
    assert!(run(&["--root", &arg, "index"]).status.success());
    let check = run(&["--root", &arg, "check"]);
    assert!(check.status.success(), "{}", stdout(&check));
    let index = fs::read_to_string(r.join("docs/work/index.md")).unwrap();
    assert!(
        index.contains(
            "# Milestones\n\n* [The next milestone](next-milestone.md) - The next moment the work waits for. | \
             Status: draft."
        ),
        "{index}"
    );
    // Once the project has a milestone, create never writes another, closed or not: the next one is the project's
    let closed = fs::read_to_string(&milestone)
        .unwrap()
        .replace("status: draft", "status: deprecated\nclosed_as: done")
        .replace(
            "# Condition",
            "# Resolution\n\nPushed on 2026-10-09.\n\n# Condition",
        );
    fs::write(&milestone, closed).unwrap();
    let out = run(&["--root", &arg, "create"]);
    assert!(!stdout(&out).contains("next-milestone"), "{}", stdout(&out));
    let check = run(&["--root", &arg, "check"]);
    assert_eq!(check.status.code(), Some(1), "{}", stdout(&check));
    assert!(
        stdout(&check).contains("no open milestone in docs/work/"),
        "{}",
        stdout(&check)
    );
}

#[test]
fn index_names_the_milestones_past_their_date() {
    let root = repo();
    let milestone = |slug: &str, status: &str, date: &str| {
        fs::write(
            root.path().join(format!("docs/work/{slug}.md")),
            format!(
                "---\ntype: Milestone\ntitle: {slug}\ndescription: D.\ntags: [operations]\nstatus: {status}\n\
                 date: {date}\n---\n\n# Condition\n\nThe tag is pushed.\n"
            ),
        )
        .unwrap();
    };
    milestone("late", "stable", "2020-01-01");
    milestone("ahead", "stable", "2999-01-01");
    let out = run(&["--root", &root_arg(root.path()), "index"]);
    assert!(out.status.success());
    assert!(
        stdout(&out).contains(
            "past its date and still open, close it or move its date: late.md (2020-01-01)"
        ),
        "{}",
        stdout(&out)
    );
    assert!(!stdout(&out).contains("ahead.md"), "{}", stdout(&out));
    // The date passing fails nothing, and the index does not depend on today
    let check = run(&["--root", &root_arg(root.path()), "check"]);
    assert!(!stdout(&check).contains("late.md"), "{}", stdout(&check));
    let index = fs::read_to_string(root.path().join("docs/work/index.md")).unwrap();
    assert!(
        index.contains("* [late](late.md) - D. | Date: 2020-01-01.\n* [ahead](ahead.md) - D. | Date: 2999-01-01."),
        "{index}"
    );
}

#[test]
fn index_names_the_items_to_measure_again() {
    let root = repo();
    let item = "---
type: Work Item
title: Old
description: Measured long ago.
tags: [operations]
status: draft
filed: 2020-01-01
verified: {by: human:someone, at: 2020-01-02T10:00:00+09:00}
stale_after: 2021-01-01T00:00:00+09:00
deadline_kind: none
deadline: an alarm
---

# Trigger

X.

# State

Y.

# Details

Z.
";
    fs::write(root.path().join("docs/work/old.md"), item).unwrap();
    let out = run(&["--root", &root_arg(root.path()), "index"]);
    assert!(out.status.success());
    assert!(
        stdout(&out).contains("past stale_after, measure the state again: old.md"),
        "{}",
        stdout(&out)
    );
}

#[test]
fn index_fails_without_the_areas() {
    let root = repo();
    fs::remove_file(root.path().join(".config/rotproof.toml")).unwrap();
    let out = run(&["--root", &root_arg(root.path()), "index"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("rotproof init --stack"));
    assert!(!root.path().join("docs/index.md").exists());
}

#[test]
fn index_fails_without_the_work_directory() {
    let root = repo();
    fs::remove_dir(root.path().join("docs/work")).unwrap();
    let out = run(&["--root", &root_arg(root.path()), "index"]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("docs/work"));
}

/// Every file under `root`, from the root with `/`, and its text.
fn tree(root: &Path) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut dirs = vec![root.to_path_buf()];
    while let Some(dir) = dirs.pop() {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                dirs.push(path);
            } else {
                let name = path.strip_prefix(root).unwrap().to_string_lossy();
                out.push((name.replace('\\', "/"), fs::read_to_string(&path).unwrap()));
            }
        }
    }
    out.sort();
    out
}

#[test]
fn create_makes_each_stack_once_and_check_passes_on_it() {
    // (stack, files that must be made, paths that must not)
    let stacks: [(&str, &[&str], &[&str]); 3] = [
        (
            "python",
            &[
                "handler/__init__.py",
                "ui/atoms/__init__.py",
                "utils/__init__.py",
            ],
            &[],
        ),
        (
            "typescript",
            &[
                "src/handler/index.ts",
                "src/ui/pages/index.ts",
                "src/domain/index.ts",
            ],
            &[],
        ),
        (
            "rust",
            &[
                "crates/handler/src/main.rs",
                "crates/domain/Cargo.toml",
                "crates/domain/src/lib.rs",
            ],
            &["crates/ui"],
        ),
    ];
    for (stack, made, not_made) in stacks {
        let root = declared(&format!("stack = \"{stack}\"\nareas = [\"a\"]\n"));
        let arg = root_arg(root.path());
        let first = create(&["--root", &arg, "create", "--yes"]);
        assert!(first.status.success(), "{stack}: {}", stdout(&first));
        for path in made
            .iter()
            .chain(&["docs/log.md", "docs/work/rules.md", "docs/index.md"])
        {
            assert!(
                root.path().join(path).is_file(),
                "{stack}: {path} was not made"
            );
            assert!(
                stdout(&first).contains(&format!("wrote {path}")),
                "{stack}: {}",
                stdout(&first)
            );
        }
        for path in not_made {
            assert!(!root.path().join(path).exists(), "{stack}: {path} was made");
        }
        let before = tree(root.path());
        let second = create(&["--root", &arg, "create"]);
        assert!(second.status.success());
        assert!(
            stdout(&second).contains("nothing to make"),
            "{stack}: {}",
            stdout(&second)
        );
        assert_eq!(
            tree(root.path()),
            before,
            "{stack}: the second run changed the tree"
        );
        let check = run(&["--root", &arg, "check"]);
        assert!(check.status.success(), "{stack}: {}", stdout(&check));
    }
}

/// The project's files at the root that `rotproof create` writes, outside hidden directories
const PROJECT_FILES: [&str; 4] = [
    "AGENTS.md",
    "CLAUDE.md",
    "README.md",
    "requirements-dev.txt",
];

#[test]
fn create_writes_the_projects_files_once() {
    // (stack, whether Rotproof is pinned from PyPI)
    for (stack, pinned) in [
        ("python", true),
        ("none", true),
        ("typescript", false),
        ("rust", false),
    ] {
        let parent = tempfile::tempdir().unwrap();
        let root = parent.path().join("my-app");
        fs::create_dir_all(root.join(".config")).unwrap();
        let absent = if stack == "none" {
            ""
        } else {
            "absent = [\"utils\"]\n"
        };
        fs::write(
            root.join(".config/rotproof.toml"),
            up_to_date(&format!("stack = \"{stack}\"\nareas = [\"a\"]\n{absent}")),
        )
        .unwrap();
        let arg = root_arg(&root);
        let out = create(&["--root", &arg, "create", "--yes"]);
        assert!(out.status.success(), "{stack}: {}", stdout(&out));
        for path in [
            "AGENTS.md",
            "CLAUDE.md",
            "README.md",
            ".gitignore",
            ".gitattributes",
        ] {
            let text = fs::read_to_string(root.join(path))
                .unwrap_or_else(|_| panic!("{stack}: {path} was not written"));
            for placeholder in ["{name}", "{map}", "{development}", "{version}", "{stack}"] {
                assert!(!text.contains(placeholder), "{stack}: {path}: {text}");
            }
        }
        let agents = fs::read_to_string(root.join("AGENTS.md")).unwrap();
        assert!(agents.starts_with("# AGENTS.md (my-app)\n"), "{agents}");
        assert!(
            fs::read_to_string(root.join("README.md"))
                .unwrap()
                .starts_with("# my-app\n")
        );
        assert_eq!(
            fs::read_to_string(root.join("CLAUDE.md")).unwrap(),
            "@AGENTS.md\n@.rotproof/AGENTS.md\n"
        );
        // The map has the layers present, and not the one declared absent
        assert_eq!(
            agents.contains("handler/` | Entry points"),
            stack != "none",
            "{stack}: {agents}"
        );
        assert!(!agents.contains("utils/` |"), "{stack}: {agents}");
        let requirements = root.join("requirements-dev.txt");
        let workflow = root.join(".github/workflows/ci.yml");
        if pinned {
            let pin = format!("rotproof=={}\n", env!("CARGO_PKG_VERSION"));
            assert!(fs::read_to_string(&requirements).unwrap().contains(&pin));
            let workflow = fs::read_to_string(&workflow).unwrap();
            assert!(
                workflow.contains("timeout-minutes:") && workflow.contains("- run: rotproof check")
            );
            assert!(!stdout(&out).contains("not written"), "{}", stdout(&out));
        } else {
            assert!(!requirements.exists() && !workflow.exists(), "{stack}");
            assert!(
                stdout(&out).contains(&format!(
                    "not written: Rotproof is not pinned and no CI workflow is written: how a {stack} project"
                )),
                "{stack}: {}",
                stdout(&out)
            );
        }
        // A Rust project's workspace, which Cargo reads as the crates of the layers present
        let workspace = root.join("Cargo.toml");
        assert_eq!(workspace.exists(), stack == "rust", "{stack}");
        if stack == "rust" {
            let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
            let out = Command::new(cargo)
                .args([
                    "metadata",
                    "--format-version",
                    "1",
                    "--no-deps",
                    "--offline",
                ])
                .current_dir(&root)
                .output()
                .unwrap();
            let metadata = String::from_utf8_lossy(&out.stdout);
            assert!(
                out.status.success(),
                "{}",
                String::from_utf8_lossy(&out.stderr)
            );
            for crate_name in ["handler", "application", "infrastructure", "domain"] {
                assert!(
                    metadata.contains(&format!("\"name\":\"{crate_name}\"")),
                    "{crate_name}: {metadata}"
                );
            }
            assert!(!metadata.contains("\"name\":\"utils\""), "{metadata}");
        }
        assert!(run(&["--root", &arg, "check"]).status.success());

        // From then on the files are the project's: an edit stays, and only a missing file is written again
        fs::write(root.join("AGENTS.md"), "mine\n").unwrap();
        fs::remove_file(root.join("CLAUDE.md")).unwrap();
        let out = create(&["--root", &arg, "create"]);
        assert_eq!(
            fs::read_to_string(root.join("AGENTS.md")).unwrap(),
            "mine\n"
        );
        assert!(stdout(&out).contains("wrote CLAUDE.md"), "{}", stdout(&out));
        assert!(
            !stdout(&out).contains("wrote AGENTS.md"),
            "{}",
            stdout(&out)
        );
    }
}

#[test]
fn the_guide_is_rotproofs_and_check_fails_until_create_rewrites_it() {
    for stack in ["python", "typescript", "rust", "none"] {
        let root = declared(&format!("stack = \"{stack}\"\nareas = [\"a\"]\n"));
        let arg = root_arg(root.path());
        assert!(
            create(&["--root", &arg, "create", "--yes"])
                .status
                .success()
        );
        let guide = root.path().join(".rotproof/AGENTS.md");
        let written = fs::read_to_string(&guide).unwrap();
        assert!(
            written.contains(&format!("stack \"{stack}\"")),
            "{stack}: {written}"
        );
        assert_eq!(
            written.contains("## The layers"),
            stack != "none",
            "{stack}: {written}"
        );
        for (edit, says) in [
            (
                Some("edited by hand\n"),
                "out of date, run `rotproof init` after an upgrade of Rotproof, `rotproof create` otherwise",
            ),
            (
                None,
                "missing, run `rotproof init` after an upgrade of Rotproof, `rotproof create` otherwise",
            ),
        ] {
            match edit {
                Some(text) => fs::write(&guide, text).unwrap(),
                None => fs::remove_file(&guide).unwrap(),
            }
            let out = run(&["--root", &arg, "check"]);
            assert_eq!(out.status.code(), Some(1), "{stack}: {}", stdout(&out));
            assert!(
                stdout(&out).contains(&format!(
                    "Rotproof's guide is up to date:\n  {says}: .rotproof/AGENTS.md"
                )),
                "{stack}: {}",
                stdout(&out)
            );
            let out = create(&["--root", &arg, "create"]);
            assert!(
                stdout(&out).contains("wrote .rotproof/AGENTS.md"),
                "{stack}: {}",
                stdout(&out)
            );
            assert_eq!(fs::read_to_string(&guide).unwrap(), written);
            assert!(run(&["--root", &arg, "check"]).status.success());
        }
    }
}

#[test]
fn create_respects_absent_and_leaves_what_it_does_not_own() {
    let root = declared(
        "stack = \"python\"\nareas = [\"a\"]\nabsent = [\"ui.templates\", \"infrastructure\"]\n",
    );
    let arg = root_arg(root.path());
    // Files of the project: a layer it already has, and a log with entries
    fs::create_dir_all(root.path().join("domain")).unwrap();
    fs::write(root.path().join("domain/model.py"), "X = 1\n").unwrap();
    fs::create_dir_all(root.path().join("docs")).unwrap();
    let log = "# Log\n\n## 2026-10-02\n\n* Mine\n";
    fs::write(root.path().join("docs/log.md"), log).unwrap();
    fs::create_dir_all(root.path().join(".claude")).unwrap();
    let settings = "{\"hooks\": {}}\n";
    fs::write(root.path().join(".claude/settings.json"), settings).unwrap();
    let out = create(&["--root", &arg, "create", "--yes"]);
    assert!(out.status.success(), "{}", stdout(&out));
    assert!(root.path().join("ui/pages/__init__.py").is_file());
    assert!(!root.path().join("ui/templates").exists());
    assert!(!root.path().join("infrastructure").exists());
    // A present layer is the project's: not even its missing __init__.py is written
    assert!(!root.path().join("domain/__init__.py").exists());
    assert_eq!(
        fs::read_to_string(root.path().join("domain/model.py")).unwrap(),
        "X = 1\n"
    );
    assert_eq!(
        fs::read_to_string(root.path().join("docs/log.md")).unwrap(),
        log
    );
    assert_eq!(
        fs::read_to_string(root.path().join(".claude/settings.json")).unwrap(),
        settings
    );
    let check = run(&["--root", &arg, "check"]);
    assert!(check.status.success(), "{}", stdout(&check));
}

#[test]
fn create_makes_layers_only_with_yes() {
    // Every stack with layers stops before writing anything, the field the declaration lacks included
    for stack in ["python", "typescript", "rust"] {
        let root = declared(&format!("stack = \"{stack}\"\n"));
        let arg = root_arg(root.path());
        let before = tree(root.path());
        let out = create(&["--root", &arg, "create"]);
        assert_eq!(out.status.code(), Some(2), "{stack}: {}", stdout(&out));
        let said = String::from_utf8_lossy(&out.stderr);
        assert!(
            said.contains("makes them only with --yes") && said.contains("Nothing was written"),
            "{stack}: {said}"
        );
        assert_eq!(tree(root.path()), before, "{stack}");
    }
    // A repository of records only has no layers to make
    let root = declared("stack = \"none\"\nareas = [\"a\"]\n");
    assert!(
        create(&["--root", &root_arg(root.path()), "create"])
            .status
            .success()
    );

    // Once made, a run that makes no layer needs no --yes; a layer taken out of absent later is listed alone
    let root = declared("stack = \"python\"\nareas = [\"a\"]\nabsent = [\"ui\"]\n");
    let arg = root_arg(root.path());
    assert!(
        create(&["--root", &arg, "create", "--yes"])
            .status
            .success()
    );
    assert!(create(&["--root", &arg, "create"]).status.success());
    declare(root.path(), "stack = \"python\"\nareas = [\"a\"]\n");
    let out = create(&["--root", &arg, "create"]);
    assert_eq!(out.status.code(), Some(2), "{}", stdout(&out));
    let said = String::from_utf8_lossy(&out.stderr);
    let listed: Vec<&str> = said.lines().filter(|l| l.starts_with("  ")).collect();
    assert_eq!(
        listed,
        [
            "  ui/ (ui)",
            "  ui/pages/ (ui.pages)",
            "  ui/templates/ (ui.templates)",
            "  ui/organisms/ (ui.organisms)",
            "  ui/molecules/ (ui.molecules)",
            "  ui/atoms/ (ui.atoms)",
        ]
    );
    assert!(!root.path().join("ui").exists());
    assert!(
        create(&["--root", &arg, "create", "--yes"])
            .status
            .success()
    );
    assert!(root.path().join("ui/atoms/__init__.py").is_file());
}

/// A work item or a spec that keeps every rule, with `tag` as its area.
fn record(kind: &str, tag: &str) -> String {
    match kind {
        "item" => format!(
            "---\ntype: Work Item\ntitle: X\ndescription: Y.\ntags: [{tag}]\nstatus: draft\nfiled: 2026-10-01\n\
             verified: {{by: human:a, at: 2026-10-01T10:00:00+09:00}}\ndeadline_kind: none\ndeadline: an alarm\n---\n\n\
             # Trigger\n\nX.\n\n# State\n\nY.\n\n# Details\n\n[log](/log.md)\n"
        ),
        _ => format!(
            "---\ntype: Spec\ntitle: X\ndescription: Y.\ntags: [{tag}]\nstatus: deprecated\nclosed_as: done\n---\n\n# Resolution\n\nDone.\n"
        ),
    }
}

#[test]
fn create_adds_the_fields_the_declaration_lacks() {
    // A declaration an older Rotproof wrote, before areas existed, with comments and values of the project's own
    let older = &up_to_date("# Ours\nstack = \"python\"\nabsent = [\"ui\"]  # no UI here\n");
    let root = declared("stack = \"python\"\nareas = [\"a\"]\nabsent = [\"ui\"]\n");
    let r = root.path();
    let arg = root_arg(r);
    assert!(
        create(&["--root", &arg, "create", "--yes"])
            .status
            .success()
    );
    fs::write(
        r.join("docs/log.md"),
        "# Log\n\n## 2026-10-02\n\n* Something\n",
    )
    .unwrap();
    fs::write(r.join("docs/work/one.md"), record("item", "operations")).unwrap();
    fs::write(r.join("docs/work/two.md"), record("item", "billing")).unwrap();
    fs::write(r.join("docs/work/three.md"), record("spec", "records")).unwrap();
    // A guide's tags are not areas
    fs::write(
        r.join("docs/work/guide.md"),
        "---\ntype: Guide\ntitle: G\ndescription: H.\ntags: [howto]\n---\n\nText.\n",
    )
    .unwrap();
    fs::write(r.join(".config/rotproof.toml"), older).unwrap();

    let check = run(&["--root", &arg, "check"]);
    assert_eq!(check.status.code(), Some(1));
    assert!(
        stdout(&check).contains("missing field `areas`: run `rotproof create`, which adds it"),
        "{}",
        stdout(&check)
    );

    let out = create(&["--root", &arg, "create"]);
    assert!(out.status.success(), "{}", stdout(&out));
    assert!(
        stdout(&out).contains(
            "added to .config/rotproof.toml: areas = [\"a\", \"billing\", \"operations\", \"records\"]"
        ),
        "{}",
        stdout(&out)
    );
    let now = fs::read_to_string(r.join(".config/rotproof.toml")).unwrap();
    // What was there is kept, byte for byte, and the new field says what it is and where its value came from
    assert!(now.starts_with(older), "{now}");
    assert!(
        now.contains("# The areas the records are grouped by")
            && now.contains("# Added by `rotproof create` with the tags the records use"),
        "{now}"
    );
    let check = run(&["--root", &arg, "check"]);
    assert!(check.status.success(), "{}", stdout(&check));

    // A value that is present is never changed, and a second run adds nothing
    let edited = now.replace(
        "[\"a\", \"billing\", \"operations\", \"records\"]",
        "[\"records\", \"operations\", \"billing\", \"a\"]",
    );
    fs::write(r.join(".config/rotproof.toml"), &edited).unwrap();
    let again = create(&["--root", &arg, "create"]);
    assert!(!stdout(&again).contains("added to"), "{}", stdout(&again));
    assert_eq!(
        fs::read_to_string(r.join(".config/rotproof.toml")).unwrap(),
        edited
    );
}

#[test]
fn create_keeps_the_line_endings_of_the_declaration() {
    // Checked out with CRLF, as git does on Windows: only the added lines are new
    let older = "# Ours\r\nstack = \"none\"\r\n";
    let root = repo();
    fs::write(root.path().join(".config/rotproof.toml"), older).unwrap();
    let out = create(&["--root", &root_arg(root.path()), "create"]);
    assert!(out.status.success(), "{}", stdout(&out));
    let now = fs::read_to_string(root.path().join(".config/rotproof.toml")).unwrap();
    assert!(now.starts_with(older), "{now:?}");
    assert!(now.contains("\r\nareas = []\r\n"), "{now:?}");
    assert!(!now.replace("\r\n", "").contains('\n'), "{now:?}");
}

#[test]
fn create_leaves_a_declaration_it_cannot_complete_as_it_was() {
    // Adding areas would not make these read: the declaration stays as the project wrote it, and nothing is made
    for (declaration, said) in [
        ("stack = \"cobol\"\n", "unknown stack"),
        (
            "stack = \"python\"\nabsent = [\"service\"]\n",
            "does not have",
        ),
        ("areas = [\"a\"]\n", "missing field `stack`"),
        ("stack = \"python\"\nextra = 1\n", "unknown field"),
    ] {
        let root = declared(declaration);
        let out = create(&["--root", &root_arg(root.path()), "create"]);
        assert_eq!(out.status.code(), Some(2), "{declaration}");
        assert!(
            String::from_utf8_lossy(&out.stderr).contains(said),
            "{declaration}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(
            fs::read_to_string(root.path().join(".config/rotproof.toml")).unwrap(),
            up_to_date(declaration)
        );
        assert!(!root.path().join("docs").exists(), "{declaration}");
    }
}

#[test]
fn create_fails_without_a_declaration_it_can_read() {
    let cases = [
        (None, "rotproof init --stack"),
        (
            Some("stack = \"cobol\"\nareas = [\"a\"]\n"),
            "unknown stack",
        ),
        (
            Some("stack = \"rust\"\nareas = [\"a\"]\nabsent = [\"ui\"]\n"),
            "does not have",
        ),
        (
            Some("stack = \"python\"\nareas = [\"a\"]\nabsnet = []\n"),
            "unknown field",
        ),
        // A declaration written before areas existed gets the field instead:
        // create_adds_the_fields_the_declaration_lacks
    ];
    for (declaration, said) in cases {
        let root = match declaration {
            Some(text) => declared(text),
            None => tempfile::tempdir().unwrap(),
        };
        let out = create(&["--root", &root_arg(root.path()), "create"]);
        assert_eq!(out.status.code(), Some(2), "{said}");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(stderr.contains(said), "{said}: {stderr}");
        assert!(
            !root.path().join("docs").exists(),
            "{said}: made docs/ all the same"
        );
    }
}

type Plant = Box<dyn Fn(&Path)>;

fn declare(root: &Path, text: &str) {
    fs::write(root.join(".config/rotproof.toml"), up_to_date(text)).unwrap();
}

fn plant_file(root: &Path, path: &str) {
    let full = root.join(path);
    fs::create_dir_all(full.parent().unwrap()).unwrap();
    fs::write(full, "x = 1\n").unwrap();
}

#[test]
fn each_difference_from_the_declaration_fails() {
    // Each case breaks one rule of a clean repository: (what the message says, how to break it)
    let cases: Vec<(&str, Plant)> = vec![
        (
            "domain is missing",
            Box::new(|r| fs::remove_dir_all(r.join("domain")).unwrap()),
        ),
        (
            "ui is declared absent, but ui/ exists",
            Box::new(|r| plant_file(r, "ui/__init__.py")),
        ),
        (
            "code outside the layers: scripts",
            Box::new(|r| plant_file(r, "scripts/tool.py")),
        ),
        (
            "code outside the layers: main.py",
            Box::new(|r| plant_file(r, "main.py")),
        ),
        // Windows runs it with Python all the same
        (
            "code outside the layers: stray.PY",
            Box::new(|r| plant_file(r, "stray.PY")),
        ),
        (
            "missing: .config/rotproof.toml",
            Box::new(|r| fs::remove_file(r.join(".config/rotproof.toml")).unwrap()),
        ),
        (
            "unknown stack",
            Box::new(|r| declare(r, "stack = \"cobol\"\nareas = [\"a\"]\n")),
        ),
        (
            "does not have",
            Box::new(|r| {
                declare(
                    r,
                    "stack = \"python\"\nareas = [\"a\"]\nabsent = [\"ui\", \"service\"]\n",
                )
            }),
        ),
        (
            "unknown field",
            Box::new(|r| {
                declare(
                    r,
                    "stack = \"python\"\nareas = [\"a\"]\nabsent = [\"ui\"]\nextra = 1\n",
                )
            }),
        ),
        // A layer cannot be switched off by listing it, or a directory holding it
        (
            "which holds or sits in the layer domain",
            Box::new(|r| {
                declare(
                    r,
                    "stack = \"python\"\nareas = [\"a\"]\nabsent = [\"ui\"]\nunchecked = [\"domain\"]\n",
                )
            }),
        ),
        // Written another way, a path would match nothing and switch nothing off
        (
            "write a path from the root",
            Box::new(|r| {
                plant_file(r, "scripts/tool.py");
                declare(
                    r,
                    "stack = \"python\"\nareas = [\"a\"]\nabsent = [\"ui\"]\nunchecked = [\"./scripts\"]\n",
                );
            }),
        ),
        (
            "unchecked lists scripts, which does not exist",
            Box::new(|r| {
                declare(
                    r,
                    "stack = \"python\"\nareas = [\"a\"]\nabsent = [\"ui\"]\nunchecked = [\"scripts\"]\n",
                )
            }),
        ),
        // The floor: with every layer declared absent, nothing would be checked
        (
            "no layer is present",
            Box::new(|r| {
                for layer in [
                    "handler",
                    "application",
                    "infrastructure",
                    "domain",
                    "utils",
                ] {
                    fs::remove_dir_all(r.join(layer)).unwrap();
                }
                declare(
                    r,
                    "stack = \"python\"\nareas = [\"a\"]\nabsent = [\"handler\", \"ui\", \"application\", \"infrastructure\", \
                     \"domain\", \"utils\"]\n",
                );
            }),
        ),
    ];
    for (said, plant) in cases {
        let root = clean_repo();
        plant(root.path());
        let out = run(&["--root", &root_arg(root.path()), "check"]);
        assert_eq!(out.status.code(), Some(1), "{said}: {}", stdout(&out));
        assert!(
            stdout(&out).contains("the tree matches .config/rotproof.toml:"),
            "{said}: {}",
            stdout(&out)
        );
        assert!(stdout(&out).contains(said), "{said}: {}", stdout(&out));
    }
}

/// A declaration whose stack does not fit still names the areas the records are grouped by, so the records are checked
/// all the same: only a declaration that cannot be read leaves them unchecked
#[test]
fn the_records_are_checked_under_a_stack_that_does_not_fit() {
    let root = clean_repo();
    declare(root.path(), "stack = \"cobol\"\nareas = [\"a\"]\n");
    fs::write(root.path().join("docs/work/broken.md"), "no frontmatter\n").unwrap();
    let out = run(&["--root", &root_arg(root.path()), "check"]);
    let said = stdout(&out);
    assert_eq!(out.status.code(), Some(1), "{said}");
    assert!(said.contains("unknown stack"), "{said}");
    assert!(!said.contains("the records are not checked"), "{said}");
    assert!(said.contains("broken.md"), "{said}");
}

/// A repository that keeps every rule, with `ui`: what `rotproof create` makes for `stack`, and a log entry.
fn repo_with_ui(stack: &str, absent: &str) -> tempfile::TempDir {
    let root = declared(&format!(
        "stack = \"{stack}\"\nareas = [\"a\"]\nabsent = [{absent}]\n"
    ));
    let out = create(&["--root", &root_arg(root.path()), "create", "--yes"]);
    assert!(out.status.success(), "{}", stdout(&out));
    fs::write(
        root.path().join("docs/log.md"),
        "# Log\n\n## 2026-10-02\n\n* Something\n",
    )
    .unwrap();
    root
}

#[test]
fn ui_holds_only_its_levels() {
    // (stack, what is planted beside the levels, what the message names, where it says atoms are)
    let cases = [
        ("python", "ui/helpers.py", "ui/helpers.py", "ui/atoms/"),
        ("python", "ui/shared/theme.py", "ui/shared", "ui/atoms/"),
        // Windows runs it with Python all the same
        ("python", "ui/Helpers.PY", "ui/Helpers.PY", "ui/atoms/"),
        (
            "typescript",
            "src/ui/theme.css",
            "src/ui/theme.css",
            "src/ui/atoms/",
        ),
        (
            "typescript",
            "src/ui/hooks/useWidth.ts",
            "src/ui/hooks",
            "src/ui/atoms/",
        ),
    ];
    for (stack, planted, named, atoms) in cases {
        let root = repo_with_ui(stack, "");
        plant_file(root.path(), planted);
        let out = run(&["--root", &root_arg(root.path()), "check"]);
        let said = stdout(&out);
        assert_eq!(out.status.code(), Some(1), "{planted}: {said}");
        assert!(
            said.contains(&format!("code in ui outside its levels: {named} (")),
            "{planted}: {said}"
        );
        // Where it goes, for a reader who does not know that atoms include parts that render nothing
        assert!(
            said.contains("visible or not") && said.contains(atoms),
            "{planted}: {said}"
        );
    }
}

#[test]
fn ui_parts_in_a_level_pass() {
    let root = repo_with_ui("python", "");
    let r = root.path();
    fs::write(r.join(".gitignore"), "ui/generated/\n").unwrap();
    for path in [
        "ui/atoms/theme/__init__.py",
        "ui/atoms/theme/provider.py",
        "ui/organisms/session/provider.py",
        "ui/pages/home.py",
        // Not code, and ignored by git
        "ui/README.md",
        "ui/generated/x.py",
    ] {
        plant_file(r, path);
    }
    let out = run(&["--root", &root_arg(r), "check"]);
    assert!(out.status.success(), "{}", stdout(&out));

    // A level declared absent that exists is its own finding, not code beside the levels too
    let root = repo_with_ui("python", "\"ui.templates\"");
    plant_file(root.path(), "ui/templates/x.py");
    let out = run(&["--root", &root_arg(root.path()), "check"]);
    let said = stdout(&out);
    assert!(
        said.contains("ui.templates is declared absent, but ui/templates/ exists"),
        "{said}"
    );
    assert!(!said.contains("outside its levels"), "{said}");
}

/// Every place of the Python layout, in the order of the grid below.
const PLACES: [&str; 11] = [
    "handler",
    "ui",
    "ui.pages",
    "ui.templates",
    "ui.organisms",
    "ui.molecules",
    "ui.atoms",
    "application",
    "infrastructure",
    "domain",
    "utils",
];

/// Whether the place of a row may import the place of a column: the rule `layers/table.toml` states (each layer's
/// `imports`, and a level of `ui` the levels below it), written out again by hand rather than computed by the check.
/// A change to the table or to the check that moves any pair fails here, so moving one is a decision made twice.
const MAY_IMPORT: [&str; 11] = [
    // h  ui pg tp or mo at ap in do ut
    "Y Y Y Y Y Y Y Y Y Y Y", // handler
    "N Y Y Y Y Y Y Y N Y Y", // ui
    "N N Y Y Y Y Y Y N Y Y", // ui.pages
    "N N N Y Y Y Y N N Y Y", // ui.templates
    "N N N N Y Y Y N N Y Y", // ui.organisms
    "N N N N N Y Y N N N Y", // ui.molecules
    "N N N N N N Y N N N Y", // ui.atoms
    "N N N N N N N Y N Y Y", // application
    "N N N N N N N N Y Y Y", // infrastructure
    "N N N N N N N N N Y Y", // domain
    "N N N N N N N N N N Y", // utils
];

#[test]
fn every_pair_of_places_is_allowed_or_fails_as_the_table_says() {
    // A stack: the file of a place, the text before its imports, how a line imports a place, how the finding says the
    // import with the verb it starts with, and how many pairs the table forbids among the places the stack has
    struct Stack {
        name: &'static str,
        file: fn(&str) -> String,
        head: &'static str,
        import: fn(&str) -> String,
        said_as: fn(&str) -> String,
        verb: &'static str,
        forbidden: usize,
    }
    let stacks = [
        Stack {
            name: "python",
            file: |from| format!("{}/__init__.py", from.replace('.', "/")),
            head: "",
            import: |to| format!("import {to}\n"),
            said_as: |to| format!("imports {to}"),
            verb: "imports",
            forbidden: 68,
        },
        Stack {
            name: "typescript",
            file: |from| format!("src/{}/index.ts", from.replace('.', "/")),
            head: "",
            import: |to| format!("import '/src/{}';\n", to.replace('.', "/")),
            said_as: |to| {
                let path = to.replace('.', "/");
                format!("imports /src/{path} (src/{path})")
            },
            verb: "imports",
            forbidden: 68,
        },
        Stack {
            // No ui: a crate per layer
            name: "rust",
            file: |from| format!("crates/{from}/Cargo.toml"),
            head: "[package]\nname = \"place\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[dependencies]\n",
            import: |to| format!("{to} = {{ path = \"../{to}\" }}\n"),
            said_as: |to| format!("depends on {to} (crates/{to})"),
            verb: "depends on",
            forbidden: 11,
        },
    ];
    for stack in stacks {
        let name = stack.name;
        let root = repo_with_ui(name, "");
        let r = root.path();
        let has = |place: &&str| name != "rust" || !place.starts_with("ui");
        let places: Vec<(usize, &str)> = PLACES
            .iter()
            .copied()
            .enumerate()
            .filter(|(_, p)| has(p))
            .collect();
        // Each place's own file imports every place, one per line, in the order of PLACES
        let imports: String = places.iter().map(|(_, to)| (stack.import)(to)).collect();
        for (_, from) in &places {
            fs::write(
                r.join((stack.file)(from)),
                format!("{}{imports}", stack.head),
            )
            .unwrap();
        }
        let out = run(&["--root", &root_arg(r), "check"]);
        let said = stdout(&out);
        let mut forbidden = 0;
        for (row, from) in &places {
            let cells: Vec<&str> = MAY_IMPORT[*row].split(' ').collect();
            assert_eq!(cells.len(), PLACES.len(), "{from}");
            for (line, (column, to)) in places.iter().enumerate() {
                let named = format!(
                    "{}:{}: {}, in {to}",
                    (stack.file)(from),
                    stack.head.lines().count() + line + 1,
                    (stack.said_as)(to)
                );
                match cells[*column] {
                    "Y" => assert!(
                        !said.contains(&named),
                        "{name}: {from} -> {to} failed:\n{said}"
                    ),
                    _ => {
                        forbidden += 1;
                        assert!(
                            said.contains(&named),
                            "{name}: {from} -> {to} passed:\n{said}"
                        );
                    }
                }
            }
        }
        // The floor: every forbidden pair is one line, and nothing else failed
        assert_eq!(forbidden, stack.forbidden, "{name}");
        assert_eq!(
            said.matches(&format!(": {} ", stack.verb)).count(),
            forbidden,
            "{name}: {said}"
        );
        assert_eq!(out.status.code(), Some(1));
        assert!(said.contains("the layers import only what layers/table.toml allows:"));
    }
}

#[test]
fn every_form_of_rust_dependency_is_judged() {
    let root = repo_with_ui("rust", "");
    let r = root.path();
    let write = |path: &str, text: &str| fs::write(r.join(path), text).unwrap();
    let package = |name: &str| {
        format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2024\"\n")
    };
    write(
        "Cargo.toml",
        "[workspace]\nmembers = [\"crates/*\"]\nresolver = \"3\"\n\n[workspace.dependencies]\n\
         application = { path = \"crates/application\" }\ncore = { path = \"crates/domain\", package = \"domain\" }\n\
         serde = \"1\"\n",
    );
    write(
        "crates/domain/Cargo.toml",
        &format!(
            "{}\n[dependencies]\n\
             serde = {{ workspace = true }}\n\
             application.workspace = true\n\
             utils = {{ path = \"../utils\" }}\n\
             infra = {{ path = \"../infrastructure\", package = \"infrastructure\" }}\n\
             regex = \"1\"\n\
             missing = {{ workspace = true }}\n\
             \n[build-dependencies]\nhandler = {{ path = \"../handler\" }}\n\
             \n[dev-dependencies]\nhandler = {{ path = \"../handler\" }}\n\
             \n[target.'cfg(windows)'.dependencies]\n\
             app = {{ path = \"../../crates/./application/\", package = \"application\" }}\n\
             outside = {{ path = \"../../..\" }}\n\
             nowhere = {{ path = \"../nowhere\" }}\n",
            package("domain")
        ),
    );
    // A member that names its workspace, and one whose named workspace is none
    write(
        "crates/utils/Cargo.toml",
        &format!(
            "{}workspace = \"../..\"\n\n[dependencies]\ncore.workspace = true\n",
            package("utils")
        ),
    );
    write(
        "crates/infrastructure/Cargo.toml",
        &format!(
            "{}workspace = \"..\"\n\n[dependencies]\ncore.workspace = true\n",
            package("infrastructure")
        ),
    );
    // Not TOML: its dependencies cannot be read
    write("crates/application/Cargo.toml", "[package\n");
    let out = run(&["--root", &root_arg(r), "check"]);
    let said = stdout(&out);
    assert_eq!(out.status.code(), Some(1), "{said}");
    for line in [
        "crates/domain/Cargo.toml:8: depends on application (crates/application, from the workspace), in application; \
         domain may import `utils`",
        "crates/domain/Cargo.toml:10: depends on infra (crates/infrastructure), in infrastructure; ",
        "crates/domain/Cargo.toml:12: missing comes from the workspace, and Cargo.toml has no missing in \
         [workspace.dependencies], so it is not checked",
        "crates/domain/Cargo.toml:15: depends on handler (crates/handler), in handler; ",
        "crates/domain/Cargo.toml:21: depends on app (crates/application), in application; ",
        "crates/utils/Cargo.toml:8: depends on core (crates/domain, from the workspace), in domain; utils imports no \
         other layer",
        "crates/infrastructure/Cargo.toml:8: core comes from the workspace, and .., which [package] workspace names, \
         is no workspace, so it is not checked",
        "crates/application/Cargo.toml:1: cannot be read as TOML (",
    ] {
        assert!(said.contains(line), "{line}:\n{said}");
    }
    assert_eq!(said.matches(": depends on ").count(), 5, "{said}");
    assert_eq!(
        said.matches("comes from the workspace").count(),
        2,
        "{said}"
    );
}

/// Make the domain crate of the Rust project at `r` depend on each of `paths`, one line each from line 7, as `d0`,
/// `d1` and so on, and say what check says.
fn check_depending_on(r: &Path, paths: &[String]) -> String {
    let lines: String = paths
        .iter()
        .enumerate()
        .map(|(i, path)| format!("d{i} = {{ path = {path:?}, package = \"application\" }}\n"))
        .collect();
    fs::write(
        r.join("crates/domain/Cargo.toml"),
        format!(
            "[package]\nname = \"domain\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[dependencies]\n{lines}"
        ),
    )
    .unwrap();
    stdout(&run(&["--root", &root_arg(r), "check"]))
}

/// Whether check says that `d<i>` lands in application, on line `7 + i`.
fn lands_in_application(said: &str, i: usize) -> bool {
    said.contains(&format!(
        "crates/domain/Cargo.toml:{}: depends on d{i} (crates/application), in application; ",
        i + 7
    ))
}

#[cfg(windows)]
#[test]
fn a_rust_path_lands_where_windows_resolves_it() {
    // Cargo builds each of these on Windows, outside a workspace, against crates/application
    let root = repo_with_ui("rust", "");
    let r = root.path();
    let paths: Vec<String> = vec![
        "../Application".into(),
        "../application.".into(),
        "../application ".into(),
        "..\\application".into(),
        format!("{}\\crates\\application", r.display()).to_uppercase(),
    ];
    let said = check_depending_on(r, &paths);
    for (i, path) in paths.iter().enumerate() {
        assert!(lands_in_application(&said, i), "{path}:\n{said}");
    }
}

#[cfg(unix)]
#[test]
fn a_rust_path_through_a_link_lands_where_the_link_points() {
    let root = repo_with_ui("rust", "");
    let r = root.path();
    fs::create_dir(r.join("vendor")).unwrap();
    std::os::unix::fs::symlink("../crates/application", r.join("vendor/app")).unwrap();
    let said = check_depending_on(r, &["../../vendor/app".into()]);
    assert!(lands_in_application(&said, 0), "{said}");
}

#[cfg(windows)]
#[test]
fn a_typescript_or_python_import_lands_where_windows_resolves_it() {
    // Vite 8.3 builds each of these TypeScript imports on Windows (tsc refuses the first two and passes a short name,
    // measured 2026-10-05), and Python imports the Python one with PYTHONCASEOK set
    let root = repo_with_ui("typescript", "");
    let r = root.path();
    fs::write(
        r.join("tsconfig.json"),
        "{ \"compilerOptions\": { \"baseUrl\": \"src\" } }",
    )
    .unwrap();
    fs::write(r.join("src/infrastructure/db.ts"), "export {}\n").unwrap();
    fs::write(
        r.join("src/domain/song.ts"),
        "import '../Infrastructure/db';\nimport '../infrastructure./db';\nimport 'Infrastructure/db';\n",
    )
    .unwrap();
    let said = stdout(&run(&["--root", &root_arg(r), "check"]));
    for (line, specifier) in [
        (1, "../Infrastructure/db"),
        (2, "../infrastructure./db"),
        (3, "Infrastructure/db"),
    ] {
        assert!(
            said.contains(&format!(
                "src/domain/song.ts:{line}: imports {specifier} (src/infrastructure/db), in infrastructure; "
            )),
            "{specifier}:\n{said}"
        );
    }

    let root = repo_with_ui("python", "");
    let r = root.path();
    fs::write(r.join("domain/model.py"), "import Infrastructure.db\n").unwrap();
    let said = stdout(&run(&["--root", &root_arg(r), "check"]));
    assert!(
        said.contains("domain/model.py:1: imports Infrastructure.db, in infrastructure; "),
        "{said}"
    );
}

#[cfg(unix)]
#[test]
fn a_typescript_import_through_a_link_lands_where_the_link_points() {
    let root = repo_with_ui("typescript", "");
    let r = root.path();
    fs::create_dir(r.join("vendor")).unwrap();
    std::os::unix::fs::symlink("../src/infrastructure", r.join("vendor/infra")).unwrap();
    fs::write(
        r.join("src/domain/song.ts"),
        "import '../../vendor/infra/db';\n",
    )
    .unwrap();
    let said = stdout(&run(&["--root", &root_arg(r), "check"]));
    assert!(
        said.contains(
            "src/domain/song.ts:1: imports ../../vendor/infra/db (src/infrastructure/db), in infrastructure; "
        ),
        "{said}"
    );
}

#[test]
fn every_form_of_typescript_import_is_judged() {
    let root = repo_with_ui("typescript", "");
    let r = root.path();
    let write = |path: &str, text: &str| {
        let full = r.join(path);
        fs::create_dir_all(full.parent().unwrap()).unwrap();
        fs::write(full, text).unwrap();
    };
    write(
        "tsconfig.json",
        "{\n  // as Vite writes it, with an alias added\n  \"compilerOptions\": { \"paths\": { \"@/*\": [\"./src/*\"] } },\n}\n",
    );
    write(
        "src/domain/song.ts",
        "import React from 'react';\nimport type { Play } from '../application/play';\nexport * from '@/infrastructure/db';\n\
         export const load = () => import('../handler/main');\nconst x = require(`../utils/x`);\n",
    );
    write(
        "src/ui/molecules/Row.tsx",
        "import { Button } from '../atoms/Button';\nimport { List } from '../organisms/List';\nimport '../index.css';\n\
         import logo from '../../domain/logo.svg?url';\nexport const Row = () => <p>{/* ../pages */}</p>;\n",
    );
    // Not a source file, ignored by git, or hidden: not read
    write("src/domain/notes.md", "import '../handler'\n");
    write(".gitignore", "src/domain/generated/\n");
    write("src/domain/generated/x.ts", "import '../../handler';\n");
    write("src/domain/.cache/x.ts", "import '../../handler';\n");
    let out = run(&["--root", &root_arg(r), "check"]);
    let said = stdout(&out);
    for line in [
        "src/domain/song.ts:2: imports ../application/play (src/application/play), in application; domain may \
         import `utils`",
        "src/domain/song.ts:3: imports @/infrastructure/db (src/infrastructure/db), in infrastructure; ",
        "src/domain/song.ts:4: imports ../handler/main (src/handler/main), in handler; ",
        "src/ui/molecules/Row.tsx:2: imports ../organisms/List (src/ui/organisms/List), in ui.organisms; \
         ui.molecules may import the levels below it, and `utils`",
        "src/ui/molecules/Row.tsx:3: imports ../index.css (src/ui/index.css), in ui outside its levels; ",
        "src/ui/molecules/Row.tsx:4: imports ../../domain/logo.svg?url (src/domain/logo.svg), in domain; ",
    ] {
        assert!(said.contains(line), "{line}:\n{said}");
    }
    assert_eq!(said.matches(": imports ").count(), 6, "{said}");
}

/// The `src/` of Vite's React starter (`npm create vite -- --template react-ts`, Vite 8.3, measured 2026-10-04), cut
/// down to the lines that import. A copy, so no test needs the network or Node; when Vite changes its starter, a file
/// it adds in `src/` fails `rotproof check` by name in a real project, so the copy cannot hide a change.
const VITE_STARTER: [(&str, &str); 6] = [
    (
        "src/main.tsx",
        "import { StrictMode } from 'react'\nimport { createRoot } from 'react-dom/client'\nimport './index.css'\n\
         import App from './App.tsx'\n\ncreateRoot(document.getElementById('root')!).render(\n  <StrictMode>\n    \
         <App />\n  </StrictMode>,\n)\n",
    ),
    (
        "src/App.tsx",
        "import { useState } from 'react'\nimport reactLogo from './assets/react.svg'\nimport './App.css'\n\n\
         function App() {\n  const [count, setCount] = useState(0)\n  return <img src={reactLogo} />\n}\n\n\
         export default App\n",
    ),
    ("src/App.css", "#center {\n  display: flex;\n}\n"),
    ("src/index.css", ":root {\n  --text: #6b6375;\n}\n"),
    (
        "src/assets/react.svg",
        "<svg xmlns=\"http://www.w3.org/2000/svg\"/>\n",
    ),
    (
        "src/assets/vite.svg",
        "<svg xmlns=\"http://www.w3.org/2000/svg\"/>\n",
    ),
];

#[test]
fn vites_starter_keeps_its_entry_point_and_says_where_the_rest_goes() {
    let root = repo_with_ui("typescript", "");
    let r = root.path();
    let write = |path: &str, text: &str| {
        let full = r.join(path);
        fs::create_dir_all(full.parent().unwrap()).unwrap();
        fs::write(full, text).unwrap();
    };
    for (path, text) in VITE_STARTER {
        write(path, text);
    }
    let arg = root_arg(r);
    let out = run(&["--root", &arg, "check"]);
    let said = stdout(&out);
    assert_eq!(out.status.code(), Some(1), "{said}");
    // main.tsx is handler's; the rest is named, with where it goes
    for outside in ["src/App.css", "src/App.tsx", "src/assets", "src/index.css"] {
        assert!(
            said.contains(&format!(
                "code outside the layers: {outside} (move it into a layer: a screen (Vite's App.tsx) goes in \
                 src/ui/pages; a style (index.css, App.css) or an image the UI shows (assets/) in src/ui/atoms; or \
                 list it in unchecked"
            )),
            "{outside}:\n{said}"
        );
    }
    assert!(!said.contains("src/main.tsx"), "{said}");

    // Moved as the finding says, with main.tsx's imports following: everything passes, and main.tsx is read as
    // handler's, which may import ui
    fs::remove_file(r.join("src/App.tsx")).unwrap();
    fs::remove_file(r.join("src/App.css")).unwrap();
    fs::remove_file(r.join("src/index.css")).unwrap();
    fs::remove_dir_all(r.join("src/assets")).unwrap();
    write(
        "src/ui/pages/App.tsx",
        "import reactLogo from '../atoms/assets/react.svg'\nimport '../atoms/App.css'\nexport default function App() \
         {\n  return <img src={reactLogo} />\n}\n",
    );
    write("src/ui/atoms/App.css", "#center {}\n");
    write("src/ui/atoms/index.css", ":root {}\n");
    write("src/ui/atoms/assets/react.svg", "<svg/>\n");
    write(
        "src/main.tsx",
        "import './ui/atoms/index.css'\nimport App from './ui/pages/App.tsx'\nexport { App }\n",
    );
    let out = run(&["--root", &arg, "check"]);
    assert!(out.status.success(), "{}", stdout(&out));
    write(
        "src/main.tsx",
        "import App from './ui/pages/App.tsx'\nconst = ;\n",
    );
    let out = run(&["--root", &arg, "check"]);
    assert!(
        stdout(&out).contains("src/main.tsx:2: cannot be read as TypeScript"),
        "{}",
        stdout(&out)
    );

    // Without handler, main.tsx belongs nowhere: code outside the layers like any other
    write("src/main.tsx", "export {}\n");
    fs::remove_dir_all(r.join("src/handler")).unwrap();
    declare(
        r,
        "stack = \"typescript\"\nareas = [\"a\"]\nabsent = [\"handler\"]\n",
    );
    let out = run(&["--root", &arg, "check"]);
    assert!(
        stdout(&out).contains("code outside the layers: src/main.tsx"),
        "{}",
        stdout(&out)
    );
}

#[test]
fn a_tsconfig_rotproof_cannot_read_fails() {
    let root = repo_with_ui("typescript", "");
    let r = root.path();
    fs::write(
        r.join("tsconfig.json"),
        "{ \"compilerOptions\": { \"paths\": { \"@/*/*\": [\"src/*\"] } } }",
    )
    .unwrap();
    let out = run(&["--root", &root_arg(r), "check"]);
    assert_eq!(out.status.code(), Some(1), "{}", stdout(&out));
    assert!(
        stdout(&out).contains(
            "tsconfig.json: paths maps \"@/*/*\" to \"src/*\", which Rotproof cannot read"
        ),
        "{}",
        stdout(&out)
    );
}

#[test]
fn every_form_of_import_is_judged() {
    let root = repo_with_ui("python", "");
    let r = root.path();
    let write = |path: &str, text: &str| {
        let full = r.join(path);
        fs::create_dir_all(full.parent().unwrap()).unwrap();
        fs::write(full, text).unwrap();
    };
    write(
        "domain/model.py",
        "import os\nimport requests\nfrom typing import TYPE_CHECKING\nif TYPE_CHECKING:\n    \
         from application import play\ndef f():\n    import infrastructure.db\n",
    );
    write(
        "ui/molecules/row.py",
        "from .. import organisms\nfrom ..atoms import button\nfrom . import field\nfrom ui import pages\nimport ui\n",
    );
    // Not a layer, ignored by git, or hidden: not read
    write("tests/test_x.py", "import handler\n");
    write(".gitignore", "domain/generated/\n");
    write("domain/generated/x.py", "import handler\n");
    write("domain/.cache/x.py", "import handler\n");
    let out = run(&["--root", &root_arg(r), "check"]);
    let said = stdout(&out);
    for line in [
        "domain/model.py:5: imports application.play, in application; domain may import `utils`",
        "domain/model.py:7: imports infrastructure.db, in infrastructure; ",
        "ui/molecules/row.py:1: imports ui.organisms, in ui.organisms; ui.molecules may import the levels below it, \
         and `utils`",
        "ui/molecules/row.py:4: imports ui.pages, in ui.pages; ",
        "ui/molecules/row.py:5: imports ui, in ui outside its levels; ",
    ] {
        assert!(said.contains(line), "{line}:\n{said}");
    }
    assert_eq!(said.matches(": imports ").count(), 5, "{said}");
}

#[test]
fn what_cannot_be_read_fails_and_what_is_not_there_is_not_judged() {
    let root = repo_with_ui("python", "\"infrastructure\"");
    let r = root.path();
    // A layer declared absent is a module the project does not have: a package of the same name is not judged
    fs::write(r.join("domain/model.py"), "import infrastructure.db\n").unwrap();
    let out = run(&["--root", &root_arg(r), "check"]);
    assert!(out.status.success(), "{}", stdout(&out));

    fs::write(
        r.join("utils/broken.py"),
        "import os\ndef (:\nimport handler\n",
    )
    .unwrap();
    fs::write(r.join("utils/latin.py"), b"x = '\xff'\n").unwrap();
    let out = run(&["--root", &root_arg(r), "check"]);
    let said = stdout(&out);
    assert_eq!(out.status.code(), Some(1), "{said}");
    let error = said
        .lines()
        .find(|line| line.contains("utils/broken.py:2: cannot be read as Python ("))
        .unwrap_or_else(|| panic!("{said}"));
    // The parser recovers, and the imports after the error are judged all the same: what it says has to fit that
    assert!(
        error.ends_with("), so the imports after it may be misread"),
        "{error}"
    );
    assert!(
        said.contains("utils/broken.py:3: imports handler, in handler; "),
        "{said}"
    );
    assert!(
        said.contains("utils/latin.py: cannot be read as UTF-8"),
        "{said}"
    );
}

#[test]
fn every_stack_with_layers_checks_everything() {
    for stack in ["python", "typescript", "rust"] {
        let root = declared(&format!("stack = \"{stack}\"\nareas = [\"a\"]\n"));
        let arg = root_arg(root.path());
        assert!(
            create(&["--root", &arg, "create", "--yes"])
                .status
                .success()
        );
        fs::write(
            root.path().join("docs/log.md"),
            "# Log\n\n## 2026-10-02\n\n* Something\n",
        )
        .unwrap();
        let out = run(&["--root", &arg, "check"]);
        let said = stdout(&out);
        // Nothing is skipped: the one line says that the layers, their direction and comments included, kept the rules
        assert_eq!(
            said, "the layers and every record keep the rules\n",
            "{stack}"
        );
    }
}

#[test]
fn code_that_is_not_the_projects_is_not_looked_at() {
    let root = clean_repo();
    let r = root.path();
    // Ignored by git, hidden, paths the stack says are not layers, a path the project lists, and a file that is not
    // code
    fs::write(r.join(".gitignore"), "venv/\n").unwrap();
    for path in [
        "venv/Lib/site-packages/pkg/__init__.py",
        ".tox/x.py",
        "tests/test_x.py",
        "scripts/tool.py",
        "notes/readme.md",
    ] {
        plant_file(r, path);
    }
    declare(
        r,
        "stack = \"python\"\nareas = [\"a\"]\nabsent = [\"ui\"]\nunchecked = [\"scripts/\"]\n",
    );
    let out = run(&["--root", &root_arg(r), "check"]);
    assert!(out.status.success(), "{}", stdout(&out));
    // A CI script is code like any other: Rotproof makes none, so it names none as an exception
    plant_file(r, "ci.py");
    let out = run(&["--root", &root_arg(r), "check"]);
    assert_eq!(out.status.code(), Some(1), "{}", stdout(&out));
    assert!(stdout(&out).contains("ci.py"), "{}", stdout(&out));
}

#[test]
fn only_the_projects_own_gitignore_hides_code() {
    // A .gitignore above the root belongs to another repository, or to no repository at all
    let outer = tempfile::tempdir().unwrap();
    fs::write(outer.path().join(".gitignore"), "scripts/\n").unwrap();
    let r = outer.path().join("project");
    fs::create_dir_all(r.join(".config")).unwrap();
    declare(
        &r,
        "stack = \"typescript\"\nareas = [\"a\"]\nabsent = [\"ui\"]\n",
    );
    assert!(
        create(&["--root", &root_arg(&r), "create", "--yes"])
            .status
            .success()
    );
    plant_file(&r, "src/scripts/tool.ts");
    let out = run(&["--root", &root_arg(&r), "check"]);
    assert_eq!(out.status.code(), Some(1), "{}", stdout(&out));
    assert!(
        stdout(&out).contains("code outside the layers: src/scripts"),
        "{}",
        stdout(&out)
    );
    // The root's own .gitignore applies inside the scope
    fs::write(r.join(".gitignore"), "src/scripts/\n").unwrap();
    let out = run(&["--root", &root_arg(&r), "check"]);
    assert!(out.status.success(), "{}", stdout(&out));
}

#[test]
fn the_command_is_named_rotproof_whatever_its_crate_is_named() {
    // The crate is the handler layer; what a user types and reads is rotproof
    assert_eq!(
        stdout(&run(&["--version"])),
        format!("rotproof {}\n", env!("CARGO_PKG_VERSION"))
    );
    let help = stdout(&run(&["--help"]));
    assert!(!help.contains("handler"), "{help}");
}

#[test]
fn an_agent_with_only_the_binary_finds_its_way_to_a_checked_project() {
    // Each step follows only what the step before printed, as an agent without the README would
    let root = tempfile::tempdir().unwrap();
    let arg = root_arg(root.path());
    let stderr = |out: &Output| String::from_utf8_lossy(&out.stderr).into_owned();

    // No arguments: what Rotproof is, and the order to start in
    let help = stderr(&run(&[]));
    assert!(help.contains("Keeps a project's structure"), "{help}");
    let steps = [
        "rotproof init --stack <stack>",
        "list in absent",
        "rotproof create",
        "rotproof check",
    ];
    let start = &help[help
        .find("Start a project:")
        .expect("the help says how to start")..];
    let at: Vec<usize> = steps
        .iter()
        .map(|s| start.find(s).unwrap_or(usize::MAX))
        .collect();
    assert!(at.windows(2).all(|w| w[0] < w[1]), "{help}");
    assert!(
        help.contains(".rotproof/AGENTS.md") && help.contains("Exit codes"),
        "{help}"
    );

    // The stacks, from init's help and from a check with nothing declared
    let init_help = stdout(&run(&["init", "--help"]));
    assert!(
        init_help.contains("python, typescript, rust, or none"),
        "{init_help}"
    );
    let out = run(&["--root", &arg, "check"]);
    assert!(
        stdout(&out).contains("rotproof init --stack <stack>` (python, typescript, rust, or none"),
        "{}",
        stdout(&out)
    );

    // Each command names the next
    let out = run(&["--root", &arg, "init", "--stack", "python"]);
    assert!(
        stdout(&out).contains("then run `rotproof create`"),
        "{}",
        stdout(&out)
    );
    // Run straight after init, create lists the layers it would make and says how to go on, writing nothing
    let out = create(&["--root", &arg, "create"]);
    assert_eq!(out.status.code(), Some(2), "{}", stdout(&out));
    assert!(
        stderr(&out).contains("  ui/pages/ (ui.pages)")
            && stderr(&out).contains("Declare in absent")
            && stderr(&out).contains("`rotproof create --yes`"),
        "{}",
        stderr(&out)
    );
    assert_eq!(tree(root.path()).len(), 1, "{:?}", tree(root.path()));
    let out = create(&["--root", &arg, "create", "--yes"]);
    assert!(
        stdout(&out).contains("next: run `rotproof check`"),
        "{}",
        stdout(&out)
    );
    fs::write(
        root.path().join("docs/log.md"),
        "# Log\n\n## 2026-10-04\n\n* Started\n",
    )
    .unwrap();
    // The milestone create wrote has no area yet, as init declared none, and the check says what is missing
    let out = run(&["--root", &arg, "check"]);
    assert_eq!(out.status.code(), Some(1), "{}", stdout(&out));
    assert!(
        stdout(&out).contains("work/next-milestone.md: tags: 0 tags")
            && stdout(&out).contains("no open milestone in docs/work/"),
        "{}",
        stdout(&out)
    );
    declare_first_area(root.path());
    let out = run(&["--root", &arg, "check"]);
    assert!(out.status.success(), "{}", stdout(&out));

    // Every command's --help says more than its -h, and how it exits
    for command in ["init", "create", "check", "guide", "index", "stop-hook"] {
        let short = stdout(&run(&[command, "-h"]));
        let long = stdout(&run(&[command, "--help"]));
        assert!(long.len() > short.len(), "{command}:\n{long}");
        assert!(long.contains("Exits"), "{command}:\n{long}");
    }
}

#[test]
fn guide_prints_what_create_writes_with_or_without_a_project() {
    let empty = tempfile::tempdir().unwrap();
    let empty_arg = root_arg(empty.path());
    for stack in ["python", "typescript", "rust", "none"] {
        let root = declared(&format!("stack = \"{stack}\"\nareas = [\"a\"]\n"));
        let arg = root_arg(root.path());
        assert!(
            create(&["--root", &arg, "create", "--yes"])
                .status
                .success()
        );
        let written = fs::read_to_string(root.path().join(".rotproof/AGENTS.md")).unwrap();
        // In the project, from its declaration; outside any project, from --stack
        let inside = run(&["--root", &arg, "guide"]);
        assert!(inside.status.success(), "{stack}");
        assert_eq!(stdout(&inside), written, "{stack}");
        let before = run(&["--root", &empty_arg, "guide", "--stack", stack]);
        assert!(before.status.success(), "{stack}");
        assert_eq!(stdout(&before), written, "{stack}");
    }
    // Nothing is written
    assert!(tree(empty.path()).is_empty(), "{:?}", tree(empty.path()));
    // Without a stack to go by, and with an unknown one: exit 2, with the stacks
    for args in [vec!["guide"], vec!["guide", "--stack", "cobol"]] {
        let mut all = vec!["--root", empty_arg.as_str()];
        all.extend(args);
        let out = run(&all);
        assert_eq!(out.status.code(), Some(2));
        let said = String::from_utf8_lossy(&out.stderr);
        assert!(said.contains("python, typescript, rust, none"), "{said}");
    }
}

#[test]
fn init_writes_a_declaration_that_create_reads() {
    for stack in ["python", "typescript", "rust", "none"] {
        let root = tempfile::tempdir().unwrap();
        let arg = root_arg(root.path());
        let out = run(&["--root", &arg, "init", "--stack", stack]);
        assert!(
            out.status.success(),
            "{stack}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(
            stdout(&out).contains("wrote .config/rotproof.toml"),
            "{stack}: {}",
            stdout(&out)
        );
        let written = fs::read_to_string(root.path().join(".config/rotproof.toml")).unwrap();
        assert!(
            written.contains(&format!("stack = \"{stack}\"")),
            "{written}"
        );
        // Only the declaration: nothing is made before the project declares what it does not have
        assert_eq!(
            tree(root.path()).len(),
            1,
            "{stack}: {:?}",
            tree(root.path())
        );
        let out = create(&["--root", &arg, "create", "--yes"]);
        assert!(out.status.success(), "{stack}: {}", stdout(&out));
        declare_first_area(root.path());
        let out = run(&["--root", &arg, "check"]);
        assert!(out.status.success(), "{stack}: {}", stdout(&out));
        let written = fs::read_to_string(root.path().join(".config/rotproof.toml")).unwrap();
        // The declaration is the project's: a second init upgrades the project's files, which are up to this version
        // already, and leaves it as it is
        declare(root.path(), &format!("{written}# edited\n"));
        let again = run(&["--root", &arg, "init", "--stack", stack]);
        assert_eq!(again.status.code(), Some(0), "{stack}: {}", stdout(&again));
        assert!(stdout(&again).contains("are up to"), "{}", stdout(&again));
        let kept = fs::read_to_string(root.path().join(".config/rotproof.toml")).unwrap();
        assert_eq!(kept, format!("{written}# edited\n"), "{stack}");
        // With another stack, it refuses: it never changes a stack
        let other = if stack == "rust" { "python" } else { "rust" };
        let again = run(&["--root", &arg, "init", "--stack", other]);
        assert_eq!(again.status.code(), Some(2), "{stack}");
        assert!(String::from_utf8_lossy(&again.stderr).contains("never changes a stack"));
    }
}

#[test]
fn init_needs_a_known_stack() {
    let root = tempfile::tempdir().unwrap();
    let arg = root_arg(root.path());
    let out = run(&["--root", &arg, "init", "--stack", "cobol"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("known: python, typescript, rust, none"));
    let out = run(&["--root", &arg, "init"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("name the stack with --stack"));
    assert!(tree(root.path()).is_empty(), "{:?}", tree(root.path()));
}

#[test]
fn init_upgrades_a_project_made_by_the_first_release() {
    // As `rotproof create` of 0.1.0 left a project: no files in the declaration, and settings without the deny rule
    let root = declared("stack = \"none\"\nareas = [\"a\"]\n");
    let r = root.path();
    let arg = root_arg(r);
    fs::write(
        r.join(".config/rotproof.toml"),
        "stack = \"none\"\nareas = [\"a\"]\n",
    )
    .unwrap();
    assert!(create(&["--root", &arg, "create"]).status.success());
    fs::write(r.join(".claude/settings.json"), "{\n  \"hooks\": {}\n}\n").unwrap();
    fs::write(
        r.join("docs/log.md"),
        "# Log\n\n## 2026-10-02\n\n* Something\n",
    )
    .unwrap();
    let version = env!("CARGO_PKG_VERSION");
    // While Rotproof is 0.1.0 itself, its files are up to date: nothing to update, and the check passes
    let out = run(&["--root", &arg, "check"]);
    if version == "0.1.0" {
        assert!(out.status.success(), "{}", stdout(&out));
        return;
    }
    assert_eq!(out.status.code(), Some(1), "{}", stdout(&out));
    assert!(
        stdout(&out).contains("the project's files are up to 0.1.0 (no files in the declaration)"),
        "{}",
        stdout(&out)
    );
    let out = run(&["--root", &arg, "init"]);
    assert!(out.status.success(), "{}", stdout(&out));
    assert!(stdout(&out).contains("updated .claude/settings.json: claude-deny-approvals"));
    let settings = fs::read_to_string(r.join(".claude/settings.json")).unwrap();
    assert!(
        settings.contains("Edit(/.config/rotproof-approved.toml)"),
        "{settings}"
    );
    let declaration = fs::read_to_string(r.join(".config/rotproof.toml")).unwrap();
    assert!(
        declaration.starts_with("stack = \"none\"\nareas = [\"a\"]\n"),
        "{declaration}"
    );
    assert!(
        declaration.contains(&format!("files = \"{version}\"")),
        "{declaration}"
    );
    let out = run(&["--root", &arg, "check"]);
    assert!(out.status.success(), "{}", stdout(&out));
}

#[test]
fn a_repository_of_records_only_makes_and_checks_only_docs() {
    let root = declared("stack = \"none\"\nareas = [\"a\"]\n");
    let arg = root_arg(root.path());
    let out = create(&["--root", &arg, "create"]);
    assert!(out.status.success(), "{}", stdout(&out));
    let made: Vec<String> = tree(root.path())
        .into_iter()
        .map(|(path, _)| path)
        .collect();
    // No layer: every file is in a hidden directory, in docs/, or one of the project's files at the root
    assert!(
        made.iter().all(|path| path.starts_with('.')
            || path.starts_with("docs/")
            || PROJECT_FILES.contains(&path.as_str())),
        "{made:?}"
    );
    // Code anywhere is not looked at, and the output says the layers were skipped
    plant_file(root.path(), "main.py");
    let out = run(&["--root", &arg, "check"]);
    assert!(out.status.success(), "{}", stdout(&out));
    assert!(
        stdout(&out).contains("the layers are not checked"),
        "{}",
        stdout(&out)
    );
    assert!(
        stdout(&out).contains("every record keeps the rules"),
        "{}",
        stdout(&out)
    );
    // The records are still checked
    fs::write(root.path().join("docs/work/index.md"), "edited by hand\n").unwrap();
    let out = run(&["--root", &arg, "check"]);
    assert_eq!(out.status.code(), Some(1), "{}", stdout(&out));
    // No layers means nothing to declare absent or unchecked
    declare(
        root.path(),
        "stack = \"none\"\nareas = [\"a\"]\nabsent = [\"ui\"]\n",
    );
    let out = run(&["--root", &arg, "check"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(
        stdout(&out).contains("absent must be empty"),
        "{}",
        stdout(&out)
    );
}

#[test]
fn a_marker_in_a_comment_fails_wherever_the_code_is() {
    let root = clean_repo();
    let r = root.path();
    let write = |path: &str, text: &str| {
        let full = r.join(path);
        fs::create_dir_all(full.parent().unwrap()).unwrap();
        fs::write(full, text).unwrap();
    };
    write("domain/model.py", "X = 1\n# TODO: split this\n");
    write("tests/test_model.py", "x = 1  # NOTE about x\n");
    // Not a comment, or in a path the project lists in unchecked
    write("domain/states.py", "TODO = 1\nPHONE = \"XXX-XXXX\"\n");
    write("scripts/generated.py", "# FIXME written by a tool\n");
    declare(
        r,
        "stack = \"python\"\nareas = [\"a\"]\nabsent = [\"ui\"]\nunchecked = [\"scripts\"]\n",
    );
    let arg = root_arg(r);
    let out = run(&["--root", &arg, "check"]);
    let said = stdout(&out);
    assert_eq!(out.status.code(), Some(1), "{said}");
    // The heading names only what was found; each finding is its place, and the line under it as written
    assert!(
        said.contains("no comment holds TODO or NOTE (work left to do goes in docs/work/"),
        "{said}"
    );
    assert!(
        said.contains("  domain/model.py:2\n    # TODO: split this\n"),
        "{said}"
    );
    assert!(
        said.contains("  tests/test_model.py:1\n    x = 1  # NOTE about x\n"),
        "{said}"
    );
    assert!(
        !said.contains("states.py") && !said.contains("generated.py"),
        "{said}"
    );
    // The same text without the markers passes
    write("domain/model.py", "X = 1\n# Split this when it grows\n");
    write("tests/test_model.py", "x = 1  # about x\n");
    let out = run(&["--root", &arg, "check"]);
    assert!(out.status.success(), "{}", stdout(&out));
}

#[test]
fn a_marker_in_a_typescript_comment_fails() {
    let root = repo_with_ui("typescript", "\"ui\"");
    let r = root.path();
    let write = |path: &str, text: &str| {
        let full = r.join(path);
        fs::create_dir_all(full.parent().unwrap()).unwrap();
        fs::write(full, text).unwrap();
    };
    write(
        "src/domain/song.ts",
        "export const x = 1;\n// TODO: split this\n",
    );
    // JSX text, a string, and a file that is not source: not comments
    write(
        "src/domain/View.tsx",
        "export const v = <p>TODO in text</p>;\nexport const s = 'FIXME';\n",
    );
    write("src/domain/notes.css", "/* HACK in a stylesheet */\n");
    let arg = root_arg(r);
    let out = run(&["--root", &arg, "check"]);
    let said = stdout(&out);
    assert_eq!(out.status.code(), Some(1), "{said}");
    assert!(
        said.contains("no comment holds TODO (work left to do"),
        "{said}"
    );
    assert!(
        said.contains("  src/domain/song.ts:2\n    // TODO: split this\n"),
        "{said}"
    );
    assert_eq!(said.matches("\n  src/").count(), 1, "{said}");
    write(
        "src/domain/song.ts",
        "export const x = 1;\n// Split this when it grows\n",
    );
    let out = run(&["--root", &arg, "check"]);
    assert!(out.status.success(), "{}", stdout(&out));
}

#[test]
fn a_marker_in_a_rust_comment_fails() {
    let root = repo_with_ui("rust", "");
    let r = root.path();
    let write = |path: &str, text: &str| {
        let full = r.join(path);
        fs::create_dir_all(full.parent().unwrap()).unwrap();
        fs::write(full, text).unwrap();
    };
    write(
        "crates/domain/src/song.rs",
        "pub fn f() {}\n// TODO: split this\n",
    );
    // A crate's tests and build script are read too
    write(
        "crates/domain/tests/song.rs",
        "/* FIXME\n   and HACK */\n#[test]\nfn t() {}\n",
    );
    write(
        "crates/handler/build.rs",
        "/// NOTE on the build\nfn main() {}\n",
    );
    // A string, a character, a lifetime, a manifest's comment, code outside `crates/`, and a path the project lists:
    // not read as comments
    write(
        "crates/domain/src/text.rs",
        "pub const S: &str = \"// TODO\";\npub const R: &str = r#\"/* FIXME */\"#;\npub fn f<'a>(x: &'a str) -> \
         &'a str { x } // a plain comment\n",
    );
    write(
        "crates/utils/Cargo.toml",
        "# TODO in a manifest\n[package]\nname = \"utils\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    );
    write(
        "xtask/src/main.rs",
        "// TODO outside the crates\nfn main() {}\n",
    );
    write(
        "crates/generated/Cargo.toml",
        "[package]\nname = \"generated\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    );
    write("crates/generated/src/lib.rs", "// XXX generated\n");
    fs::write(
        r.join(".config/rotproof.toml"),
        up_to_date(
            "stack = \"rust\"\nareas = [\"a\"]\nabsent = []\nunchecked = [\"crates/generated\"]\n",
        ),
    )
    .unwrap();
    let arg = root_arg(r);
    let out = run(&["--root", &arg, "check"]);
    let said = stdout(&out);
    assert_eq!(out.status.code(), Some(1), "{said}");
    assert!(
        said.contains("no comment holds TODO, FIXME, HACK or NOTE (work left to do"),
        "{said}"
    );
    for found in [
        "  crates/domain/src/song.rs:2\n    // TODO: split this\n",
        "  crates/domain/tests/song.rs:1\n    /* FIXME\n",
        "  crates/domain/tests/song.rs:2\n    and HACK */\n",
        "  crates/handler/build.rs:1\n    /// NOTE on the build\n",
    ] {
        assert!(said.contains(found), "{found}:\n{said}");
    }
    assert_eq!(said.matches("\n  crates/").count(), 4, "{said}");
    for path in [
        "crates/domain/src/song.rs",
        "crates/domain/tests/song.rs",
        "crates/handler/build.rs",
    ] {
        write(path, "// A plain comment\n");
    }
    let out = run(&["--root", &arg, "check"]);
    assert!(out.status.success(), "{}", stdout(&out));
}

#[test]
fn the_help_of_check_names_the_markers_of_the_check() {
    // Built from the check's own list, in both forms of asking for it
    let sentence = domain::markers::either(&domain::markers::MARKERS);
    assert_eq!(sentence, "TODO, FIXME, XXX, HACK or NOTE");
    for args in [&["check", "--help"][..], &["help", "check"]] {
        let said = stdout(&run(args));
        assert!(
            said.contains(&format!("that no comment holds {sentence}, that")),
            "{args:?}: {said}"
        );
    }
}

/// Run the stop hook from `root` with `input` on stdin.
fn stop_hook(root: &Path, input: &str) -> Output {
    use std::io::Write;
    use std::process::Stdio;
    let mut child = Command::new(env!("CARGO_BIN_EXE_rotproof"))
        .args(["--root", &root_arg(root), "stop-hook"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the binary runs");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

fn git(root: &Path, args: &[&str]) {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["-c", "user.name=t", "-c", "user.email=t@example.com"])
        .args(args)
        .output()
        .expect("git runs");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn the_stop_hook_sends_the_agent_back_once_while_docs_did_not_change() {
    let root = declared("stack = \"none\"\nareas = [\"a\"]\n");
    let r = root.path();
    assert!(create(&["--root", &root_arg(r), "create"]).status.success());
    // create wrote the settings that run the hook, for Claude Code only
    let settings = fs::read_to_string(r.join(".claude/settings.json")).unwrap();
    assert!(
        settings.contains("\"command\": \"rotproof stop-hook\"") && settings.contains("\"Stop\""),
        "{settings}"
    );
    // and that deny Claude Code editing the approvals file, which only a person adds to
    assert!(
        settings.contains("\"deny\": [\n      \"Edit(/.config/rotproof-approved.toml)\"\n    ]"),
        "{settings}"
    );
    assert!(!r.join(".gemini").exists());
    git(r, &["init", "-q"]);
    git(r, &["add", "-A"]);
    git(r, &["commit", "-q", "-m", "start"]);
    let open = r#"{"hook_event_name": "Stop", "stop_hook_active": false, "last_assistant_message": "Done; the Linux path is not checked."}"#;

    // From a directory inside the project too
    fs::create_dir_all(r.join("src/deep")).unwrap();
    for from in [r.to_path_buf(), r.join("src/deep")] {
        let out = stop_hook(&from, open);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let said = stdout(&out);
        assert!(said.contains("\"additionalContext\""), "{said}");
        assert!(said.contains("\\\"not checked\\\""), "{said}");
    }
    // Once per stop
    let again = open.replace("\"stop_hook_active\": false", "\"stop_hook_active\": true");
    assert_eq!(stdout(&stop_hook(r, &again)), "");
    // Nothing open
    let done = open.replace("the Linux path is not checked", "every path passes");
    assert_eq!(stdout(&stop_hook(r, &done)), "");
    // Recorded: a change in docs/, new files included
    fs::write(r.join("docs/work/new.md"), "x\n").unwrap();
    let out = stop_hook(r, open);
    assert!(out.status.success());
    assert_eq!(stdout(&out), "");
    // Outside a project, nothing is said
    let outside = tempfile::tempdir().unwrap();
    let out = stop_hook(outside.path(), open);
    assert!(out.status.success());
    assert_eq!(stdout(&out), "");
}

#[test]
fn a_broken_stop_hook_exits_1_never_2() {
    // Claude Code reads exit code 2 from a Stop hook as "do not stop"
    let root = declared("stack = \"none\"\nareas = [\"a\"]\n");
    for input in ["not json", "{}"] {
        let out = stop_hook(root.path(), input);
        assert_eq!(out.status.code(), Some(1), "{input}");
        assert!(
            String::from_utf8_lossy(&out.stderr).contains("rotproof stop-hook:"),
            "{input}"
        );
    }
    // Not a git repository: git fails, and the hook says so
    let open = r#"{"hook_event_name": "Stop", "stop_hook_active": false, "last_assistant_message": "未確認"}"#;
    let out = stop_hook(root.path(), open);
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("git status failed"));
}

#[test]
fn every_edit_of_a_knowledge_document_is_named_in_the_log() {
    let root = clean_repo();
    let r = root.path();
    let arg = root_arg(r);
    assert!(
        r.join("docs/knowledge/rules.md").is_file(),
        "create makes docs/knowledge/"
    );
    let doc = "---\ntype: Knowledge\ntitle: API\ndescription: How the API is shaped.\ntags: [a]\nstatus: stable\n---\n\n# Shape\n\nOne endpoint.\n";
    fs::write(r.join("docs/knowledge/api.md"), doc).unwrap();
    assert!(run(&["--root", &arg, "index"]).status.success());

    // Not in the log: the failure gives the line to write
    let out = run(&["--root", &arg, "check"]);
    let said = stdout(&out);
    assert_eq!(out.status.code(), Some(1), "{said}");
    assert!(
        said.contains("the log names every knowledge document as it is now:"),
        "{said}"
    );
    let line = said
        .split('`')
        .find(|part| part.starts_with("* **Knowledge**: knowledge/api.md@"))
        .unwrap_or_else(|| panic!("no line to write: {said}"))
        .to_string();

    // Written in the log entry: it passes
    let log = format!("# Log\n\n## 2026-10-02\n\n* Something\n  {line}\n");
    fs::write(r.join("docs/log.md"), &log).unwrap();
    let out = run(&["--root", &arg, "check"]);
    assert!(out.status.success(), "{}", stdout(&out));

    // Edited after its entry: it fails again
    fs::write(
        r.join("docs/knowledge/api.md"),
        doc.replace("One endpoint.", "Two endpoints."),
    )
    .unwrap();
    let out = run(&["--root", &arg, "check"]);
    assert_eq!(out.status.code(), Some(1), "{}", stdout(&out));
    assert!(stdout(&out).contains("knowledge/api.md: no log entry names it as it is now"));

    // The log names a document that is not there
    fs::write(r.join("docs/knowledge/api.md"), doc).unwrap();
    fs::write(
        r.join("docs/log.md"),
        format!("{log}  * **Knowledge**: knowledge/gone.md@0123abcd\n"),
    )
    .unwrap();
    let out = run(&["--root", &arg, "check"]);
    let said = stdout(&out);
    assert_eq!(out.status.code(), Some(1), "{said}");
    assert!(
        said.contains("the log points only at real knowledge documents:\n  no such document: docs/knowledge/gone.md"),
        "{said}"
    );
}

#[test]
fn a_knowledge_document_belongs_in_docs_knowledge() {
    let root = clean_repo();
    let r = root.path();
    fs::write(
        r.join("docs/work/api.md"),
        "---\ntype: Knowledge\ntitle: API\ndescription: D.\ntags: [a]\nstatus: stable\n---\n",
    )
    .unwrap();
    let out = run(&["--root", &root_arg(r), "check"]);
    let said = stdout(&out);
    assert_eq!(out.status.code(), Some(1), "{said}");
    assert!(
        said.contains(
            "every document is a known type in its place:\n  work/api.md: type \"Knowledge\" does not belong in \
             docs/work (Spec, Work Item, Milestone, Guide)"
        ),
        "{said}"
    );
}

/// An approvals file with one entry for each `(from, import)`.
fn approvals(entries: &[(&str, &str)]) -> String {
    entries
        .iter()
        .map(|(from, import)| {
            format!(
                "[[kept]]\nfrom = \"{from}\"\nimport = \"{import}\"\nreason = \"the framework loads it\"\n\
                 approved = {{ by = \"someone\", at = \"2026-10-06T09:00:00+09:00\" }}\n\n"
            )
        })
        .collect()
}

#[test]
fn an_approved_import_passes_and_is_printed_and_a_stale_approval_fails() {
    let root = clean_repo();
    let r = root.path();
    fs::write(
        r.join("domain/model.py"),
        "import infrastructure.db\nimport application\n\ndef f():\n    import infrastructure.db\n",
    )
    .unwrap();
    let path = r.join(".config/rotproof-approved.toml");
    let check = || {
        let out = run(&["--root", &root_arg(r), "check"]);
        (out.status.code(), stdout(&out))
    };
    // Without an entry, both imports fail
    let (code, said) = check();
    assert_eq!(code, Some(1), "{said}");
    assert!(
        said.contains("domain/model.py:1: imports infrastructure.db"),
        "{said}"
    );
    assert!(
        said.contains("domain/model.py:2: imports application"),
        "{said}"
    );

    // One entry approves its import on every line, and only that import; it is printed once
    fs::write(
        &path,
        approvals(&[("domain/model.py", "infrastructure.db")]),
    )
    .unwrap();
    let (code, said) = check();
    assert_eq!(code, Some(1), "{said}");
    assert!(said.contains(
        "approved by a person in .config/rotproof-approved.toml (1):\n  infrastructure.db in domain/model.py: \
         the framework loads it (someone, 2026-10-06T09:00:00+09:00)\n"
    ), "{said}");
    assert!(!said.contains("domain/model.py:1:"), "{said}");
    assert!(!said.contains("domain/model.py:5:"), "{said}");
    assert!(
        said.contains("domain/model.py:2: imports application"),
        "{said}"
    );

    // Both approved: the check passes, and still prints them
    fs::write(
        &path,
        approvals(&[
            ("domain/model.py", "infrastructure.db"),
            ("domain/model.py", "application"),
        ]),
    )
    .unwrap();
    let (code, said) = check();
    assert_eq!(code, Some(0), "{said}");
    assert!(said.contains("(2):"), "{said}");

    // The import fixed, its approval fails until it goes
    fs::write(r.join("domain/model.py"), "import application\n").unwrap();
    let (code, said) = check();
    assert_eq!(code, Some(1), "{said}");
    assert!(said.contains(
        "every approval matches a forbidden import:\n  .config/rotproof-approved.toml: approved, but no such \
         forbidden import: infrastructure.db in domain/model.py"
    ), "{said}");

    // A file named otherwise approves nothing, on any system, and says so
    fs::remove_file(&path).unwrap();
    fs::write(
        r.join(".config/Rotproof-Approved.toml"),
        approvals(&[("domain/model.py", "application")]),
    )
    .unwrap();
    let (code, said) = check();
    assert_eq!(code, Some(1), "{said}");
    assert!(
        said.contains("is not read: the approvals file is .config/rotproof-approved.toml"),
        "{said}"
    );
    assert!(
        said.contains("domain/model.py:1: imports application"),
        "{said}"
    );
    fs::remove_file(r.join(".config/Rotproof-Approved.toml")).unwrap();

    // A broken file names what is wrong
    fs::write(&path, "[[kept]]\nfrom = \"domain/model.py\"\n").unwrap();
    let (code, said) = check();
    assert_eq!(code, Some(1), "{said}");
    assert!(said.contains("every approval matches a forbidden import:\n  .config/rotproof-approved.toml: missing field"), "{said}");
}

#[test]
fn approve_refuses_without_a_terminal_and_prune_removes_only_what_matches_nothing() {
    let root = clean_repo();
    let r = root.path();
    fs::write(r.join("domain/model.py"), "import infrastructure.db\n").unwrap();
    let path = r.join(".config/rotproof-approved.toml");

    // The test's stdin is no terminal, as an agent's shell has none: nothing is asked, nothing written
    let out = run(&[
        "--root",
        &root_arg(r),
        "approve",
        "domain/model.py",
        "infrastructure.db",
    ]);
    assert_eq!(out.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("stdin is not a terminal"),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(!path.exists());
    // The file and the import are both needed, unless pruning
    let out = run(&["--root", &root_arg(r), "approve", "domain/model.py"]);
    assert_eq!(out.status.code(), Some(2));

    // Nothing to prune without a file
    let out = run(&["--root", &root_arg(r), "approve", "--prune"]);
    assert_eq!(out.status.code(), Some(0));
    assert!(
        stdout(&out).contains("nothing to prune"),
        "{}",
        stdout(&out)
    );
    assert!(!path.exists());

    // One entry still matches, one does not: only the second goes, and its comment with it is kept above the next
    fs::write(
        &path,
        format!(
            "# our note\n{}",
            approvals(&[
                ("domain/gone.py", "infrastructure.db"),
                ("domain/model.py", "infrastructure.db"),
            ])
        ),
    )
    .unwrap();
    let out = run(&["--root", &root_arg(r), "approve", "--prune"]);
    assert_eq!(out.status.code(), Some(0), "{}", stdout(&out));
    assert!(
        stdout(&out).contains(
            "removed from .config/rotproof-approved.toml: infrastructure.db in domain/gone.py"
        ),
        "{}",
        stdout(&out)
    );
    let left = fs::read_to_string(&path).unwrap();
    assert!(left.starts_with("# our note\n"), "{left}");
    assert!(!left.contains("domain/gone.py"), "{left}");
    assert!(left.contains("domain/model.py"), "{left}");
    let out = run(&["--root", &root_arg(r), "check"]);
    assert_eq!(out.status.code(), Some(0), "{}", stdout(&out));

    // Pruning never lets through what failed: with the import fixed, its entry goes and the check passes
    fs::write(r.join("domain/model.py"), "import utils\n").unwrap();
    let out = run(&["--root", &root_arg(r), "approve", "--prune"]);
    assert_eq!(out.status.code(), Some(0), "{}", stdout(&out));
    assert!(!fs::read_to_string(&path).unwrap().contains("[[kept]]"));
    let out = run(&["--root", &root_arg(r), "check"]);
    assert_eq!(out.status.code(), Some(0), "{}", stdout(&out));
}

/// Every `key: hash` the check asks to write in follows, from its output.
fn to_write(said: &str) -> Vec<(String, String)> {
    said.split('`')
        .filter_map(|part| {
            let (key, hash) = part.rsplit_once(": ")?;
            (hash.len() == 8 && hash.bytes().all(|b| b.is_ascii_hexdigit()) && !key.contains(' '))
                .then(|| (key.to_string(), hash.to_string()))
        })
        .collect()
}

#[test]
fn a_knowledge_document_fails_when_what_it_follows_changes_unreviewed() {
    let root = clean_repo();
    let r = root.path();
    let arg = root_arg(r);
    fs::write(
        r.join("domain/model.py"),
        "import os\n\n@dataclass\nclass Song:\n    title: str\n\n    def play(self):\n        return 1\n\n\
         def other():\n    return 2\n",
    )
    .unwrap();
    fs::write(r.join("domain/kinds.py"), "KINDS = 1\n").unwrap();
    let keys = [
        "domain/model.py",
        "domain/model.py::Song",
        "domain/model.py::Song.play",
        "domain/",
    ];
    let document = |pins: &[(String, String)]| {
        let follows: String = pins
            .iter()
            .map(|(key, hash)| format!("  {key}: \"{hash}\"\n"))
            .collect();
        format!(
            "---\ntype: Knowledge\ntitle: Model\ndescription: The model.\ntags: [a]\nstatus: stable\nfollows:\n\
             {follows}---\n\n# Shape\n\nA song plays.\n"
        )
    };
    // The check names every key and the hash it has now: pinned at those, the document matches its code
    let placeholders: Vec<(String, String)> = keys
        .iter()
        .map(|key| (key.to_string(), "00000000".to_string()))
        .collect();
    fs::write(r.join("docs/knowledge/model.md"), document(&placeholders)).unwrap();
    let said = stdout(&run(&["--root", &arg, "check"]));
    let pins: Vec<(String, String)> = to_write(&said)
        .into_iter()
        .filter(|(key, _)| keys.contains(&key.as_str()))
        .collect();
    assert_eq!(pins.len(), keys.len(), "{said}");
    let pin = |pins: &[(String, String)]| {
        fs::write(r.join("docs/knowledge/model.md"), document(pins)).unwrap();
        assert!(run(&["--root", &arg, "index"]).status.success());
        let said = stdout(&run(&["--root", &arg, "check"]));
        let line = said
            .split('`')
            .find(|part| part.starts_with("* **Knowledge**: knowledge/model.md@"))
            .map(str::to_string);
        if let Some(line) = line {
            fs::write(
                r.join("docs/log.md"),
                format!("# Log\n\n## 2026-10-02\n\n* Something\n  {line}\n"),
            )
            .unwrap();
        }
    };
    pin(&pins);
    let check = || {
        let out = run(&["--root", &arg, "check"]);
        (out.status.code(), stdout(&out))
    };
    let (code, said) = check();
    assert_eq!(code, Some(0), "{said}");

    // A change elsewhere in the file fails the file, not the definitions
    fs::write(
        r.join("domain/model.py"),
        fs::read_to_string(r.join("domain/model.py"))
            .unwrap()
            .replace("return 2", "return 3"),
    )
    .unwrap();
    let (code, said) = check();
    assert_eq!(code, Some(1), "{said}");
    let changed: Vec<String> = to_write(&said).into_iter().map(|(key, _)| key).collect();
    assert_eq!(changed, ["domain/model.py", "domain/"], "{said}");
    assert!(said.contains(
        "every knowledge document matches the code it follows:\n  knowledge/model.md follows domain/model.py, \
         which changed since the document was last reviewed"
    ), "{said}");

    // A decorator is part of its definition
    fs::write(
        r.join("domain/model.py"),
        fs::read_to_string(r.join("domain/model.py"))
            .unwrap()
            .replace("@dataclass", "@dataclass(frozen=True)"),
    )
    .unwrap();
    let (_, said) = check();
    assert!(
        to_write(&said)
            .iter()
            .any(|(key, _)| key == "domain/model.py::Song"),
        "{said}"
    );
    assert!(
        !to_write(&said)
            .iter()
            .any(|(key, _)| key == "domain/model.py::Song.play"),
        "{said}"
    );

    // A file added to a followed directory changes it
    let pinned = to_write(&said);
    pin(&pinned
        .iter()
        .cloned()
        .chain(
            pins.iter()
                .filter(|(k, _)| !pinned.iter().any(|(p, _)| p == k))
                .cloned(),
        )
        .collect::<Vec<_>>());
    let (code, said) = check();
    assert_eq!(code, Some(0), "{said}");
    fs::write(r.join("domain/more.py"), "MORE = 1\n").unwrap();
    let (code, said) = check();
    assert_eq!(code, Some(1), "{said}");
    assert_eq!(
        to_write(&said)
            .into_iter()
            .map(|(key, _)| key)
            .collect::<Vec<_>>(),
        ["domain/"],
        "{said}"
    );

    // A definition gone fails, as a dangling link does
    fs::write(r.join("domain/model.py"), "def other():\n    return 3\n").unwrap();
    let (_, said) = check();
    assert!(
        said.contains("follows domain/model.py::Song.play, which is not there"),
        "{said}"
    );

    // A key Rotproof cannot follow fails the document's format
    fs::write(
        r.join("docs/knowledge/model.md"),
        document(&[("domain/model.rs::f".into(), "00000000".into())]),
    )
    .unwrap();
    let (code, said) = check();
    assert_eq!(code, Some(1), "{said}");
    assert!(
        said.contains("only Python's definitions are read"),
        "{said}"
    );
}

#[test]
fn the_re_pin_of_followed_code_refuses_without_a_terminal() {
    let root = clean_repo();
    let r = root.path();
    let arg = root_arg(r);
    let doc = "---\ntype: Knowledge\ntitle: M\ndescription: D.\ntags: [a]\nstatus: stable\nfollows:\n  \
               domain/__init__.py: \"00000000\"\n---\n\n# Shape\n\nText.\n";
    fs::write(r.join("docs/knowledge/model.md"), doc).unwrap();
    let out = run(&["--root", &arg, "approve", "--reviewed"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("stdin is not a terminal"),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        fs::read_to_string(r.join("docs/knowledge/model.md")).unwrap(),
        doc
    );
    // One act at a time
    let out = run(&["--root", &arg, "approve", "--reviewed", "--prune"]);
    assert_eq!(out.status.code(), Some(2));
    let out = run(&["--root", &arg, "approve", "--reviewed", "a.py", "b"]);
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn follows_prints_the_hash_a_document_pins_and_writes_nothing() {
    let root = clean_repo();
    let r = root.path();
    let arg = root_arg(r);
    fs::write(r.join("domain/model.py"), "def play():\n    return 1\n").unwrap();
    let out = run(&[
        "--root",
        &arg,
        "follows",
        "domain/model.py::play",
        "domain/",
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let lines: Vec<String> = stdout(&out).lines().map(str::to_string).collect();
    assert_eq!(lines.len(), 2, "{lines:?}");
    assert!(
        lines[0].starts_with("domain/model.py::play: \""),
        "{lines:?}"
    );
    // Pasted into follows as printed, the document matches its code
    let follows: String = lines.iter().map(|line| format!("  {line}\n")).collect();
    fs::write(
        r.join("docs/knowledge/model.md"),
        format!(
            "---\ntype: Knowledge\ntitle: M\ndescription: D.\ntags: [a]\nstatus: stable\nfollows:\n{follows}---\n\n\
             # Shape\n\nIt plays.\n"
        ),
    )
    .unwrap();
    assert!(run(&["--root", &arg, "index"]).status.success());
    let said = stdout(&run(&["--root", &arg, "check"]));
    assert!(!said.contains("matches the code it follows"), "{said}");

    // What is not there, or not a key, fails with exit 2, after printing what could be hashed
    let out = run(&[
        "--root",
        &arg,
        "follows",
        "domain/model.py",
        "domain/gone.py",
        "domain\\x.py",
    ]);
    assert_eq!(out.status.code(), Some(2));
    assert!(
        stdout(&out).starts_with("domain/model.py: \""),
        "{}",
        stdout(&out)
    );
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("domain/gone.py: not there") && err.contains("not a path from the root"),
        "{err}"
    );
    // A key is needed
    assert_eq!(run(&["--root", &arg, "follows"]).status.code(), Some(2));
}
