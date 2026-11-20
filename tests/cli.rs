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

/// `rotproof create` with `args` (`--root <root> create ...`).
fn create(args: &[&str]) -> Output {
    run(args)
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// A repository of records only with an empty bundle: the directories of `docs/`, nothing in them.
fn repo() -> tempfile::TempDir {
    let root = declared("stack = \"none\"\nareas = [\"operations\"]\n");
    fs::create_dir_all(root.path().join("docs/knowledge")).unwrap();
    root
}

/// A repository with only `.config/rotproof.toml`, its files up to this version of Rotproof, and, for a Python project,
/// what every one of them has: the manifest and the lock file at the root, which the tests that are about something
/// else need to pass the check of the lock (`unlocked` makes a Python project that has neither).
fn declared(declaration: &str) -> tempfile::TempDir {
    let root = unlocked(declaration);
    if declaration.contains("stack = \"python\"") {
        let roots = unit_roots(declaration);
        let mut all = vec![""];
        all.extend(roots.iter().map(String::as_str));
        locked(root.path(), &all);
    }
    root
}

/// The directory of the manifest of each `[[unit]]` of `declaration`: its `root`, or its `path` when it says none.
fn unit_roots(declaration: &str) -> Vec<String> {
    let value = |table: &str, key: &str| {
        table.lines().find_map(|line| {
            let rest = line.trim_start().strip_prefix(key)?.trim_start();
            let rest = rest.strip_prefix('=')?.trim();
            Some(rest.trim_matches('"').to_string())
        })
    };
    declaration
        .split("[[unit]]")
        .skip(1)
        .filter_map(|table| {
            // Up to the next table that is not a unit's own, which ends the unit
            let table = table.split("\n[dependencies]").next().unwrap_or(table);
            value(table, "root").or_else(|| value(table, "path"))
        })
        .collect()
}

/// A repository with only `.config/rotproof.toml`, whatever its stack.
fn unlocked(declaration: &str) -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir_all(root.path().join(".config")).unwrap();
    fs::write(
        root.path().join(".config/rotproof.toml"),
        up_to_date(declaration),
    )
    .unwrap();
    point_at_the_guide(root.path());
    root
}

/// The line that points the project's agent at the guide, in a `CLAUDE.md` of the project's own: the answer to the
/// question `rotproof create` asks, which the tests that are about something else need to pass the check.
fn point_at_the_guide(root: &Path) {
    fs::write(root.join(POINTER_FILE), POINTER).unwrap();
}

/// Where [`point_at_the_guide`] writes, and what
const POINTER_FILE: &str = "CLAUDE.md";
const POINTER: &str = "The rules Rotproof keeps: @.rotproof/AGENTS.md\n";

/// A `pyproject.toml` in each of the directories `roots` of `root` ("" is the root), and a `uv.lock` at the root.
fn locked(root: &Path, roots: &[&str]) {
    for dir in roots {
        fs::create_dir_all(root.join(dir)).unwrap();
        fs::write(
            root.join(dir).join("pyproject.toml"),
            "[project]\nname = \"x\"\ndependencies = []\n",
        )
        .unwrap();
    }
    fs::write(root.join("uv.lock"), "").unwrap();
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

/// The line of the declaration that gives `stack` its fullest UI: every layer the stack can have. Empty for a
/// repository of records only, which has no UI.
fn ui_line(stack: &str) -> &'static str {
    match stack {
        "python" | "typescript" => "ui_design = \"atomic\"\n",
        "none" => "",
        _ => "ui_design = \"none\"\n",
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
    let root = declared("stack = \"python\"\nareas = [\"a\"]\nui_design = \"none\"\n");
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
    let text = "\u{feff}---\ntype: Knowledge\ntitle: X\ndescription: Y.\ntags: [a]\nstatus: stable\n---\n\n# Why\n\nZ.\n";
    fs::write(docs.join("knowledge/x.md"), text).unwrap();
    fs::write(
        docs.join("log.md"),
        "\u{feff}# Log\n\n## 2026-10-02\n\n* Something\n",
    )
    .unwrap();
    let out = run(&["--root", &root_arg(root.path()), "index"]);
    assert!(out.status.success(), "{}", stdout(&out));
    assert!(
        fs::read_to_string(docs.join("knowledge/index.md"))
            .unwrap()
            .contains("[X](x.md)")
    );
    let out = run(&["--root", &root_arg(root.path()), "check"]);
    assert!(out.status.success(), "{}", stdout(&out));
}

#[test]
fn a_knowledge_document_past_its_stale_after_is_warned_of_and_fails_nothing() {
    let root = clean_repo();
    let docs = root.path().join("docs");
    let with = |stale_after: &str| {
        format!(
            "---\ntype: Knowledge\ntitle: X\ndescription: Y.\ntags: [a]\nstatus: stable\nstale_after: {stale_after}\n\
             ---\n\n# Why\n\nZ.\n"
        )
    };
    fs::write(docs.join("knowledge/x.md"), with("2026-01-01")).unwrap();
    let out = run(&["--root", &root_arg(root.path()), "index"]);
    assert!(out.status.success(), "{}", stdout(&out));
    let out = run(&["--root", &root_arg(root.path()), "check"]);
    let said = stdout(&out);
    assert!(out.status.success(), "{said}");
    assert!(
        said.contains("past their stale_after (a warning, which fails nothing) (1):\n  knowledge/x.md: stale_after 2026-01-01 has come"),
        "{said}"
    );
    assert!(
        said.contains("the layers and every record keep the rules"),
        "{said}"
    );
    // A day far ahead warns of nothing
    fs::write(docs.join("knowledge/x.md"), with("9999-01-01")).unwrap();
    let out = run(&["--root", &root_arg(root.path()), "check"]);
    let said = stdout(&out);
    assert!(
        out.status.success() && !said.contains("stale_after"),
        "{said}"
    );
}

#[test]
fn every_kind_of_warning_is_printed_under_its_heading_in_order_and_fails_nothing() {
    let root = clean_repo();
    let r = root.path();
    let arg = root_arg(r);
    let late = "---\ntype: Knowledge\ntitle: X\ndescription: Y.\ntags: [a]\nstatus: stable\nstale_after: 2026-01-01\n\
                ---\n\n# Why\n\nZ.\n";
    fs::write(r.join("docs/knowledge/x.md"), late).unwrap();
    fs::write(
        r.join("docs/knowledge/w.md"),
        late.replace("title: X", "title: W"),
    )
    .unwrap();
    fs::create_dir_all(r.join("domain")).unwrap();
    fs::write(r.join("domain/model.py"), "X = 1  # pyright: basic\n").unwrap();
    fs::write(
        r.join("domain/song.py"),
        "Y = 2\n# mypy: disable-error-code=\"arg-type\"\n",
    )
    .unwrap();
    let out = run(&["--root", &arg, "index"]);
    assert!(out.status.success(), "{}", stdout(&out));
    let out = run(&["--root", &arg, "check"]);
    let said = stdout(&out);
    assert!(out.status.success(), "{said}");
    // The documents that are due come first, then the comments, each kind under one heading with its own count
    let due = said
        .find("to look at again, past their stale_after (a warning, which fails nothing) (2):\n")
        .unwrap_or_else(|| panic!("{said}"));
    let weakened = said
        .find("comments that may make the type checker check less, to look at (a warning, which fails nothing) (2):\n")
        .unwrap_or_else(|| panic!("{said}"));
    assert!(due < weakened, "{said}");
    assert_eq!(
        said.matches("(a warning, which fails nothing)").count(),
        2,
        "{said}"
    );
    assert!(
        said[due..weakened].contains("  knowledge/w.md: stale_after 2026-01-01 has come")
            && said[due..weakened].contains("  knowledge/x.md: stale_after 2026-01-01 has come"),
        "{said}"
    );
    // A detail of several lines stays under its warning
    assert!(
        said[weakened..].contains("  domain/model.py:1\n    X = 1  # pyright: basic\n"),
        "{said}"
    );
    assert!(
        said.ends_with("the layers and every record keep the rules\n"),
        "{said}"
    );
    // Only the comments: no heading of the documents
    fs::write(
        r.join("docs/knowledge/x.md"),
        late.replace("2026-01-01", "9999-01-01"),
    )
    .unwrap();
    fs::write(
        r.join("docs/knowledge/w.md"),
        late.replace("title: X", "title: W")
            .replace("2026-01-01", "9999-01-01"),
    )
    .unwrap();
    let out = run(&["--root", &arg, "check"]);
    let said = stdout(&out);
    assert!(out.status.success(), "{said}");
    assert!(
        !said.contains("stale_after")
            && said.contains("check less, to look at (a warning, which fails nothing) (2):"),
        "{said}"
    );
    // None: no heading at all
    fs::write(r.join("domain/model.py"), "X = 1\n").unwrap();
    fs::write(r.join("domain/song.py"), "Y = 2\n").unwrap();
    let out = run(&["--root", &arg, "check"]);
    let said = stdout(&out);
    assert!(
        out.status.success() && !said.contains("a warning"),
        "{said}"
    );
}

#[test]
fn a_knowledge_document_that_cannot_be_read_is_named_by_check_as_by_index_and_create() {
    let root = clean_repo();
    let r = root.path();
    let arg = root_arg(r);
    fs::write(r.join("docs/knowledge/bad.md"), [0xff, 0xfe, 0xfa]).unwrap();
    for command in [&["check"][..], &["index"], &["create", "--yes"]] {
        let mut args = vec!["--root", &arg];
        args.extend(command);
        let out = run(&args);
        let said = String::from_utf8_lossy(&out.stderr).into_owned();
        assert_eq!(out.status.code(), Some(2), "{command:?}: {said}");
        assert_eq!(
            said, "docs/knowledge/bad.md: stream did not contain valid UTF-8\n",
            "{command:?}"
        );
    }
}

#[test]
fn each_broken_rule_fails_under_its_check() {
    let doc = |body: &str| {
        format!(
            "---\ntype: Knowledge\ntitle: X\ndescription: Y.\ntags: [a]\nstatus: stable\n---\n\n# Why\n\n{body}\n"
        )
    };
    // Each case breaks one rule of a clean repository: (the check that must name it, what to write)
    let cases: [(&str, &str, String); 16] = [
        // A day guessed rather than read from the clock
        (
            "no day in the log is later than today",
            "docs/log.md",
            "# Log\n\n## 2999-01-01\n\n* Something\n".into(),
        ),
        (
            "the bundle is seen",
            "docs/knowledge/rules.md",
            String::new(),
        ),
        ("the bundle is seen", "docs/index.md", String::new()),
        (
            "every knowledge document keeps the format",
            "docs/knowledge/x.md",
            "# no frontmatter\n".into(),
        ),
        // The area of a document is declared
        (
            "every knowledge document keeps the format",
            "docs/knowledge/x.md",
            doc("Z.").replace("tags: [a]", "tags: [b]"),
        ),
        (
            "every link in docs/knowledge/ and the log resolves",
            "docs/knowledge/x.md",
            doc("As [the spec](/work/gone.md#its-heading) says."),
        ),
        // The log is history, and keeps its links to the records of work it named
        (
            "every link in docs/knowledge/ and the log resolves",
            "docs/log.md",
            "# Log\n\n## 2026-10-02\n\n* [gone](/work/gone.md)\n".into(),
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
        ("the log keeps its structure", "docs/log.md", String::new()),
        // A document Rotproof would not read
        (
            "every file in docs/knowledge/ is read",
            "docs/knowledge/deeper/x.md",
            doc("Z."),
        ),
        (
            "every generated file is up to date",
            "docs/knowledge/index.md",
            "edited by hand\n".into(),
        ),
        (
            "every generated file is up to date",
            "docs/index.md",
            "edited by hand\n".into(),
        ),
        // The rules a project reads are the rules its Rotproof checks
        (
            "every generated file is up to date",
            "docs/knowledge/rules.md",
            "---\ntype: Guide\ntitle: Knowledge rules\ndescription: Our own.\n---\n".into(),
        ),
        (
            "the areas are distinct headings",
            ".config/rotproof.toml",
            "stack = \"python\"\nareas = [\"a\", \"A\"]\nui_design = \"none\"\n".into(),
        ),
        // Without the areas, no document can be judged: the check says so instead of passing them
        (
            "the bundle is seen",
            ".config/rotproof.toml",
            "stack = \"python\"\nui_design = \"none\"\n".into(),
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

/// A link to a file of code, in any language, shows a name the check has looked up: code naming a definition (as a key
/// of `follows` does) or the file, and never prose.
#[test]
fn a_link_to_code_shows_a_name_that_is_checked() {
    let check = "every link in docs/knowledge/ and the log resolves";
    let root = clean_repo();
    let r = root.path();
    fs::create_dir_all(r.join("tools")).unwrap();
    fs::write(
        r.join("tools/table.rs"),
        "pub struct Table;\nimpl Table {\n    pub fn add(&self) {}\n}\n",
    )
    .unwrap();
    fs::write(
        r.join("tools/router.ts"),
        "export class Router { add() {} }\n",
    )
    .unwrap();
    fs::write(
        r.join("tools/helper.py"),
        "class Bundle:\n    def render(self):\n        pass\n",
    )
    .unwrap();
    let log = |entry: &str| {
        fs::write(
            r.join("docs/log.md"),
            format!("# Log\n\n## 2026-10-02\n\n* {entry}\n"),
        )
        .unwrap();
        let out = run(&["--root", &root_arg(r), "check"]);
        stdout(&out)
    };
    for entry in [
        "[`Table.add`](../tools/table.rs)",
        "[`Table`](../tools/table.rs)",
        "[`table.rs`](../tools/table.rs)",
        "[`tools/table.rs`](../tools/table.rs)",
        "[`Router.add`](../tools/router.ts)",
        "[`Bundle.render`](../tools/helper.py)",
    ] {
        let said = log(entry);
        assert!(!said.contains(check), "{entry}: {said}");
    }
    for (entry, why) in [
        // Prose names nothing a check has looked up
        (
            "[the table](../tools/table.rs)",
            "is code naming a definition, or the file or a path ending in it",
        ),
        (
            "[Router](../tools/router.ts)",
            "is code naming a definition, or the file or a path ending in it",
        ),
        (
            "[the bundle](../tools/helper.py)",
            "is code naming a definition, or the file or a path ending in it",
        ),
        // A method is named with its type
        (
            "[`render`](../tools/helper.py)",
            "a method is named with its type",
        ),
        (
            "[`Table.gone`](../tools/table.rs)",
            "no definition named Table.gone",
        ),
        // A path is its last parts whole
        (
            "[`ols/table.rs`](../tools/table.rs)",
            "no definition named ols/table.rs",
        ),
        // The file's name with another directory before it, as a record whose directory moved
        (
            "[`lib/table.rs`](../tools/table.rs)",
            "no definition named lib/table.rs",
        ),
        (
            "[`/tools/table.rs`](../tools/table.rs)",
            "no definition named /tools/table.rs",
        ),
        // A fragment is no name
        ("[`Table`](../tools/table.rs#L1)", "has no fragment"),
    ] {
        let said = log(entry);
        assert!(
            said.contains(check) && said.contains(why),
            "{entry}: {said}"
        );
    }
}

/// The rest of `docs/` is the project's: the records of work of a project made before 1.0, broken or not, and any
/// directory of its own, are not read.
#[test]
fn the_rest_of_docs_is_not_read() {
    let root = clean_repo();
    let docs = root.path().join("docs");
    for (path, text) in [
        ("work/index.md", "edited by hand\n"),
        ("work/rules.md", ""),
        (
            "work/x.md",
            "---\ntype: Work Item\nuntil: [gone]\n---\n\n[gone](/no_such.md)\n",
        ),
        ("plans/next.md", "# no frontmatter\n"),
        ("notes.md", "# no frontmatter\n"),
    ] {
        let full = docs.join(path);
        fs::create_dir_all(full.parent().unwrap()).unwrap();
        fs::write(full, text).unwrap();
    }
    let out = run(&["--root", &root_arg(root.path()), "check"]);
    assert!(out.status.success(), "{}", stdout(&out));
    // And rotproof index writes nothing there
    let out = run(&["--root", &root_arg(root.path()), "index"]);
    assert!(out.status.success(), "{}", stdout(&out));
    assert_eq!(
        fs::read_to_string(docs.join("work/index.md")).unwrap(),
        "edited by hand\n"
    );
}

#[test]
fn a_name_in_another_case_is_missing() {
    // Windows opens each of these under the name Rotproof looks for; Linux and GitHub do not. Renamed in two steps, as
    // a case-only rename is not a rename on Windows
    let cases = [
        (
            "docs/knowledge/rules.md",
            "docs/knowledge/Rules.md",
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
            "docs/knowledge/index.md",
            "docs/knowledge/Index.md",
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
fn index_writes_every_index_file() {
    let root = repo();
    let out = run(&["--root", &root_arg(root.path()), "index"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    for path in [
        "docs/index.md",
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
    assert!(!root.path().join("docs/work").exists());
    let knowledge = fs::read_to_string(root.path().join("docs/knowledge/index.md")).unwrap();
    assert!(knowledge.contains("# Guides\n\n* ["), "{knowledge}");
}

#[test]
fn index_run_again_writes_no_file_and_says_so() {
    let root = repo();
    let arg = root_arg(root.path());
    let first = run(&["--root", &arg, "index"]);
    assert!(first.status.success(), "{}", stdout(&first));
    assert!(
        !stdout(&first).contains("nothing written"),
        "{}",
        stdout(&first)
    );
    let paths = [
        "docs/index.md",
        "docs/knowledge/rules.md",
        "docs/knowledge/index.md",
    ];
    let before: Vec<_> = paths
        .iter()
        .map(|path| fs::read_to_string(root.path().join(path)).unwrap())
        .collect();
    let second = run(&["--root", &arg, "index"]);
    assert!(second.status.success(), "{}", stdout(&second));
    assert_eq!(
        stdout(&second),
        "nothing written: every index file is up to date\n"
    );
    for (path, before) in paths.iter().zip(&before) {
        assert_eq!(&fs::read_to_string(root.path().join(path)).unwrap(), before);
    }
    // One that is out of date is written, and named alone
    fs::write(root.path().join("docs/knowledge/index.md"), "stale\n").unwrap();
    let third = run(&["--root", &arg, "index"]);
    assert_eq!(stdout(&third), "wrote docs/knowledge/index.md\n");
    assert_eq!(
        fs::read_to_string(root.path().join("docs/knowledge/index.md")).unwrap(),
        before[2]
    );
}

#[test]
fn index_names_what_it_left_out() {
    let root = repo();
    fs::write(
        root.path().join("docs/knowledge/broken.md"),
        "# no frontmatter\n",
    )
    .unwrap();
    let out = run(&["--root", &root_arg(root.path()), "index"]);
    assert!(out.status.success());
    assert!(
        stdout(&out).contains("left out of the index, fix it: knowledge/broken.md: no frontmatter"),
        "{}",
        stdout(&out)
    );
    // The index says so too: a reader of the index alone would otherwise take its lists as whole
    let index = fs::read_to_string(root.path().join("docs/knowledge/index.md")).unwrap();
    assert!(
        index.contains(
            "# Left out\n\n* [broken.md](broken.md) - Not listed or counted below until it is fixed: no frontmatter"
        ),
        "{index}"
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
fn before_is_no_command_and_fails_as_every_unknown_command_does() {
    let root = repo();
    let r = root_arg(root.path());
    for args in [
        vec!["--root", &r, "before", "next-milestone"],
        vec!["--root", &r, "before", "next-milestone", "--all"],
        vec!["--root", &r, "before"],
        vec!["--root", "no/such/directory", "before", "x"],
    ] {
        let out = run(&args);
        assert_eq!(out.status.code(), Some(2), "{args:?}");
        assert_eq!(stdout(&out), "", "{args:?}");
        let said = String::from_utf8_lossy(&out.stderr);
        assert!(
            said.contains("unrecognized subcommand 'before'"),
            "{args:?}: {said}"
        );
    }
    assert!(!stdout(&run(&["--help"])).contains("  before"));
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
                "composition/__init__.py",
                "ui/atoms/__init__.py",
                "infrastructure/inbound/__init__.py",
            ],
            // The entry file is no layer, so it is not made
            &["__main__.py", "handler", "utils"],
        ),
        (
            "typescript",
            &[
                "src/composition/index.ts",
                "src/ui/pages/index.ts",
                "src/protocol/outbound/index.ts",
            ],
            &["src/main.tsx"],
        ),
        (
            "rust",
            &[
                "crates/composition/src/lib.rs",
                "crates/domain/Cargo.toml",
                "crates/infrastructure/src/inbound/mod.rs",
            ],
            &["crates/ui"],
        ),
    ];
    for (stack, made, not_made) in stacks {
        let root = declared(&format!(
            "stack = \"{stack}\"\nareas = [\"a\"]\n{}",
            ui_line(stack)
        ));
        let arg = root_arg(root.path());
        let first = create(&["--root", &arg, "create", "--yes"]);
        assert!(first.status.success(), "{stack}: {}", stdout(&first));
        for path in made
            .iter()
            .chain(&["docs/log.md", "docs/knowledge/rules.md", "docs/index.md"])
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
const PROJECT_FILES: [&str; 1] = ["README.md"];

#[test]
fn create_writes_the_projects_files_once() {
    // (stack, where it installs Rotproof from)
    for (stack, channel) in [
        ("python", "pypi"),
        ("none", "release"),
        ("typescript", "npm"),
        ("rust", "release"),
    ] {
        let parent = tempfile::tempdir().unwrap();
        let root = parent.path().join("my-app");
        fs::create_dir_all(root.join(".config")).unwrap();
        let absent = if stack == "none" {
            ""
        } else {
            "absent = [\"application\"]\n"
        };
        fs::write(
            root.join(".config/rotproof.toml"),
            up_to_date(&format!(
                "stack = \"{stack}\"\nareas = [\"a\"]\n{}{absent}",
                ui_line(stack)
            )),
        )
        .unwrap();
        let arg = root_arg(&root);
        // Every file is written, and the question of the agent is asked, which stops it with 2
        let out = create(&["--root", &arg, "create", "--yes"]);
        assert_eq!(out.status.code(), Some(2), "{stack}: {}", stdout(&out));
        assert!(stdout(&out).contains("ask the person"), "{stack}");
        for path in ["README.md", ".gitignore", ".gitattributes"] {
            let text = fs::read_to_string(root.join(path))
                .unwrap_or_else(|_| panic!("{stack}: {path} was not written"));
            for placeholder in ["{name}", "{development}", "{version}", "{stack}"] {
                assert!(!text.contains(placeholder), "{stack}: {path}: {text}");
            }
        }
        // Nothing an agent reads by itself: what an agent is told is the project's
        for path in ["AGENTS.md", "CLAUDE.md"] {
            assert!(!root.join(path).exists(), "{stack}: {path}");
        }
        point_at_the_guide(&root);
        assert!(
            fs::read_to_string(root.join("README.md"))
                .unwrap()
                .starts_with("# my-app\n")
        );
        let requirements = root.join("requirements-dev.txt");
        let workflow = root.join(".github/workflows/ci.yml");
        let readme = fs::read_to_string(root.join("README.md")).unwrap();
        if channel == "pypi" {
            // The pin is in the dev group of the manifest, which a lock file locks: no requirements file, and the
            // workflow of uv, the tool of a project that has no lock file yet
            assert!(!requirements.exists(), "{stack}");
            let pin = format!("dev = [\"rotproof=={}\"]\n", env!("CARGO_PKG_VERSION"));
            let manifest = fs::read_to_string(root.join("pyproject.toml")).unwrap();
            assert!(
                manifest.contains("[dependency-groups]\n") && manifest.contains(&pin),
                "{manifest}"
            );
            assert!(manifest.contains("name = \"my-app\""), "{manifest}");
            let workflow = fs::read_to_string(&workflow).unwrap();
            for part in [
                "timeout-minutes:",
                "- run: uv sync --locked",
                "- run: uv run --no-sync rotproof check",
            ] {
                assert!(workflow.contains(part), "{stack}: {part}: {workflow}");
            }
            assert!(!workflow.contains("requirements"), "{workflow}");
            assert!(readme.contains("uv sync"), "{readme}");
            let ignore = fs::read_to_string(root.join(".gitignore")).unwrap();
            assert!(ignore.lines().any(|line| line == ".venv/"), "{ignore}");
            // The lock file is what no command here writes: a tool makes it from the manifest
            assert!(
                stdout(&out).contains(
                    "not written: the lock file (uv.lock, poetry.lock, pdm.lock or pylock.toml)"
                ),
                "{}",
                stdout(&out)
            );
            assert!(!root.join("uv.lock").exists(), "{stack}");
            assert_eq!(
                run(&["--root", &arg, "check"]).status.code(),
                Some(1),
                "{stack}"
            );
            fs::write(root.join("uv.lock"), "").unwrap();
        } else if channel == "release" {
            // No Python: the version is pinned in the workflow, which takes the release's archive and checks it
            assert!(!requirements.exists(), "{stack}");
            let workflow = fs::read_to_string(&workflow).unwrap();
            let version = env!("CARGO_PKG_VERSION");
            for part in [
                format!("ROTPROOF_VERSION: \"{version}\""),
                "gh release download \"v$ROTPROOF_VERSION\" --repo secta113/rotproof".into(),
                "sha256sum --check --ignore-missing SHA256SUMS".into(),
                "timeout-minutes:".into(),
                "- run: rotproof check".into(),
            ] {
                assert!(workflow.contains(&part), "{stack}: {part}: {workflow}");
            }
            assert!(workflow.contains("${{ github.token }}"), "{workflow}");
            assert!(
                readme.contains(&format!(
                    "gh release download v{version} --repo secta113/rotproof"
                )),
                "{stack}: {readme}"
            );
            assert!(!stdout(&out).contains("not written"), "{}", stdout(&out));
        } else {
            // npm: the pin goes in package.json, the project's own file, so create writes the workflow, which runs
            // only the Rotproof the lockfile names, and says how to add the pin
            assert_eq!(channel, "npm");
            assert!(!requirements.exists(), "{stack}");
            assert!(!root.join("package.json").exists(), "{stack}");
            let workflow = fs::read_to_string(&workflow).unwrap();
            for part in [
                "timeout-minutes:",
                "- run: npm ci",
                "- run: npx --no -- rotproof check",
            ] {
                assert!(workflow.contains(part), "{stack}: {part}: {workflow}");
            }
            let pin = format!("npm install -D -E rotproof@{}", env!("CARGO_PKG_VERSION"));
            assert!(readme.contains(&pin), "{stack}: {readme}");
            assert!(
                stdout(&out).contains(&format!(
                    "not written: the pin in package.json, which is the project's: until it pins Rotproof (`{pin}`)"
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
            for crate_name in ["composition", "infrastructure", "protocol", "domain"] {
                assert!(
                    metadata.contains(&format!("\"name\":\"{crate_name}\"")),
                    "{crate_name}: {metadata}"
                );
            }
            assert!(!metadata.contains("\"name\":\"application\""), "{metadata}");
        }
        assert!(run(&["--root", &arg, "check"]).status.success());

        // From then on the files are the project's: an edit stays, and only a missing file is written again
        fs::write(root.join("README.md"), "mine\n").unwrap();
        fs::remove_file(root.join(".gitattributes")).unwrap();
        let out = create(&["--root", &arg, "create"]);
        assert_eq!(
            fs::read_to_string(root.join("README.md")).unwrap(),
            "mine\n"
        );
        assert!(
            stdout(&out).contains("wrote .gitattributes"),
            "{}",
            stdout(&out)
        );
        assert!(
            !stdout(&out).contains("wrote README.md"),
            "{}",
            stdout(&out)
        );
    }
}

#[test]
fn the_guide_is_rotproofs_and_check_fails_until_create_rewrites_it() {
    for stack in ["python", "typescript", "rust", "none"] {
        let root = declared(&format!(
            "stack = \"{stack}\"\nareas = [\"a\"]\n{}",
            ui_line(stack)
        ));
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
                "out of date, run `rotproof create`",
            ),
            (None, "missing, run `rotproof create`"),
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
fn a_guide_behind_an_upgrade_is_written_by_init_and_one_changed_under_the_same_version_by_create() {
    // The files are up to an older Rotproof: the upgrade is init, which also sets files
    let root = declared(
        "stack = \"none\"
areas = [\"a\"]
files = \"1.0.0\"
",
    );
    let arg = root_arg(root.path());
    let out = run(&["--root", &arg, "check"]);
    assert_eq!(out.status.code(), Some(1), "{}", stdout(&out));
    assert!(
        stdout(&out).contains(
            "Rotproof's guide is up to date:
  missing, run `rotproof init`: .rotproof/AGENTS.md"
        ),
        "{}",
        stdout(&out)
    );
    // A declaration with no files is the first release's
    fs::write(
        root.path().join(".config/rotproof.toml"),
        "stack = \"none\"
areas = [\"a\"]
",
    )
    .unwrap();
    let out = run(&["--root", &arg, "check"]);
    assert!(
        stdout(&out).contains("missing, run `rotproof init`: .rotproof/AGENTS.md"),
        "{}",
        stdout(&out)
    );
    // Brought to this version, only the guide is behind, and create writes it
    let out = create(&["--root", &arg, "init"]);
    assert!(out.status.success(), "{}", stdout(&out));
    fs::write(
        root.path().join(".rotproof/AGENTS.md"),
        "old
",
    )
    .unwrap();
    let out = run(&["--root", &arg, "check"]);
    assert!(
        stdout(&out).contains("out of date, run `rotproof create`: .rotproof/AGENTS.md"),
        "{}",
        stdout(&out)
    );
    assert!(
        !stdout(&out).contains("run `rotproof init`"),
        "{}",
        stdout(&out)
    );
}

const RUNNING: &str = ".rotproof/running.md";

#[test]
fn the_page_on_running_rotproof_is_written_and_check_fails_for_it_until_create_rewrites_it() {
    for stack in ["python", "typescript", "rust", "none"] {
        let root = declared(&format!(
            "stack = \"{stack}\"\nareas = [\"a\"]\n{}",
            ui_line(stack)
        ));
        let r = root.path();
        let arg = root_arg(r);
        let out = create(&["--root", &arg, "create", "--yes"]);
        assert!(out.status.success(), "{stack}: {}", stdout(&out));
        assert!(
            stdout(&out).contains(&format!("wrote {RUNNING}")),
            "{stack}: {}",
            stdout(&out)
        );
        // Nowhere an agent loads by itself
        assert!(!r.join(".claude/skills").exists(), "{stack}");
        assert!(!r.join(".agents").exists(), "{stack}");
        let written = fs::read_to_string(r.join(RUNNING)).unwrap();
        // One text for every stack, and it carries no version, so an upgrade rewrites it only when it changed
        assert!(written.starts_with("<!-- Generated by"), "{stack}");
        assert!(!written.contains(env!("CARGO_PKG_VERSION")), "{stack}");
        assert!(run(&["--root", &arg, "check"]).status.success(), "{stack}");
        // The second run writes nothing of it
        let again = create(&["--root", &arg, "create"]);
        assert!(!stdout(&again).contains(RUNNING), "{}", stdout(&again));
        for (hurt, says) in [("edited by hand\n", "out of date"), ("", "missing")] {
            if hurt.is_empty() {
                fs::remove_file(r.join(RUNNING)).unwrap();
            } else {
                fs::write(r.join(RUNNING), hurt).unwrap();
            }
            let out = run(&["--root", &arg, "check"]);
            assert_eq!(out.status.code(), Some(1), "{stack}: {}", stdout(&out));
            assert!(
                stdout(&out).contains(&format!(
                    "Rotproof's page on running it is up to date:\n  {says}, run `rotproof create`: {RUNNING}\n"
                )),
                "{stack}: {}",
                stdout(&out)
            );
            let out = create(&["--root", &arg, "create"]);
            assert!(
                stdout(&out).contains(&format!("wrote {RUNNING}")),
                "{stack}: {}",
                stdout(&out)
            );
            assert_eq!(fs::read_to_string(r.join(RUNNING)).unwrap(), written);
            assert!(run(&["--root", &arg, "check"]).status.success());
        }
    }
}

#[test]
fn a_project_made_before_the_page_on_running_rotproof_gets_it_from_init_and_is_told_so_by_check() {
    let root = declared("stack = \"none\"\nareas = [\"a\"]\nfiles = \"1.0.0\"\n");
    let r = root.path();
    let arg = root_arg(r);
    let out = run(&["--root", &arg, "check"]);
    assert_eq!(out.status.code(), Some(1), "{}", stdout(&out));
    assert!(
        stdout(&out).contains(&format!("missing, run `rotproof init`: {RUNNING}")),
        "{}",
        stdout(&out)
    );
    let out = run(&["--root", &arg, "init"]);
    assert!(out.status.success(), "{}", stdout(&out));
    assert!(
        stdout(&out).contains(&format!("wrote {RUNNING}")),
        "{}",
        stdout(&out)
    );
    assert!(r.join(RUNNING).is_file());
    assert!(run(&["--root", &arg, "check"]).status.success());
}

#[test]
fn the_page_on_running_rotproof_names_only_commands_there_are() {
    let root = declared("stack = \"none\"\nareas = [\"a\"]\n");
    let r = root.path();
    let arg = root_arg(r);
    assert!(create(&["--root", &arg, "create"]).status.success());
    let page = fs::read_to_string(r.join(RUNNING)).unwrap();
    let mut named: Vec<&str> = page
        .split("`rotproof ")
        .skip(1)
        .filter_map(|rest| rest.split(|c: char| !c.is_ascii_alphabetic()).next())
        .filter(|word| !word.is_empty())
        .collect();
    named.sort_unstable();
    named.dedup();
    assert!(named.len() >= 6, "{named:?}");
    for command in named {
        let out = run(&[command, "--help"]);
        assert!(
            out.status.success(),
            "{RUNNING} says `rotproof {command}`, which is no command: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

#[test]
fn the_guide_sends_to_the_page_on_running_rotproof_and_does_not_say_what_it_says() {
    for stack in ["python", "none"] {
        let root = declared(&format!(
            "stack = \"{stack}\"\nareas = [\"a\"]\n{}",
            ui_line(stack)
        ));
        let r = root.path();
        let arg = root_arg(r);
        assert!(
            create(&["--root", &arg, "create", "--yes"])
                .status
                .success()
        );
        let guide = fs::read_to_string(r.join(".rotproof/AGENTS.md")).unwrap();
        assert!(guide.contains(&format!("Read `{RUNNING}`")), "{stack}");
        assert!(r.join(RUNNING).is_file());
        // What the page says is not said in the guide too
        assert!(!guide.contains("commit of its own"), "{stack}: {guide}");
        assert!(!guide.contains("rotproof follows knowledge/"), "{stack}");
    }
}

#[test]
fn an_upgrade_asks_whether_the_agent_reads_the_guide_until_the_project_answers_and_a_decline_is_kept()
 {
    let root = tempfile::tempdir().unwrap();
    let r = root.path();
    let arg = root_arg(r);
    fs::create_dir_all(r.join(".config")).unwrap();
    let declaration = "stack = \"none\"\nareas = [\"a\"]\nfiles = \"1.0.0\"\n";
    fs::write(r.join(".config/rotproof.toml"), declaration).unwrap();
    fs::write(r.join("AGENTS.md"), "# Mine\n\nNo pointer here.\n").unwrap();
    // init does the upgrade, sets files, and then asks, naming the file that is there; it exits 2
    let out = run(&["--root", &arg, "init"]);
    assert_eq!(out.status.code(), Some(2), "{}", stdout(&out));
    let said = stdout(&out);
    assert!(
        said.contains("the project's files are up to")
            && said.contains("ask the person: should this project's agent read")
            && said.contains("(here: AGENTS.md)")
            && !said.contains("next: run `rotproof check`"),
        "{said}"
    );
    let written = fs::read_to_string(r.join(".config/rotproof.toml")).unwrap();
    assert!(
        written.contains(&format!("files = \"{}\"", env!("CARGO_PKG_VERSION"))),
        "{written}"
    );
    // The project's AGENTS.md is never written
    assert_eq!(
        fs::read_to_string(r.join("AGENTS.md")).unwrap(),
        "# Mine\n\nNo pointer here.\n"
    );
    let out = run(&["--root", &arg, "check"]);
    assert_eq!(out.status.code(), Some(1), "{}", stdout(&out));
    assert!(
        stdout(&out).contains(
            "the project answered whether its agent reads Rotproof's guide:\n  no AGENTS.md or CLAUDE.md names \
             .rotproof/AGENTS.md, and declined in .config/rotproof.toml does not name \"guide-pointer\""
        ),
        "{}",
        stdout(&out)
    );
    // The person says no: declined, and from then on nothing asks or fails for it
    fs::write(
        r.join(".config/rotproof.toml"),
        written.replace("declined = []", "declined = [\"guide-pointer\"]"),
    )
    .unwrap();
    let out = run(&["--root", &arg, "check"]);
    assert!(out.status.success(), "{}", stdout(&out));
    for args in [["--root", &arg, "init"], ["--root", &arg, "create"]] {
        let out = run(&args);
        assert!(out.status.success(), "{:?}: {}", args, stdout(&out));
        assert!(!stdout(&out).contains("ask the person"), "{}", stdout(&out));
    }
}

#[test]
fn a_project_whose_claude_md_already_imports_the_guide_is_not_asked() {
    // As every project rotproof create made before it wrote no CLAUDE.md
    let root = tempfile::tempdir().unwrap();
    let r = root.path();
    let arg = root_arg(r);
    fs::create_dir_all(r.join(".config")).unwrap();
    fs::write(
        r.join(".config/rotproof.toml"),
        "stack = \"none\"\nareas = [\"a\"]\nfiles = \"1.0.0\"\n",
    )
    .unwrap();
    fs::write(r.join("CLAUDE.md"), "@AGENTS.md\n@.rotproof/AGENTS.md\n").unwrap();
    let out = run(&["--root", &arg, "init"]);
    assert!(out.status.success(), "{}", stdout(&out));
    assert!(!stdout(&out).contains("ask the person"), "{}", stdout(&out));
    assert!(run(&["--root", &arg, "check"]).status.success());
}

#[test]
fn guide_without_a_stack_prints_what_create_writes_for_the_declared_ui() {
    for (stack, ui) in [
        ("typescript", "none"),
        ("typescript", "atomic"),
        ("python", "none"),
        ("python", "atomic"),
        ("rust", "none"),
    ] {
        let root = declared(&format!(
            "stack = \"{stack}\"\nareas = [\"a\"]\nui_design = \"{ui}\"\n"
        ));
        let arg = root_arg(root.path());
        assert!(
            create(&["--root", &arg, "create", "--yes"])
                .status
                .success(),
            "{stack} {ui}"
        );
        let written = fs::read_to_string(root.path().join(".rotproof/AGENTS.md")).unwrap();
        let out = run(&["--root", &arg, "guide"]);
        assert!(out.status.success(), "{stack} {ui}");
        assert_eq!(stdout(&out), written, "{stack} {ui}");
        // Named with the same stack and UI, it is the same text
        let named = run(&["--root", &arg, "guide", "--stack", stack, "--ui-design", ui]);
        assert_eq!(stdout(&named), written, "{stack} {ui}");
    }
    // The two UIs of a stack are two texts, so the comparison above says something
    let ts = |ui: &str| {
        let root = declared(&format!(
            "stack = \"typescript\"\nareas = [\"a\"]\nui_design = \"{ui}\"\n"
        ));
        stdout(&run(&["--root", &root_arg(root.path()), "guide"]))
    };
    assert_ne!(ts("none"), ts("atomic"));
    // A declaration from before `ui` has the fullest the stack offers, as the guide of a stack alone does
    let root = declared("stack = \"typescript\"\nareas = [\"a\"]\n");
    let out = run(&["--root", &root_arg(root.path()), "guide"]);
    assert_eq!(stdout(&out), ts("atomic"));
}

#[test]
fn guide_takes_the_ui_with_a_stack_as_init_does_and_needs_no_project() {
    let root = tempfile::tempdir().unwrap();
    let arg = root_arg(root.path());
    let guide = |args: &[&str]| run(&[&["--root", &arg, "guide"], args].concat());
    let none = guide(&["--stack", "typescript", "--ui-design", "none"]);
    let atomic = guide(&["--stack", "typescript", "--ui-design", "atomic"]);
    assert!(none.status.success() && atomic.status.success());
    assert!(
        stdout(&atomic).contains("| `ui.pages` |")
            && stdout(&atomic).contains("| `presentation.models` |"),
        "{}",
        stdout(&atomic)
    );
    assert!(
        !stdout(&none).contains("| `ui.pages` |") && !stdout(&none).contains("| `presentation"),
        "{}",
        stdout(&none)
    );
    // --ui, the name the field had, is the same flag
    let named_before = guide(&["--stack", "typescript", "--ui", "none"]);
    assert_eq!(stdout(&named_before), stdout(&none));
    // Without --ui-design it is the fullest, as before
    let fullest = guide(&["--stack", "typescript"]);
    assert_eq!(stdout(&fullest), stdout(&atomic));
    // The values and the checks of init, in its words, and an exit of 2
    for (args, says) in [
        (
            &["--stack", "typescript", "--ui-design", "x"][..],
            "unknown UI \"x\" for the typescript stack (known: atomic, none)",
        ),
        (
            &["--stack", "rust", "--ui-design", "atomic"][..],
            "unknown UI \"atomic\" for the rust stack (known: none)",
        ),
        (
            &["--stack", "none", "--ui-design", "none"][..],
            "stack \"none\" has no layers, so it has no UI",
        ),
    ] {
        let out = guide(args);
        assert_eq!(out.status.code(), Some(2), "{args:?}");
        assert!(
            String::from_utf8_lossy(&out.stderr).contains(says),
            "{args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(out.stdout.is_empty(), "{args:?}");
    }
    // A UI means a stack: the declaration's is not the one to pick it for
    let out = guide(&["--ui-design", "none"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("--stack"));
}

/// The units of the example in Rotproof's README, under `## Units`: the first ```toml block there.
fn readme_units() -> String {
    let readme =
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("README.md")).unwrap();
    let section = readme.split("\n## Units\n").nth(1).unwrap();
    let block = section.split("```toml\n").nth(1).unwrap();
    block.split("```").next().unwrap().to_string()
}

/// The commented example of units `rotproof init` writes at the end of a declaration, taken out of its comment.
fn uncommented_units(declaration: &str) -> String {
    let at = declaration.find("# [[unit]]").unwrap();
    let units: Vec<&str> = declaration[at..]
        .lines()
        .map(|line| {
            line.strip_prefix("# ")
                .unwrap_or(line.trim_start_matches('#'))
        })
        .collect();
    format!("{}{}\n", &declaration[..at], units.join("\n"))
}

#[test]
fn the_units_of_the_readme_and_of_the_declaration_init_writes_pass_the_check_once_created() {
    // As a person would take them: the README's Python monorepo, and the example init writes for each stack
    let readme = up_to_date("stack = \"python\"\nareas = [\"a\"]\nui_design = \"none\"\n")
        + "\n"
        + &readme_units();
    let mut declarations = vec![("python (README)", readme)];
    for stack in ["python", "typescript", "rust"] {
        let root = tempfile::tempdir().unwrap();
        let arg = root_arg(root.path());
        let out = run(&[
            "--root",
            &arg,
            "init",
            "--stack",
            stack,
            "--ui-design",
            "none",
        ]);
        assert!(out.status.success(), "{stack}: {}", error_of(&out));
        let written = fs::read_to_string(root.path().join(".config/rotproof.toml")).unwrap();
        declarations.push((stack, uncommented_units(&written)));
    }
    for (name, declaration) in declarations {
        let root = unlocked(&declaration);
        let arg = root_arg(root.path());
        let out = create(&["--root", &arg, "create", "--yes"]);
        assert!(
            out.status.success(),
            "{name}: {}\n{}",
            stdout(&out),
            error_of(&out)
        );
        if declaration.contains("stack = \"python\"") {
            // Each unit's manifest is written at its root, and only the lock file, which a tool makes, is left
            for manifest in ["packages/billing/pyproject.toml", "apps/cli/pyproject.toml"] {
                let text = fs::read_to_string(root.path().join(manifest));
                assert!(
                    text.is_ok_and(|t| t.starts_with("[project]\n")),
                    "{name}: {manifest}"
                );
            }
            fs::write(root.path().join("uv.lock"), "").unwrap();
        }
        let out = run(&["--root", &arg, "check"]);
        assert!(
            out.status.success(),
            "{name}:\n{declaration}\n{}",
            stdout(&out)
        );
    }
}

#[test]
fn create_respects_absent_and_leaves_what_it_does_not_own() {
    let root = declared(
        "stack = \"python\"\nareas = [\"a\"]\nui_design = \"atomic\"\nabsent = [\"ui.templates\", \"infrastructure\"]\n",
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

    // Once made, a run that makes no layer needs no --yes; the layers of a UI declared later are listed alone
    let root = declared("stack = \"python\"\nareas = [\"a\"]\nui_design = \"none\"\n");
    let arg = root_arg(root.path());
    assert!(
        create(&["--root", &arg, "create", "--yes"])
            .status
            .success()
    );
    assert!(create(&["--root", &arg, "create"]).status.success());
    declare(
        root.path(),
        "stack = \"python\"\nareas = [\"a\"]\nui_design = \"atomic\"\n",
    );
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
            "  presentation/ (presentation)",
            "  presentation/models/ (presentation.models)",
            "  presentation/display/ (presentation.display)",
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

/// A knowledge document that keeps every rule, titled `title`, with `tag` as its area.
fn knowledge(title: &str, tag: &str) -> String {
    format!(
        "---\ntype: Knowledge\ntitle: {title}\ndescription: Y.\ntags: [{tag}]\nstatus: stable\n---\n\n# Why\n\nZ.\n"
    )
}

#[test]
fn create_adds_the_fields_the_declaration_lacks() {
    // A declaration an older Rotproof wrote, before areas and ui existed, with comments and values of the project's own
    let older = &up_to_date(
        "# Ours\nstack = \"python\"\nabsent = [\"ui.templates\"]  # no templates here\n",
    );
    let root = declared(
        "stack = \"python\"\nareas = [\"a\"]\nui_design = \"atomic\"\nabsent = [\"ui.templates\"]\n",
    );
    let r = root.path();
    let arg = root_arg(r);
    assert!(
        create(&["--root", &arg, "create", "--yes"])
            .status
            .success()
    );
    let documents = [
        ("one.md", knowledge("One", "operations")),
        ("two.md", knowledge("Two", "billing")),
        ("three.md", knowledge("Three", "records")),
    ];
    for (name, text) in &documents {
        fs::write(r.join("docs/knowledge").join(name), text).unwrap();
    }
    // A guide's tags are not areas
    fs::write(
        r.join("docs/knowledge/guide.md"),
        "---\ntype: Guide\ntitle: G\ndescription: H.\ntags: [howto]\n---\n\nText.\n",
    )
    .unwrap();
    // Nor are the tags of the records of work Rotproof no longer reads
    fs::create_dir_all(r.join("docs/work")).unwrap();
    fs::write(
        r.join("docs/work/old.md"),
        "---\ntype: Work Item\ntags: [work]\n---\n",
    )
    .unwrap();
    fs::write(r.join(".config/rotproof.toml"), older).unwrap();

    let check = run(&["--root", &arg, "check"]);
    assert_eq!(check.status.code(), Some(1));
    assert!(
        stdout(&check).contains(
            "missing field `areas`: run `rotproof init` after an upgrade of Rotproof, `rotproof create` otherwise; \
             either adds it"
        ),
        "{}",
        stdout(&check)
    );

    let out = create(&["--root", &arg, "create"]);
    assert!(out.status.success(), "{}", stdout(&out));
    assert!(
        stdout(&out).contains(
            "added to .config/rotproof.toml: areas = [\"billing\", \"operations\", \"records\"]"
        ),
        "{}",
        stdout(&out)
    );
    // The UI whose layers the tree holds: ui/ is there
    assert!(
        stdout(&out).contains("ui_design = \"atomic\""),
        "{}",
        stdout(&out)
    );
    let now = fs::read_to_string(r.join(".config/rotproof.toml")).unwrap();
    // What was there is kept, byte for byte, and the new field says what it is and where its value came from
    assert!(now.starts_with(older), "{now}");
    assert!(
        now.contains("# The areas the knowledge documents are grouped by")
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
fn a_declaration_without_ui_names_init_for_the_upgrade_and_create_otherwise() {
    // As a project made before ui existed has it: an older version in files, and no ui
    for files in ["1.0.0-rc.2", env!("CARGO_PKG_VERSION")] {
        let root = declared(&format!(
            "stack = \"python\"\nareas = []\nfiles = \"{files}\"\n"
        ));
        let arg = root_arg(root.path());
        let check = run(&["--root", &arg, "check"]);
        assert_eq!(check.status.code(), Some(1), "{}", stdout(&check));
        assert!(
            stdout(&check).contains(
                "missing field `ui_design`: run `rotproof init` after an upgrade of Rotproof, `rotproof create` otherwise; \
                 either adds it"
            ),
            "{files}: {}",
            stdout(&check)
        );
    }
    // The command it names does the upgrade: ui added and files raised, which create alone would not do
    let root = declared("stack = \"python\"\nareas = []\nfiles = \"1.0.0-rc.2\"\n");
    let arg = root_arg(root.path());
    let out = create(&["--root", &arg, "init"]);
    assert!(out.status.success(), "{}", stdout(&out));
    let now = fs::read_to_string(root.path().join(".config/rotproof.toml")).unwrap();
    assert!(
        now.contains("ui_design = ")
            && now.contains(&format!("files = \"{}\"", env!("CARGO_PKG_VERSION"))),
        "{now}"
    );
}

#[test]
fn a_declaration_that_names_ui_is_renamed_by_init_and_check_says_so_until_then() {
    // As a project made before the field was ui_design has it, checked out with CRLF
    let older = "stack = \"python\"\r\nareas = []\r\n# Our screens\r\nui = \"none\"  # no screens\r\nfiles = \"2.0.0-rc.1\"\r\n";
    let root = declared(older);
    let arg = root_arg(root.path());
    let path = root.path().join(".config/rotproof.toml");
    fs::write(&path, older).unwrap();
    let check = run(&["--root", &arg, "check"]);
    assert_eq!(check.status.code(), Some(1), "{}", stdout(&check));
    assert!(
        stdout(&check).contains(
            "unknown field `ui`, expected one of "
        ) && stdout(&check).contains(
            "the field ui is named ui_design from Rotproof 2.0.0-rc.2: run `rotproof init`, which renames it"
        ),
        "{}",
        stdout(&check)
    );
    // A UI named on the command line is set against the value the field holds, under either name
    let out = create(&["--root", &arg, "init", "--ui", "atomic"]);
    assert_eq!(out.status.code(), Some(2), "{}", stdout(&out));
    assert_eq!(fs::read_to_string(&path).unwrap(), older);
    // The upgrade renames it, and keeps the value, the comments and the line endings
    let out = create(&["--root", &arg, "init"]);
    assert!(
        stdout(&out).contains("renamed in .config/rotproof.toml: ui to ui_design"),
        "{}{}",
        stdout(&out),
        String::from_utf8_lossy(&out.stderr)
    );
    let now = fs::read_to_string(&path).unwrap();
    assert!(
        now.contains("# Our screens\r\nui_design = \"none\"  # no screens\r\n")
            && !now.contains("\nui ="),
        "{now}"
    );
    let check = run(&["--root", &arg, "check"]);
    assert!(
        !stdout(&check).contains("unknown field"),
        "{}",
        stdout(&check)
    );
    // Once renamed, nothing is renamed again
    let out = create(&["--root", &arg, "init"]);
    assert!(!stdout(&out).contains("renamed in"), "{}", stdout(&out));
}

#[test]
fn create_keeps_the_line_endings_and_the_byte_order_mark_of_the_declaration() {
    // Checked out with CRLF, as git does on Windows, and saved by a tool that writes a byte order mark or not: only
    // the added lines are new
    for older in [
        "# Ours\r\nstack = \"none\"\r\n",
        "\u{feff}# Ours\r\nstack = \"none\"\r\n",
    ] {
        let root = repo();
        fs::write(root.path().join(".config/rotproof.toml"), older).unwrap();
        let out = create(&["--root", &root_arg(root.path()), "create"]);
        assert!(out.status.success(), "{}", stdout(&out));
        let now = fs::read_to_string(root.path().join(".config/rotproof.toml")).unwrap();
        assert!(now.starts_with(older), "{now:?}");
        assert!(now.contains("\r\nareas = []\r\n"), "{now:?}");
        assert!(!now.replace("\r\n", "").contains('\n'), "{now:?}");
    }
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
            Some("stack = \"rust\"\nareas = [\"a\"]\nui_design = \"none\"\nabsent = [\"ui\"]\n"),
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
        // With ui_design = "none", ui/ is no layer
        (
            "code outside the layers: ui",
            Box::new(|r| plant_file(r, "ui/__init__.py")),
        ),
        (
            "absent names \"ui\", which ui_design = \"none\" leaves out already",
            Box::new(|r| {
                declare(
                    r,
                    "stack = \"python\"\nareas = [\"a\"]\nui_design = \"none\"\nabsent = [\"ui\"]\n",
                )
            }),
        ),
        (
            "domain is declared absent, but domain/ exists",
            Box::new(|r| {
                declare(
                    r,
                    "stack = \"python\"\nareas = [\"a\"]\nui_design = \"none\"\nabsent = [\"domain\"]\n",
                )
            }),
        ),
        (
            "code outside the layers: scripts",
            Box::new(|r| plant_file(r, "scripts/tool.py")),
        ),
        (
            "code outside the layers: main.py",
            Box::new(|r| plant_file(r, "main.py")),
        ),
        // A layer of the table before 0.4.0 says where its code goes, on every run until it is moved
        (
            "code outside the layers: handler (the layer handler of the table before 0.4.0, which is gone: its entry \
             points go in infrastructure.inbound",
            Box::new(|r| plant_file(r, "handler/__init__.py")),
        ),
        (
            "code outside the layers: utils (the layer utils of the table before 0.4.0, which is gone: each part goes \
             in the layer whose concept it speaks",
            Box::new(|r| plant_file(r, "utils/text.py")),
        ),
        // Windows runs it with Python all the same
        (
            "code outside the layers: stray.PY",
            Box::new(|r| plant_file(r, "stray.PY")),
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
                    "stack = \"python\"\nareas = [\"a\"]\nui_design = \"none\"\nabsent = [\"service\"]\n",
                )
            }),
        ),
        (
            "unknown field",
            Box::new(|r| {
                declare(
                    r,
                    "stack = \"python\"\nareas = [\"a\"]\nui_design = \"none\"\nextra = 1\n",
                )
            }),
        ),
        // A layer cannot be switched off by listing it, or a directory holding it
        (
            "which holds or sits in the layer domain",
            Box::new(|r| {
                declare(
                    r,
                    "stack = \"python\"\nareas = [\"a\"]\nui_design = \"none\"\nunchecked = [\"domain\"]\n",
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
                    "stack = \"python\"\nareas = [\"a\"]\nui_design = \"none\"\nunchecked = [\"./scripts\"]\n",
                );
            }),
        ),
        (
            "unchecked lists scripts, which does not exist",
            Box::new(|r| {
                declare(
                    r,
                    "stack = \"python\"\nareas = [\"a\"]\nui_design = \"none\"\nunchecked = [\"scripts\"]\n",
                )
            }),
        ),
        // `moving` is judged as `unchecked` is: it cannot switch a layer off, and it names a path that is there
        (
            "moving lists domain, which holds or sits in the layer domain",
            Box::new(|r| {
                declare(
                    r,
                    "stack = \"python\"\nareas = [\"a\"]\nui_design = \"none\"\nmoving = [\"domain\"]\n",
                )
            }),
        ),
        (
            "moving lists \"./legacy\": write a path from the root",
            Box::new(|r| {
                plant_file(r, "legacy/old.py");
                declare(
                    r,
                    "stack = \"python\"\nareas = [\"a\"]\nui_design = \"none\"\nmoving = [\"./legacy\"]\n",
                );
            }),
        ),
        (
            "moving lists legacy, which does not exist: remove it",
            Box::new(|r| {
                declare(
                    r,
                    "stack = \"python\"\nareas = [\"a\"]\nui_design = \"none\"\nmoving = [\"legacy\"]\n",
                )
            }),
        ),
        // The floor: with every layer declared absent, nothing would be checked
        (
            "no layer is present",
            Box::new(|r| {
                for layer in [
                    "composition",
                    "infrastructure",
                    "application",
                    "protocol",
                    "domain",
                ] {
                    fs::remove_dir_all(r.join(layer)).unwrap();
                }
                declare(
                    r,
                    "stack = \"python\"\nareas = [\"a\"]\nui_design = \"none\"\nabsent = [\"composition\", \
                     \"infrastructure\", \"application\", \"protocol\", \"domain\"]\n",
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

/// Without a declaration nothing can be checked against it: no broken rule, as a file that cannot be read is none, so
/// CI tells a project that has not adopted Rotproof from one that breaks its rules
#[test]
fn check_without_a_declaration_exits_2_and_says_how_to_write_one() {
    let root = clean_repo();
    fs::remove_file(root.path().join(".config/rotproof.toml")).unwrap();
    let out = run(&["--root", &root_arg(root.path()), "check"]);
    assert_eq!(out.status.code(), Some(2), "{}", stdout(&out));
    let said = String::from_utf8_lossy(&out.stderr);
    assert!(
        said.contains(
            "missing: .config/rotproof.toml. Write it with `rotproof init --stack <stack>`"
        ),
        "{said}"
    );
    // A declaration that is there but breaks a rule is still a broken rule
    declare(
        root.path(),
        "stack = \"python\"\nareas = [\"a\"]\nextra = 1\n",
    );
    let out = run(&["--root", &root_arg(root.path()), "check"]);
    assert_eq!(out.status.code(), Some(1), "{}", stdout(&out));
}

/// A declaration whose stack does not fit still names the areas the records are grouped by, so the records are checked
/// all the same: only a declaration that cannot be read leaves them unchecked
#[test]
fn the_records_are_checked_under_a_stack_that_does_not_fit() {
    let root = clean_repo();
    declare(root.path(), "stack = \"cobol\"\nareas = [\"a\"]\n");
    fs::write(
        root.path().join("docs/knowledge/broken.md"),
        "no frontmatter\n",
    )
    .unwrap();
    let out = run(&["--root", &root_arg(root.path()), "check"]);
    let said = stdout(&out);
    assert_eq!(out.status.code(), Some(1), "{said}");
    assert!(said.contains("unknown stack"), "{said}");
    assert!(!said.contains("the records are not checked"), "{said}");
    assert!(said.contains("broken.md"), "{said}");
}

/// A repository that keeps every rule, with every layer `stack` can have (`ui` where it offers it): what
/// `rotproof create` makes, and a log entry.
fn repo_with_ui(stack: &str, absent: &str) -> tempfile::TempDir {
    let root = declared(&format!(
        "stack = \"{stack}\"\nareas = [\"a\"]\n{}absent = [{absent}]\n",
        ui_line(stack)
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

#[test]
fn presentation_holds_no_view_in_typescript() {
    for planted in [
        "src/presentation/models/Form.tsx",
        "src/presentation/display/row.jsx",
        // Any case
        "src/presentation/models/Form.TSX",
        // A test whose name does not tell it is a test is a view all the same
        "src/presentation/models/useForm.tests.tsx",
    ] {
        let root = repo_with_ui("typescript", "");
        plant_file(root.path(), planted);
        let out = run(&["--root", &root_arg(root.path()), "check"]);
        let said = stdout(&out);
        assert_eq!(out.status.code(), Some(1), "{planted}: {said}");
        assert!(
            said.contains(&format!(
                "a view in presentation: {planted} (presentation holds no view, so no .tsx or .jsx file sits there: \
                 keep what the screen holds and decides in .ts files, and move the component into src/ui ("
            )),
            "{planted}: {said}"
        );
        assert_eq!(said.matches("a view in").count(), 1, "{planted}: {said}");
        // Removed, it passes: nothing else was wrong
        fs::remove_file(root.path().join(planted)).unwrap();
        let out = run(&["--root", &root_arg(root.path()), "check"]);
        assert!(out.status.success(), "{planted} removed: {}", stdout(&out));
    }

    // What holds no view in presentation, and views elsewhere, pass; so does a project with no UI at all
    let root = repo_with_ui("typescript", "");
    for path in [
        "src/presentation/models/useForm.ts",
        "src/presentation/models/formContext.ts",
        "src/presentation/display/row.ts",
        "src/ui/pages/Home.tsx",
        "src/ui/atoms/Button.jsx",
        "src/composition/Providers.tsx",
        // A test of a hook that needs a wrapper written in JSX is a test, as the file of Python's `tests/` is
        "src/presentation/models/useForm.test.tsx",
        "src/presentation/models/useForm.spec.jsx",
        "src/presentation/models/__tests__/wrapper.tsx",
    ] {
        plant_file(root.path(), path);
    }
    let out = run(&["--root", &root_arg(root.path()), "check"]);
    assert!(out.status.success(), "{}", stdout(&out));

    // Python has no such file in its code, and a project with ui_design = "none" has no presentation to judge
    let root = repo_with_ui("python", "");
    plant_file(root.path(), "presentation/models/view.tsx");
    let out = run(&["--root", &root_arg(root.path()), "check"]);
    assert!(out.status.success(), "{}", stdout(&out));
    let root = declared("stack = \"typescript\"\nareas = [\"a\"]\nui_design = \"none\"\n");
    let out = create(&["--root", &root_arg(root.path()), "create", "--yes"]);
    assert!(out.status.success(), "{}", stdout(&out));
    plant_file(root.path(), "src/presentation/Form.tsx");
    let said = stdout(&run(&["--root", &root_arg(root.path()), "check"]));
    assert!(!said.contains("a view in"), "{said}");
}

/// Writes `text` at `path` of `root`, making its directories.
fn put(root: &Path, path: &str, text: &str) {
    let full = root.join(path);
    fs::create_dir_all(full.parent().unwrap()).unwrap();
    fs::write(full, text).unwrap();
}

#[test]
fn a_python_name_is_read_as_python_scopes_read_it_by_the_environment_check() {
    let root = clean_repo();
    let r = root.path();
    // The forms that read the environment or the home directory, through every way the file may name them, in a layer
    put(
        r,
        "domain/forbidden.py",
        "import os\nimport os.path as p\nfrom os import getenv as g\nfrom pathlib import Path\n\n\n\
         def read():\n    return (\n        os.environ['A'],\n        g('B'),\n        p.expanduser('~'),\n        \
         Path.home(),\n    )\n",
    );
    put(r, "application/star.py", "from os import *\n");
    // What is no read: a parameter called `os`, a local that is, and the names of the same modules that read nothing
    put(
        r,
        "domain/allowed.py",
        "import os\nfrom pathlib import PurePath\n\n\ndef a(os):\n    return os.environ\n\n\ndef b():\n    os = 1\n    \
         return os.getenv('A')\n\n\ndef c():\n    return os.path.join('a', 'b'), PurePath('a').name\n",
    );
    // `infrastructure` alone may read it
    put(
        r,
        "infrastructure/outbound/settings.py",
        "import os\nfrom os import *\nfrom pathlib import Path\n\nHOME = Path.home()\nA = os.environ['A']\n",
    );
    let out = run(&["--root", &root_arg(r), "check"]);
    let said = stdout(&out);
    assert_eq!(out.status.code(), Some(1), "{said}");
    let star = "(a star import: the names it brings in cannot be read, so name them: `from os import path`)";
    assert!(
        said.ends_with(&format!(
            "  application/star.py:1: os.* {star}\n    from os import *\n\
             \x20 domain/forbidden.py:3: os.getenv\n    from os import getenv as g\n\
             \x20 domain/forbidden.py:9: os.environ\n    os.environ['A'],\n\
             \x20 domain/forbidden.py:10: os.getenv\n    g('B'),\n\
             \x20 domain/forbidden.py:11: os.path.expanduser\n    p.expanduser('~'),\n\
             \x20 domain/forbidden.py:12: pathlib.Path.home\n    Path.home(),\n"
        )),
        "{said}"
    );
    assert!(
        !said.contains("allowed.py") && !said.contains("settings.py"),
        "{said}"
    );
    // Without the forbidden files, the allowed ones and `infrastructure` pass
    fs::remove_file(r.join("domain/forbidden.py")).unwrap();
    fs::remove_file(r.join("application/star.py")).unwrap();
    let out = run(&["--root", &root_arg(r), "check"]);
    assert!(out.status.success(), "{}", stdout(&out));
}

#[test]
fn each_python_form_that_reads_the_environment_fails_alone_and_a_shadowed_name_does_not() {
    let root = clean_repo();
    let r = root.path();
    // (what the file holds, what the check names)
    let forbidden = [
        // What the check found before names of scopes were read
        (
            "import os\nx = os.environ['A']\n",
            "domain/x.py:2: os.environ",
        ),
        (
            "import os as o\nx = o.getenv('A')\n",
            "domain/x.py:2: os.getenv",
        ),
        (
            "import os.path\nos.putenv('A', '1')\n",
            "domain/x.py:2: os.putenv",
        ),
        ("from os import environ\n", "domain/x.py:1: os.environ"),
        (
            "from os import getenv as g\ng('A')\n",
            "domain/x.py:1: os.getenv",
        ),
        // The names that came with scopes
        (
            "import os\nos.path.expandvars('$A')\n",
            "domain/x.py:2: os.path.expandvars",
        ),
        (
            "import os\nos.path.expanduser('~')\n",
            "domain/x.py:2: os.path.expanduser",
        ),
        (
            "from os import path\npath.expanduser('~')\n",
            "domain/x.py:2: os.path.expanduser",
        ),
        (
            "import posixpath\nposixpath.expanduser('~')\n",
            "domain/x.py:2: posixpath.expanduser",
        ),
        (
            "import ntpath\nntpath.expandvars('x')\n",
            "domain/x.py:2: ntpath.expandvars",
        ),
        (
            "import genericpath\ngenericpath.expanduser('~')\n",
            "domain/x.py:2: genericpath.expanduser",
        ),
        (
            "from pathlib import Path\nPath.home()\n",
            "domain/x.py:2: pathlib.Path.home",
        ),
        (
            "from pathlib import PosixPath\nPosixPath.home()\n",
            "domain/x.py:2: pathlib.PosixPath.home",
        ),
        (
            "import pathlib\npathlib.WindowsPath.home()\n",
            "domain/x.py:2: pathlib.WindowsPath.home",
        ),
        ("from os import *\n", "domain/x.py:1: os.*"),
        ("from os.path import *\n", "domain/x.py:1: os.path.*"),
        ("from pathlib import *\n", "domain/x.py:1: pathlib.*"),
    ];
    for (source, named) in forbidden {
        put(r, "domain/x.py", source);
        let out = run(&["--root", &root_arg(r), "check"]);
        let said = stdout(&out);
        assert_eq!(out.status.code(), Some(1), "{source}\n{said}");
        assert!(said.contains(&format!("\n  {named}")), "{source}\n{said}");
    }
    // A name that is not the module, or of a module that reads nothing from the environment
    let allowed = [
        "import os\ndef f(os):\n    return os.environ\n",
        "def f():\n    os = {}\n    return os.getenv('A')\n",
        "import os\nclass C:\n    os = {}\n    x = os.environ\n",
        "import os\nx = [os.environ for os in range(3)]\n",
        "from os import path\nx = path.join('a', 'b')\n",
        "from pathlib import PurePath\nx = PurePath('a')\n",
        "import os\nx = os.fspath('a')\n",
    ];
    for source in allowed {
        put(r, "domain/x.py", source);
        let out = run(&["--root", &root_arg(r), "check"]);
        assert!(out.status.success(), "{source}\n{}", stdout(&out));
    }
}

#[test]
fn each_typescript_form_that_reads_the_environment_fails_alone_and_a_shadowed_name_does_not() {
    let root = typescript_with_vitest();
    let r = root.path();
    // (what the file holds, what the check names)
    let forbidden = [
        (
            "export const x = process.env.A;\n",
            "src/domain/x.ts:1: process.env",
        ),
        (
            "export const x = process['env'].A;\n",
            "src/domain/x.ts:1: process.env",
        ),
        (
            "export const x = globalThis.process.env.A;\n",
            "src/domain/x.ts:1: process.env",
        ),
        (
            "export const x = window.process.env.A;\n",
            "src/domain/x.ts:1: process.env",
        ),
        (
            "export const x = (self as any).process.env.A;\n",
            "src/domain/x.ts:1: process.env",
        ),
        (
            "const { env } = process;\nexport const x = env.A;\n",
            "src/domain/x.ts:1: process.env",
        ),
        (
            "const { env: e } = globalThis.process;\nexport const x = e.A;\n",
            "src/domain/x.ts:1: process.env",
        ),
        (
            "import { env } from 'node:process';\nexport const x = env.A;\n",
            "src/domain/x.ts:1: process.env",
        ),
        (
            "import proc from 'process';\nexport const x = proc.env.A;\n",
            "src/domain/x.ts:2: process.env",
        ),
        (
            "import * as proc from 'node:process';\nexport const x = proc.env.A;\n",
            "src/domain/x.ts:2: process.env",
        ),
        (
            "export const x = import.meta.env.VITE_A;\n",
            "src/domain/x.ts:1: import.meta.env",
        ),
        (
            "export const x = import.meta.env;\n",
            "src/domain/x.ts:1: import.meta.env",
        ),
        (
            "const { MODE, VITE_A } = import.meta.env;\nexport const x = VITE_A;\n",
            "src/domain/x.ts:1: import.meta.env",
        ),
        // A name that a function binds does not hide the use outside it
        (
            "export const a = (process: any) => process.env;\nexport const x = process.env.A;\n",
            "src/domain/x.ts:2: process.env",
        ),
        // What `require` and `import()` with a literal bind is the module, as an import's
        (
            "const process = require('process');\nexport const x = process.env.A;\n",
            "src/domain/x.ts:2: process.env",
        ),
        (
            "const p = require('node:process');\nexport const x = p.env.A;\n",
            "src/domain/x.ts:2: process.env",
        ),
        (
            "const { env } = require('node:process');\nexport const x = env.A;\n",
            "src/domain/x.ts:1: process.env",
        ),
        (
            "const p = await import('process');\nexport const x = p.env.A;\n",
            "src/domain/x.ts:2: process.env",
        ),
        (
            "const { env } = await import('node:process');\nexport const x = env.A;\n",
            "src/domain/x.ts:1: process.env",
        ),
    ];
    for (source, named) in forbidden {
        put(r, "src/domain/x.ts", source);
        let out = run(&["--root", &root_arg(r), "check"]);
        let said = stdout(&out);
        assert_eq!(out.status.code(), Some(1), "{source}\n{said}");
        assert!(said.contains(&format!("\n  {named}")), "{source}\n{said}");
    }
    // A name the file binds is not the global, and what reads nothing from the environment passes
    let allowed = [
        "const p = require('path');\nexport const x = p.join('a', 'b');\n",
        "const p = require('path');\nexport const x = p.env.A;\n",
        "export function f(process: any) { return process.env.A; }\n",
        "export function f() { const process = { env: {} as any }; return process.env.A; }\n",
        "export const f = (process: { env: string }) => process.env;\n",
        "export function f() { try { return 1; } catch (process: any) { return process.env; } }\n",
        "export const x = [1].map((process) => process.toString());\n",
        "export type E = typeof process.env;\n",
        "export const x = typeof process.argv;\n",
        "export const x = import.meta.env.DEV && import.meta.env.MODE;\n",
        "const { DEV, PROD } = import.meta.env;\nexport const x = DEV || PROD;\n",
        "const config = { env: 'dev' };\nexport const x = config.env;\n",
    ];
    for source in allowed {
        put(r, "src/domain/x.ts", source);
        let out = run(&["--root", &root_arg(r), "check"]);
        assert!(out.status.success(), "{source}\n{}", stdout(&out));
    }
    // What reads nothing from the environment but imports `process` is no read, and is I/O of the inner layers by its
    // import (a `require` that a parameter hides is an import to the reader of imports, as to the direction check)
    for source in [
        "export function f(require: any) { const process = require('process'); return process.env.A; }\n",
        "import { argv } from 'node:process';\nexport const x = argv;\n",
    ] {
        put(r, "src/domain/x.ts", source);
        let out = run(&["--root", &root_arg(r), "check"]);
        let said = stdout(&out);
        assert_eq!(out.status.code(), Some(1), "{source}\n{said}");
        assert!(
            said.contains("uses `process`") && !said.contains("only infrastructure reads"),
            "{source}\n{said}"
        );
    }
}

/// `rotproof check` in `root`: its exit code and what it printed.
fn checked(root: &Path) -> (Option<i32>, String) {
    let out = run(&["--root", &root_arg(root), "check"]);
    (out.status.code(), stdout(&out))
}

/// The lines of `file` that the findings in `said` name, in ascending order: the line a finding starts with, and the
/// ones it lists after "on lines" (a finding is one for each entry in a file, with every line of it)
fn named_lines(said: &str, file: &str) -> Vec<usize> {
    let mut lines = Vec::new();
    for line in said.lines() {
        let Some(rest) = line.strip_prefix(&format!("  {file}:")) else {
            continue;
        };
        let (first, rest) = rest.split_once(": ").unwrap_or((rest, ""));
        lines.extend(first.parse::<usize>());
        if let Some((_, listed)) = rest.rsplit_once(", on lines ") {
            lines.extend(listed.split(", ").filter_map(|n| n.parse::<usize>().ok()));
        }
    }
    lines.sort_unstable();
    lines.dedup();
    lines
}

const IO_HEADING: &str =
    "the inner layers do no I/O (domain, protocol, application and presentation import nothing";
const IO_FIX: &str = "move the I/O into infrastructure.outbound, behind a port in protocol.outbound that the use case is given";

/// Plant each source at `path`, check, and expect the check to fail under the heading of the I/O of the inner layers
/// with `line` and what it uses of `entry`; then expect each of `free` to pass.
fn plant_io(r: &Path, path: &str, forbidden: &[(&str, usize, &str)], free: &[&str], reset: &str) {
    for (source, line, uses) in forbidden {
        put(r, path, source);
        let (code, said) = checked(r);
        assert_eq!(code, Some(1), "{source}\n{said}");
        assert!(said.contains(IO_HEADING), "{source}\n{said}");
        // What a finding says of what it found: a use, or a scheme or a glob in its own words
        let what = if uses.starts_with("imports ") || uses.starts_with("a glob") {
            uses.to_string()
        } else {
            format!("uses {uses}, which does I/O")
        };
        // A finding is one for each entry in a file: it starts with the first line of the entry and lists the others
        assert!(
            said.lines()
                .any(|one| one.starts_with(&format!("  {path}:"))
                    && one.contains(&format!("{IO_FIX}: {what}"))
                    && named_lines(one, path).contains(line)),
            "{source}\n{said}"
        );
    }
    for source in free {
        put(r, path, source);
        let (code, said) = checked(r);
        assert_eq!(code, Some(0), "{source}\n{said}");
    }
    put(r, path, reset);
}

#[test]
fn the_inner_layers_do_no_io_in_python_and_each_form_planted_fails() {
    let root = clean_repo();
    let r = root.path();
    let (code, said) = checked(r);
    assert_eq!(code, Some(0), "{said}");
    plant_io(
        r,
        "domain/x.py",
        &[
            ("import shutil\n", 1, "`shutil`"),
            ("import urllib.request\n", 1, "`urllib.request`"),
            ("import urllib.request as r\n", 1, "`urllib.request`"),
            ("from urllib import request\n", 1, "`urllib.request`"),
            (
                "from urllib.request import urlopen\n",
                1,
                "`urllib.request` (urlopen)",
            ),
            (
                "import urllib\nurllib.request.urlopen('x')\n",
                2,
                "`urllib.request` (urlopen)",
            ),
            ("import _socket\n", 1, "`_socket`"),
            ("from subprocess import *\n", 1, "`subprocess`"),
            (
                "import importlib\nimportlib.import_module('sqlite3')\n",
                2,
                "`sqlite3`",
            ),
            (
                "from asyncio import create_subprocess_exec as c\n",
                1,
                "`asyncio.create_subprocess_exec`",
            ),
            (
                "import asyncio\n\n\nasync def f():\n    return await asyncio.open_connection('h', 1)\n",
                5,
                "`asyncio.open_connection`",
            ),
            ("import argparse\nimport shutil\n", 1, "`argparse`"),
            ("from logging import handlers\n", 1, "`logging.handlers`"),
            (
                "import tempfile\n\nx = tempfile.mkdtemp()\n",
                3,
                "`tempfile` (mkdtemp)",
            ),
        ],
        &[
            "import os\nimport os.path\nimport pathlib\nimport json\nimport io\nimport logging\nimport asyncio\n",
            "from urllib import parse\nimport urllib.error\nimport zipfile\nimport tarfile\nimport gzip\n",
            "import sys\nimport time\nimport datetime\nimport csv\nimport mmap\nimport ctypes\nimport bz2, lzma\n",
            "import http\nfrom http import HTTPStatus\nimport multiprocessing\nimport importlib\n",
        ],
        "",
    );
    // The files and layers that are not judged: the I/O layer, what wires and draws, the entry file, and the tests
    let io = "import shutil\nshutil.rmtree('x')\n";
    for path in [
        "infrastructure/outbound/x.py",
        "infrastructure/inbound/x.py",
        "composition/x.py",
        "__main__.py",
        "tests/test_x.py",
    ] {
        put(r, path, io);
    }
    let (code, said) = checked(r);
    // Only the marker and structure rules may say anything of them, never the I/O of the inner layers
    assert!(!said.contains(IO_HEADING), "{said}");
    assert!(!said.contains("does I/O"), "{said}");
    assert!(code == Some(0) || code == Some(1));
    // The heading and the way out are the guide's too: the whole table, and what is no fix
    let guide = stdout(&run(&["guide", "--stack", "python"]));
    for name in [
        "`shutil`",
        "`urllib.request`",
        "`_socket`",
        "`asyncio.open_connection`",
        "`argparse`",
    ] {
        assert!(guide.contains(name), "{name}");
    }
    assert!(guide.contains("The inner layers do no I/O."));
    assert!(
        guide.contains("moving the code to a path in `unchecked` or `moving` is not a fix"),
        "{guide}"
    );
}

#[test]
fn an_approved_use_of_io_prints_the_names_the_file_uses_now_and_a_stale_one_fails() {
    let root = clean_repo();
    let r = root.path();
    let named = Some("[user]\n\tname = Someone Else\n");
    let source = |names: &str| format!("import shutil\n\n\ndef f(a, b):\n{names}");
    put(
        r,
        "domain/model.py",
        &source("    shutil.copyfile(a, b)\n    shutil.rmtree(b)\n    shutil.rmtree(a)\n"),
    );
    // One approval for the file and the entry covers every use of it, whatever the names
    let (code, said) = checked(r);
    assert_eq!(code, Some(1), "{said}");
    // ... and the check says it as one finding, with every line, and the source text of the first
    assert!(
        said.contains(
            "  domain/model.py:1: move the I/O into infrastructure.outbound, behind a port in protocol.outbound that the use \
             case is given: uses `shutil` (copyfile, rmtree), which does I/O, on lines 1, 5, 6, 7\n    import shutil\n"
        ) && said.matches("domain/model.py:").count() == 1,
        "{said}"
    );
    let out = approve_with_git(
        r,
        named,
        &[
            "domain/model.py",
            "shutil",
            "--reason",
            "the report is written here until the port is made",
        ],
    );
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}{}",
        stdout(&out),
        String::from_utf8_lossy(&out.stderr)
    );
    let (code, said) = checked(r);
    assert_eq!(code, Some(0), "{said}");
    assert!(
        said.contains(
            "approved in .config/rotproof-approved.toml (1):\n  shutil in domain/model.py: the report is written here \
             until the port is made (Someone Else, "
        ),
        "{said}"
    );
    assert!(said.contains("; now used: copyfile, rmtree\n"), "{said}");
    // The names change, so the reason is read again
    put(r, "domain/model.py", &source("    shutil.move(a, b)\n"));
    let (code, said) = checked(r);
    assert_eq!(code, Some(0), "{said}");
    assert!(
        said.contains("; now used: move\n") && !said.contains("copyfile"),
        "{said}"
    );
    // Another entry of the file is not covered
    put(
        r,
        "domain/model.py",
        &format!("import tempfile\n{}", source("    shutil.move(a, b)\n")),
    );
    let (code, said) = checked(r);
    assert_eq!(code, Some(1), "{said}");
    assert!(
        said.contains("domain/model.py:1: ") && said.contains("uses `tempfile`"),
        "{said}"
    );
    assert!(!said.contains("uses `shutil`"), "{said}");
    // Only what the check finds is approved
    let out = approve_with_git(r, named, &["domain/model.py", "socket", "--reason", "no"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("no forbidden import of socket"));
    // An approval that matches nothing fails, and `--prune` takes it away
    put(r, "domain/model.py", "import os\n");
    let (code, said) = checked(r);
    assert_eq!(code, Some(1), "{said}");
    assert!(
        said.contains("approved, but no such forbidden import: shutil in domain/model.py"),
        "{said}"
    );
    let out = run(&["--root", &root_arg(r), "approve", "--prune"]);
    assert_eq!(out.status.code(), Some(0), "{}", stdout(&out));
    let (code, said) = checked(r);
    assert_eq!(code, Some(0), "{said}");
}

#[test]
fn each_python_name_that_does_io_in_part_fails_alone_and_what_is_pure_passes() {
    let root = clean_repo();
    let r = root.path();
    // (what the file holds, the line, what the finding says it found)
    let forbidden = [
        (
            "import os\nx = os.getcwd()\n",
            2,
            "uses `os.getcwd`, which does I/O",
        ),
        (
            "import os\nx = os.path.exists('a')\n",
            2,
            "uses `os.path.exists`, which does I/O",
        ),
        (
            "import sys\nx = sys.platform\n",
            2,
            "uses `sys.platform`, which does I/O",
        ),
        (
            "import logging\nlogging.basicConfig()\n",
            2,
            "uses `logging.basicConfig`, which does I/O",
        ),
        (
            "import time\nx = time.mktime(t)\n",
            2,
            "uses `time.mktime`, which does I/O",
        ),
        (
            "import time\nx = time.time()\n",
            2,
            "reads the clock (time.time), which does I/O (entry `clock`)",
        ),
        (
            "from datetime import datetime\nx = datetime.now()\n",
            2,
            "reads the clock (datetime.datetime.now), which does I/O (entry `clock`)",
        ),
        (
            "from dataclasses import field\nfrom datetime import datetime\nx = field(default_factory=datetime.now)\n",
            3,
            "reads the clock (datetime.datetime.now)",
        ),
        (
            "import time\nx = time.gmtime()\n",
            2,
            "reads the clock (time.gmtime)",
        ),
        (
            "import uuid\nfrom typing import Annotated\nfrom pydantic import Field\n\n\nclass M:\n    \
             id: Annotated[str, Field(default_factory=uuid.uuid4)]\n",
            7,
            "draws on randomness (uuid.uuid4), which does I/O (entry `random`)",
        ),
        (
            "import random\nx = random.Random()\n",
            2,
            "draws on randomness (random.Random)",
        ),
        (
            "import random\nx = random.choice([1, 2])\n",
            2,
            "draws on randomness (random.choice)",
        ),
        ("import secrets\n", 1, "draws on randomness (secrets)"),
        (
            "import os\nx = os.urandom(8)\n",
            2,
            "draws on randomness (os.urandom)",
        ),
        (
            "x = input()\n",
            1,
            "uses the terminal or opens a file (input), which does I/O (entry `terminal`)",
        ),
        (
            "print('x')\n",
            1,
            "uses the terminal or opens a file (print)",
        ),
        (
            "with open('f') as f:\n    pass\n",
            1,
            "uses the terminal or opens a file (open)",
        ),
        (
            "import sys\nsys.stdout.write('x')\n",
            2,
            "uses the terminal or opens a file (sys.stdout)",
        ),
        (
            "import io\nx = io.open('f')\n",
            2,
            "uses the terminal or opens a file (io.open)",
        ),
        (
            "import sys\nx = sys.argv\n",
            2,
            "reads the command-line arguments (sys.argv), which does I/O (entry `argv`)",
        ),
        (
            "from pathlib import Path\nx = Path('a')\n",
            2,
            "uses `pathlib.Path`, which does I/O: a path as a value is `PurePath`",
        ),
        // The import line fails with the use that runs
        (
            "from pathlib import Path\nx = Path('a')\n",
            1,
            "uses `pathlib.Path`, which does I/O",
        ),
        (
            "from pathlib import Path\n\n\ndef f(x):\n    return isinstance(x, Path)\n",
            5,
            "uses `pathlib.Path`, which does I/O",
        ),
        (
            "from pathlib import Path as P\nx = P.cwd()\n",
            2,
            "uses `pathlib.Path` (cwd), which does I/O",
        ),
        (
            "from pathlib import Path\nf = Path\n",
            2,
            "uses `pathlib.Path`, which does I/O",
        ),
        (
            "import pathlib\nx = pathlib.PosixPath('a')\n",
            2,
            "uses `pathlib.PosixPath`, which does I/O",
        ),
        (
            "def f(p):\n    return p.open()\n",
            2,
            "calls `.open(...)`, a method of a path that does I/O",
        ),
        (
            "def f(p):\n    return p.read_text()\n",
            2,
            "calls `.read_text(...)`, a method of a path that does I/O",
        ),
        (
            "def f(p):\n    return p.exists()\n",
            2,
            "calls `.exists(...)`",
        ),
        (
            "def f(p):\n    return p.open('w')\n",
            2,
            "calls `.open(...)`",
        ),
        ("import os\nsh = os\n", 2, "uses the module `os` as a value"),
        (
            "import os\nx = getattr(os, 'system')\n",
            2,
            "uses the module `os` as a value",
        ),
        (
            "import sys\nx = vars(sys)\n",
            2,
            "uses the module `sys` as a value",
        ),
        (
            "from time import *\n",
            1,
            "a star import of `time`, which brings in names that do I/O",
        ),
        ("from datetime import *\n", 1, "a star import of `datetime`"),
        (
            "x = eval('1')\n",
            1,
            "uses `eval`: what it runs or imports cannot be read",
        ),
        ("exec('x = 1')\n", 1, "uses `exec`"),
        (
            "import importlib\nimportlib.import_module(name)\n",
            2,
            "uses `importlib.import_module`",
        ),
    ];
    for (source, line, what) in forbidden {
        put(r, "domain/x.py", source);
        let (code, said) = checked(r);
        assert_eq!(code, Some(1), "{source}\n{said}");
        assert!(said.contains(IO_HEADING), "{source}\n{said}");
        // The finding is one for the entry of the file: it names the first line and lists the others
        assert!(
            named_lines(&said, "domain/x.py").contains(&line) && said.contains(what),
            "{source}\n{said}"
        );
    }
    // What is pure, a name the file binds, a use in a type position, and what is judged by its arguments
    let free = [
        "import os\nimport sys\nimport time\nimport logging\nx = (os.fspath('a'), os.path.join('a', 'b'), \
         sys.version_info, time.strptime('1', '%S'), logging.getLogger('x'), logging.INFO)\ntime.sleep(1)\n",
        "import time\nx = time.gmtime(0)\ny = time.strftime('%Y', x)\n",
        "import random\nx = random.Random(1)\n",
        "import uuid\nx = uuid.uuid5(uuid.NAMESPACE_DNS, 'a')\n",
        "from pathlib import PurePath\nx = PurePath('a').name\n",
        "def f(input, print):\n    return input(print)\n",
        "import sys\nx: sys.stdout = None\n",
        "def f(p, name):\n    return p.exists(1), p.open(name), p.open(*name), p.get(), p.read(), re.match(a, b).group()\n",
        // `Path` written in a type position alone: the import is free too, with an alias and under `TYPE_CHECKING`
        "from pathlib import Path\n\n\ndef load_config(path: str | Path = 'config.yaml') -> list[Path]:\n    return []\n",
        "from pathlib import Path as P\n\n\ndef load(path: P) -> P:\n    return path\n",
        "from pathlib import Path\n",
        "from typing import TYPE_CHECKING\n\nif TYPE_CHECKING:\n    from pathlib import Path\n\n\n\
         def load(path: 'Path') -> None:\n    return None\n",
        "from typing import TYPE_CHECKING\n\nif TYPE_CHECKING:\n    from pathlib import PosixPath\n\n\n\
         def load(path: PosixPath) -> PosixPath:\n    return path\n",
        "import os\ndef f(os):\n    return check(os)\n",
        "from datetime import datetime\nx = datetime.fromtimestamp(0)\n",
        "import zipfile, tarfile, gzip\n",
    ];
    for source in free {
        put(r, "domain/x.py", source);
        let (code, said) = checked(r);
        assert_eq!(code, Some(0), "{source}\n{said}");
    }
    // The layers that do the I/O, wire and draw, and the entry file and the tests do all of it
    fs::remove_file(r.join("domain/x.py")).unwrap();
    let all = "import os, sys, time, random\nprint(os.getcwd(), sys.argv, time.time(), random.random())\n\
               def f(p):\n    return p.read_text()\n";
    for path in [
        "infrastructure/outbound/x.py",
        "infrastructure/inbound/x.py",
        "composition/x.py",
        "__main__.py",
        "tests/test_x.py",
    ] {
        put(r, path, all);
    }
    let (code, said) = checked(r);
    assert!(!said.contains("does I/O"), "{said}");
    assert!(code == Some(0) || code == Some(1));
}

#[test]
fn an_act_of_a_group_and_a_name_of_a_module_judged_by_its_names_are_approved_by_their_own_entries()
{
    let root = clean_repo();
    let r = root.path();
    let named = Some("[user]\n\tname = Someone Else\n");
    let approve = |entry: &str| {
        let out = approve_with_git(
            r,
            named,
            &["domain/model.py", entry, "--reason", "the port comes later"],
        );
        assert_eq!(
            out.status.code(),
            Some(0),
            "{entry}: {}{}",
            stdout(&out),
            String::from_utf8_lossy(&out.stderr)
        );
    };
    let source = |body: &str| {
        format!("import os\nimport time\nfrom datetime import datetime\n\n\ndef f(p):\n{body}")
    };
    put(
        r,
        "domain/model.py",
        &source(
            "    a = datetime.now()\n    b = time.time()\n    c = os.getcwd()\n    return p.read_text()\n",
        ),
    );
    let (code, said) = checked(r);
    assert_eq!(code, Some(1), "{said}");
    // The act of a group is one entry, whichever names the file reaches it by
    approve("clock");
    let (code, said) = checked(r);
    assert_eq!(code, Some(1), "{said}");
    assert!(
        said.contains("uses `os.getcwd`") && said.contains("calls `.read_text(...)`"),
        "{said}"
    );
    assert!(
        !said.contains("reads the clock"),
        "one approval covers both names\n{said}"
    );
    assert!(
        said.contains("clock in domain/model.py: the port comes later (Someone Else, ")
            && said.contains("; now used: datetime.datetime.now, time.time\n"),
        "{said}"
    );
    // A name of a module judged by its names, and a method of the net, are entries of their own
    approve("os.getcwd");
    approve("read_text");
    let (code, said) = checked(r);
    assert_eq!(code, Some(0), "{said}");
    assert!(
        said.contains("approved in .config/rotproof-approved.toml (3):"),
        "{said}"
    );
    assert!(
        said.contains("  os.getcwd in domain/model.py: the port comes later")
            && said.contains("  read_text in domain/model.py: the port comes later"),
        "{said}"
    );
    // The names change, so the reason is read again; and what is no longer used is a stale entry
    put(
        r,
        "domain/model.py",
        &source("    a = time.monotonic\n    return p.read_text()\n"),
    );
    let (code, said) = checked(r);
    assert_eq!(code, Some(1), "{said}");
    assert!(said.contains("; now used: time.monotonic\n"), "{said}");
    assert!(
        said.contains("approved, but no such forbidden import: os.getcwd in domain/model.py"),
        "{said}"
    );
    // Only an entry the check names is approved: `time` is a module, the entry is `clock`
    let out = approve_with_git(r, named, &["domain/model.py", "time", "--reason", "no"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("no forbidden import of time"));
    let out = run(&["--root", &root_arg(r), "approve", "--prune"]);
    assert_eq!(out.status.code(), Some(0), "{}", stdout(&out));
    let (code, said) = checked(r);
    assert_eq!(code, Some(0), "{said}");
    // The name of a group that no file uses is stale as any other entry
    put(r, "domain/model.py", "import os\n");
    let (code, said) = checked(r);
    assert_eq!(code, Some(1), "{said}");
    assert!(
        said.contains("approved, but no such forbidden import: clock in domain/model.py"),
        "{said}"
    );
}

#[test]
fn a_method_the_project_defines_is_left_out_of_the_net_and_the_line_is_printed_on_every_run() {
    let root = clean_repo();
    let r = root.path();
    put(
        r,
        "domain/a.py",
        "def f(p, store):\n    return p.exists(), p.read_text(), store.mkdir('x'), p.mkdir()\n",
    );
    let (code, said) = checked(r);
    assert_eq!(code, Some(1), "{said}");
    assert!(!said.contains("I/O net:"), "{said}");
    for method in ["exists", "read_text", "mkdir"] {
        assert!(said.contains(&format!("calls `.{method}(...)`")), "{said}");
    }
    // The project defines a method of the same name and number of parameters, in any file of it
    put(
        r,
        "tests/test_store.py",
        "class Store:\n    def exists(self): ...\n    def mkdir(self, name): ...\n    def read_text(self, a, b): ...\n",
    );
    let (code, said) = checked(r);
    assert_eq!(code, Some(1), "{said}");
    assert!(
        said.contains(
            "I/O net: left out, as the project defines a method of the same name and number of parameters: \
             exists(0), mkdir(1), read_text(2)\n"
        ),
        "{said}"
    );
    // `exists()` and `mkdir('x')` are left out; `read_text()` has another number of parameters, and so has `mkdir()`
    assert!(
        !said.contains("calls `.exists(...)`")
            && said.contains("calls `.read_text(...)`")
            && said.contains("domain/a.py:2: "),
        "{said}"
    );
    assert_eq!(said.matches("calls `.mkdir(...)`").count(), 1, "{said}");
    // It is printed when nothing fails too
    put(r, "domain/a.py", "def f(p):\n    return p.get()\n");
    let (code, said) = checked(r);
    assert_eq!(code, Some(0), "{said}");
    assert!(said.contains("I/O net: left out"), "{said}");
}

#[test]
fn a_file_is_one_finding_for_each_entry_whatever_the_number_of_lines_it_does_it_on() {
    let root = clean_repo();
    let r = root.path();
    // Thirteen prints and two clocks: two findings, each with all its lines, the first line's text and the names
    let mut source = String::from("import time\n");
    for at in 0..13 {
        source.push_str(&format!("print({at})\n"));
        if at == 3 || at == 8 {
            source.push_str("t = time.time()\n");
        }
    }
    put(r, "domain/a.py", &source);
    let (code, said) = checked(r);
    assert_eq!(code, Some(1), "{said}");
    assert_eq!(said.matches("  domain/a.py:").count(), 2, "{said}");
    assert!(
        said.contains("  domain/a.py:2: return what is to be shown")
            && said.contains(
                "uses the terminal or opens a file (print), which does I/O (entry `terminal`), on lines 2, 3, 4, 5, 7, 8, \
                 9, 10, 11, 13, 14, 15, 16\n    print(0)\n"
            ),
        "{said}"
    );
    assert!(
        said.contains(
            "reads the clock (time.time), which does I/O (entry `clock`), on lines 6, 12\n    t = time.time()\n"
        ),
        "{said}"
    );
    // The key of the approval is the file and the entry, as before: one approval of `terminal` covers the 13 prints
    let named = Some("[user]\n\tname = Someone Else\n");
    let out = approve_with_git(
        r,
        named,
        &[
            "domain/a.py",
            "terminal",
            "--reason",
            "the report is printed here",
        ],
    );
    assert_eq!(out.status.code(), Some(0), "{}", stdout(&out));
    let (code, said) = checked(r);
    assert_eq!(code, Some(1), "{said}");
    assert_eq!(said.matches("  domain/a.py:").count(), 1, "{said}");
    assert!(
        said.contains("reads the clock")
            && said.contains("terminal in domain/a.py: the report is printed here"),
        "{said}"
    );
    assert!(said.contains("; now used: print\n"), "{said}");
}

#[test]
fn open_with_no_argument_is_judged_everywhere_unless_the_project_defines_one() {
    let root = clean_repo();
    let r = root.path();
    put(
        r,
        "domain/a.py",
        "def f(x, name):\n    return x.open(), x.open(name)\n",
    );
    let (code, said) = checked(r);
    assert_eq!(code, Some(1), "{said}");
    // The call with no argument alone is judged, in the one line
    assert!(said.contains("domain/a.py:2: "), "{said}");
    assert_eq!(said.matches("calls `.open(...)`").count(), 1, "{said}");
    // A method `open(self)` of the project leaves it out, and says so; `open('r')` is still judged
    put(
        r,
        "tests/test_store.py",
        "class Store:\n    def open(self): ...\n",
    );
    let (code, said) = checked(r);
    assert_eq!(code, Some(0), "{said}");
    assert!(
        said.contains("same name and number of parameters: open(0)"),
        "{said}"
    );
    put(r, "domain/a.py", "def f(x):\n    return x.open('r')\n");
    let (code, said) = checked(r);
    assert_eq!(code, Some(1), "{said}");
    assert!(said.contains("calls `.open(...)`"), "{said}");
}

#[test]
fn the_guide_of_a_python_project_lists_the_allowlists_in_full_and_the_way_out() {
    let guide = stdout(&run(&["guide", "--stack", "python"]));
    for name in [
        // os, os.path, sys, time, logging: what is pure
        "`fspath`",
        "`extsep`",
        "`splitroot`",
        "`commonprefix`",
        "`version_info`",
        "`getsizeof`",
        "`strptime`",
        "`sleep`",
        "`getLogger`",
        "`LoggerAdapter`",
        // the groups, `Path` and the net
        "`datetime.datetime.now`",
        "`default_factory=datetime.now`",
        "`secrets`",
        "`uuid4`",
        "`pathlib.PosixPath`",
        "`PurePath`",
        "`read_text`",
        "`is_junction`",
        "`resolve`",
        "`encoding`",
        "`clock`",
        "`terminal`",
        "`argv`",
        "`Clock = Callable[[], datetime]`",
        "`eval`",
    ] {
        assert!(guide.contains(name), "{name}");
    }
    assert!(
        guide.contains("`os.getcwd` or `sys.platform` for a name of a module judged by its names"),
        "{guide}"
    );
    assert!(
        guide.contains("Approving the import,\n  renaming or respelling it"),
        "{guide}"
    );
    // The other stacks do not claim what is not checked for them yet, or what is another stack's
    for stack in ["typescript", "rust"] {
        let guide = stdout(&run(&["guide", "--stack", stack]));
        assert!(
            !guide.contains("`Clock = Callable[[], datetime]`"),
            "{stack}"
        );
        assert!(!guide.contains("`fspath`"), "{stack}");
        assert!(!guide.contains("`os.getcwd` or `sys.platform`"), "{stack}");
    }
    let guide = stdout(&run(&["guide", "--stack", "rust"]));
    assert!(!guide.contains("`() => Date`"));
    assert!(!guide.contains("`Math.random`"));
}

#[test]
fn the_inner_layers_do_no_io_in_typescript_and_the_table_comes_before_the_manifest() {
    let root = repo_with_ui("typescript", "");
    let r = root.path();
    let (code, said) = checked(r);
    assert_eq!(code, Some(0), "{said}");
    plant_io(
        r,
        "src/domain/x.ts",
        &[
            ("import fs from 'fs';\n", 1, "`fs`"),
            (
                "import { readFile } from 'node:fs/promises';\n",
                1,
                "`fs` (readFile)",
            ),
            (
                "import * as cp from \"child_process\";\ncp.exec('x');\n",
                1,
                "`child_process` (exec)",
            ),
            ("const net = require('node:net');\n", 1, "`net`"),
            ("export * from 'node:tty';\n", 1, "`tty`"),
            ("import sqlite from 'node:sqlite';\n", 1, "`sqlite`"),
            ("import type { Stats } from 'fs';\n", 1, "`fs` (Stats)"),
            ("export const a = import('node:http');\n", 1, "`http`"),
            (
                "import { Database } from 'bun:sqlite';\n",
                1,
                "imports `bun:sqlite`: a scheme other than node: names what cannot be judged",
            ),
            (
                "import x from 'npm:left-pad';\n",
                1,
                "imports `npm:left-pad`: a scheme other than node:",
            ),
            (
                "import x from 'https://esm.sh/x';\n",
                1,
                "imports `https://esm.sh/x`: a scheme other than node:",
            ),
            (
                "import dns from 'node:dns/promises';\nimport rl from 'readline';\n",
                1,
                "`dns`",
            ),
        ],
        &[
            "import os from 'node:os';\nimport { join } from 'path';\nimport crypto from 'crypto';\n",
            "import { Readable } from 'node:stream';\nimport { URL } from 'node:url';\nimport util from 'util';\n",
            "import { EventEmitter } from 'events';\nimport { Buffer } from 'buffer';\nimport zlib from 'node:zlib';\n",
            "import { Worker } from 'worker_threads';\nimport { setTimeout } from 'timers';\nimport t from 'node:test';\n",
            "import { performance } from 'perf_hooks';\nimport { join } from './other';\nexport const x = join;\n",
        ],
        "",
    );
    // A bare name is Node's even when the manifest lists a package of that name
    put(
        r,
        "package.json",
        "{\n  \"name\": \"shop\",\n  \"dependencies\": { \"fs\": \"^1\" }\n}\n",
    );
    put(
        r,
        "src/domain/x.ts",
        "import fs from 'fs';\nexport const x = fs;\n",
    );
    let (code, said) = checked(r);
    assert_eq!(code, Some(1), "{said}");
    assert!(
        said.contains(&format!(
            "  src/domain/x.ts:1: {IO_FIX}: uses `fs`, which does I/O, on lines 1, 2\n"
        )),
        "{said}"
    );
    put(r, "package.json", "{\n  \"name\": \"shop\"\n}\n");
    // The files and layers that are not judged
    let io = "import fs from 'node:fs';\nexport const x = fs;\n";
    for path in [
        "src/infrastructure/outbound/x.ts",
        "src/composition/x.ts",
        "src/main.tsx",
        "src/domain/x.test.ts",
        "src/domain/x.spec.tsx",
        "src/domain/__tests__/x.ts",
        "src/ui/atoms/x.ts",
    ] {
        put(r, path, io);
    }
    put(r, "src/domain/x.ts", "");
    let (_, said) = checked(r);
    assert!(!said.contains(IO_HEADING), "{said}");
    let guide = stdout(&run(&["guide", "--stack", "typescript"]));
    for name in [
        "`fs`",
        "`child_process`",
        "`sqlite`",
        "`_tls_wrap`",
        "`bun:`",
    ] {
        assert!(guide.contains(name), "{name}");
    }
}

#[test]
fn a_screens_model_is_told_to_get_what_it_needs_from_its_page() {
    let root = repo_with_ui("typescript", "");
    let r = root.path();
    put(
        r,
        "src/presentation/models/x.ts",
        "import fs from 'node:fs';\nexport const x = fs;\n",
    );
    put(
        r,
        "src/application/y.ts",
        "import fs from 'node:fs';\nexport const y = fs;\n",
    );
    let (code, said) = checked(r);
    assert_eq!(code, Some(1), "{said}");
    assert!(
        said.contains(
            "  src/presentation/models/x.ts:1: a screen's model does no I/O: it gets what it needs from its page, \
             through a hook in `ui.atoms`, or through a use case of protocol.inbound it is given: uses `fs`, which does \
             I/O, on lines 1, 2\n"
        ),
        "{said}"
    );
    assert!(
        said.contains(&format!(
            "  src/application/y.ts:1: {IO_FIX}: uses `fs`, which does I/O, on lines 1, 2\n"
        )),
        "{said}"
    );
}

#[test]
fn each_typescript_global_that_does_io_fails_alone_and_what_is_pure_passes() {
    let root = repo_with_ui("typescript", "");
    let r = root.path();
    let (code, said) = checked(r);
    assert_eq!(code, Some(0), "{said}");
    // (what the file holds, the line, what the finding says it found)
    let forbidden = [
        (
            "export const x = fetch('a');\n",
            1,
            "uses `fetch`, which does I/O",
        ),
        (
            "export const x = window.fetch('a');\n",
            1,
            "uses `fetch`, which does I/O",
        ),
        (
            "export const x = globalThis.fetch('a');\n",
            1,
            "uses `fetch`, which does I/O",
        ),
        (
            "export const x = (self as any).fetch('a');\n",
            1,
            "uses `fetch`, which does I/O",
        ),
        // A reference counts as a call
        (
            "export const f = fetch;\n",
            1,
            "uses `fetch`, which does I/O",
        ),
        (
            "export const x = document.title;\n",
            1,
            "uses `document` (document.title), which does I/O",
        ),
        (
            "export const x = window.localStorage.getItem('a');\n",
            1,
            "uses `localStorage` (localStorage.getItem), which does I/O",
        ),
        (
            "export const x = sessionStorage.length;\n",
            1,
            "uses `sessionStorage` (sessionStorage.length), which does I/O",
        ),
        (
            "export const x = matchMedia('(min-width: 1px)').matches;\n",
            1,
            "uses `matchMedia`, which does I/O",
        ),
        (
            "export const x = navigator.userAgent;\n",
            1,
            "uses `navigator` (navigator.userAgent), which does I/O",
        ),
        (
            "export const x = new WebSocket('ws://a');\n",
            1,
            "uses `WebSocket`, which does I/O",
        ),
        (
            "export const x = new XMLHttpRequest();\n",
            1,
            "uses `XMLHttpRequest`, which does I/O",
        ),
        (
            "export const x = requestAnimationFrame(() => 1);\n",
            1,
            "uses `requestAnimationFrame`, which does I/O",
        ),
        (
            "export const x = new IntersectionObserver(() => 1);\n",
            1,
            "uses `IntersectionObserver`, which does I/O",
        ),
        (
            "export const x = crypto.subtle.digest('SHA-256', new Uint8Array(1));\n",
            1,
            "uses `crypto` (crypto.subtle), which does I/O",
        ),
        (
            "export const x = Buffer.from('a');\n",
            1,
            "uses `Buffer` (Buffer.from), which does I/O",
        ),
        // `process`, but for `process.env`
        (
            "export const x = process.cwd();\n",
            1,
            "uses `process.cwd`, which does I/O",
        ),
        (
            "export const x = process.platform;\n",
            1,
            "uses `process.platform`, which does I/O",
        ),
        (
            "const { cwd } = process;\nexport const x = cwd();\n",
            1,
            "uses `process.cwd`, which does I/O",
        ),
        (
            "export const x = process;\n",
            1,
            "uses the module `process` as a value",
        ),
        // The clock
        (
            "export const x = Date.now();\n",
            1,
            "reads the clock (Date.now), which does I/O (entry `clock`)",
        ),
        (
            "export const x = new Date();\n",
            1,
            "reads the clock (new Date()), which does I/O (entry `clock`)",
        ),
        (
            "export const x = Date();\n",
            1,
            "reads the clock (Date()), which does I/O (entry `clock`)",
        ),
        (
            "export const f = (...a: number[]) => new Date(...(a as []));\n",
            1,
            "reads the clock (new Date())",
        ),
        (
            "export const x = performance.now();\n",
            1,
            "reads the clock (performance.now)",
        ),
        (
            "export const x = process.hrtime.bigint();\n",
            1,
            "reads the clock (process.hrtime)",
        ),
        (
            "export const x = Temporal.Now.instant();\n",
            1,
            "reads the clock (Temporal.Now.instant)",
        ),
        // A reference counts as a call
        (
            "import { useState } from 'react';\nexport const x = useState(Date.now);\n",
            2,
            "reads the clock (Date.now)",
        ),
        (
            "export const f = Date.now;\n",
            1,
            "reads the clock (Date.now)",
        ),
        // Randomness
        (
            "export const x = Math.random();\n",
            1,
            "draws on randomness (Math.random), which does I/O (entry `random`)",
        ),
        (
            "export const x = crypto.randomUUID();\n",
            1,
            "draws on randomness (crypto.randomUUID)",
        ),
        (
            "export const x = globalThis.crypto.getRandomValues(new Uint8Array(1));\n",
            1,
            "draws on randomness (crypto.getRandomValues)",
        ),
        (
            "export const f = Math.random;\n",
            1,
            "draws on randomness (Math.random)",
        ),
        // The terminal and the arguments
        (
            "console.log('a');\n",
            1,
            "uses the terminal or opens a file (console.log), which does I/O (entry `terminal`)",
        ),
        (
            "export const f = console.error;\n",
            1,
            "uses the terminal or opens a file (console.error)",
        ),
        (
            "process.stdout.write('a');\n",
            1,
            "uses the terminal or opens a file (process.stdout)",
        ),
        (
            "export const x = process.argv.slice(2);\n",
            1,
            "reads the command-line arguments (process.argv), which does I/O (entry `argv`)",
        ),
        // What cannot be read
        (
            "export const x = eval('1');\n",
            1,
            "uses `eval`: what it runs or imports cannot be read",
        ),
        (
            "export const x = window.eval('1');\n",
            1,
            "uses `eval`: what it runs or imports cannot be read",
        ),
        (
            "export const f = new Function('return 1');\n",
            1,
            "uses `Function`: what it runs or imports cannot be read",
        ),
        (
            "export const f = Function('return 1');\n",
            1,
            "uses `Function`: what it runs or imports cannot be read",
        ),
        (
            "export const f = eval;\n",
            1,
            "uses `eval`: what it runs or imports cannot be read",
        ),
        (
            "export const f = (name: string) => import(name);\n",
            1,
            "uses `import()`: what it runs or imports cannot be read",
        ),
        (
            "export const f = (name: string) => require(name);\n",
            1,
            "uses `require`: what it runs or imports cannot be read",
        ),
    ];
    for (source, line, what) in forbidden {
        put(r, "src/domain/x.ts", source);
        let (code, said) = checked(r);
        assert_eq!(code, Some(1), "{source}\n{said}");
        assert!(said.contains(IO_HEADING), "{source}\n{said}");
        assert!(
            said.contains(&format!("  src/domain/x.ts:{line}: ")) && said.contains(what),
            "{source}\n{said}"
        );
    }
    // What is pure, a name the file binds, a name in a type position and the operand of `typeof`
    let free = [
        "export const x = [new Map(), new Set(), JSON.stringify({}), Math.max(1, 2), Number('1'), Object.keys({}), \
         Symbol('a'), new Error('x'), new TypeError('x'), parseInt('1'), Intl.NumberFormat, structuredClone(1)];\n",
        "export const x = [new URL('http://a'), new URLSearchParams(), new Headers(), new TextEncoder(), \
         new AbortController(), new Uint8Array(1), setTimeout, clearTimeout, queueMicrotask, atob, btoa];\n",
        // A moment that is given is pure
        "export const x = [new Date(0), new Date(2020, 1), new Date('2020-01-01'), Date.parse('x'), Date.UTC(1), Date];\n",
        "export const x = globalThis;\n",
        "export type W = typeof window;\nexport let c: Date | undefined;\nexport const w: Window | undefined = undefined;\n",
        "export const x = typeof window !== 'undefined' && typeof document !== 'undefined' && typeof process === 'object';\n",
        "export const x = typeof process.argv + typeof Date.now + typeof fetch;\n",
        "export function f(fetch: (u: string) => void) { return fetch('a'); }\n",
        "export const g = (Date: { now(): number }) => Date.now();\n",
        "export function h(localStorage: Storage) { return localStorage.getItem('a'); }\n",
        "export function k() { const console = { log: (s: string) => s }; return console.log('a'); }\n",
        "export function m(process: { cwd(): string }) { return process.cwd(); }\n",
        "import { fetch } from './net';\nexport const x = fetch('a');\n",
        "import { readFile } from './files';\nexport const x = readFile;\n",
        "export const x = import.meta.url;\n",
    ];
    for source in free {
        put(r, "src/domain/x.ts", source);
        let (code, said) = checked(r);
        assert_eq!(code, Some(0), "{source}\n{said}");
    }
    // `process.env` is the environment check's alone, and is not reported twice
    put(r, "src/domain/x.ts", "export const x = process.env.A;\n");
    let (code, said) = checked(r);
    assert_eq!(code, Some(1), "{said}");
    assert!(
        said.contains("src/domain/x.ts:1: process.env") && !said.contains(IO_HEADING),
        "{said}"
    );
    // The layers that do the I/O, wire and draw, the entry file and the tests do all of it
    put(r, "src/domain/x.ts", "");
    let all = "export const x = [fetch('a'), Date.now(), Math.random(), localStorage.length, process.cwd()];\n\
               console.log(process.argv, eval('1'));\n";
    for path in [
        "src/infrastructure/outbound/x.ts",
        "src/infrastructure/inbound/x.ts",
        "src/composition/x.ts",
        "src/ui/atoms/x.ts",
        "src/main.tsx",
        "src/domain/x.test.ts",
        "src/domain/x.spec.tsx",
        "src/domain/__tests__/x.ts",
    ] {
        put(r, path, all);
    }
    let (_, said) = checked(r);
    assert!(
        !said.contains(IO_HEADING) && !said.contains("does I/O"),
        "{said}"
    );
}

#[test]
fn a_global_that_does_io_says_what_to_do_in_the_layer_it_is_in() {
    let root = repo_with_ui("typescript", "");
    let r = root.path();
    put(
        r,
        "src/presentation/models/x.ts",
        "export const x = window.matchMedia('(min-width: 1px)').matches;\nexport const y = Date.now();\n",
    );
    put(
        r,
        "src/application/y.ts",
        "export const y = Date.now();\nexport const z = localStorage.length;\nexport const a = process.argv;\n",
    );
    let (code, said) = checked(r);
    assert_eq!(code, Some(1), "{said}");
    // A screen's model gets it from its page, through a hook in `ui.atoms`, or through a use case
    assert!(
        said.contains(
            "  src/presentation/models/x.ts:1: a screen's model does no I/O: it gets what it needs from its page, \
             through a hook in `ui.atoms`, or through a use case of protocol.inbound it is given: uses `matchMedia`, \
             which does I/O\n"
        ),
        "{said}"
    );
    assert!(
        said.contains(
            "  src/presentation/models/x.ts:2: a screen's model does no I/O: it gets the time now from its page, \
             through a hook in `ui.atoms`, or through a use case of protocol.inbound it is given: reads the clock \
             (Date.now), which does I/O (entry `clock`)\n"
        ),
        "{said}"
    );
    // The other layers pass the value in, or take it from a port that may be a callable alias
    assert!(
        said.contains(
            "  src/application/y.ts:1: pass the time now in as an argument, or take it from a port in protocol.outbound \
             that the use case is given (a port may be a callable alias, `() => Date`): reads the clock (Date.now), \
             which does I/O (entry `clock`)\n"
        ),
        "{said}"
    );
    assert!(
        said.contains(&format!(
            "  src/application/y.ts:2: {IO_FIX}: uses `localStorage` (localStorage.length), which does I/O\n"
        )),
        "{said}"
    );
    assert!(
        said.contains(
            "  src/application/y.ts:3: read the arguments in infrastructure and pass the value in as an argument: \
             reads the command-line arguments (process.argv), which does I/O (entry `argv`)\n"
        ),
        "{said}"
    );
}

#[test]
fn an_approved_typescript_global_is_an_entry_by_its_name_and_a_group_is_one_entry() {
    let root = repo_with_ui("typescript", "");
    let r = root.path();
    let named = Some("[user]\n\tname = Someone Else\n");
    let approve = |entry: &str| {
        let out = approve_with_git(
            r,
            named,
            &[
                "src/domain/model.ts",
                entry,
                "--reason",
                "the port comes later",
            ],
        );
        assert_eq!(
            out.status.code(),
            Some(0),
            "{entry}: {}{}",
            stdout(&out),
            String::from_utf8_lossy(&out.stderr)
        );
    };
    let source = |body: &str| format!("export function f() {{\n{body}}}\n");
    put(
        r,
        "src/domain/model.ts",
        &source(
            "  const a = Date.now();\n  const b = performance.now();\n  const c = localStorage.getItem('k');\n  \
             const d = process.cwd();\n  const e = fetch('u');\n  return [a, b, c, d, e];\n",
        ),
    );
    let (code, said) = checked(r);
    assert_eq!(code, Some(1), "{said}");
    // The act of a group is one entry, whichever names the file reaches it by
    approve("clock");
    let (code, said) = checked(r);
    assert_eq!(code, Some(1), "{said}");
    assert!(!said.contains("reads the clock"), "{said}");
    assert!(
        said.contains("clock in src/domain/model.ts: the port comes later (Someone Else, ")
            && said.contains("; now used: Date.now, performance.now\n"),
        "{said}"
    );
    // A global is an entry of its own, and so is a member of `process`
    approve("localStorage");
    approve("process.cwd");
    let (code, said) = checked(r);
    assert_eq!(code, Some(1), "{said}");
    assert!(
        said.contains("uses `fetch`")
            && !said.contains("uses `localStorage`")
            && !said.contains("uses `process.cwd`"),
        "{said}"
    );
    approve("fetch");
    let (code, said) = checked(r);
    assert_eq!(code, Some(0), "{said}");
    assert!(
        said.contains("approved in .config/rotproof-approved.toml (4):")
            && said.contains("  fetch in src/domain/model.ts: the port comes later")
            && said.contains("  localStorage in src/domain/model.ts: the port comes later")
            && said.contains("; now used: localStorage.getItem\n"),
        "{said}"
    );
    // The names change, so the reason is read again; and what is no longer used is a stale entry
    put(
        r,
        "src/domain/model.ts",
        &source("  const a = Date.now();\n  const c = localStorage.length;\n  return [a, c];\n"),
    );
    let (code, said) = checked(r);
    assert_eq!(code, Some(1), "{said}");
    assert!(
        said.contains("; now used: localStorage.length\n")
            && !said.contains("; now used: localStorage.getItem"),
        "{said}"
    );
    for stale in ["process.cwd", "fetch"] {
        assert!(
            said.contains(&format!(
                "approved, but no such forbidden import: {stale} in src/domain/model.ts"
            )),
            "{stale}\n{said}"
        );
    }
    // A group not approved is not covered by another's approval
    put(
        r,
        "src/domain/model.ts",
        &source("  const a = Math.random();\n  const c = localStorage.length;\n  return [a, c];\n"),
    );
    let (_, said) = checked(r);
    assert!(said.contains("draws on randomness (Math.random)"), "{said}");
    // Only what the check finds is approved
    let out = approve_with_git(
        r,
        named,
        &["src/domain/model.ts", "navigator", "--reason", "no"],
    );
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("no forbidden import of navigator"));
    // The environment check is not approved
    put(
        r,
        "src/domain/model.ts",
        "export const x = process.env.A;\n",
    );
    let out = approve_with_git(
        r,
        named,
        &["src/domain/model.ts", "process.env", "--reason", "no"],
    );
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn the_guide_of_a_typescript_project_lists_the_allowlist_in_full_and_the_way_out() {
    let guide = stdout(&run(&["guide", "--stack", "typescript"]));
    for name in [
        // the allowlist of globals
        "`Object`",
        "`FinalizationRegistry`",
        "`AggregateError`",
        "`Intl`",
        "`Float64Array`",
        "`encodeURIComponent`",
        "`structuredClone`",
        "`setInterval`",
        "`URLSearchParams`",
        "`TextDecoder`",
        "`DOMException`",
        "`FormData`",
        // what is not on it, and the groups
        "`localStorage`",
        "`matchMedia`",
        "`Date.now`",
        "`new Date()`",
        "`performance`",
        "`Temporal.Now.*`",
        "`Math.random`",
        "`randomUUID`",
        "`getRandomValues`",
        "`console`",
        "`process.argv`",
        "`process.cwd`",
        "`() => Date`",
        "`eval`",
        "`Function`",
        "`clock`",
        "`random`",
        "`terminal`",
        "`argv`",
    ] {
        assert!(guide.contains(name), "{name}\n{guide}");
    }
    assert!(
        guide.contains("`fetch`, `localStorage` or `process.cwd` for a global judged by its name"),
        "{guide}"
    );
    assert!(
        guide.contains("through a hook in `ui.atoms`, or through a use case it is given"),
        "{guide}"
    );
    assert!(
        guide.contains("Approving the import,\n  renaming or respelling it"),
        "{guide}"
    );
}

#[test]
fn the_inner_crates_do_no_io_in_rust_and_a_test_is_free() {
    let root = repo_with_ui("rust", "");
    let r = root.path();
    let (code, said) = checked(r);
    assert_eq!(code, Some(0), "{said}");
    let lib = std::fs::read_to_string(r.join("crates/domain/src/lib.rs")).unwrap();
    plant_io(
        r,
        "crates/domain/src/lib.rs",
        &[
            (
                "pub fn a() { let _ = std::fs::read(\"x\"); }\n",
                1,
                "`std::fs` (read)",
            ),
            (
                "use std::process::Command;\n",
                1,
                "`std::process` (Command)",
            ),
            (
                "use std::os::unix::fs::PermissionsExt;\n",
                1,
                "`std::os::fs` (PermissionsExt)",
            ),
            (
                "pub fn a() { println!(\"x\"); }\n",
                1,
                "`std::io` (println!)",
            ),
            ("pub fn a() { std::dbg!(1); }\n", 1, "`std::io` (dbg!)"),
            ("use std::io::stdin;\n", 1, "`std::io` (stdin)"),
            ("use std::net::TcpStream;\n", 1, "`std::net` (TcpStream)"),
            (
                "use std::env::current_dir;\n",
                1,
                "`std::env` (current_dir)",
            ),
            (
                "use std::fs as f;\npub fn a() { f::write(\"a\", \"b\").ok(); }\n",
                2,
                "`std::fs` (write)",
            ),
            (
                "use std as s;\npub fn a() { s::process::exit(0); s::io::stdout(); }\n",
                2,
                "`std::io` (stdout)",
            ),
        ],
        &[
            "use std::io::Write;\nuse std::path::PathBuf;\nuse std::process::{ExitCode, exit};\n",
            "use std::env::{consts, split_paths};\nuse std::net::{IpAddr, SocketAddr};\npub fn a() { let _ = format!(\"x\"); }\n",
            "#[cfg(test)]\nmod tests {\n    use std::fs;\n    #[test]\n    fn t() { fs::read(\"x\").ok(); println!(\"x\"); }\n}\n",
            "#[cfg(test)]\nuse std::process::Command;\n",
            "#![cfg(test)]\nuse std::fs;\n",
        ],
        &lib,
    );
    // A glob names nothing: a glob of the module, and one above it, fail
    for (source, said_of) in [
        ("use std::fs::*;\n", "a glob of `std::fs`"),
        ("use std::*;\n", "a glob of `std`"),
    ] {
        put(r, "crates/domain/src/lib.rs", source);
        let (code, said) = checked(r);
        assert_eq!(code, Some(1), "{source}\n{said}");
        assert!(
            said.contains(said_of) && said.contains("cannot be read"),
            "{source}\n{said}"
        );
    }
    put(r, "crates/domain/src/lib.rs", &lib);
    // What is outside the crate's `src/`, in a test module's file, in the I/O crate and in the entry file is not judged
    put(
        r,
        "crates/domain/tests/it.rs",
        "use std::fs;\n#[test]\nfn t() { std::fs::read(\"x\").ok(); }\n",
    );
    put(
        r,
        "crates/domain/benches/b.rs",
        "use std::process::Command;\n",
    );
    put(
        r,
        "crates/domain/build.rs",
        "fn main() { std::fs::write(\"a\", \"b\").ok(); }\n",
    );
    put(
        r,
        "crates/infrastructure/src/outbound/mod.rs",
        "pub fn f() { std::fs::read(\"x\").ok(); }\n",
    );
    put(r, "src/main.rs", "fn main() { println!(\"x\"); }\n");
    put(
        r,
        "crates/domain/src/lib.rs",
        &format!("{lib}#[cfg(test)]\nmod checks;\n"),
    );
    put(
        r,
        "crates/domain/src/checks.rs",
        "use std::fs;\nuse std::process::Command;\n",
    );
    let (_, said) = checked(r);
    assert!(!said.contains(IO_HEADING), "{said}");
    // The same file is code once it is no module under `#[cfg(test)]`
    put(
        r,
        "crates/domain/src/lib.rs",
        &format!("{lib}mod checks;\n"),
    );
    let (code, said) = checked(r);
    assert_eq!(code, Some(1), "{said}");
    assert!(said.contains("  crates/domain/src/checks.rs:1: "), "{said}");
    let guide = stdout(&run(&["guide", "--stack", "rust"]));
    for name in [
        "`std::fs`",
        "`Command`",
        "`println!`",
        "`std::os::fs`",
        "`current_exe`",
    ] {
        assert!(guide.contains(name), "{name}");
    }
}

#[test]
fn the_environment_check_reads_a_rust_test_as_the_check_of_the_io_does() {
    let root = repo_with_ui("rust", "");
    let r = root.path();
    let lib = std::fs::read_to_string(r.join("crates/domain/src/lib.rs")).unwrap();
    let reads = "std::env::temp_dir()";
    let read = |what: &str| format!("pub fn t() {{ let _ = ({what}, std::env::var(\"A\")); }}\n");
    let (code, said) = checked(r);
    assert_eq!(code, Some(0), "{said}");
    // The same read in a file of the layer's `src/` that is no test fails, and says where
    put(r, "crates/domain/src/other.rs", &read(reads));
    let (code, said) = checked(r);
    assert_eq!(code, Some(1), "{said}");
    assert!(
        said.contains("crates/domain/src/other.rs:1: std::env::temp_dir")
            && said.contains("crates/domain/src/other.rs:1: std::env::var"),
        "{said}"
    );
    std::fs::remove_file(r.join("crates/domain/src/other.rs")).unwrap();
    // Each form that is a test passes: the items under `#[cfg(test)]` (a module with a body, a function, a use), the
    // file of a `#[cfg(test)] mod x;` as `x.rs` and as `x/mod.rs`, with the files under it, a file that starts
    // with `#![cfg(test)]`, and a crate's `tests/`
    put(
        r,
        "crates/domain/src/lib.rs",
        &format!(
            "{lib}#[cfg(test)]\nmod tests {{\n    fn t() {{ let _ = {reads}; }}\n}}\n#[cfg(test)]\nfn u() {{ let _ = std::env::var(\"B\"); }}\n#[cfg(test)]\nuse std::env::var;\n"
        ),
    );
    let (code, said) = checked(r);
    assert_eq!(code, Some(0), "{said}");
    put(
        r,
        "crates/domain/src/lib.rs",
        &format!("{lib}#[cfg(test)]\nmod checks;\n#[cfg(test)]\nmod more;\n"),
    );
    put(r, "crates/domain/src/checks.rs", &read(reads));
    put(r, "crates/domain/src/more/mod.rs", &read(reads));
    put(r, "crates/domain/src/more/deeper.rs", &read(reads));
    let (code, said) = checked(r);
    assert_eq!(code, Some(0), "{said}");
    put(
        r,
        "crates/domain/src/lib.rs",
        &format!("{lib}#[cfg(test)]\nmod checks;\n"),
    );
    std::fs::remove_dir_all(r.join("crates/domain/src/more")).unwrap();
    put(
        r,
        "crates/domain/src/checks.rs",
        &format!("#![cfg(test)]\n{}", read(reads)),
    );
    put(r, "crates/domain/tests/it.rs", &read(reads));
    let (code, said) = checked(r);
    assert_eq!(code, Some(0), "{said}");
    // What is no test of the same crate fails still, whatever sits beside it: the module not declared under
    // `#[cfg(test)]`, and the same read in the code outside the item
    put(r, "crates/domain/src/checks.rs", &read(reads));
    put(
        r,
        "crates/domain/src/lib.rs",
        &format!("{lib}mod checks;\n"),
    );
    let (code, said) = checked(r);
    assert_eq!(code, Some(1), "{said}");
    assert!(
        said.contains("crates/domain/src/checks.rs:1: std::env::temp_dir"),
        "{said}"
    );
    put(
        r,
        "crates/domain/src/lib.rs",
        &format!("{lib}#[cfg(test)]\nmod t {{}}\npub fn u() {{ let _ = {reads}; }}\n"),
    );
    let (code, said) = checked(r);
    assert_eq!(code, Some(1), "{said}");
    assert!(said.contains("std::env::temp_dir"), "{said}");
}

/// Plant each source in the domain crate of a Rust project: the check fails under the heading of the I/O of the inner
/// layers, on `line`, and says `said`. The crate is put back as it was.
fn plant_rust(r: &Path, planted: &[(&str, usize, &str)], reset: &str) {
    let path = "crates/domain/src/lib.rs";
    for (source, line, said_of) in planted {
        put(r, path, source);
        let (code, said) = checked(r);
        assert_eq!(code, Some(1), "{source}\n{said}");
        assert!(said.contains(IO_HEADING), "{source}\n{said}");
        assert!(
            said.contains(&format!("  {path}:{line}: ")) && said.contains(said_of),
            "{source}\n{said_of}\n{said}"
        );
    }
    put(r, path, reset);
}

#[test]
fn each_rust_name_that_does_io_in_part_fails_alone_and_what_is_pure_passes() {
    let root = repo_with_ui("rust", "");
    let r = root.path();
    let lib = std::fs::read_to_string(r.join("crates/domain/src/lib.rs")).unwrap();
    let path_fn = |call: &str| format!("pub fn a(p: &std::path::Path) {{ let _ = p.{call}; }}\n");
    let net = "a method of a path that does I/O";
    let mut planted: Vec<(String, usize, String)> = Vec::new();
    // The names of `std::env` that are I/O, each an entry of its own under `std::env`
    for (source, name) in [
        (
            "pub fn a() { let _ = std::env::current_dir(); }\n",
            "current_dir",
        ),
        (
            "use std::env;\npub fn a() { env::set_current_dir(\"x\").ok(); }\n",
            "set_current_dir",
        ),
        (
            "use std::env as e;\npub fn a() { let _ = e::current_exe(); }\n",
            "current_exe",
        ),
        ("pub fn a(x: std::env::Args) {}\n", "Args"),
    ] {
        let line = source.lines().count();
        planted.push((
            source.to_string(),
            line,
            format!("uses `std::env` ({name}), which does I/O"),
        ));
    }
    // The command-line arguments are `argv`
    for (source, name) in [
        (
            "pub fn a() { let _ = std::env::args(); }\n",
            "std::env::args",
        ),
        (
            "use std::env::args_os;
",
            "std::env::args_os",
        ),
    ] {
        planted.push((
            source.to_string(),
            1,
            format!(
                "read the arguments in infrastructure and pass the value in as an argument: reads the command-line \
                 arguments ({name}), which does I/O (entry `argv`)"
            ),
        ));
    }
    // The clocks: as a call, as a reference, through a rename and through `use`, and the epoch read as the time now
    for (source, name) in [
        (
            "pub fn a() { let _ = std::time::SystemTime::now(); }\n",
            "std::time::SystemTime::now",
        ),
        (
            "use std::time::Instant;\npub fn a() { let _ = Instant::now(); }\n",
            "std::time::Instant::now",
        ),
        (
            "use std::time::Instant;\npub fn a() { let f = Instant::now; let _ = f; }\n",
            "std::time::Instant::now",
        ),
        (
            "use std::time::SystemTime as T;\npub fn a() { let _ = T::now(); }\n",
            "std::time::SystemTime::now",
        ),
        (
            "use std as s;\npub fn a() { let _ = s::time::Instant::now(); }\n",
            "std::time::Instant::now",
        ),
        (
            "pub fn a() { let _ = std::time::UNIX_EPOCH.elapsed(); }\n",
            "std::time::UNIX_EPOCH.elapsed",
        ),
        (
            "use std::time::SystemTime;\npub fn a() { let _ = SystemTime::UNIX_EPOCH.elapsed(); }\n",
            "std::time::UNIX_EPOCH.elapsed",
        ),
        (
            "use std::time::UNIX_EPOCH as E;\npub fn a() { let _ = E.elapsed(); }\n",
            "std::time::UNIX_EPOCH.elapsed",
        ),
    ] {
        planted.push((
            source.to_string(),
            source.lines().count(),
            format!(
                "pass the time now in as an argument, or take it from a port in protocol.outbound that the use case is \
                 given (a port may be a callable alias, `Box<dyn Fn() -> SystemTime>`, a generic `F: Fn() -> \
                 SystemTime`): reads the clock ({name}), which does I/O (entry `clock`)"
            ),
        ));
    }
    // The net of the names of a path's methods, on any receiver
    for call in [
        "canonicalize()",
        "read_dir()",
        "read_link()",
        "try_exists()",
        "symlink_metadata()",
        "is_symlink()",
        "exists()",
        "is_file()",
        "is_dir()",
        "metadata()",
    ] {
        let name = call.trim_end_matches("()");
        planted.push((path_fn(call), 1, format!("calls `.{name}(...)`, {net}")));
    }
    planted.push((
        "pub fn a(store: &Store) { let _ = store.canonicalize(1, 2); }\n".to_string(),
        1,
        format!("calls `.canonicalize(...)`, {net}"),
    ));
    // A method named by the standard library's path, a reference included, and `std::path::absolute`
    for (source, said_of) in [
        (
            "use std::path::Path;\npub fn a() -> bool { Path::exists(Path::new(\"x\")) }\n",
            "uses `exists` (Path::exists), which does I/O",
        ),
        (
            "pub fn a(v: Vec<&std::path::Path>) { let _ = v.into_iter().filter(|p| p.is_dir()).count(); let _ = std::path::Path::is_file; }\n",
            "uses `is_file` (Path::is_file), which does I/O",
        ),
        (
            "pub fn a() { let _ = std::path::absolute(\"x\"); }\n",
            "uses `std::path` (absolute), which does I/O",
        ),
        (
            "use std::path::absolute;\n",
            "uses `std::path` (absolute), which does I/O",
        ),
    ] {
        planted.push((
            source.to_string(),
            source.lines().count(),
            said_of.to_string(),
        ));
    }
    // What cannot be read: an `extern` block, and a glob of `std::env` or `std::time`
    for (source, said_of) in [
        (
            "extern \"C\" {\n    fn abs(x: i32) -> i32;\n}\n",
            "declares an `extern` block",
        ),
        (
            "unsafe extern \"C\" {\n    pub safe fn sqrt(x: f64) -> f64;\n}\n",
            "declares an `extern` block",
        ),
        ("use std::env::*;\n", "a glob of `std::env`"),
        ("use std::time::*;\n", "a glob of `std::time`"),
    ] {
        planted.push((source.to_string(), 1, said_of.to_string()));
    }
    let planted: Vec<(&str, usize, &str)> = planted
        .iter()
        .map(|(source, line, said)| (source.as_str(), *line, said.as_str()))
        .collect();
    // `line` of a source is the line of its last statement, which is where each form above is written; the extern
    // blocks and the globs start on the first
    plant_rust(r, &planted, &lib);

    // What is pure passes
    let free = [
        "use std::env::{consts, split_paths, join_paths, VarError, JoinPathsError, SplitPaths};\npub fn a() -> &'static str { std::env::consts::OS }\n",
        "pub fn a(p: &std::path::Path, store: &Store) { let _ = (p.exists(1), p.is_file(2, 3), p.metadata(|a, b| a), p.get(), p.exist()); let _ = store; }\n",
        "pub fn a(p: &std::path::Path) -> std::path::PathBuf { p.join(\"x\").to_path_buf() }\n",
        "use std::time::{Duration, SystemTime};\npub fn a(t: SystemTime) -> Duration { Duration::from_secs(1) + t.duration_since(SystemTime::UNIX_EPOCH).unwrap() }\n",
        "pub struct Instant;\nimpl Instant { pub fn now() -> u8 { 1 } }\npub fn a() -> u8 { Instant::now() }\n",
        "extern crate alloc;\npub extern \"C\" fn callback() {}\npub type F = extern \"C\" fn(u8);\n",
        "pub fn a() -> Vec<u8> { vec![] }\n#[cfg(test)]\nmod tests {\n    use std::time::Instant;\n    #[test]\n    fn t() { let _ = (Instant::now(), std::env::current_dir(), std::path::Path::new(\"x\").exists()); }\n    extern \"C\" { fn x(); }\n}\n",
    ];
    for source in free {
        put(r, "crates/domain/src/lib.rs", source);
        let (code, said) = checked(r);
        assert_eq!(code, Some(0), "{source}\n{said}");
    }
    // The functions of the environment are the environment check's alone, and are not reported twice
    put(
        r,
        "crates/domain/src/lib.rs",
        "pub fn a() { let _ = std::env::var(\"A\"); let _ = std::env::temp_dir(); }\n",
    );
    let (code, said) = checked(r);
    assert_eq!(code, Some(1), "{said}");
    assert!(
        said.contains("std::env::var") && !said.contains(IO_HEADING),
        "{said}"
    );
    // The outer layers, the entry file and the tests do all of it
    put(r, "crates/domain/src/lib.rs", &lib);
    let all = "pub fn a(p: &std::path::Path) { let _ = (std::time::Instant::now(), std::env::args(), std::env::current_dir(), p.canonicalize(), p.exists()); }\nextern \"C\" { fn abs(x: i32) -> i32; }\n";
    for path in [
        "crates/infrastructure/src/outbound/mod.rs",
        "crates/domain/tests/it.rs",
        "crates/domain/benches/b.rs",
        "crates/domain/build.rs",
        "src/main.rs",
    ] {
        put(r, path, all);
    }
    let (_, said) = checked(r);
    assert!(!said.contains(IO_HEADING), "{said}");
}

#[test]
fn a_rust_method_the_project_defines_is_left_out_of_the_net_and_the_line_is_printed_on_every_run() {
    let root = repo_with_ui("rust", "");
    let r = root.path();
    let file = "crates/domain/src/lib.rs";
    let lib = "pub fn f(p: &std::path::Path, store: &Store) {\n    let a = p.exists();\n    let b = p.read_dir();\n    let c = store.exists();\n    let d = store.read_dir(p);\n    let e = store.is_file();\n    let _ = (a, b, c, d, e);\n}\n";
    put(r, file, lib);
    let (code, said) = checked(r);
    assert_eq!(code, Some(1), "{said}");
    assert!(!said.contains("I/O net:"), "{said}");
    let flagged = |said: &str| named_lines(said, file);
    assert_eq!(flagged(&said), [2, 3, 4, 5, 6], "{said}");
    // The project defines a method of the same name and number of parameters, in any file of it: a trait of another
    // layer, an impl in a test
    put(
        r,
        "crates/infrastructure/src/outbound/mod.rs",
        "pub trait Store { fn exists(&self) -> bool; fn read_dir(&self, p: &std::path::Path); }\n",
    );
    put(
        r,
        "crates/domain/tests/fake.rs",
        "struct Fake;\nimpl Fake { fn is_file(&self, a: u8, b: u8) {} fn metadata(&self) {} }\n",
    );
    let (code, said) = checked(r);
    assert_eq!(code, Some(1), "{said}");
    assert!(
        said.contains(
            "I/O net: left out, as the project defines a method of the same name and number of parameters: \
             exists(0), metadata(0), read_dir(1)\n"
        ),
        "{said}"
    );
    // `exists()` and `read_dir(p)` are left out on any receiver; `read_dir()` takes none, and `is_file()` is called
    // with none where the project's takes two
    assert_eq!(flagged(&said), [3, 6], "{said}");
    // The same names with another number of parameters leave nothing out of those calls
    put(
        r,
        "crates/infrastructure/src/outbound/mod.rs",
        "pub trait Store { fn exists(&self, a: u8) -> bool; fn read_dir(&self); }\n",
    );
    let (_, said) = checked(r);
    assert_eq!(flagged(&said), [2, 4, 5, 6], "{said}");
    // It is printed when nothing fails too
    put(r, file, "pub fn f(a: u8) -> u8 { a }\n");
    let (code, said) = checked(r);
    assert_eq!(code, Some(0), "{said}");
    assert!(said.contains("I/O net: left out"), "{said}");
}

#[test]
fn an_approved_rust_name_is_an_entry_by_its_name_and_a_group_is_one_entry() {
    let root = repo_with_ui("rust", "");
    let r = root.path();
    let file = "crates/domain/src/lib.rs";
    let named = Some("[user]\n\tname = Someone Else\n");
    let approve = |entry: &str| {
        let out = approve_with_git(r, named, &[file, entry, "--reason", "the port comes later"]);
        assert_eq!(
            out.status.code(),
            Some(0),
            "{entry}: {}{}",
            stdout(&out),
            String::from_utf8_lossy(&out.stderr)
        );
    };
    put(
        r,
        file,
        "use std::time::{Instant, SystemTime};\npub fn f(p: &std::path::Path) {\n    let a = Instant::now();\n    let b = SystemTime::now();\n    let c = std::env::current_dir();\n    let d = std::env::args();\n    let e = p.canonicalize();\n    let g = std::path::absolute(\"x\");\n    let _ = (a, b, c, d, e, g);\n}\nextern \"C\" { fn abs(x: i32) -> i32; }\n",
    );
    let (code, said) = checked(r);
    assert_eq!(code, Some(1), "{said}");
    // The act of a group is one entry, whichever names the file reaches it by
    approve("clock");
    let (code, said) = checked(r);
    assert_eq!(code, Some(1), "{said}");
    assert!(!said.contains("reads the clock"), "{said}");
    assert!(
        said.contains("clock in crates/domain/src/lib.rs: the port comes later (Someone Else, ")
            && said.contains("; now used: std::time::Instant::now, std::time::SystemTime::now\n"),
        "{said}"
    );
    // A name of `std::env` is an entry of its own, a method of the net its own, and `std::path` its own
    approve("std::env");
    approve("canonicalize");
    // A name of `std::env` approved does not cover the arguments, which are a group of their own
    let (_, said) = checked(r);
    assert!(said.contains("reads the command-line arguments"), "{said}");
    approve("argv");
    approve("std::path");
    let (code, said) = checked(r);
    assert_eq!(code, Some(1), "{said}");
    assert!(
        said.contains("declares an `extern` block")
            && !said.contains("uses `std::env`")
            && !said.contains("calls `.canonicalize(...)`")
            && !said.contains("(entry `argv`)")
            && !said.contains("uses `std::path`"),
        "{said}"
    );
    assert!(
        said.contains("; now used: current_dir\n") && said.contains("; now used: std::env::args\n"),
        "{said}"
    );
    approve("extern");
    let (code, said) = checked(r);
    assert_eq!(code, Some(0), "{said}");
    assert!(
        said.contains("approved in .config/rotproof-approved.toml (6):"),
        "{said}"
    );
    // The names change, so the reason is read again; and what is no longer used is a stale entry
    put(
        r,
        file,
        "use std::time::Instant;\npub fn f() {\n    let a = Instant::now();\n    let c = std::env::set_current_dir(\"x\");\n    let _ = (a, c);\n}\n",
    );
    let (code, said) = checked(r);
    assert_eq!(code, Some(1), "{said}");
    assert!(
        said.contains("; now used: set_current_dir\n")
            && !said.contains("; now used: current_dir\n"),
        "{said}"
    );
    for stale in ["canonicalize", "argv", "std::path", "extern"] {
        assert!(
            said.contains(&format!(
                "approved, but no such forbidden import: {stale} in {file}"
            )),
            "{stale}\n{said}"
        );
    }
    // Only what the check finds is approved
    let out = approve_with_git(r, named, &[file, "std::net", "--reason", "no"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("no forbidden import of std::net"));
    // The environment check is not approved
    put(r, file, "pub fn f() { let _ = std::env::var(\"A\"); }\n");
    let out = approve_with_git(r, named, &[file, "std::env::var", "--reason", "no"]);
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn the_guide_of_a_rust_project_lists_the_allowlist_in_full_and_the_way_out() {
    let guide = stdout(&run(&["guide", "--stack", "rust"]));
    for name in [
        // std::env: what is pure, and what is not
        "`consts`",
        "`split_paths`",
        "`join_paths`",
        "`VarError`",
        "`JoinPathsError`",
        "`SplitPaths`",
        "`current_dir`",
        "`args_os`",
        // the clocks, the net and what cannot be read
        "`std::time::SystemTime::now`",
        "`std::time::Instant::now`",
        "`UNIX_EPOCH.elapsed()`",
        "`canonicalize`",
        "`read_dir`",
        "`read_link`",
        "`try_exists`",
        "`symlink_metadata`",
        "`is_symlink`",
        "`exists`",
        "`is_file`",
        "`is_dir`",
        "`metadata`",
        "`std::path::absolute`",
        "`extern`",
        "`Path::exists`",
        "`Box<dyn Fn() -> SystemTime>`",
        "`F: Fn() -> SystemTime`",
        "`clock`",
        "`argv`",
    ] {
        assert!(guide.contains(name), "{name}\n{guide}");
    }
    assert!(
        guide.contains("`std::env` (`current_dir`) for a name of it that is I/O"),
        "{guide}"
    );
    assert!(
        guide.contains("Approving the import,\n  renaming or respelling it"),
        "{guide}"
    );
    // The other stacks do not claim what is Rust's
    for stack in ["python", "typescript"] {
        let guide = stdout(&run(&["guide", "--stack", stack]));
        assert!(!guide.contains("`Box<dyn Fn() -> SystemTime>`"), "{stack}");
        assert!(!guide.contains("`UNIX_EPOCH.elapsed()`"), "{stack}");
    }
}

/// The `package.json` of a TypeScript project that lists `vitest` for development and `react` to run.
const VITEST_PACKAGE: &str = "{\n  \"name\": \"shop\",\n  \"dependencies\": { \"react\": \"^18\" },\n  \
                              \"devDependencies\": { \"vitest\": \"^2\", \"@types/node\": \"^22\" }\n}\n";

/// A TypeScript project that keeps every rule and lists `vitest` for development, with `react` imported by its UI.
fn typescript_with_vitest() -> tempfile::TempDir {
    let root = repo_with_ui("typescript", "");
    put(root.path(), "package.json", VITEST_PACKAGE);
    put(
        root.path(),
        "src/ui/atoms/Button.tsx",
        "import React from 'react';\nexport const Button = () => <p />;\n",
    );
    let out = run(&["--root", &root_arg(root.path()), "check"]);
    assert!(out.status.success(), "{}", stdout(&out));
    root
}

#[test]
fn a_typescript_test_is_told_by_its_name_and_every_rule_reads_it_as_one_and_says_how_many() {
    let root = typescript_with_vitest();
    let r = root.path();
    // Each file does what no code of a layer may, and sits where a test may: in a layer, in a directory of tests, and
    // in no layer at all. A file under `__tests__/` is a test whatever it is called, and a JSON file is read by none
    put(
        r,
        "src/domain/song.test.ts",
        "import { test } from 'vitest';\nimport { play } from '../application/play';\n\
         import { helper } from './__tests__/helper';\nexport const a = process.env.A;\n",
    );
    put(
        r,
        "src/domain/__tests__/helper.ts",
        "import { db } from '../../infrastructure/db';\nexport const helper = import.meta.env.VITE_X;\n",
    );
    put(r, "src/domain/__tests__/fixture.json", "{}\n");
    put(
        r,
        "src/helpers.spec.ts",
        "import { test } from 'vitest';\nimport { Home } from './ui/pages/Home';\n",
    );
    put(
        r,
        "src/presentation/models/useForm.test.tsx",
        "export const Wrapper = () => <p />;\n",
    );
    let out = run(&["--root", &root_arg(r), "check"]);
    let said = stdout(&out);
    assert_eq!(
        said,
        "tests: 4 files read as tests (*.test.*, *.spec.*, __tests__/)\n\
         the layers and every record keep the rules\n"
    );
    // One file is one: said in the singular
    fs::remove_file(r.join("src/domain/song.test.ts")).unwrap();
    fs::remove_file(r.join("src/domain/__tests__/helper.ts")).unwrap();
    fs::remove_file(r.join("src/helpers.spec.ts")).unwrap();
    let said = stdout(&run(&["--root", &root_arg(r), "check"]));
    assert!(
        said.starts_with("tests: 1 file read as a test (*.test.*, *.spec.*, __tests__/)\n"),
        "{said}"
    );

    // A test is code for the checks of comments, as the files of Python's `tests/` are
    put(
        r,
        "src/domain/song.test.ts",
        "// TODO: more\n// @ts-ignore\nexport const a = 1;\n",
    );
    let out = run(&["--root", &root_arg(r), "check"]);
    let said = stdout(&out);
    assert_eq!(out.status.code(), Some(1), "{said}");
    assert!(
        said.contains("  src/domain/song.test.ts:1\n    // TODO: more\n"),
        "{said}"
    );
    assert!(
        said.contains("  src/domain/song.test.ts:2\n    // @ts-ignore\n"),
        "{said}"
    );
    assert!(said.starts_with("tests: 2 files read as tests"), "{said}");

    // The same names in a path the project keeps out of the checks are not read, so not counted
    put(r, "src/legacy/old.test.ts", "export const a = 1;\n");
    let said = stdout(&run(&["--root", &root_arg(r), "check"]));
    assert!(said.starts_with("tests: 3 files read as tests"), "{said}");
    declare(
        r,
        "stack = \"typescript\"\nareas = [\"a\"]\nui_design = \"atomic\"\nunchecked = [\"src/legacy\"]\n",
    );
    let said = stdout(&run(&["--root", &root_arg(r), "check"]));
    assert!(said.starts_with("tests: 2 files read as tests"), "{said}");

    // Python and Rust tell no test by its name, and say nothing of tests
    for stack in ["python", "rust"] {
        let root = repo_with_ui(stack, "");
        let said = stdout(&run(&["--root", &root_arg(root.path()), "check"]));
        assert!(!said.contains("tests:"), "{stack}: {said}");
    }
}

#[test]
fn code_that_is_not_a_test_imports_no_test_in_typescript_and_in_python() {
    let finding = |file: &str, line: usize, import: &str, target: &str| {
        format!(
            "{file}:{line}: imports {import} ({target}), a test (*.test.*, *.spec.*, __tests__/); code that is not a \
             test imports no test: move what it needs out of the test into a layer"
        )
    };
    // The import of a layer's code by a file named as a test is none of this, and a test importing a test passes
    let root = typescript_with_vitest();
    let r = root.path();
    put(
        r,
        "src/domain/__tests__/helper.ts",
        "export const helper = 1;\n",
    );
    put(r, "src/domain/song.test.ts", "export const song = 1;\n");
    put(
        r,
        "src/application/play.test.ts",
        "import { helper } from '../domain/__tests__/helper';\nimport { song } from '../domain/song.test';\n\
         import { play } from './play';\n",
    );
    put(r, "src/application/play.ts", "export const play = 1;\n");
    let out = run(&["--root", &root_arg(r), "check"]);
    assert!(out.status.success(), "{}", stdout(&out));

    // Each form of an import of a test by code that is not one, planted and removed
    for (path, text, line, import, target) in [
        (
            "src/application/play.ts",
            "import { helper } from '../domain/__tests__/helper';\n",
            1,
            "../domain/__tests__/helper",
            "src/domain/__tests__/helper",
        ),
        (
            "src/application/play.ts",
            "export const play = 1;\nimport { song } from '../domain/song.test';\n",
            2,
            "../domain/song.test",
            "src/domain/song.test",
        ),
        (
            "src/domain/song.ts",
            "import { song } from './song.test.ts';\n",
            1,
            "./song.test.ts",
            "src/domain/song.test.ts",
        ),
        (
            "src/main.tsx",
            "import { play } from './application/play.test';\n",
            1,
            "./application/play.test",
            "src/application/play.test",
        ),
        // A package's entry that is a test, through an alias of `paths`
        (
            "src/ui/atoms/Button.tsx",
            "import React from 'react';\nconst helper = import('@/domain/__tests__/helper');\n",
            2,
            "@/domain/__tests__/helper",
            "src/domain/__tests__/helper",
        ),
    ] {
        let original = fs::read_to_string(r.join(path)).ok();
        put(
            r,
            "tsconfig.json",
            "{ \"compilerOptions\": { \"paths\": { \"@/*\": [\"./src/*\"] } } }\n",
        );
        let kept = original.clone().unwrap_or_default();
        put(r, path, &format!("{kept}{text}"));
        let out = run(&["--root", &root_arg(r), "check"]);
        let said = stdout(&out);
        assert_eq!(out.status.code(), Some(1), "{path}: {said}");
        let wanted = finding(path, line + kept.lines().count(), import, target);
        assert!(
            said.contains(&wanted),
            "{path}: wanted\n{wanted}\nin\n{said}"
        );
        assert_eq!(said.matches(": imports ").count(), 1, "{path}: {said}");
        // Put back, it passes
        match original {
            Some(original) => put(r, path, &original),
            None => fs::remove_file(r.join(path)).unwrap(),
        }
        let out = run(&["--root", &root_arg(r), "check"]);
        assert!(out.status.success(), "{path} put back: {}", stdout(&out));
    }

    // An approval keeps one, as it keeps any forbidden import
    put(
        r,
        "src/application/play.ts",
        "import { helper } from '../domain/__tests__/helper';\n",
    );
    let out = approve_with_git(
        r,
        Some("[user]\n\tname = Someone\n"),
        &[
            "src/application/play.ts",
            "../domain/__tests__/helper",
            "--reason",
            "the helper is moved next week",
        ],
    );
    assert_eq!(out.status.code(), Some(0), "{}", stdout(&out));
    let out = run(&["--root", &root_arg(r), "check"]);
    assert!(out.status.success(), "{}", stdout(&out));
    assert!(stdout(&out).contains("approved in"), "{}", stdout(&out));

    // Python: a layer, the entry file and a unit's package file import no module of `tests/`, and a test imports both
    let root = repo_with_ui("python", "");
    let r = root.path();
    put(r, "tests/helper.py", "x = 1\n");
    put(
        r,
        "tests/test_model.py",
        "from tests import helper\nimport tests.helper\nfrom domain import model\n",
    );
    put(r, "domain/model.py", "x = 1\n");
    let out = run(&["--root", &root_arg(r), "check"]);
    assert!(out.status.success(), "{}", stdout(&out));
    let in_tests = |file: &str, line: usize, import: &str| {
        format!(
            "{file}:{line}: imports {import}, in tests/, which holds tests; code that is not a test imports no \
             test: move what it needs out of the test into a layer"
        )
    };
    for (path, text, import) in [
        (
            "domain/model.py",
            "from tests import helper\n",
            "tests.helper",
        ),
        ("domain/model.py", "import tests.helper\n", "tests.helper"),
        (
            "domain/model.py",
            "from tests.helper import x\n",
            "tests.helper.x",
        ),
        ("infrastructure/inbound/web.py", "import tests\n", "tests"),
        ("__main__.py", "import tests.helper\n", "tests.helper"),
    ] {
        put(r, path, text);
        let out = run(&["--root", &root_arg(r), "check"]);
        let said = stdout(&out);
        assert_eq!(out.status.code(), Some(1), "{path}: {said}");
        let wanted = in_tests(path, 1, import);
        assert!(
            said.contains(&wanted),
            "{path}: wanted\n{wanted}\nin\n{said}"
        );
        assert_eq!(said.matches(": imports ").count(), 1, "{path}: {said}");
        fs::remove_file(r.join(path)).unwrap();
        put(r, "domain/model.py", "x = 1\n");
    }
    // A project with no `tests/` has no test to import: `tests` is a package of someone else's
    fs::remove_dir_all(r.join("tests")).unwrap();
    put(r, "domain/model.py", "import tests\n");
    let out = run(&["--root", &root_arg(r), "check"]);
    assert!(
        !stdout(&out).contains("which holds tests"),
        "{}",
        stdout(&out)
    );
}

#[test]
fn only_a_test_imports_a_development_dependency_of_a_typescript_project() {
    let root = typescript_with_vitest();
    let r = root.path();
    put(
        r,
        "src/domain/song.test.ts",
        "import { test } from 'vitest';\n",
    );
    put(
        r,
        "src/domain/__tests__/helper.ts",
        "import { vi } from 'vitest';\n",
    );
    put(
        r,
        "src/infrastructure/outbound/song.ts",
        "import type { Buffer } from '@types/node';\n",
    );
    let out = run(&["--root", &root_arg(r), "check"]);
    assert!(out.status.success(), "{}", stdout(&out));

    // The code of a layer (one that may import a package: the allowlist of the inner layers is another rule), the
    // entry file and a file outside the layers that is no test
    for (path, expected) in [
        (
            "src/infrastructure/outbound/song.ts",
            "src/infrastructure/outbound/song.ts:1",
        ),
        ("src/main.tsx", "src/main.tsx:1"),
    ] {
        let original = fs::read_to_string(r.join(path)).unwrap_or_default();
        put(r, path, "import { vi } from 'vitest';\n");
        let out = run(&["--root", &root_arg(r), "check"]);
        let said = stdout(&out);
        assert_eq!(out.status.code(), Some(1), "{path}: {said}");
        let wanted = format!(
            "{expected}: imports vitest, which package.json:4 lists for development only: the code of a layer uses \
             runtime dependencies only, so move vitest to dependencies, or import it in a test alone (a file named \
             *.test.* or *.spec.*, or in __tests__/)"
        );
        assert!(
            said.contains(&wanted),
            "{path}: wanted\n{wanted}\nin\n{said}"
        );
        // Moved to `dependencies`, as the finding says, it passes
        put(
            r,
            "package.json",
            &VITEST_PACKAGE
                .replace(
                    "\"react\": \"^18\" }",
                    "\"react\": \"^18\", \"vitest\": \"^2\" }",
                )
                .replace("\"vitest\": \"^2\", \"@types", "\"@types"),
        );
        let out = run(&["--root", &root_arg(r), "check"]);
        assert!(out.status.success(), "{path} moved: {}", stdout(&out));
        put(r, "package.json", VITEST_PACKAGE);
        if original.is_empty() {
            fs::remove_file(r.join(path)).unwrap();
        } else {
            put(r, path, &original);
        }
    }

    // A runtime dependency that only tests import is moved to `devDependencies`, as in Python
    put(
        r,
        "package.json",
        &VITEST_PACKAGE.replace(
            "\"react\": \"^18\" }",
            "\"react\": \"^18\", \"left-pad\": \"1\" }",
        ),
    );
    put(r, "src/domain/pad.test.ts", "import pad from 'left-pad';\n");
    let out = run(&["--root", &root_arg(r), "check"]);
    let said = stdout(&out);
    assert_eq!(out.status.code(), Some(1), "{said}");
    assert!(
        said.contains(
            "package.json:3: lists the dependency left-pad in dependencies, and only tests import it: move it to \
             devDependencies"
        ),
        "{said}"
    );

    // A file outside `src/` is read by no check, as before
    put(r, "src/domain/pad.test.ts", "export const a = 1;\n");
    put(r, "package.json", VITEST_PACKAGE);
    put(
        r,
        "vite.config.ts",
        "import { defineConfig } from 'vitest/config';\n",
    );
    let out = run(&["--root", &root_arg(r), "check"]);
    assert!(out.status.success(), "{}", stdout(&out));
}

/// Every place of the Python layout, in the order of the grid below.
const PLACES: [&str; 18] = [
    "composition",
    "ui",
    "ui.pages",
    "ui.templates",
    "ui.organisms",
    "ui.molecules",
    "ui.atoms",
    "presentation",
    "presentation.models",
    "presentation.display",
    "infrastructure",
    "infrastructure.inbound",
    "infrastructure.outbound",
    "application",
    "protocol",
    "protocol.inbound",
    "protocol.outbound",
    "domain",
];

/// Whether the place of a row may import the place of a column: the rule `layers/table.toml` states (each layer's
/// `imports`, which may name one level of another layer, and a level of an ordered layer, `ui` or `presentation`,
/// the levels below it), written out again by hand rather than computed by the check. A change to the table or to the
/// check that moves any pair fails here, so moving one is a decision made twice.
const MAY_IMPORT: [&str; 18] = [
    // co ui pg tp or mo at pe pm pd if ii io ap pr pi po do
    "Y Y Y Y Y Y Y Y Y Y Y Y Y Y Y Y Y Y", // composition
    "N Y Y Y Y Y Y N Y Y N N N N N Y N N", // ui
    "N N Y Y Y Y Y N Y Y N N N N N Y N N", // ui.pages
    "N N N Y Y Y Y N N Y N N N N N N N N", // ui.templates
    "N N N N Y Y Y N N Y N N N N N N N N", // ui.organisms
    "N N N N N Y Y N N N N N N N N N N N", // ui.molecules
    "N N N N N N Y N N N N N N N N N N N", // ui.atoms
    "N N N N N N N Y Y Y N N N N Y Y Y Y", // presentation
    "N N N N N N N N Y Y N N N N N Y N N", // presentation.models
    "N N N N N N N N N Y N N N N N N N N", // presentation.display
    "N N N N N N N N N N Y Y Y N Y Y Y Y", // infrastructure
    "N N N N N N N N N N N Y N N N Y N N", // infrastructure.inbound
    "N N N N N N N N N N N N Y N N N Y Y", // infrastructure.outbound
    "N N N N N N N N N N N N N Y Y Y Y Y", // application
    "N N N N N N N N N N N N N N Y Y Y Y", // protocol
    "N N N N N N N N N N N N N N N Y N N", // protocol.inbound
    "N N N N N N N N N N N N N N N N Y Y", // protocol.outbound
    "N N N N N N N N N N N N N N N N N Y", // domain
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
            forbidden: 241,
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
            forbidden: 241,
        },
        Stack {
            // No ui, and a crate per layer: the levels inside a crate are judged by another test
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
        // Rust has no UI, and its levels are modules, not manifests
        let has = |place: &&str| {
            name != "rust"
                || !(place.starts_with("ui")
                    || place.starts_with("presentation")
                    || place.contains('.'))
        };
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
        assert!(said.contains(
            "the layers import only what the layer table in .rotproof/AGENTS.md allows:"
        ));
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
             proto = {{ path = \"../protocol\", package = \"protocol\" }}\n\
             infra = {{ path = \"../infrastructure\", package = \"infrastructure\" }}\n\
             regex = \"1\"\n\
             missing = {{ workspace = true }}\n\
             \n[build-dependencies]\ncomposition = {{ path = \"../composition\" }}\n\
             \n[dev-dependencies]\ncomposition = {{ path = \"../composition\" }}\n\
             \n[target.'cfg(windows)'.dependencies]\n\
             app = {{ path = \"../../crates/./application/\", package = \"application\" }}\n\
             outside = {{ path = \"../../..\" }}\n\
             nowhere = {{ path = \"../nowhere\" }}\n",
            package("domain")
        ),
    );
    // A member that names its workspace, and one whose named workspace is none
    write(
        "crates/protocol/Cargo.toml",
        &format!(
            "{}workspace = \"../..\"\n\n[dependencies]\napplication.workspace = true\n",
            package("protocol")
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
         domain imports no other layer",
        "crates/domain/Cargo.toml:9: depends on proto (crates/protocol), in protocol; ",
        "crates/domain/Cargo.toml:10: depends on infra (crates/infrastructure), in infrastructure; ",
        "crates/domain/Cargo.toml:12: missing comes from the workspace, and Cargo.toml has no missing in \
         [workspace.dependencies], so it cannot be judged: add it there, or give it a path",
        "crates/domain/Cargo.toml:15: depends on composition (crates/composition), in composition; ",
        "crates/domain/Cargo.toml:21: depends on app (crates/application), in application; ",
        "crates/protocol/Cargo.toml:8: depends on application (crates/application, from the workspace), in \
         application; protocol may import `domain`",
        "crates/infrastructure/Cargo.toml:8: core comes from the workspace, and .., which [package] workspace names, \
         is no workspace, so it cannot be judged: declare the workspace, or give core a path",
        "crates/application/Cargo.toml:1: cannot be read as TOML (",
        "), so its dependencies cannot be judged: fix the file",
    ] {
        assert!(said.contains(line), "{line}:\n{said}");
    }
    assert_eq!(said.matches(": depends on ").count(), 6, "{said}");
    assert_eq!(
        said.matches("comes from the workspace").count(),
        2,
        "{said}"
    );
}

#[test]
fn the_levels_inside_a_rust_crate_and_its_entry_file_are_judged() {
    let root = repo_with_ui("rust", "");
    let r = root.path();
    let write = |path: &str, text: &str| fs::write(r.join(path), text).unwrap();
    // A fresh project passes: the root package, its entry file and the levels as modules
    let arg = root_arg(r);
    let out = run(&["--root", &arg, "check"]);
    assert!(out.status.success(), "{}", stdout(&out));

    // The crate may depend on domain, and its inbound level may not use it
    write(
        "crates/infrastructure/Cargo.toml",
        "[package]\nname = \"infrastructure\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[dependencies]\n\
         domain = { path = \"../domain\" }\n",
    );
    write(
        "crates/infrastructure/src/inbound/cli.rs",
        "use crate::outbound::Db;\nuse super::args;\nuse domain::Song;\n",
    );
    write(
        "crates/infrastructure/src/outbound/mod.rs",
        "use domain::Song;\n",
    );
    write(
        "crates/infrastructure/src/lib.rs",
        "//! Doc\n\npub mod inbound;\npub mod outbound;\npub use outbound::*;\n",
    );
    write("crates/infrastructure/src/shared.rs", "pub fn f() {}\n");
    let manifest = fs::read_to_string(r.join("Cargo.toml")).unwrap();
    write(
        "Cargo.toml",
        &manifest.replace(
            "[workspace]",
            "[dependencies]\napplication = { path = \"crates/application\" }\n\n[workspace]",
        ),
    );
    let out = run(&["--root", &arg, "check"]);
    let said = stdout(&out);
    assert_eq!(out.status.code(), Some(1), "{said}");
    for line in [
        "crates/infrastructure/src/inbound/cli.rs:1: uses crate::outbound::Db, in infrastructure.outbound; \
         infrastructure.inbound may import `protocol.inbound`",
        "crates/infrastructure/src/inbound/cli.rs:3: uses domain::Song, in domain; ",
        "crates/infrastructure/src/lib.rs:5: the root of a crate with levels only declares their modules",
        "code in infrastructure outside its levels: crates/infrastructure/src/shared.rs",
        "Cargo.toml:8: depends on application (crates/application), in application; src/main.rs may import \
         `composition`",
    ] {
        assert!(said.contains(line), "{line}:\n{said}");
    }
    assert_eq!(said.matches(": uses ").count(), 2, "{said}");
    assert_eq!(said.matches(": depends on ").count(), 1, "{said}");
}

#[test]
fn the_root_package_of_a_rust_project_holds_its_entry_file_and_no_other_code() {
    let root = repo_with_ui("rust", "");
    let r = root.path();
    let arg = root_arg(r);
    let plant = |path: &str| {
        fs::create_dir_all(r.join(path).parent().unwrap()).unwrap();
        fs::write(r.join(path), "pub fn f() {}\n").unwrap();
    };
    let check = || run(&["--root", &arg, "check"]);
    let declaration = |unchecked: &str| {
        declare(
            r,
            &format!(
                "stack = \"rust\"\nareas = [\"a\"]\n{}unchecked = [{unchecked}]\n",
                ui_line("rust")
            ),
        )
    };
    // What Cargo keeps outside `src/` in the root package, and what is outside the root package, is not looked at
    for path in [
        "build.rs",
        "tests/it.rs",
        "benches/b.rs",
        "examples/e.rs",
        "xtask/src/main.rs",
        "src/data.json",
    ] {
        plant(path);
    }
    let out = check();
    assert!(out.status.success(), "{}", stdout(&out));

    // Every other `.rs` file of `src/` is code outside the layers, a directory by its name, and in any case
    for path in [
        "src/pricing.rs",
        "src/lib.rs",
        "src/bin/tool.rs",
        "src/util/mod.rs",
        "src/BIG.RS",
    ] {
        plant(path);
    }
    let out = check();
    let said = stdout(&out);
    assert_eq!(out.status.code(), Some(1), "{said}");
    for outside in [
        "src/pricing.rs",
        "src/lib.rs",
        "src/bin",
        "src/util",
        "src/BIG.RS",
    ] {
        let line = format!(
            "code outside the layers: {outside} (move it into a layer: a rule of the program goes in crates/domain, a \
             use case in crates/application, what wires them in crates/composition; src/main.rs is the one file of \
             the root package's src/ that is no layer; or list it in unchecked in .config/rotproof.toml)"
        );
        assert!(said.contains(&line), "{line}:\n{said}");
    }
    assert_eq!(
        said.matches("code outside the layers: ").count(),
        5,
        "{said}"
    );

    // A path the project lists is exempt, a file or a directory, and the entry file needs no listing
    declaration("\"src/pricing.rs\", \"src/lib.rs\", \"src/bin\", \"src/util\", \"src/BIG.RS\"");
    let out = check();
    assert!(out.status.success(), "{}", stdout(&out));
    declaration("\"src/pricing.rs\"");
    let said = stdout(&check());
    assert!(!said.contains("src/pricing.rs"), "{said}");
    assert_eq!(
        said.matches("code outside the layers: ").count(),
        4,
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
        "import path from 'node:path';\nimport type { Play } from '../application/play';\nexport * from '@/infrastructure/db';\n\
         export const load = () => import('../composition/main');\nconst x = require(`../vendor/x`);\n\
         const where = import.meta.resolve('../composition/worker');\n",
    );
    write(
        "src/ui/molecules/Row.tsx",
        "import { Button } from '../atoms/Button';\nimport { List } from '../organisms/List';\nimport '../index.css';\n\
         import logo from '../../domain/logo.svg?url';\nexport const Row = () => <p>{/* ../pages */}</p>;\n",
    );
    // A package is no layer: it is the dependencies check's, and the allowlist's where the layer may not import it
    write(
        "src/infrastructure/outbound/view.ts",
        "import React from 'react';\n",
    );
    // Not a source file, ignored by git, or hidden: not read
    write("src/domain/notes.md", "import '../composition'\n");
    write(".gitignore", "src/domain/generated/\n");
    write("src/domain/generated/x.ts", "import '../../composition';\n");
    write("src/domain/.cache/x.ts", "import '../../composition';\n");
    let out = run(&["--root", &root_arg(r), "check"]);
    let said = stdout(&out);
    for line in [
        "src/domain/song.ts:2: imports ../application/play (src/application/play), in application; domain imports \
         no other layer",
        "src/domain/song.ts:3: imports @/infrastructure/db (src/infrastructure/db), in infrastructure; ",
        "src/domain/song.ts:4: imports ../composition/main (src/composition/main), in composition; ",
        "src/domain/song.ts:6: imports ../composition/worker (src/composition/worker), in composition; ",
        "src/ui/molecules/Row.tsx:2: imports ../organisms/List (src/ui/organisms/List), in ui.organisms; \
         ui.molecules may import the levels below it",
        "src/ui/molecules/Row.tsx:3: imports ../index.css (src/ui/index.css), in ui outside its levels; ",
        "src/ui/molecules/Row.tsx:4: imports ../../domain/logo.svg?url (src/domain/logo.svg), in domain; ",
    ] {
        assert!(said.contains(line), "{line}:\n{said}");
    }
    assert_eq!(said.matches(": imports ").count(), 7, "{said}");
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
    // main.tsx is the entry file; the rest is named, with where it goes
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

    // Moved as the finding says, with what main.tsx imported moved into composition, which main.tsx calls: everything
    // passes
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
        "src/composition/app.tsx",
        "import '../ui/atoms/index.css'\nimport App from '../ui/pages/App.tsx'\nexport { App }\n",
    );
    write(
        "src/main.tsx",
        "import { App } from './composition/app'\nexport { App }\n",
    );
    let out = run(&["--root", &arg, "check"]);
    assert!(out.status.success(), "{}", stdout(&out));
    write(
        "src/main.tsx",
        "import { App } from './composition/app'\nconst = ;\n",
    );
    let out = run(&["--root", &arg, "check"]);
    assert!(
        stdout(&out).contains("src/main.tsx:2: cannot be read as TypeScript"),
        "{}",
        stdout(&out)
    );

    // As the starter wrote it, main.tsx imports the UI itself, which only composition may
    write(
        "src/main.tsx",
        "import './ui/atoms/index.css'\nimport App from './ui/pages/App.tsx'\nexport { App }\n",
    );
    let out = run(&["--root", &arg, "check"]);
    assert!(
        stdout(&out).contains(
            "src/main.tsx:2: imports ./ui/pages/App.tsx (src/ui/pages/App.tsx), in ui.pages; src/main.tsx may \
             import `composition`"
        ),
        "{}",
        stdout(&out)
    );

    // The entry file is no layer: with composition declared absent, it is still not code outside the layers
    write("src/main.tsx", "export {}\n");
    fs::remove_dir_all(r.join("src/composition")).unwrap();
    declare(
        r,
        "stack = \"typescript\"\nareas = [\"a\"]\nui_design = \"atomic\"\nabsent = [\"composition\"]\n",
    );
    let out = run(&["--root", &arg, "check"]);
    assert!(out.status.success(), "{}", stdout(&out));
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
fn a_tsconfig_that_extends_itself_is_a_finding_and_not_a_crash() {
    let root = repo_with_ui("typescript", "");
    let r = root.path();
    // `./tsconfig.json` and `tsconfig.json` name one file, so the cycle is there however it is spelled
    fs::write(
        r.join("tsconfig.json"),
        "{ \"extends\": \"./tsconfig.json\" }",
    )
    .unwrap();
    let out = run(&["--root", &root_arg(r), "check"]);
    assert_eq!(out.status.code(), Some(1), "{}", stdout(&out));
    assert!(
        stdout(&out).contains("tsconfig.json: extends itself through tsconfig.json"),
        "{}",
        stdout(&out)
    );
}

#[test]
fn every_form_of_import_is_judged() {
    let root = repo_with_ui("python", "");
    let r = root.path();
    // `requests` is listed, so that only the imports between layers are found
    fs::write(
        r.join("pyproject.toml"),
        "[project]\nname = \"x\"\ndependencies = [\"requests\"]\n",
    )
    .unwrap();
    let write = |path: &str, text: &str| {
        let full = r.join(path);
        fs::create_dir_all(full.parent().unwrap()).unwrap();
        fs::write(full, text).unwrap();
    };
    write(
        "domain/model.py",
        "import os\nimport json\nfrom typing import TYPE_CHECKING\nif TYPE_CHECKING:\n    \
         from application import play\ndef f():\n    import infrastructure.db\n",
    );
    write(
        "ui/molecules/row.py",
        "from .. import organisms\nfrom ..atoms import button\nfrom . import field\nfrom ui import pages\nimport ui\n",
    );
    // The entry file, outside the layers, is read too
    write(
        "__main__.py",
        "import composition\nfrom application import play\nimport requests\n",
    );
    // Not a layer, ignored by git, or hidden: not read
    write("tests/test_x.py", "import composition\n");
    write(".gitignore", "domain/generated/\n");
    write("domain/generated/x.py", "import composition\n");
    write("domain/.cache/x.py", "import composition\n");
    let out = run(&["--root", &root_arg(r), "check"]);
    let said = stdout(&out);
    for line in [
        "domain/model.py:5: imports application.play, in application; domain imports no other layer",
        "domain/model.py:7: imports infrastructure.db, in infrastructure; ",
        "ui/molecules/row.py:1: imports ui.organisms, in ui.organisms; ui.molecules may import the levels below it\n",
        "ui/molecules/row.py:4: imports ui.pages, in ui.pages; ",
        "ui/molecules/row.py:5: imports ui, in ui outside its levels; ",
        "__main__.py:2: imports application.play, in application; __main__.py may import `composition`",
    ] {
        assert!(said.contains(line), "{line}:\n{said}");
    }
    assert_eq!(said.matches(": imports ").count(), 6, "{said}");
}

#[test]
fn what_cannot_be_read_fails_and_what_is_not_there_is_not_judged() {
    let root = repo_with_ui("python", "\"infrastructure\"");
    let r = root.path();
    // A layer declared absent is a module the project does not have: a package of the same name is not judged, and
    // the manifest lists one
    fs::write(
        r.join("pyproject.toml"),
        "[project]\nname = \"x\"\ndependencies = [\"infrastructure\"]\n",
    )
    .unwrap();
    fs::write(r.join("domain/model.py"), "import infrastructure.db\n").unwrap();
    // It is a package all the same, which the domain may import once it is allowed there
    let declaration = fs::read_to_string(r.join(".config/rotproof.toml")).unwrap();
    fs::write(
        r.join(".config/rotproof.toml"),
        format!("{declaration}\n[dependencies]\nallowed = {{ domain = [\"infrastructure\"] }}\n"),
    )
    .unwrap();
    let out = run(&["--root", &root_arg(r), "check"]);
    assert!(out.status.success(), "{}", stdout(&out));

    fs::write(
        r.join("domain/broken.py"),
        "import os\ndef (:\nimport composition\n",
    )
    .unwrap();
    fs::write(r.join("domain/latin.py"), b"x = '\xff'\n").unwrap();
    let out = run(&["--root", &root_arg(r), "check"]);
    let said = stdout(&out);
    assert_eq!(out.status.code(), Some(1), "{said}");
    let error = said
        .lines()
        .find(|line| line.contains("domain/broken.py:2: cannot be read as Python ("))
        .unwrap_or_else(|| panic!("{said}"));
    // The parser recovers, and the imports after the error are judged all the same: what it says has to fit that
    assert!(
        error.ends_with("), so the imports after it may be misread"),
        "{error}"
    );
    assert!(
        said.contains("domain/broken.py:3: imports composition, in composition; "),
        "{said}"
    );
    assert!(
        said.contains(
            "domain/latin.py: cannot be read as UTF-8, so its imports cannot be read: save it as UTF-8"
        ),
        "{said}"
    );
}

#[test]
fn every_stack_with_layers_checks_everything() {
    for stack in ["python", "typescript", "rust"] {
        let root = declared(&format!(
            "stack = \"{stack}\"\nareas = [\"a\"]\n{}",
            ui_line(stack)
        ));
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
        // Nothing is skipped: the one line says that the layers, their direction and comments included, kept the rules.
        // Before it, a TypeScript project says how many files it read as tests, even when there are none
        let tests = if stack == "typescript" {
            "tests: 0 files read as tests (*.test.*, *.spec.*, __tests__/)\n"
        } else {
            ""
        };
        assert_eq!(
            said,
            format!("{tests}the layers and every record keep the rules\n"),
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
        "stack = \"python\"\nareas = [\"a\"]\nui_design = \"none\"\nunchecked = [\"scripts/\"]\n",
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
fn code_in_a_path_that_is_moving_is_kept_out_of_the_checks_of_the_layers_and_the_markers() {
    let root = clean_repo();
    let r = root.path();
    let declare_moving = |moving: &str| {
        declare(
            r,
            &format!(
                "stack = \"python\"\nareas = [\"a\"]\nui_design = \"none\"\nmoving = [{moving}]\n"
            ),
        )
    };
    write_file(r, "legacy/order.py", "# TODO: split this\nimport domain\n");
    write_file(r, "old.py", "# FIXME: and this\n");
    // Not listed: code outside the layers, and its marker, fail
    let out = run(&["--root", &root_arg(r), "check"]);
    assert_eq!(out.status.code(), Some(1), "{}", stdout(&out));
    assert!(
        stdout(&out).contains("code outside the layers: legacy"),
        "{}",
        stdout(&out)
    );
    // Listed, a directory with a `/` at its end and a file: neither fails, and a layer's own marker still does
    declare_moving("\"legacy/\", \"old.py\"");
    let out = run(&["--root", &root_arg(r), "check"]);
    assert!(out.status.success(), "{}", stdout(&out));
    write_file(r, "domain/model.py", "# TODO: split this\n");
    let out = run(&["--root", &root_arg(r), "check"]);
    assert_eq!(out.status.code(), Some(1), "{}", stdout(&out));
    assert!(
        stdout(&out).contains("domain/model.py:1"),
        "{}",
        stdout(&out)
    );
    // The code that is moving is named for what to move first, and by no finding
    assert!(
        !stdout(&out).contains("legacy/order.py:1"),
        "{}",
        stdout(&out)
    );
    // A repository of records only has no code to move
    declare(
        r,
        "stack = \"none\"\nareas = [\"a\"]\nmoving = [\"legacy\"]\n",
    );
    let out = run(&["--root", &root_arg(r), "check"]);
    assert_eq!(out.status.code(), Some(1), "{}", stdout(&out));
    assert!(
        stdout(&out).contains("moving must be empty"),
        "{}",
        stdout(&out)
    );
}

#[test]
fn check_counts_the_code_that_is_moving_and_names_the_files_to_move_first_and_fails_nothing() {
    let root = clean_repo();
    let r = root.path();
    let arg = root_arg(r);
    let moving = |paths: &str| {
        declare(
            r,
            &format!(
                "stack = \"python\"\nareas = [\"a\"]\nui_design = \"none\"\nmoving = [{paths}]\n"
            ),
        )
    };
    let said = || {
        let out = run(&["--root", &arg, "check"]);
        // The code that is moving is counted, and the check passes all the same
        assert!(out.status.success(), "{}", stdout(&out));
        stdout(&out)
            .lines()
            .filter(|line| line.starts_with("moving") || line.starts_with("  "))
            .collect::<Vec<_>>()
            .join("\n")
    };
    // A project that lists nothing hears nothing
    assert_eq!(said(), "");
    write_file(r, "legacy/money.py", "import os\n");
    write_file(r, "legacy/order.py", "from legacy import money\n");
    write_file(r, "legacy/a.py", "from legacy import b\n");
    write_file(r, "legacy/b.py", "from legacy.a import x\n");
    write_file(r, "legacy/report.py", "from legacy.order import total\n");
    moving("\"legacy\"");
    assert_eq!(
        said(),
        "moving: 5 files are not in a layer yet, and none is checked. Move these next (they import no other file \
         still moving), each with the layers it could go in:\n  legacy/a.py, legacy/b.py import each other, so \
         move them together:\n    legacy/a.py: any layer\n    legacy/b.py: any layer\n  legacy/money.py: any layer"
    );
    // The count falls when a file is moved, and what waited for it is next, with the layers that pass for its import
    // of the layer it went to
    fs::remove_file(r.join("legacy/money.py")).unwrap();
    write_file(r, "domain/money.py", "import os\n");
    write_file(r, "legacy/order.py", "from domain import money\n");
    assert_eq!(
        said(),
        "moving: 4 files are not in a layer yet, and none is checked. Move these next (they import no other file \
         still moving), each with the layers it could go in:\n  legacy/a.py, legacy/b.py import each other, so \
         move them together:\n    legacy/a.py: any layer\n    legacy/b.py: any layer\n  legacy/order.py: domain, \
         protocol.outbound, application, infrastructure.outbound, composition"
    );
    // More than the next few are counted, not named
    for i in 0..6 {
        write_file(r, &format!("legacy/leaf{i}.py"), "");
    }
    let said_now = said();
    assert!(
        said_now.starts_with("moving: 10 files are not in a layer yet"),
        "{said_now}"
    );
    assert!(
        said_now.ends_with("  and 3 more that import no other file still moving"),
        "{said_now}"
    );
    // A path with no code left in it says to take it out
    for entry in fs::read_dir(r.join("legacy")).unwrap() {
        fs::remove_file(entry.unwrap().path()).unwrap();
    }
    fs::write(r.join("legacy/notes.txt"), "x\n").unwrap();
    assert_eq!(
        said(),
        "moving: no code is left in the paths `moving` lists in .config/rotproof.toml: take them out"
    );
}

#[test]
fn check_counts_the_rust_code_that_is_moving_and_names_none_to_move_first() {
    let root = declared("stack = \"rust\"\nareas = [\"a\"]\nui_design = \"none\"\n");
    let r = root.path();
    let arg = root_arg(r);
    assert!(
        create(&["--root", &arg, "create", "--yes"])
            .status
            .success()
    );
    write_file(
        r,
        "crates/legacy/Cargo.toml",
        "[package]\nname = \"legacy\"\n",
    );
    write_file(r, "crates/legacy/src/lib.rs", "pub mod a;\n");
    write_file(r, "crates/legacy/src/a.rs", "");
    declare(
        r,
        "stack = \"rust\"\nareas = [\"a\"]\nui_design = \"none\"\nmoving = [\"crates/legacy\"]\n",
    );
    let out = run(&["--root", &arg, "check"]);
    let said = stdout(&out);
    assert!(
        said.contains(
            "moving: 2 files are not in a layer yet, and none is checked. Rotproof does not read what a Rust file \
             imports, so it names none to move first\n"
        ),
        "{said}"
    );
}

#[test]
fn moving_lists_no_tests_and_the_tests_in_a_path_that_is_moving_are_not_counted() {
    // Python: `tests/` is a place the layout reads as tests, so it is no code to move into a layer
    let root = clean_repo();
    let r = root.path();
    let arg = root_arg(r);
    write_file(r, "tests/test_a.py", "import os\n");
    write_file(r, "legacy/old.py", "import os\n");
    declare(
        r,
        "stack = \"python\"\nareas = [\"a\"]\nui_design = \"none\"\nmoving = [\"tests\", \"legacy\"]\n",
    );
    let out = run(&["--root", &arg, "check"]);
    let said = stdout(&out);
    assert_eq!(out.status.code(), Some(1), "{said}");
    assert!(
        said.contains(
            "moving lists tests, which the layout reads as tests: tests stay where they are, so take it out of moving"
        ),
        "{said}"
    );
    declare(
        r,
        "stack = \"python\"\nareas = [\"a\"]\nui_design = \"none\"\nmoving = [\"legacy\"]\n",
    );
    let out = run(&["--root", &arg, "check"]);
    let said = stdout(&out);
    assert!(out.status.success(), "{said}");
    assert!(
        said.contains("moving: 1 file is not in a layer yet"),
        "{said}"
    );
    // TypeScript: a test is told by its name, so one in a path that is moving is neither counted nor given layers
    let root = repo_with_ui("typescript", "");
    let r = root.path();
    let arg = root_arg(r);
    write_file(r, "src/legacy/old.ts", "export const a = 1;\n");
    write_file(
        r,
        "src/legacy/old.test.ts",
        "import { a } from \"./old\";\n",
    );
    let declaration = |moving: &str| {
        format!(
            "stack = \"typescript\"\nareas = [\"a\"]\n{}moving = [{moving}]\n",
            ui_line("typescript")
        )
    };
    declare(r, &declaration("\"src/legacy\""));
    let out = run(&["--root", &arg, "check"]);
    let said = stdout(&out);
    assert!(out.status.success(), "{said}");
    assert!(
        said.contains("moving: 1 file is not in a layer yet"),
        "{said}"
    );
    assert!(!said.contains("old.test.ts"), "{said}");
    declare(r, &declaration("\"src/legacy/old.test.ts\""));
    let out = run(&["--root", &arg, "check"]);
    let said = stdout(&out);
    assert_eq!(out.status.code(), Some(1), "{said}");
    assert!(
        said.contains("moving lists src/legacy/old.test.ts, which the layout reads as tests"),
        "{said}"
    );
}

#[test]
fn check_leaves_out_of_the_layers_a_moving_file_could_go_in_those_the_io_check_would_fail_it_in() {
    let root = clean_repo();
    let r = root.path();
    let arg = root_arg(r);
    let moving = |rest: &str| {
        declare(
            r,
            &format!(
                "stack = \"python\"\nareas = [\"a\"]\nui_design = \"none\"\nmoving = [\"legacy\"]\nfiles = \"{}\"\n{rest}",
                env!("CARGO_PKG_VERSION")
            ),
        )
    };
    let said = || {
        stdout(&run(&["--root", &arg, "check"]))
            .lines()
            .filter(|line| line.starts_with("  legacy"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    write_file(r, "legacy/model.py", "import pydantic\n");
    write_file(r, "legacy/store.py", "import shutil\n");
    write_file(r, "legacy/pure.py", "import json\n");
    moving("");
    // The code that is moving is counted, and the check passes all the same
    let out = run(&["--root", &arg, "check"]);
    assert!(out.status.success(), "{}", stdout(&out));
    // A file that imports a table entry, and one that imports a package no list allows, may not go in the four layers
    // the check judges, and a file that does neither may go anywhere
    let free = "infrastructure.outbound, infrastructure.inbound, composition";
    assert_eq!(
        said(),
        format!(
            "  legacy/model.py: {free}\n  legacy/pure.py: any layer\n  legacy/store.py: {free}"
        )
    );
    // Allowed for `domain`, the package is no reason to leave it out of that layer
    moving("[dependencies]\nallowed = { domain = [\"pydantic\"] }\n");
    assert_eq!(
        said(),
        format!(
            "  legacy/model.py: domain, {free}\n  legacy/pure.py: any layer\n  legacy/store.py: {free}"
        )
    );
    // What reads it from a layer that may not import I/O leaves it no place at all, and the line says why
    write_file(r, "domain/uses.py", "from legacy import store\n");
    assert!(
        said().contains(
            "  legacy/store.py: no layer passes the direction check and the check of the I/O of the inner layers, \
             for what it imports, what it does and what imports it"
        ),
        "{}",
        said()
    );
}

#[test]
fn check_lists_what_the_inner_layers_import_of_code_outside_the_layers_and_fails_nothing() {
    let root = clean_repo();
    let r = root.path();
    let arg = root_arg(r);
    let declare_paths = |paths: &str| {
        declare(
            r,
            &format!("stack = \"python\"\nareas = [\"a\"]\nui_design = \"none\"\n{paths}"),
        )
    };
    let listed = || {
        let out = run(&["--root", &arg, "check"]);
        assert!(out.status.success(), "{}", stdout(&out));
        stdout(&out)
            .lines()
            .skip_while(|line| !line.starts_with("inner layers read code outside the layers"))
            .take_while(|line| line.starts_with("inner layers") || line.starts_with("  "))
            .collect::<Vec<_>>()
            .join("\n")
    };
    write_file(r, "legacy/io_helper.py", "import shutil\n");
    write_file(r, "scripts/tool.py", "");
    write_file(
        r,
        "domain/x.py",
        "import os\nfrom legacy import io_helper\nimport scripts.tool\n",
    );
    write_file(
        r,
        "infrastructure/outbound/y.py",
        "import legacy.io_helper\n",
    );
    // Nothing is listed while no path is: the code outside the layers fails as code outside the layers does
    declare_paths("");
    let out = run(&["--root", &arg, "check"]);
    assert_eq!(out.status.code(), Some(1), "{}", stdout(&out));
    assert!(
        !stdout(&out).contains("inner layers read code"),
        "{}",
        stdout(&out)
    );
    // Listed, the helper is not judged, and the import of it is named, apart from what fails
    declare_paths("unchecked = [\"scripts\"]\nmoving = [\"legacy\"]\n");
    assert_eq!(
        listed(),
        "inner layers read code outside the layers (not judged) (2):\n  \
         domain/x.py:2 imports legacy.io_helper, in a path of moving\n  \
         domain/x.py:3 imports scripts.tool, in a path of unchecked"
    );
    // Nothing is listed once no inner layer imports it
    write_file(r, "domain/x.py", "import os\n");
    assert_eq!(listed(), "");
}

#[test]
fn check_lists_what_a_rust_inner_layer_depends_on_in_a_path_of_unchecked_or_moving() {
    let root = declared("stack = \"rust\"\nareas = [\"a\"]\nui_design = \"none\"\n");
    let r = root.path();
    let arg = root_arg(r);
    assert!(
        create(&["--root", &arg, "create", "--yes"])
            .status
            .success()
    );
    write_file(
        r,
        "crates/legacy/Cargo.toml",
        "[package]\nname = \"legacy\"\n",
    );
    write_file(r, "crates/legacy/src/lib.rs", "");
    let manifest = fs::read_to_string(r.join("crates/domain/Cargo.toml")).unwrap();
    fs::write(
        r.join("crates/domain/Cargo.toml"),
        format!("{manifest}\n[dependencies]\nlegacy = {{ path = \"../legacy\" }}\n"),
    )
    .unwrap();
    declare(
        r,
        "stack = \"rust\"\nareas = [\"a\"]\nui_design = \"none\"\nmoving = [\"crates/legacy\"]\n",
    );
    let said = stdout(&run(&["--root", &arg, "check"]));
    assert!(
        said.contains(
            "inner layers read code outside the layers (not judged) (1):\n  \
             crates/domain/Cargo.toml:"
        ) && said.contains(" imports legacy, in a path of moving\n"),
        "{said}"
    );
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
        "stack = \"typescript\"\nareas = [\"a\"]\nui_design = \"none\"\n",
    );
    point_at_the_guide(&r);
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
    // The command line is a module of the infrastructure crate; what a user types and reads is rotproof
    assert_eq!(
        stdout(&run(&["--version"])),
        format!("rotproof {}\n", env!("CARGO_PKG_VERSION"))
    );
    let help = stdout(&run(&["--help"]));
    assert!(!help.contains("infrastructure"), "{help}");
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
    // `rotproof create` alone first, to see the layers it would make, then with --yes: the help does not lead past the
    // moment the layers are decided
    let steps = [
        "rotproof init --stack <stack>",
        "--ui-design <ui>",
        "list in absent",
        "rotproof create   ",
        "rotproof create --yes",
        "answer what create asks",
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
        init_help.contains("python, typescript, rust, or none")
            && init_help.contains("atomic (the view in ui"),
        "{init_help}"
    );
    let out = run(&["--root", &arg, "check"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&out.stderr)
            .contains("rotproof init --stack <stack>` (python, typescript, rust, or none"),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    // Each command names the next
    let out = run(&[
        "--root",
        &arg,
        "init",
        "--stack",
        "python",
        "--ui-design",
        "atomic",
    ]);
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
    // With --yes it makes them, and asks the one question that is the person's: whether the agent reads the guide. It
    // exits 2 until that is answered, and says so
    let out = create(&["--root", &arg, "create", "--yes"]);
    assert_eq!(out.status.code(), Some(2), "{}", stdout(&out));
    let asked = stdout(&out);
    assert!(
        asked.contains("ask the person: should this project's agent read the rules Rotproof keeps")
            && asked
                .contains("there is no AGENTS.md or CLAUDE.md: ask the person which file to make")
            && asked.contains("no: add \"guide-pointer\" to declined in .config/rotproof.toml")
            && !asked.contains("next: run `rotproof check`"),
        "{asked}"
    );
    assert!(
        stderr(&out).contains("`rotproof check` fails until it is answered"),
        "{}",
        stderr(&out)
    );
    fs::write(
        root.path().join("docs/log.md"),
        "# Log\n\n## 2026-10-04\n\n* Started\n",
    )
    .unwrap();
    // The check fails on it too, so that it cannot be skipped
    let out = run(&["--root", &arg, "check"]);
    assert_eq!(out.status.code(), Some(1), "{}", stdout(&out));
    assert!(
        stdout(&out).contains("the project answered whether its agent reads Rotproof's guide"),
        "{}",
        stdout(&out)
    );
    // The person says yes: the line the question gives, in the file they chose
    let line = asked
        .lines()
        .skip_while(|line| !line.starts_with("  yes: "))
        .nth(1)
        .unwrap()
        .trim();
    fs::write(root.path().join("AGENTS.md"), format!("{line}\n")).unwrap();
    // Nothing is left for a person to write before the first check passes but the lock file, which a tool makes: no
    // area is needed until the first knowledge document
    let out = run(&["--root", &arg, "check"]);
    assert_eq!(out.status.code(), Some(1), "{}", stdout(&out));
    assert!(
        stdout(&out).contains("pyproject.toml has no lock file in the root or above it")
            && !stdout(&out).contains("whether its agent reads"),
        "{}",
        stdout(&out)
    );
    // Answered, the question is not asked again
    let out = create(&["--root", &arg, "create"]);
    assert!(out.status.success(), "{}", stdout(&out));
    assert!(!stdout(&out).contains("ask the person"), "{}", stdout(&out));
    fs::write(root.path().join("uv.lock"), "").unwrap();
    let out = run(&["--root", &arg, "check"]);
    assert!(out.status.success(), "{}", stdout(&out));

    // Every command's --help says more than its -h, and how it exits: every command `rotproof --help` lists, so one
    // added later is not left out
    let help = stdout(&run(&["--help"]));
    let commands: Vec<&str> = help
        .lines()
        .skip_while(|line| *line != "Commands:")
        .skip(1)
        .take_while(|line| !line.is_empty())
        .filter_map(|line| {
            line.strip_prefix("  ")
                .filter(|rest| !rest.starts_with(' '))
        })
        .filter_map(|rest| rest.split_whitespace().next())
        .filter(|command| *command != "help")
        .collect();
    assert!(commands.len() >= 7, "{commands:?}\n{help}");
    for command in commands {
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
        let root = declared(&format!(
            "stack = \"{stack}\"\nareas = [\"a\"]\n{}",
            ui_line(stack)
        ));
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
        let mut args = vec!["--root", &arg, "init", "--stack", stack];
        if matches!(stack, "python" | "typescript") {
            args.extend(["--ui-design", "atomic"]);
        }
        let out = run(&args);
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
        point_at_the_guide(root.path());
        let out = create(&["--root", &arg, "create", "--yes"]);
        assert!(out.status.success(), "{stack}: {}", stdout(&out));
        if stack == "python" {
            // The lock file is made by the tool of the project
            fs::write(root.path().join("uv.lock"), "").unwrap();
        }
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
    // A stack that offers a UI needs it named: one with screens and one without start from the same command
    let out = run(&["--root", &arg, "init", "--stack", "typescript"]);
    assert_eq!(out.status.code(), Some(2));
    let said = String::from_utf8_lossy(&out.stderr);
    assert!(
        said.contains("name the UI with --ui-design (atomic, none)"),
        "{said}"
    );
    assert!(tree(root.path()).is_empty(), "{:?}", tree(root.path()));
}

#[test]
fn init_upgrades_a_project_made_by_the_first_release() {
    // As `rotproof create` of 0.1.0 left a project: no files in the declaration, and settings without the deny rule,
    // which nothing adds any more
    let root = declared("stack = \"none\"\nareas = [\"a\"]\n");
    let r = root.path();
    let arg = root_arg(r);
    fs::write(
        r.join(".config/rotproof.toml"),
        "stack = \"none\"\nareas = [\"a\"]\n",
    )
    .unwrap();
    assert!(create(&["--root", &arg, "create"]).status.success());
    fs::create_dir_all(r.join(".claude")).unwrap();
    let settings = "{\n  \"hooks\": {}\n}\n";
    fs::write(r.join(".claude/settings.json"), settings).unwrap();
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
    // Nothing in the settings is Rotproof's to take out: it says nothing of them and leaves them as they are
    assert!(
        !stdout(&out).contains("updated .claude/settings.json"),
        "{}",
        stdout(&out)
    );
    assert_eq!(
        fs::read_to_string(r.join(".claude/settings.json")).unwrap(),
        settings
    );
    // Every field it adds to the declaration is named, as `rotproof create` names those it adds
    for field in [format!("files = \"{version}\""), "declined = []".into()] {
        assert!(
            stdout(&out).contains(&format!("added to .config/rotproof.toml: {field}\n")),
            "{}",
            stdout(&out)
        );
    }
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
    // A declaration with files and without declined: only declined is added, and said
    fs::write(
        r.join(".config/rotproof.toml"),
        "stack = \"none\"\nareas = [\"a\"]\nfiles = \"0.1.0\"\n",
    )
    .unwrap();
    let out = run(&["--root", &arg, "init"]);
    assert!(out.status.success(), "{}", stdout(&out));
    let printed = stdout(&out);
    let added: Vec<&str> = printed
        .lines()
        .filter(|line| line.starts_with("added to"))
        .collect();
    assert_eq!(
        added,
        ["added to .config/rotproof.toml: declined = []"],
        "{}",
        stdout(&out)
    );
    // Nothing to add: nothing said
    let out = run(&["--root", &arg, "init"]);
    assert!(!stdout(&out).contains("added to"), "{}", stdout(&out));
}

#[test]
fn init_writes_no_settings_and_asks_no_person_for_a_file_that_is_not_there() {
    // A project of the first release without .claude/settings.json: no update has a file to change, and the refresh of
    // the same run writes none, as `rotproof create` has no settings of its own
    let root = declared("stack = \"none\"\nareas = [\"a\"]\n");
    let r = root.path();
    let arg = root_arg(r);
    fs::write(
        r.join(".config/rotproof.toml"),
        "stack = \"none\"\nareas = [\"a\"]\n",
    )
    .unwrap();
    let version = env!("CARGO_PKG_VERSION");
    if version == "0.1.0" {
        return;
    }
    let out = run(&["--root", &arg, "init"]);
    assert!(out.status.success(), "{}", stdout(&out));
    assert!(
        !stdout(&out).contains(".claude/settings.json"),
        "{}",
        stdout(&out)
    );
    assert!(!stdout(&out).contains("needs a person"), "{}", stdout(&out));
    assert!(!r.join(".claude/settings.json").exists());
    let declaration = fs::read_to_string(r.join(".config/rotproof.toml")).unwrap();
    assert!(
        declaration.contains(&format!("files = \"{version}\"")),
        "{declaration}"
    );
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
            || PROJECT_FILES.contains(&path.as_str())
            || path == POINTER_FILE),
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
    fs::write(
        root.path().join("docs/knowledge/index.md"),
        "edited by hand\n",
    )
    .unwrap();
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
    write("domain/song.py", "Y = 2  # HACK about y\n");
    write("tests/test_model.py", "x = 1  # NOTE about x\n");
    // Not a comment, or in a path the project lists in unchecked
    write("domain/states.py", "TODO = 1\nPHONE = \"XXX-XXXX\"\n");
    write("scripts/generated.py", "# FIXME written by a tool\n");
    declare(
        r,
        "stack = \"python\"\nareas = [\"a\"]\nui_design = \"none\"\nunchecked = [\"scripts\"]\n",
    );
    let arg = root_arg(r);
    let out = run(&["--root", &arg, "check"]);
    let said = stdout(&out);
    assert_eq!(out.status.code(), Some(1), "{said}");
    // The heading names only what was found; each finding is its place, and the line under it as written
    assert!(
        said.contains(
            "no comment holds TODO or HACK (fix the finding now, write it as one line in the task's issue, or let \
             it go; then take the comment out. Why the code is written so stays, without the word)"
        ),
        "{said}"
    );
    assert!(
        said.contains("  domain/model.py:2\n    # TODO: split this\n"),
        "{said}"
    );
    assert!(
        said.contains("  domain/song.py:1\n    Y = 2  # HACK about y\n"),
        "{said}"
    );
    // Knowledge kept in a comment, under a heading of its own, which asks where it belongs
    let note = said
        .find(domain::markers::KNOWLEDGE_HEADING)
        .unwrap_or_else(|| panic!("{said}"));
    assert!(
        said[note..].contains("  tests/test_model.py:1\n    x = 1  # NOTE about x\n"),
        "{said}"
    );
    assert!(!said[note..].contains("domain/"), "{said}");
    assert!(
        !said.contains("states.py") && !said.contains("generated.py"),
        "{said}"
    );
    // The same text without the markers passes
    write("domain/model.py", "X = 1\n# Split this when it grows\n");
    write("domain/song.py", "Y = 2  # about y\n");
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
        said.contains("no comment holds TODO (fix the finding now"),
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
        "crates/composition/build.rs",
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
        "crates/protocol/Cargo.toml",
        "# TODO in a manifest\n[package]\nname = \"protocol\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
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
            "stack = \"rust\"\nareas = [\"a\"]\nui_design = \"none\"\nabsent = []\nunchecked = [\"crates/generated\"]\n",
        ),
    )
    .unwrap();
    let arg = root_arg(r);
    let out = run(&["--root", &arg, "check"]);
    let said = stdout(&out);
    assert_eq!(out.status.code(), Some(1), "{said}");
    assert!(
        said.contains("no comment holds TODO, FIXME or HACK (fix the finding now")
            && said.contains(domain::markers::KNOWLEDGE_HEADING),
        "{said}"
    );
    for found in [
        "  crates/domain/src/song.rs:2\n    // TODO: split this\n",
        "  crates/domain/tests/song.rs:1\n    /* FIXME\n",
        "  crates/domain/tests/song.rs:2\n    and HACK */\n",
        "  crates/composition/build.rs:1\n    /// NOTE on the build\n",
    ] {
        assert!(said.contains(found), "{found}:\n{said}");
    }
    assert_eq!(said.matches("\n  crates/").count(), 4, "{said}");
    for path in [
        "crates/domain/src/song.rs",
        "crates/domain/tests/song.rs",
        "crates/composition/build.rs",
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
        // The way out for work an issue tracks, in the forms the check accepts, and what it never looks at
        let said = said.replace("\n ", " ").replace('\n', " ");
        for link in [
            domain::markers::TodoLink::Number,
            domain::markers::TodoLink::Key,
        ] {
            assert!(said.contains(link.example()), "{args:?}: {said}");
        }
        assert!(
            said.contains("todo_link in the declaration")
                && said.contains("under `linked TODO`")
                && said.contains("Whether the issue exists or is open is not checked"),
            "{args:?}: {said}"
        );
    }
}

/// The declaration of a Python project with no screens that declares `todo_link` as `link`, over `clean_repo`'s.
fn declare_todo_link(root: &Path, link: &str) {
    declare(
        root,
        &format!(
            "stack = \"python\"\nareas = [\"a\"]\nui_design = \"none\"\ntodo_link = \"{link}\"\n"
        ),
    );
}

fn write_file(root: &Path, path: &str, text: &str) {
    let full = root.join(path);
    fs::create_dir_all(full.parent().unwrap()).unwrap();
    fs::write(full, text).unwrap();
}

#[test]
fn a_comment_tied_to_an_issue_is_listed_on_every_run_and_fails_nothing() {
    let root = clean_repo();
    let r = root.path();
    declare_todo_link(r, "number");
    write_file(
        r,
        "domain/model.py",
        "X = 1\n# TODO(#123): split this when the second price arrives\n",
    );
    write_file(
        r,
        "tests/test_price.py",
        "y = 2  # TODO(#9): cover the rounding\n",
    );
    let arg = root_arg(r);
    let out = run(&["--root", &arg, "check"]);
    let said = stdout(&out);
    assert_eq!(out.status.code(), Some(0), "{said}");
    // Apart from the findings, and each line with its place and its reference, sorted by path
    assert_eq!(
        said,
        "linked TODO (2)\n  domain/model.py:2      #123\n  tests/test_price.py:1  #9\n\
         the layers and every record keep the rules\n"
    );
    assert!(out.stderr.is_empty());
    // On every run, and the same each time
    let again = run(&["--root", &arg, "check"]);
    assert_eq!(stdout(&again), said);
    // What a CI step reads: the place is the first field of a line, the reference the last
    let read: Vec<(String, String)> = said
        .lines()
        .filter(|line| line.starts_with("  "))
        .map(|line| {
            let fields: Vec<&str> = line.split_whitespace().collect();
            (fields[0].to_string(), fields[fields.len() - 1].to_string())
        })
        .collect();
    assert_eq!(
        read,
        [
            ("domain/model.py:2".to_string(), "#123".to_string()),
            ("tests/test_price.py:1".to_string(), "#9".to_string())
        ]
    );
    // Taking the comment out empties the list, and nothing is printed for it
    write_file(r, "domain/model.py", "X = 1\n");
    write_file(r, "tests/test_price.py", "y = 2\n");
    let out = run(&["--root", &arg, "check"]);
    assert_eq!(stdout(&out), "the layers and every record keep the rules\n");
}

#[test]
fn a_key_is_listed_where_the_declaration_says_key() {
    let root = clean_repo();
    let r = root.path();
    declare_todo_link(r, "key");
    write_file(r, "domain/model.py", "# TODO(ABC-12): split this\n");
    // A number where a key is declared fails, and the key is no marker of its own kind
    write_file(r, "domain/song.py", "# TODO(#12): split this\n");
    write_file(
        r,
        "domain/price.py",
        "# TODO(XXX-3): one\n# TODO(NOTE-4): two\n",
    );
    let out = run(&["--root", &root_arg(r), "check"]);
    let said = stdout(&out);
    assert_eq!(out.status.code(), Some(1), "{said}");
    assert!(
        said.starts_with(
            "linked TODO (3)\n  domain/model.py:1  ABC-12\n  domain/price.py:1  XXX-3\n  domain/price.py:2  NOTE-4\n"
        ),
        "{said}"
    );
    assert!(
        said.contains("  domain/song.py:1\n    # TODO(#12): split this\n"),
        "{said}"
    );
    assert_eq!(said.matches("\n  domain/").count(), 4, "{said}");
    // The heading shows the form for the declared value, after the three ways out
    assert!(
        said.contains(
            "no comment holds TODO (fix the finding now, write it as one line in the task's issue, or let it go; \
             then take the comment out. Why the code is written so stays, without the word. Work that an existing \
             issue already tracks may stay as TODO(ABC-123): what is left, with that issue's key)"
        ),
        "{said}"
    );
    assert!(!said.contains("XXX-3\n    "), "{said}");
}

#[test]
fn a_comment_not_in_the_form_fails_and_the_heading_offers_the_form_after_the_ways_out() {
    let root = clean_repo();
    let r = root.path();
    declare_todo_link(r, "number");
    let heading = "no comment holds TODO (fix the finding now, write it as one line in the task's issue, or let it go; \
                   then take the comment out. Why the code is written so stays, without the word. Work that an \
                   existing issue already tracks may stay as TODO(#123): what is left, with that issue's number)";
    for bad in [
        "# TODO later",
        "# TODO #123",
        "# TODO(): x",
        "# TODO(#123)",
        "# TODO(#123) x",
        "# TODO(me): x",
        "# TODO(#1, #2): x",
        "# TODO(ABC-123): x",
        "# TODO(#0123): x",
    ] {
        write_file(r, "domain/model.py", &format!("X = 1\n{bad}\n"));
        let out = run(&["--root", &root_arg(r), "check"]);
        let said = stdout(&out);
        assert_eq!(out.status.code(), Some(1), "{bad}: {said}");
        assert!(said.contains(heading), "{bad}: {said}");
        assert!(
            said.contains(&format!("  domain/model.py:2\n    {bad}\n")),
            "{bad}: {said}"
        );
        assert!(!said.contains("linked TODO"), "{bad}: {said}");
    }
    // The other markers fail as they do without the declaration, and the form is no way out for them
    for other in ["FIXME", "XXX", "HACK", "NOTE"] {
        write_file(r, "domain/model.py", &format!("# {other}(#12): x\n"));
        let out = run(&["--root", &root_arg(r), "check"]);
        let said = stdout(&out);
        assert_eq!(out.status.code(), Some(1), "{other}: {said}");
        assert!(!said.contains("linked TODO"), "{other}: {said}");
        assert!(!said.contains("TODO(#123)"), "{other}: {said}");
    }
    // A line that holds a linked comment and another marker fails, and is not listed
    write_file(r, "domain/model.py", "# TODO(#12): x, and HACK\n");
    write_file(r, "domain/song.py", "# TODO(#13): y, TODO later\n");
    let out = run(&["--root", &root_arg(r), "check"]);
    let said = stdout(&out);
    assert_eq!(out.status.code(), Some(1), "{said}");
    assert!(!said.contains("linked TODO"), "{said}");
    assert!(said.contains("no comment holds TODO or HACK ("), "{said}");
    // With the marker that keeps knowledge, the line is listed and that marker fails as when alone
    write_file(r, "domain/model.py", "# TODO(#12): x, see NOTE\n");
    write_file(r, "domain/song.py", "Y = 1\n");
    let out = run(&["--root", &root_arg(r), "check"]);
    let said = stdout(&out);
    assert_eq!(out.status.code(), Some(1), "{said}");
    assert!(
        said.starts_with("linked TODO (1)\n  domain/model.py:1  #12\n"),
        "{said}"
    );
    assert!(
        said.contains(domain::markers::KNOWLEDGE_HEADING)
            && said.contains("  domain/model.py:1\n    # TODO(#12): x, see NOTE\n"),
        "{said}"
    );
    assert!(!said.contains("no comment holds TODO"), "{said}");
}

#[test]
fn what_is_found_file_by_file_comes_in_the_byte_order_of_the_paths() {
    // Made in an order that is neither the byte order nor the one NTFS lists them in, with `mid2.py` and the directory
    // `mid2/` (a path is compared whole: `.` comes before `/`), so the order is the same on every file system
    let root = clean_repo();
    let r = root.path();
    declare_todo_link(r, "number");
    let made = [
        "zeta.py",
        "alpha.py",
        "mid.py",
        "Beta.py",
        "b10.py",
        "b2.py",
        "_under.py",
        "song.py",
        "mid2/x.py",
        "mid2.py",
        "price.py",
    ];
    for (n, name) in made.iter().enumerate() {
        write_file(
            r,
            &format!("domain/{name}"),
            &format!("# TODO(#{}): later\n# NOTE a note\n", n + 1),
        );
    }
    let out = run(&["--root", &root_arg(r), "check"]);
    let said = stdout(&out);
    assert_eq!(out.status.code(), Some(1), "{said}");
    let mut sorted: Vec<String> = made.iter().map(|name| format!("domain/{name}")).collect();
    sorted.sort();
    assert_eq!(
        sorted[5..8],
        ["domain/mid.py", "domain/mid2.py", "domain/mid2/x.py"]
    );
    // The linked list, then the finding of the NOTE: each names every file once, in the same order
    let places: Vec<&str> = said
        .lines()
        .filter(|line| line.starts_with("  ") && !line.starts_with("   "))
        .map(|line| line.split_whitespace().next().unwrap())
        .collect();
    let expected: Vec<String> = sorted
        .iter()
        .map(|path| format!("{path}:1"))
        .chain(sorted.iter().map(|path| format!("{path}:2")))
        .collect();
    assert_eq!(places, expected, "{said}");
}

#[test]
fn the_linked_list_comes_before_the_warnings_and_the_findings() {
    let root = clean_repo();
    let r = root.path();
    declare_todo_link(r, "number");
    write_file(r, "domain/model.py", "# TODO(#12): x\n");
    write_file(r, "domain/song.py", "X = 1  # pyright: basic\n");
    let out = run(&["--root", &root_arg(r), "check"]);
    let said = stdout(&out);
    assert_eq!(out.status.code(), Some(0), "{said}");
    let linked = said.find("linked TODO (1)").unwrap();
    let warned = said.find("(a warning, which fails nothing)").unwrap();
    assert!(linked < warned, "{said}");
    write_file(r, "domain/price.py", "# FIXME y\n");
    let said = stdout(&run(&["--root", &root_arg(r), "check"]));
    let found = said.find("no comment holds FIXME").unwrap();
    assert!(
        said.find("linked TODO (1)").unwrap() < said.find("(a warning").unwrap()
            && said.find("(a warning").unwrap() < found,
        "{said}"
    );
}

#[test]
fn without_todo_link_the_form_is_a_marker_and_the_output_has_nothing_new() {
    let root = clean_repo();
    let r = root.path();
    write_file(r, "domain/model.py", "# TODO(#123): split this\n");
    let out = run(&["--root", &root_arg(r), "check"]);
    let said = stdout(&out);
    assert_eq!(out.status.code(), Some(1), "{said}");
    assert!(
        said.contains(
            "no comment holds TODO (fix the finding now, write it as one line in the task's issue, or let it go; \
             then take the comment out. Why the code is written so stays, without the word):\n"
        ),
        "{said}"
    );
    assert!(
        !said.contains("linked") && !said.contains("TODO(#123): what"),
        "{said}"
    );
    assert!(
        said.contains("  domain/model.py:1\n    # TODO(#123): split this\n"),
        "{said}"
    );
}

#[test]
fn an_unknown_todo_link_or_one_without_code_fails_the_declaration() {
    let root = clean_repo();
    let r = root.path();
    declare_todo_link(r, "issue");
    let out = run(&["--root", &root_arg(r), "check"]);
    let said = stdout(&out);
    assert_eq!(out.status.code(), Some(1), "{said}");
    assert!(
        said.contains(".config/rotproof.toml: unknown todo_link \"issue\" (known: number, key)"),
        "{said}"
    );
    // No code to read with records only: it says nothing, whatever its value
    for link in ["number", "key"] {
        let root = repo();
        declare(
            root.path(),
            &format!("stack = \"none\"\nareas = [\"operations\"]\ntodo_link = \"{link}\"\n"),
        );
        let out = run(&["--root", &root_arg(root.path()), "check"]);
        let said = stdout(&out);
        assert_eq!(out.status.code(), Some(1), "{said}");
        assert!(
            said.contains("stack = \"none\" has no layers")
                && said.contains(&format!("take out todo_link = \"{link}\"")),
            "{said}"
        );
    }
}

#[test]
fn a_comment_tied_to_an_issue_is_read_in_typescript_and_rust_too() {
    let ts = repo_with_ui("typescript", "\"ui\"");
    declare(
        ts.path(),
        "stack = \"typescript\"\nareas = [\"a\"]\nui_design = \"atomic\"\nabsent = [\"ui\"]\ntodo_link = \"key\"\n",
    );
    write_file(
        ts.path(),
        "src/domain/song.ts",
        "export const x = 1;\n// TODO(ABC-7): split this\nexport const s = '// TODO(ABC-8): in a string';\n",
    );
    let out = run(&["--root", &root_arg(ts.path()), "check"]);
    let said = stdout(&out);
    assert_eq!(out.status.code(), Some(0), "{said}");
    assert!(
        said.starts_with("linked TODO (1)\n  src/domain/song.ts:2  ABC-7\n"),
        "{said}"
    );
    let rust = repo_with_ui("rust", "");
    declare(
        rust.path(),
        "stack = \"rust\"\nareas = [\"a\"]\nui_design = \"none\"\nabsent = []\ntodo_link = \"number\"\n",
    );
    write_file(
        rust.path(),
        "crates/domain/src/song.rs",
        "/// TODO(#7): split this\npub fn f() {}\n",
    );
    let out = run(&["--root", &root_arg(rust.path()), "check"]);
    let said = stdout(&out);
    assert_eq!(out.status.code(), Some(0), "{said}");
    assert!(
        said.starts_with("linked TODO (1)\n  crates/domain/src/song.rs:1  #7\n"),
        "{said}"
    );
}

/// Run the binary with `args` as given, as Claude Code runs a hook: with `input` written to its stdin.
fn run_with_input(args: &[&str], input: &str) -> Output {
    use std::io::Write;
    use std::process::Stdio;
    let mut child = Command::new(env!("CARGO_BIN_EXE_rotproof"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the binary runs");
    // The binary exits on its command line before it reads any input
    let _ = child.stdin.take().unwrap().write_all(input.as_bytes());
    child.wait_with_output().unwrap()
}

#[test]
fn create_writes_no_settings_for_claude_code_neither_a_hook_nor_a_deny_rule() {
    let root = declared("stack = \"none\"\nareas = [\"a\"]\n");
    let r = root.path();
    assert!(create(&["--root", &root_arg(r), "create"]).status.success());
    // It wrote them once, to run no hook and to deny editing the approvals file, which an agent may now approve
    // through; a project that has them is upgraded by `rotproof init`
    assert!(!r.join(".claude/settings.json").exists(), "{:?}", tree(r));
    assert!(!r.join(".gemini").exists());
}

#[test]
fn a_command_line_that_names_a_removed_hook_says_so_and_exits_1_never_2() {
    // The two hooks Rotproof wrote into a project's Claude Code settings are no commands any more, but a settings file
    // that runs the binary by path keeps them through an upgrade. Claude Code reads exit code 2 from a Stop hook as
    // "send Claude back to work" and from a UserPromptSubmit hook as "block the prompt", where 1 is only shown: each
    // such command line fails with 1, whatever else is wrong with it, and says in one line what to do
    let root = tempfile::tempdir().unwrap();
    let arg = root_arg(root.path());
    let stop = r#"{"hook_event_name": "Stop", "stop_hook_active": false, "last_assistant_message": "未確認"}"#;
    let prompt = r#"{"hook_event_name": "UserPromptSubmit", "prompt": "go on"}"#;
    let start = r#"{"hook_event_name": "SessionStart"}"#;
    for (args, hook, input) in [
        (vec!["stop-hook"], "stop-hook", stop),
        (vec!["time-hook"], "time-hook", prompt),
        (vec!["time-hook"], "time-hook", start),
        (vec!["stop-hook"], "stop-hook", "not json"),
        (vec!["stop-hook"], "stop-hook", ""),
        (vec!["--root", &arg, "stop-hook"], "stop-hook", stop),
        (vec!["--root", &arg, "time-hook"], "time-hook", prompt),
        (
            vec!["--root", "no/such/directory", "time-hook"],
            "time-hook",
            start,
        ),
        (vec!["stop-hook", "extra"], "stop-hook", stop),
        (vec!["time-hook", "--bogus"], "time-hook", prompt),
        (
            vec!["--root", &arg, "stop-hook", "extra"],
            "stop-hook",
            stop,
        ),
        (
            vec!["--root", &arg, "time-hook", "--bogus"],
            "time-hook",
            prompt,
        ),
        (vec!["--bogus", "stop-hook"], "stop-hook", stop),
        (vec!["stop-hook", "--help"], "stop-hook", stop),
        (vec!["time-hook", "-h"], "time-hook", prompt),
    ] {
        let out = run_with_input(&args, input);
        assert_eq!(out.status.code(), Some(1), "{args:?}");
        assert_eq!(stdout(&out), "", "{args:?}");
        let said = String::from_utf8_lossy(&out.stderr);
        assert_eq!(said.lines().count(), 1, "{args:?}: {said}");
        assert!(
            said.starts_with(&format!(
                "rotproof {hook} was removed in Rotproof 1.0 and does nothing: in a project, run `rotproof init`, \
                 which takes the hook out of .claude/settings.json; elsewhere, remove the hook that runs it from \
                 your Claude Code settings"
            )),
            "{args:?}: {said}"
        );
    }
    // The version is not a failure, wherever the hook's name is
    let out = run_with_input(&["--version", "time-hook"], "{}");
    assert_eq!(out.status.code(), Some(0));
    assert!(!stdout(&out).is_empty());
    // Neither command is in the help
    let help = stdout(&run(&["--help"]));
    assert!(
        !help.contains("stop-hook") && !help.contains("time-hook"),
        "{help}"
    );
    // Any other command line is as it was: 2
    for args in [
        vec!["check", "extra"],
        vec!["index", "--bogus"],
        vec!["--bogus", "check"],
        vec!["bogus-command"],
        vec!["stop-hook-not", "x"],
        vec!["check", "stop-hook"],
    ] {
        let out = run_with_input(&args, "{}");
        assert_eq!(out.status.code(), Some(2), "{args:?}");
    }
}

#[test]
fn a_knowledge_document_is_not_named_in_the_log() {
    let root = clean_repo();
    let r = root.path();
    let arg = root_arg(r);
    let doc = "---\ntype: Knowledge\ntitle: API\ndescription: How the API is shaped.\ntags: [a]\nstatus: stable\n---\n\n# Shape\n\nOne endpoint.\n";
    fs::write(r.join("docs/knowledge/api.md"), doc).unwrap();
    assert!(run(&["--root", &arg, "index"]).status.success());
    // Added or edited, with no log entry naming it: git records the edit, and follows keeps it true to the code
    let out = run(&["--root", &arg, "check"]);
    assert!(out.status.success(), "{}", stdout(&out));
    // The Knowledge fields of the entries written before 1.0 stay as history, whatever they name
    fs::write(
        r.join("docs/log.md"),
        "# Log\n\n## 2026-10-02\n\n* Something\n  * **Knowledge**: knowledge/gone.md@0123abcd, knowledge/api.md@00000000\n",
    )
    .unwrap();
    let out = run(&["--root", &arg, "check"]);
    assert!(out.status.success(), "{}", stdout(&out));
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
        "approved in .config/rotproof-approved.toml (1):\n  infrastructure.db in domain/model.py: \
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
fn more_approvals_than_approved_max_fail_the_check_and_no_limit_means_none() {
    let root = clean_repo();
    let r = root.path();
    fs::write(
        r.join("domain/model.py"),
        "import infrastructure.db\nimport application\n",
    )
    .unwrap();
    fs::write(
        r.join(".config/rotproof-approved.toml"),
        approvals(&[
            ("domain/model.py", "infrastructure.db"),
            ("domain/model.py", "application"),
        ]),
    )
    .unwrap();
    let own = fs::read_to_string(r.join(".config/rotproof.toml")).unwrap();
    let declare = |max: &str| {
        fs::write(r.join(".config/rotproof.toml"), format!("{own}{max}")).unwrap();
        let out = run(&["--root", &root_arg(r), "check"]);
        (out.status.code(), stdout(&out))
    };
    // No approved_max: two approvals, and no limit
    let (code, said) = declare("");
    assert_eq!(code, Some(0), "{said}");
    // At the limit, and under it, the check passes
    for max in ["approved_max = 2\n", "approved_max = 5\n"] {
        let (code, said) = declare(max);
        assert_eq!(code, Some(0), "{max}: {said}");
    }
    // Over it, the check fails and says which file holds the limit; the approvals are still printed
    let (code, said) = declare("approved_max = 1\n");
    assert_eq!(code, Some(1), "{said}");
    assert!(
        said.contains(
            "the approvals stay within approved_max:\n  .config/rotproof-approved.toml: 2 approvals, and approved_max \
             in .config/rotproof.toml is 1:"
        ),
        "{said}"
    );
    assert!(
        said.contains("approved in .config/rotproof-approved.toml (2):"),
        "{said}"
    );
    // Zero allows none
    let (code, said) = declare("approved_max = 0\n");
    assert_eq!(code, Some(1), "{said}");
    assert!(
        said.contains("2 approvals, and approved_max in .config/rotproof.toml is 0"),
        "{said}"
    );
    // A limit that is no count is a declaration that cannot be read
    let (code, said) = declare("approved_max = -1\n");
    assert_eq!(code, Some(1), "{said}");
    assert!(
        said.contains(".config/rotproof.toml: invalid value: integer `-1`, expected u32"),
        "{said}"
    );
}

/// `rotproof approve ...` in `root`, with git's configuration as `config` says it (the text of a global config file, or
/// none): neither the machine's own nor the user's decides who an approval is signed by.
fn approve_with_git(root: &Path, config: Option<&str>, args: &[&str]) -> Output {
    let dir = tempfile::tempdir().unwrap();
    let global = dir.path().join("gitconfig");
    if let Some(text) = config {
        fs::write(&global, text).unwrap();
    }
    Command::new(env!("CARGO_BIN_EXE_rotproof"))
        .args(["--root", &root_arg(root), "approve"])
        .args(args)
        .env("GIT_CONFIG_GLOBAL", &global)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .expect("the binary runs")
}

#[test]
fn the_guide_and_the_help_say_the_baseline_is_written_once_by_the_users_ask_and_never_raised() {
    let flat = |text: String| text.split_whitespace().collect::<Vec<_>>().join(" ");
    for stack in ["python", "typescript", "rust"] {
        let guide = flat(stdout(&run(&["guide", "--stack", stack])));
        assert!(
            guide.contains("**A baseline is written once, when the user asks, and only shrinks.**")
                && guide.contains("Never run `rotproof baseline` again, never add an entry, and never raise `count`"),
            "{stack}: {guide}"
        );
    }
    let help = flat(stdout(&run(&["baseline", "--help"])));
    assert!(
        help.contains("Run it only when the user asked for it, never write the baseline again and never raise its count"),
        "{help}"
    );
    // The page on running Rotproof, which every project gets
    let root = clean_repo();
    let page = flat(fs::read_to_string(root.path().join(".rotproof/running.md")).unwrap());
    assert!(
        page.contains("`rotproof baseline`: only when the user asked you to freeze the violations there are, and only once."),
        "{page}"
    );
}

#[test]
fn approve_needs_a_reason_and_asks_nothing_and_prune_removes_only_what_matches_nothing() {
    let root = clean_repo();
    let r = root.path();
    fs::write(r.join("domain/model.py"), "import infrastructure.db\n").unwrap();
    let path = r.join(".config/rotproof-approved.toml");
    let named = Some("[user]\n\tname = Someone Else\n");
    let args = ["domain/model.py", "infrastructure.db"];
    let with_reason = |reason: &'static str| [args[0], args[1], "--reason", reason];
    let said = |out: &Output| String::from_utf8_lossy(&out.stderr).into_owned();

    // The reason is required: no reason, and an empty one, write nothing and ask nothing
    let out = approve_with_git(r, named, &args);
    assert_eq!(out.status.code(), Some(2));
    assert!(said(&out).contains("--reason"), "{}", said(&out));
    let out = approve_with_git(r, named, &with_reason("  "));
    assert_eq!(out.status.code(), Some(2));
    assert!(said(&out).contains("--reason is empty"), "{}", said(&out));
    // The file and the import are both needed, unless pruning, and a reason is no part of pruning
    let out = approve_with_git(r, named, &["domain/model.py", "--reason", "why"]);
    assert_eq!(out.status.code(), Some(2));
    let out = approve_with_git(r, named, &["--prune", "--reason", "why"]);
    assert_eq!(out.status.code(), Some(2));
    // Only what the check forbids is approved
    let out = approve_with_git(
        r,
        named,
        &["domain/model.py", "application", "--reason", "why"],
    );
    assert_eq!(out.status.code(), Some(2));
    assert!(said(&out).contains("no forbidden import"), "{}", said(&out));
    // The approval is signed with git's user.name, and without one it is not written
    let out = approve_with_git(r, None, &with_reason("the ORM's session"));
    assert_eq!(out.status.code(), Some(2));
    assert!(
        said(&out).contains("user.name is not set"),
        "{}",
        said(&out)
    );
    assert!(!path.exists());

    // Given a reason and a name it asks nothing, though stdin is no terminal, and the entry is what was given
    let out = approve_with_git(
        r,
        named,
        &with_reason("the \"ORM\" builds its session here"),
    );
    assert_eq!(out.status.code(), Some(0), "{}{}", stdout(&out), said(&out));
    assert!(
        stdout(&out).contains("wrote .config/rotproof-approved.toml"),
        "{}",
        stdout(&out)
    );
    let written = fs::read_to_string(&path).unwrap();
    assert!(written.contains("by = \"Someone Else\""), "{written}");
    let out = run(&["--root", &root_arg(r), "check"]);
    assert_eq!(out.status.code(), Some(0), "{}", stdout(&out));
    assert!(
        stdout(&out).contains(
            "approved in .config/rotproof-approved.toml (1):\n  infrastructure.db in domain/model.py: the \"ORM\" \
             builds its session here (Someone Else, "
        ),
        "{}",
        stdout(&out)
    );
    // Once: it is approved already
    let out = approve_with_git(r, named, &with_reason("again"));
    assert_eq!(out.status.code(), Some(2));
    assert!(said(&out).contains("approved already"), "{}", said(&out));
    fs::remove_file(&path).unwrap();

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
    fs::write(r.join("domain/model.py"), "import os\n").unwrap();
    let out = run(&["--root", &root_arg(r), "approve", "--prune"]);
    assert_eq!(out.status.code(), Some(0), "{}", stdout(&out));
    assert!(!fs::read_to_string(&path).unwrap().contains("[[kept]]"));
    let out = run(&["--root", &root_arg(r), "check"]);
    assert_eq!(out.status.code(), Some(0), "{}", stdout(&out));
}

/// `rotproof baseline ...` in `root`.
fn baseline_in(root: &Path, args: &[&str]) -> Output {
    let root = root_arg(root);
    let mut all = vec!["--root", root.as_str(), "baseline"];
    all.extend_from_slice(args);
    run(&all)
}

/// What `domain/model.py` holds in the tests of the baseline: three violations, each a finding of its own
const THREE: &str = "import infrastructure.db\nimport application\nimport shutil\n\n\ndef f(a):\n    shutil.rmtree(a)\n";

#[test]
fn a_baseline_freezes_what_fails_once_a_new_violation_fails_and_a_line_says_how_many_remain() {
    let root = clean_repo();
    let r = root.path();
    let path = r.join(".config/rotproof-baseline.toml");
    let said = |out: &Output| String::from_utf8_lossy(&out.stderr).into_owned();
    put(r, "domain/model.py", THREE);
    let (code, text) = checked(r);
    assert_eq!(code, Some(1), "{text}");
    // Nothing is printed of a baseline that is not there
    assert!(!text.contains("baseline"), "{text}");

    let out = baseline_in(r, &[]);
    assert_eq!(out.status.code(), Some(0), "{}{}", stdout(&out), said(&out));
    assert!(
        stdout(&out).contains("wrote .config/rotproof-baseline.toml: 3 entries"),
        "{}",
        stdout(&out)
    );
    let written = fs::read_to_string(&path).unwrap();
    assert!(written.contains("count = 3\n"), "{written}");
    for entry in ["infrastructure.db", "application", "shutil"] {
        assert!(
            written.contains(&format!(
                "[[entry]]\nfrom = \"domain/model.py\"\nimport = \"{entry}\"\n"
            )),
            "{written}"
        );
    }
    // No reason and no name: nobody decided to keep it
    assert!(
        !written.contains("reason") && !written.contains("by = "),
        "{written}"
    );

    // What it froze passes, and the one line says how many remain, without naming any
    let (code, text) = checked(r);
    assert_eq!(code, Some(0), "{text}");
    assert!(
        text.contains("baseline: 3 entries remain in .config/rotproof-baseline.toml\n"),
        "{text}"
    );
    assert!(!text.contains("infrastructure.db"), "{text}");

    // A new violation fails: another import in the file, and the same import in another file
    put(r, "domain/model.py", &format!("import tempfile\n{THREE}"));
    let (code, text) = checked(r);
    assert_eq!(code, Some(1), "{text}");
    assert!(
        text.contains("uses `tempfile`") && !text.contains("uses `shutil`"),
        "{text}"
    );
    put(r, "domain/model.py", THREE);
    put(r, "domain/other.py", "import infrastructure.db\n");
    let (code, text) = checked(r);
    assert_eq!(code, Some(1), "{text}");
    assert!(
        text.contains("domain/other.py:1: imports infrastructure.db"),
        "{text}"
    );
    fs::remove_file(r.join("domain/other.py")).unwrap();

    // It is written once: the file is there, and is not written again, which would take a new violation in
    put(r, "domain/model.py", &format!("import tempfile\n{THREE}"));
    let out = baseline_in(r, &[]);
    assert_eq!(out.status.code(), Some(2));
    assert!(said(&out).contains("is there already"), "{}", said(&out));
    assert_eq!(fs::read_to_string(&path).unwrap(), written);

    // An entry that matches nothing fails, and `--prune` removes it and lowers the count
    put(
        r,
        "domain/model.py",
        "import infrastructure.db\nimport shutil\n",
    );
    let (code, text) = checked(r);
    assert_eq!(code, Some(1), "{text}");
    assert!(
        text.contains("baselined, but no such forbidden import: application in domain/model.py"),
        "{text}"
    );
    let out = baseline_in(r, &["--prune"]);
    assert_eq!(out.status.code(), Some(0), "{}{}", stdout(&out), said(&out));
    assert!(
        stdout(&out).contains(
            "removed from .config/rotproof-baseline.toml: application in domain/model.py"
        ) && stdout(&out).contains("count in .config/rotproof-baseline.toml is 2 now, from 3"),
        "{}",
        stdout(&out)
    );
    let left = fs::read_to_string(&path).unwrap();
    assert!(
        left.contains("count = 2\n") && !left.contains("\"application\""),
        "{left}"
    );
    let (code, text) = checked(r);
    assert_eq!(code, Some(0), "{text}");
    assert!(text.contains("baseline: 2 entries remain"), "{text}");
    let out = baseline_in(r, &["--prune"]);
    assert!(
        stdout(&out).contains("nothing to prune"),
        "{}",
        stdout(&out)
    );

    // More entries than the count fail, though the entry added matches a violation: a new one, added by hand
    put(
        r,
        "domain/model.py",
        "import infrastructure.db\nimport shutil\nimport socket\n",
    );
    let (code, text) = checked(r);
    assert_eq!(code, Some(1), "{text}");
    fs::write(
        &path,
        format!("{left}\n[[entry]]\nfrom = \"domain/model.py\"\nimport = \"socket\"\n"),
    )
    .unwrap();
    let (code, text) = checked(r);
    assert_eq!(code, Some(1), "{text}");
    assert!(
        text.contains("the baseline stays within its count:\n  .config/rotproof-baseline.toml: 3 entries, and count is 2,"),
        "{text}"
    );
    fs::write(&path, &left).unwrap();
    put(
        r,
        "domain/model.py",
        "import infrastructure.db\nimport shutil\n",
    );

    // A count above the entries leaves room, which pruning takes away though it removes no entry
    fs::write(&path, left.replace("count = 2\n", "count = 5\n")).unwrap();
    let out = baseline_in(r, &["--prune"]);
    assert!(
        stdout(&out).contains("is 2 now, from 5"),
        "{}",
        stdout(&out)
    );

    // The file is spent once everything in it is fixed
    put(r, "domain/model.py", "x = 1\n");
    let out = baseline_in(r, &["--prune"]);
    assert_eq!(out.status.code(), Some(0), "{}{}", stdout(&out), said(&out));
    let (code, text) = checked(r);
    assert_eq!(code, Some(0), "{text}");
    assert!(
        text.contains(
            "baseline: no entry remains in .config/rotproof-baseline.toml: remove the file\n"
        ),
        "{text}"
    );
}

#[test]
fn a_baseline_leaves_the_approved_to_the_approvals_and_is_refused_where_it_cannot_freeze() {
    let named = Some("[user]\n\tname = Someone Else\n");
    let root = clean_repo();
    let r = root.path();
    let path = r.join(".config/rotproof-baseline.toml");
    let said = |out: &Output| String::from_utf8_lossy(&out.stderr).into_owned();

    // Nothing to freeze
    let out = baseline_in(r, &[]);
    assert_eq!(out.status.code(), Some(2));
    assert!(said(&out).contains("nothing to baseline"), "{}", said(&out));
    assert!(!path.exists());
    // Nothing to prune without a file
    let out = baseline_in(r, &["--prune"]);
    assert_eq!(out.status.code(), Some(0), "{}", said(&out));
    assert!(!path.exists());

    // An approved import is the approval's: the baseline does not hold it
    put(
        r,
        "domain/model.py",
        "import infrastructure.db\nimport application\n",
    );
    let approve = |import: &str| {
        let out = approve_with_git(
            r,
            named,
            &["domain/model.py", import, "--reason", "the ORM"],
        );
        assert_eq!(out.status.code(), Some(0), "{}", said(&out));
    };
    approve("infrastructure.db");
    let out = baseline_in(r, &[]);
    assert_eq!(out.status.code(), Some(0), "{}", said(&out));
    let written = fs::read_to_string(&path).unwrap();
    assert!(
        written.contains("count = 1\n")
            && written.contains("\"application\"")
            && !written.contains("infrastructure.db"),
        "{written}"
    );
    // A baselined import approved later is the approval's, and its entry matches nothing
    approve("application");
    let (code, text) = checked(r);
    assert_eq!(code, Some(1), "{text}");
    assert!(
        text.contains("baselined, but no such forbidden import: application in domain/model.py"),
        "{text}"
    );
    let out = baseline_in(r, &["--prune"]);
    assert_eq!(out.status.code(), Some(0), "{}", said(&out));
    let (code, text) = checked(r);
    assert_eq!(code, Some(0), "{text}");

    // Code still moving: the baseline waits until everything is in a layer
    let root = declared(
        "stack = \"python\"\nareas = [\"a\"]\nui_design = \"none\"\nmoving = [\"legacy\"]\n",
    );
    let m = root.path();
    let out = create(&["--root", &root_arg(m), "create", "--yes"]);
    assert!(out.status.success(), "{}", stdout(&out));
    put(m, "legacy/a.py", "x = 1\n");
    put(m, "domain/model.py", "import infrastructure.db\n");
    let out = baseline_in(m, &[]);
    assert_eq!(out.status.code(), Some(2));
    assert!(
        said(&out).contains("`moving` in .config/rotproof.toml still lists legacy"),
        "{}",
        said(&out)
    );
    assert!(!m.join(".config/rotproof-baseline.toml").exists());

    // A repository of records only has no layers to freeze, and an entry there matches nothing
    let records = repo();
    let out = baseline_in(records.path(), &[]);
    assert_eq!(out.status.code(), Some(2));
    assert!(said(&out).contains("no layers"), "{}", said(&out));
    put(
        records.path(),
        ".config/rotproof-baseline.toml",
        "count = 1\n\n[[entry]]\nfrom = \"a.py\"\nimport = \"b\"\n",
    );
    let (code, text) = checked(records.path());
    assert_eq!(code, Some(1), "{text}");
    assert!(
        text.contains("baselined, but no such forbidden import: b in a.py"),
        "{text}"
    );

    // A baseline that cannot be read fails, and a count that is missing is said
    let broken = clean_repo();
    put(
        broken.path(),
        ".config/rotproof-baseline.toml",
        "[[entry]]\nfrom = \"a.py\"\nimport = \"b\"\n",
    );
    let (code, text) = checked(broken.path());
    assert_eq!(code, Some(1), "{text}");
    assert!(text.contains("count is missing"), "{text}");
    let out = baseline_in(broken.path(), &["--prune"]);
    assert_eq!(out.status.code(), Some(2), "{}", stdout(&out));
}

#[test]
fn approve_takes_several_imports_of_one_file_for_one_reason_all_or_none() {
    let root = clean_repo();
    let r = root.path();
    fs::write(
        r.join("domain/model.py"),
        "import infrastructure.db\nimport infrastructure.web\n",
    )
    .unwrap();
    let path = r.join(".config/rotproof-approved.toml");
    let named = Some("[user]\n\tname = Someone Else\n");
    let said = |out: &Output| String::from_utf8_lossy(&out.stderr).into_owned();
    let approve = |imports: &[&str]| {
        let mut args = vec!["domain/model.py"];
        args.extend_from_slice(imports);
        args.extend_from_slice(&["--reason", "the framework wires both here"]);
        approve_with_git(r, named, &args)
    };

    // One name that is not forbidden stops the others, and every name that cannot be approved is said
    let out = approve(&[
        "infrastructure.db",
        "application",
        "infrastructure.web",
        "os",
    ]);
    assert_eq!(out.status.code(), Some(2));
    assert!(
        said(&out).contains("nothing is approved, as 2 of the 4 imports cannot be:"),
        "{}",
        said(&out)
    );
    assert!(
        said(&out).contains("no forbidden import of application"),
        "{}",
        said(&out)
    );
    assert!(
        said(&out).contains("no forbidden import of os"),
        "{}",
        said(&out)
    );
    assert!(!path.exists());
    // A name given twice is refused before anything is read
    let out = approve(&["infrastructure.db", "infrastructure.db"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(said(&out).contains("named twice"), "{}", said(&out));
    assert!(!path.exists());

    // Both, in one run: an entry for each, with the same reason, and the check passes
    let out = approve(&["infrastructure.db", "infrastructure.web"]);
    assert_eq!(out.status.code(), Some(0), "{}{}", stdout(&out), said(&out));
    assert!(
        stdout(&out).contains(
            "wrote .config/rotproof-approved.toml: infrastructure.db, infrastructure.web in domain/model.py kept by \
             Someone Else"
        ),
        "{}",
        stdout(&out)
    );
    let written = fs::read_to_string(&path).unwrap();
    assert_eq!(written.matches("[[kept]]").count(), 2, "{written}");
    assert_eq!(
        written
            .matches("reason = \"the framework wires both here\"")
            .count(),
        2,
        "{written}"
    );
    let out = run(&["--root", &root_arg(r), "check"]);
    assert_eq!(out.status.code(), Some(0), "{}", stdout(&out));
    assert!(
        stdout(&out).contains("approved in .config/rotproof-approved.toml (2):"),
        "{}",
        stdout(&out)
    );
    // One approved already stops the run again, and the file stays as it was
    fs::write(
        r.join("domain/model.py"),
        "import infrastructure.db\nimport infrastructure.web\nimport infrastructure.mail\n",
    )
    .unwrap();
    let out = approve(&["infrastructure.mail", "infrastructure.db"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(said(&out).contains("approved already"), "{}", said(&out));
    assert_eq!(fs::read_to_string(&path).unwrap(), written);
}

/// Every key the check asks to pin in knowledge/model.md with `rotproof follows`, from its output.
fn to_pin(said: &str) -> Vec<String> {
    said.split('`')
        .filter_map(|part| part.strip_prefix("rotproof follows knowledge/model.md "))
        .map(str::to_string)
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
    // The check names the command that pins every key: run as it says, the document matches its code
    let placeholders: Vec<(String, String)> = keys
        .iter()
        .map(|key| (key.to_string(), "00000000".to_string()))
        .collect();
    fs::write(r.join("docs/knowledge/model.md"), document(&placeholders)).unwrap();
    assert!(run(&["--root", &arg, "index"]).status.success());
    let said = stdout(&run(&["--root", &arg, "check"]));
    let pins = to_pin(&said);
    assert_eq!(pins, keys, "{said}");
    let pin = |keys: &[String]| {
        let mut args = vec!["--root", &arg, "follows", "knowledge/model.md"];
        args.extend(keys.iter().map(String::as_str));
        let out = run(&args);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let said = stdout(&out);
        assert!(said.contains("wrote docs/knowledge/model.md"), "{said}");
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
    assert_eq!(to_pin(&said), ["domain/model.py", "domain/"], "{said}");
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
        to_pin(&said)
            .iter()
            .any(|key| key == "domain/model.py::Song"),
        "{said}"
    );
    assert!(
        !to_pin(&said)
            .iter()
            .any(|key| key == "domain/model.py::Song.play"),
        "{said}"
    );

    // A file added to a followed directory changes it
    pin(&to_pin(&said));
    let (code, said) = check();
    assert_eq!(code, Some(0), "{said}");
    fs::write(r.join("domain/more.py"), "MORE = 1\n").unwrap();
    let (code, said) = check();
    assert_eq!(code, Some(1), "{said}");
    assert_eq!(to_pin(&said), ["domain/"], "{said}");

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
        document(&[("domain/model.md::f".into(), "00000000".into())]),
    )
    .unwrap();
    let (code, said) = check();
    assert_eq!(code, Some(1), "{said}");
    assert!(
        said.contains("outside the files whose definitions are read"),
        "{said}"
    );
}

#[test]
fn a_records_only_repository_follows_definitions_and_directories_of_any_language() {
    // `stack = "none"`: no layers, and the keys still read the code of every language
    let root = repo();
    let r = root.path();
    let arg = root_arg(r);
    let out = create(&["--root", &arg, "create", "--yes"]);
    assert!(out.status.success(), "{}", stdout(&out));
    fs::write(
        r.join("docs/log.md"),
        "# Log\n\n## 2026-10-02\n\n* Something\n",
    )
    .unwrap();
    fs::create_dir_all(r.join("web")).unwrap();
    let router =
        "export class Router {\n  add(a: string) { return 1; }\n}\nexport const other = 1;\n";
    let table = "/// A table.\npub struct Table;\n\nimpl Table {\n    pub fn add(&self) -> u8 {\n        1\n    }\n}\n\n\
                 fn other() {}\n";
    fs::write(r.join("web/api.ts"), router).unwrap();
    fs::write(r.join("web/table.rs"), table).unwrap();
    let keys = ["web/api.ts::Router.add", "web/table.rs::Table", "web/"];
    let follows: String = keys
        .iter()
        .map(|key| format!("  {key}: \"00000000\"\n"))
        .collect();
    fs::write(
        r.join("docs/knowledge/model.md"),
        format!(
            "---\ntype: Knowledge\ntitle: Model\ndescription: The model.\ntags: [operations]\nstatus: stable\n\
             follows:\n{follows}---\n\n# Shape\n\nA table.\n"
        ),
    )
    .unwrap();
    assert!(run(&["--root", &arg, "index"]).status.success());
    let check = || {
        let out = run(&["--root", &arg, "check"]);
        (out.status.code(), stdout(&out))
    };
    let (code, said) = check();
    assert_eq!(code, Some(1), "{said}");
    assert_eq!(to_pin(&said), keys, "{said}");
    let pin = |keys: &[String]| {
        let mut args = vec!["--root", &arg, "follows", "knowledge/model.md"];
        args.extend(keys.iter().map(String::as_str));
        let out = run(&args);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    };
    pin(&to_pin(&said));
    let (code, said) = check();
    assert_eq!(code, Some(0), "{said}");

    // Code outside the definitions followed fails the directory alone
    fs::write(r.join("web/api.ts"), router.replace("= 1;", "= 2;")).unwrap();
    fs::write(
        r.join("web/table.rs"),
        table.replace("fn other() {}", "fn other() { }"),
    )
    .unwrap();
    let (code, said) = check();
    assert_eq!(code, Some(1), "{said}");
    assert_eq!(to_pin(&said), ["web/"], "{said}");
    pin(&to_pin(&said));

    // A change inside them fails the definition and the directory, and not the other definition
    fs::write(r.join("web/api.ts"), router.replace("return 1", "return 3")).unwrap();
    fs::write(
        r.join("web/table.rs"),
        table.replace("        1\n", "        2\n"),
    )
    .unwrap();
    let (_, said) = check();
    assert_eq!(
        to_pin(&said),
        ["web/api.ts::Router.add", "web/table.rs::Table", "web/"],
        "{said}"
    );
    pin(&to_pin(&said));

    // A definition gone fails as a dangling link does
    fs::write(r.join("web/api.ts"), "export const other = 1;\n").unwrap();
    let (code, said) = check();
    assert_eq!(code, Some(1), "{said}");
    assert!(
        said.contains("follows web/api.ts::Router.add, which is not there"),
        "{said}"
    );

    // A file the parser gives up on fails as unreadable, not as a definition gone
    fs::write(r.join("web/api.ts"), router.replace("= 1;", "= ;")).unwrap();
    let (code, said) = check();
    assert_eq!(code, Some(1), "{said}");
    assert!(
        said.contains(
            "follows web/api.ts::Router.add: web/api.ts cannot be parsed, so its definitions cannot be read: line 4:"
        ),
        "{said}"
    );
    assert!(!said.contains("which is not there"), "{said}");
}

#[test]
fn approve_has_no_reviewed_flag_and_follows_is_the_one_way_to_re_pin() {
    let root = clean_repo();
    let r = root.path();
    let arg = root_arg(r);
    let doc = "---\ntype: Knowledge\ntitle: M\ndescription: D.\ntags: [a]\nstatus: stable\nfollows:\n  \
               domain/__init__.py: \"00000000\"\n---\n\n# Shape\n\nText.\n";
    fs::write(r.join("docs/knowledge/model.md"), doc).unwrap();
    // The flag is gone: clap refuses it, and nothing is written
    let out = run(&["--root", &arg, "approve", "--reviewed"]);
    assert_eq!(out.status.code(), Some(2));
    let said = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(said.contains("unexpected argument '--reviewed'"), "{said}");
    assert_eq!(
        fs::read_to_string(r.join("docs/knowledge/model.md")).unwrap(),
        doc
    );
    assert!(!stdout(&run(&["approve", "--help"])).contains("--reviewed"));
    // The check names the command that pins the key, and it is `rotproof follows`
    let out = run(&["--root", &arg, "check"]);
    let said = stdout(&out);
    assert!(
        said.contains("`rotproof follows knowledge/model.md domain/__init__.py`"),
        "{said}"
    );
}

#[test]
fn follows_pins_the_keys_in_the_document_and_prints_the_log_line() {
    let root = clean_repo();
    let r = root.path();
    let arg = root_arg(r);
    let doc = r.join("docs/knowledge/model.md");
    fs::write(r.join("domain/model.py"), "def play():\n    return 1\n").unwrap();
    let before = "---\ntype: Knowledge\ntitle: M\ndescription: D.\ntags: [a]\nstatus: stable\n---\n\n# Shape\n\n\
                  It plays.\n";
    fs::write(&doc, before).unwrap();
    assert!(run(&["--root", &arg, "index"]).status.success());

    // What is not there, or not a key, fails with exit 2 and writes nothing, the keys that could be pinned too
    let out = run(&[
        "--root",
        &arg,
        "follows",
        "knowledge/model.md",
        "domain/model.py",
        "domain/gone.py",
        "domain\\x.py",
    ]);
    assert_eq!(out.status.code(), Some(2));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("domain/gone.py: not there") && err.contains("not a path from the root"),
        "{err}"
    );
    assert_eq!(fs::read_to_string(&doc).unwrap(), before);

    // Named from the root, the keys are added, and the check passes with no log line to write
    let out = run(&[
        "--root",
        &arg,
        "follows",
        "docs/knowledge/model.md",
        "domain/model.py::play",
        "domain/",
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let said = stdout(&out);
    assert!(
        said.contains("  domain/model.py::play: added ") && said.contains("  domain/: added "),
        "{said}"
    );
    let text = fs::read_to_string(&doc).unwrap();
    assert!(
        text.contains("follows:\n  domain/model.py::play: \""),
        "{text}"
    );
    assert!(said.contains("wrote docs/knowledge/model.md"), "{said}");
    assert!(!said.contains("**Knowledge**"), "{said}");
    let out = run(&["--root", &arg, "check"]);
    assert!(out.status.success(), "{}", stdout(&out));

    // Pinned as the code is now: nothing written
    let out = run(&["--root", &arg, "follows", "knowledge/model.md", "domain/"]);
    assert!(out.status.success());
    assert!(stdout(&out).contains("nothing written"), "{}", stdout(&out));
    assert_eq!(fs::read_to_string(&doc).unwrap(), text);

    // Called as before 0.4.0, with the keys alone, it says the new form and writes nothing
    let out = run(&["--root", &arg, "follows", "domain/model.py", "domain/"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("takes the document first"),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(fs::read_to_string(&doc).unwrap(), text);
    // A document and a key are needed
    assert_eq!(run(&["--root", &arg, "follows"]).status.code(), Some(2));
    assert_eq!(
        run(&["--root", &arg, "follows", "knowledge/model.md"])
            .status
            .code(),
        Some(2)
    );
}

#[test]
fn follows_pins_a_table_of_a_toml_file_and_the_check_fails_when_only_that_table_changes() {
    let root = clean_repo();
    let r = root.path();
    let arg = root_arg(r);
    let doc = r.join("docs/knowledge/model.md");
    let units =
        "areas = [\"a\"]\nfiles = \"1\"\n\n[[unit]]\nname = \"x\"\n\n[[unit]]\nname = \"y\"\n";
    fs::write(r.join("domain/units.toml"), units).unwrap();
    fs::write(
        &doc,
        "---\ntype: Knowledge\ntitle: M\ndescription: D.\ntags: [a]\nstatus: stable\n---\n\n# Shape\n\nIt plays.\n",
    )
    .unwrap();
    assert!(run(&["--root", &arg, "index"]).status.success());

    // A table that is not there is said as a definition that is not there is, and nothing is written
    let before = fs::read_to_string(&doc).unwrap();
    let out = run(&[
        "--root",
        &arg,
        "follows",
        "knowledge/model.md",
        "domain/units.toml::gone",
    ]);
    assert_eq!(out.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("domain/units.toml::gone: not there"),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(fs::read_to_string(&doc).unwrap(), before);

    let out = run(&[
        "--root",
        &arg,
        "follows",
        "knowledge/model.md",
        "domain/units.toml::unit",
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        fs::read_to_string(&doc)
            .unwrap()
            .contains("follows:\n  domain/units.toml::unit: \""),
        "{}",
        fs::read_to_string(&doc).unwrap()
    );
    let out = run(&["--root", &arg, "check"]);
    assert!(out.status.success(), "{}", stdout(&out));

    // The fields above the tables are not the table
    fs::write(
        r.join("domain/units.toml"),
        units
            .replace("\"1\"", "\"2\"")
            .replace("[\"a\"]", "[\"a\", \"b\"]"),
    )
    .unwrap();
    let out = run(&["--root", &arg, "check"]);
    assert!(out.status.success(), "{}", stdout(&out));

    // A unit is
    fs::write(r.join("domain/units.toml"), units.replace("\"y\"", "\"z\"")).unwrap();
    let out = run(&["--root", &arg, "check"]);
    assert_eq!(out.status.code(), Some(1), "{}", stdout(&out));
    assert!(
        stdout(&out).contains(
            "follows domain/units.toml::unit, which changed since the document was last reviewed"
        ),
        "{}",
        stdout(&out)
    );
}

/// The declaration of a Python monorepo of two units, its fields above the tables and `shared` as `shared`.
fn two_units(shared: &str) -> String {
    format!(
        "stack = \"python\"\nareas = [\"a\"]\nui_design = \"none\"\nfiles = \"{}\"\n\n[[unit]]\nname = \"core\"\n\
         path = \"packages/core/src/core\"\nshared = {shared}\n\n[[unit]]\nname = \"billing\"\n\
         path = \"packages/billing/src/billing\"\n",
        env!("CARGO_PKG_VERSION")
    )
}

#[test]
fn a_project_that_declares_units_gets_the_layers_of_each_and_is_checked_as_one() {
    let root = declared(&two_units("true"));
    let r = root.path();
    let arg = root_arg(r);
    let out = create(&["--root", &arg, "create", "--yes"]);
    assert!(out.status.success(), "{}", stdout(&out));
    for unit in ["core", "billing"] {
        for layer in ["domain", "composition", "infrastructure/inbound"] {
            let made = format!("packages/{unit}/src/{unit}/{layer}/__init__.py");
            assert!(r.join(&made).is_file(), "{made}");
        }
    }
    // No unit is implicit: the layers of the root are not made
    assert!(!r.join("domain").exists());
    fs::write(
        r.join("docs/log.md"),
        "# Log\n\n## 2026-10-02\n\n* Something\n",
    )
    .unwrap();
    let out = run(&["--root", &arg, "check"]);
    assert!(out.status.success(), "{}", stdout(&out));

    // What a unit lacks is its own: a layer deleted from one unit is missing there, not in the other
    fs::remove_dir_all(r.join("packages/billing/src/billing/application")).unwrap();
    let said = stdout(&run(&["--root", &arg, "check"]));
    assert!(
        said.contains("packages/billing/src/billing/application/"),
        "{said}"
    );
    assert!(
        !said.contains("packages/core/src/core/application/"),
        "{said}"
    );
}

#[test]
fn the_map_a_project_keeps_in_agents_md_is_told_when_it_is_behind_by_check_and_init() {
    let absent = "path = \"packages/billing/src/billing\"\n";
    let root =
        declared(&two_units("true").replace(absent, &format!("{absent}absent = [\"protocol\"]\n")));
    let r = root.path();
    let arg = root_arg(r);
    let out = create(&["--root", &arg, "create", "--yes"]);
    assert!(out.status.success(), "{}", stdout(&out));
    // Rotproof writes no AGENTS.md: a project without one has no map to judge. The CLAUDE.md is the test's own
    assert!(!r.join("AGENTS.md").exists());
    assert_eq!(fs::read_to_string(r.join(POINTER_FILE)).unwrap(), POINTER);
    fs::write(
        r.join("docs/log.md"),
        "# Log\n\n## 2026-10-02\n\n* Something\n",
    )
    .unwrap();
    let out = run(&["--root", &arg, "check"]);
    assert!(
        out.status.success() && !stdout(&out).contains("the map in AGENTS.md"),
        "{}",
        stdout(&out)
    );

    // The project keeps a map of its own, a row for each layer of each unit, in its own words
    let row = |unit: &str, layer: &str| {
        format!("| `packages/{unit}/src/{unit}/{layer}/` | {unit}: mine |\n")
    };
    let mut rows = String::new();
    for layer in [
        "composition",
        "infrastructure",
        "application",
        "protocol",
        "domain",
    ] {
        rows.push_str(&row("core", layer));
        if layer != "protocol" {
            rows.push_str(&row("billing", layer));
        }
    }
    let agents = format!(
        "# AGENTS.md\n\n## Map\n\n| Path | Content |\n|---|---|\n{rows}| `docs/` | Records |\n"
    );
    fs::write(r.join("AGENTS.md"), &agents).unwrap();
    let out = run(&["--root", &arg, "check"]);
    assert!(
        out.status.success() && !stdout(&out).contains("AGENTS.md has no row"),
        "{}",
        stdout(&out)
    );

    // The layer is declared absent no more, and made: the map is behind, which is said and fails nothing
    let declaration = fs::read_to_string(r.join(".config/rotproof.toml")).unwrap();
    fs::write(
        r.join(".config/rotproof.toml"),
        declaration.replace("absent = [\"protocol\"]\n", ""),
    )
    .unwrap();
    let out = create(&["--root", &arg, "create", "--yes"]);
    assert!(out.status.success(), "{}", stdout(&out));
    let behind = "AGENTS.md has no row in its map for packages/billing/src/billing/protocol/, the layer protocol of \
                  the unit billing: add it, or declare the layer absent in .config/rotproof.toml";
    let out = run(&["--root", &arg, "check"]);
    let said = stdout(&out);
    assert!(out.status.success(), "{said}");
    assert!(
        said.contains(
            "the map in AGENTS.md lists the layers .config/rotproof.toml declares (a warning, which fails nothing) (1):"
        ),
        "{said}"
    );
    assert!(said.contains(&format!("  {behind}\n")), "{said}");
    let out = run(&["--root", &arg, "init"]);
    assert!(out.status.success(), "{}", stdout(&out));
    assert!(
        stdout(&out).contains(&format!("note: {behind}\n")),
        "{}",
        stdout(&out)
    );

    // Add the row, and both are quiet; the file is the project's, and was never written over
    assert_eq!(fs::read_to_string(r.join("AGENTS.md")).unwrap(), agents);
    fs::write(
        r.join("AGENTS.md"),
        agents.replace(
            "| `docs/`",
            &format!("{}| `docs/`", row("billing", "protocol")),
        ),
    )
    .unwrap();
    let said = stdout(&run(&["--root", &arg, "check"]));
    assert!(!said.contains("AGENTS.md has no row"), "{said}");
    let said = stdout(&run(&["--root", &arg, "init"]));
    assert!(!said.contains("note: "), "{said}");
}

#[test]
fn a_declaration_of_units_that_cannot_be_read_says_every_reason() {
    for (declaration, said) in [
        (
            two_units("false").replace("name = \"billing\"", "name = \"core\""),
            vec!["two units are named \"core\""],
        ),
        (
            two_units("false")
                .replace("packages/core/src/core", "packages/billing/src/billing")
                .replace("name = \"billing\"", "name = \"Bad Name\""),
            vec![
                "unit name \"Bad Name\" is not a name: start with a lowercase letter, then lowercase letters, digits \
                 and _",
                "units \"core\" and \"Bad Name\" both have path \"packages/billing/src/billing\"",
            ],
        ),
        (
            two_units("true").replace(
                "shared = true\n",
                "shared = true\nentry = [\"packages/core/src/core/__main__.py\"]\n",
            ),
            vec![
                "unit \"core\" is shared and declares entry: an app (a unit with entry) wires the units",
            ],
        ),
        (
            two_units("false").replace(
                "ui_design = \"none\"\n",
                "ui_design = \"none\"\nabsent = [\"domain\"]\n",
            ),
            vec![
                "absent lists [\"domain\"], and the project declares units: name what each unit lacks in its own",
            ],
        ),
        (
            two_units("false") + "absent = [\"domainx\"]\n",
            vec![
                "unit \"billing\" names \"domainx\" in absent, which the python layout does not have",
            ],
        ),
        (
            two_units("false")
                + "absent = [\"infrastructure.inbound\", \"infrastructure.outbound\"]\n",
            vec![
                "unit \"billing\" names every level of infrastructure in absent, but not infrastructure itself, \
                 which holds only its levels: list infrastructure, so that it is not made as a directory with \
                 nothing in it",
            ],
        ),
    ] {
        // No manifest or lock file: `create` makes no directory of a declaration that cannot be read
        let root = unlocked(&declaration);
        let arg = root_arg(root.path());
        for command in [&["create"][..], &["check"]] {
            let mut args = vec!["--root", &arg];
            args.extend(command);
            let out = run(&args);
            let all = format!("{}{}", stdout(&out), String::from_utf8_lossy(&out.stderr));
            let all = all.split_whitespace().collect::<Vec<_>>().join(" ");
            for line in &said {
                let line = line.split_whitespace().collect::<Vec<_>>().join(" ");
                assert!(all.contains(&line), "{command:?}: {line}\n{all}");
            }
            assert!(!out.status.success(), "{command:?}: {declaration}");
        }
        assert!(!root.path().join("packages").exists(), "{declaration}");
    }
    // A repository of records only has no units
    let root = declared(&format!(
        "stack = \"none\"\nareas = [\"a\"]\nfiles = \"{}\"\n[[unit]]\nname = \"a\"\npath = \"a\"\n",
        env!("CARGO_PKG_VERSION")
    ));
    let out = run(&["--root", &root_arg(root.path()), "check"]);
    let said = stdout(&out);
    assert!(
        said.contains("stack = \"none\" has no layers, so it has no units: take out [[unit]]"),
        "{said}"
    );
}

#[test]
fn follows_pins_the_units_of_the_declaration_and_the_check_fails_when_a_unit_changes_not_the_fields_above()
 {
    let root = declared(&two_units("false"));
    let r = root.path();
    let arg = root_arg(r);
    assert!(
        create(&["--root", &arg, "create", "--yes"])
            .status
            .success(),
        "create"
    );
    fs::write(
        r.join("docs/log.md"),
        "# Log\n\n## 2026-10-02\n\n* Something\n",
    )
    .unwrap();
    let doc = r.join("docs/knowledge/units.md");
    fs::write(
        &doc,
        "---\ntype: Knowledge\ntitle: Units\ndescription: Why the code is in two units.\ntags: [a]\nstatus: stable\n---\n\n\
         # Why two\n\nThey are bought apart.\n",
    )
    .unwrap();
    assert!(run(&["--root", &arg, "index"]).status.success());
    let key = ".config/rotproof.toml::unit";
    let out = run(&["--root", &arg, "follows", "knowledge/units.md", key]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let pinned = fs::read_to_string(&doc).unwrap();
    assert!(
        pinned.contains(&format!("follows:\n  {key}: \"")),
        "{pinned}"
    );
    let out = run(&["--root", &arg, "check"]);
    assert!(out.status.success(), "{}", stdout(&out));

    // The fields above the tables are not the tables: `areas` and `files` change, and the document still holds
    let before = fs::read_to_string(r.join(".config/rotproof.toml")).unwrap();
    declare(
        r,
        &before
            .replace("areas = [\"a\"]", "areas = [\"a\", \"b\"]")
            .replace(
                &format!("files = \"{}\"", env!("CARGO_PKG_VERSION")),
                &format!("files = \"{}\"\ndeclined = []", env!("CARGO_PKG_VERSION")),
            ),
    );
    let out = run(&["--root", &arg, "check"]);
    assert!(out.status.success(), "{}", stdout(&out));

    // A unit is: the document is to be read again
    declare(r, &before.replace("shared = false", "shared = true"));
    let out = run(&["--root", &arg, "check"]);
    let said = stdout(&out);
    assert_eq!(out.status.code(), Some(1), "{said}");
    assert!(
        said.contains(&format!(
            "follows {key}, which changed since the document was last reviewed"
        )),
        "{said}"
    );
    // Reviewing it pins the new units, and the check passes again once the list of who reads the shared unit is written
    let out = run(&["--root", &arg, "follows", "knowledge/units.md", key]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        said.contains("out of date, run `rotproof index`: .rotproof/readers.md"),
        "{said}"
    );
    let out = run(&["--root", &arg, "index"]);
    assert!(out.status.success(), "{}", stdout(&out));
    let out = run(&["--root", &arg, "check"]);
    assert!(out.status.success(), "{}", stdout(&out));

    // A project with no unit has no such table, as a definition that is not there is said
    declare(
        r,
        &format!(
            "stack = \"python\"\nareas = [\"a\"]\nui_design = \"none\"\nfiles = \"{}\"\n",
            env!("CARGO_PKG_VERSION")
        ),
    );
    let out = run(&["--root", &arg, "follows", "knowledge/units.md", key]);
    assert_eq!(out.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&out.stderr).contains(&format!("{key}: not there")),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// A Python modular monolith of three units, made by `rotproof create` and kept: `core` is shared, in a package of its
/// own with its manifest (`root`), `billing` is not, and `app` is an app that holds only `composition` and
/// `infrastructure`, with its entry file outside its path, which `rotproof create` makes. Each unit's own `__init__.py`
/// is there, and the manifest of the project at the root.
fn modular_monolith() -> tempfile::TempDir {
    let root = declared(&format!(
        "stack = \"python\"\nareas = [\"a\"]\nui_design = \"none\"\nfiles = \"{}\"\n\n[[unit]]\nname = \"core\"\n\
         path = \"packages/core/src/core\"\nroot = \"packages/core\"\nshared = true\n\n[[unit]]\nname = \"billing\"\n\
         path = \"modules/billing\"\n\n[[unit]]\nname = \"app\"\npath = \"modules/app\"\nentry = [\"bin/run.py\"]\n\
         absent = [\"domain\", \"application\", \"protocol\"]\n\n[dependencies]\n\
         without_import = [\"core\", \"billing\"]\n",
        env!("CARGO_PKG_VERSION")
    ));
    let r = root.path();
    let out = create(&["--root", &root_arg(r), "create", "--yes"]);
    assert!(out.status.success(), "{}", stdout(&out));
    fs::write(
        r.join("docs/log.md"),
        "# Log\n\n## 2026-10-02\n\n* Something\n",
    )
    .unwrap();
    for file in [
        "pyproject.toml",
        "packages/core/pyproject.toml",
        "packages/core/src/core/__init__.py",
        "modules/billing/__init__.py",
    ] {
        fs::write(r.join(file), "").unwrap();
    }
    // The units that import another list it, by the name of its manifest, which `core` has none of: the name of the
    // unit
    for (unit, name, listed) in [
        ("billing", "billing", "\"core\""),
        ("app", "app", "\"core\", \"billing\""),
    ] {
        fs::write(
            r.join(format!("modules/{unit}/pyproject.toml")),
            format!("[project]\nname = \"{name}\"\ndependencies = [{listed}]\n"),
        )
        .unwrap();
    }
    root
}

/// What `rotproof check` prints under the structure heading, and whether it passed.
fn structure_findings(root: &Path) -> (bool, String) {
    let out = run(&["--root", &root_arg(root), "check"]);
    let said = stdout(&out);
    let findings = said
        .split("the tree matches .config/rotproof.toml:\n")
        .nth(1)
        .map(|rest| rest.split("\n\n").next().unwrap_or(rest))
        .unwrap_or("")
        .trim_end()
        .lines()
        // Up to the heading of the next check, whose findings are another matter
        .take_while(|line| line.starts_with("  "))
        .collect::<Vec<_>>()
        .join("\n");
    (out.status.success(), findings)
}

#[test]
fn a_modular_monolith_of_three_units_passes_and_each_new_finding_names_its_unit() {
    let root = modular_monolith();
    let r = root.path();
    // `create` made the entry file the app declares, outside its path, and the check passes right after it
    let entry = fs::read_to_string(r.join("bin/run.py")).unwrap();
    assert!(
        entry.starts_with("\"\"\"The entry file: it calls `composition`"),
        "{entry}"
    );
    let out = run(&["--root", &root_arg(r), "check"]);
    assert!(out.status.success(), "{}", stdout(&out));
    // What is moved out of the project's tree to be put back is kept beside it, not in it: in the tree it would be
    // code in no unit
    let kept = tempfile::tempdir().unwrap();

    // Whatever a unit lacks is its own: a layer taken from one unit is missing there, and in no other
    let layer = r.join("modules/billing/domain");
    let backup = kept.path().join("domain");
    fs::rename(&layer, &backup).unwrap();
    let (passed, found) = structure_findings(r);
    assert!(!passed);
    assert_eq!(
        found,
        "  domain of unit billing is missing: modules/billing/domain/ (run `rotproof create`, or name it in the \
         absent of the unit in .config/rotproof.toml)"
    );
    fs::rename(&backup, &layer).unwrap();

    // A layer the unit declares absent that is there
    fs::create_dir_all(r.join("modules/app/domain")).unwrap();
    fs::write(r.join("modules/app/domain/__init__.py"), "").unwrap();
    let (_, found) = structure_findings(r);
    assert_eq!(
        found,
        "  domain of unit app is declared absent, but modules/app/domain/ exists: remove one or the other"
    );
    fs::remove_dir_all(r.join("modules/app/domain")).unwrap();

    // The floor is each unit's: with no layer left in billing, it says so, and the other units are not asked
    fs::rename(r.join("modules/billing"), kept.path().join("billing")).unwrap();
    fs::create_dir_all(r.join("modules/billing")).unwrap();
    fs::write(r.join("modules/billing/__init__.py"), "").unwrap();
    let (_, found) = structure_findings(r);
    assert!(
        found.starts_with("  composition of unit billing is missing: modules/billing/composition/"),
        "{found}"
    );
    assert!(
        found.ends_with(
            "  no layer is present in unit billing: the python layout has modules/billing/composition/, \
             modules/billing/infrastructure/, modules/billing/application/, modules/billing/protocol/, \
             modules/billing/domain/"
        ),
        "{found}"
    );
    assert!(
        !found.contains("of unit core") && !found.contains("of unit app"),
        "{found}"
    );
    fs::remove_dir_all(r.join("modules/billing")).unwrap();

    // A unit that is not there says it all
    let (_, found) = structure_findings(r);
    assert_eq!(
        found,
        "  unit billing is missing: modules/billing/ (make it with rotproof create, or take the unit out of \
         .config/rotproof.toml)"
    );
    fs::rename(kept.path().join("billing"), r.join("modules/billing")).unwrap();

    // An entry file that is not there, outside the unit's path as it is
    fs::remove_file(r.join("bin/run.py")).unwrap();
    let (_, found) = structure_findings(r);
    assert_eq!(found, "  the entry bin/run.py of unit app is missing");
    fs::write(r.join("bin/run.py"), "").unwrap();

    // Code in no unit's path fails as code outside the layers, wherever it was moved to; the paths that are no layer
    // (a package keeps its `tests/` in its root) and what `unchecked` keeps do not
    fs::create_dir_all(r.join("scripts")).unwrap();
    fs::create_dir_all(r.join("packages/core/tests")).unwrap();
    for file in ["helper.py", "scripts/a.py", "packages/core/tests/test_a.py"] {
        fs::write(r.join(file), "").unwrap();
    }
    let (_, found) = structure_findings(r);
    let said = |what: &str| {
        format!(
            "  code outside the layers: {what} (it is in no unit: move it into a layer of one, or list it in unchecked \
             in .config/rotproof.toml)"
        )
    };
    assert_eq!(found, format!("{}\n{}", said("helper.py"), said("scripts")));
    // A file taken out of a unit's path is found the same, and its place in the unit is not the one to blame
    fs::rename(r.join("helper.py"), r.join("modules/helper.py")).unwrap();
    fs::remove_dir_all(r.join("scripts")).unwrap();
    let (_, found) = structure_findings(r);
    assert_eq!(found, said("modules"));
    fs::remove_file(r.join("modules/helper.py")).unwrap();
    let (passed, found) = structure_findings(r);
    assert!(passed, "{found}");

    // Code in a unit's path outside its layers fails there, and `unchecked` keeps it, but cannot switch the unit off
    fs::write(r.join("modules/billing/helpers.py"), "").unwrap();
    let (_, found) = structure_findings(r);
    assert_eq!(
        found,
        "  code outside the layers of unit billing: modules/billing/helpers.py (move it into a layer, or list it in \
         unchecked in .config/rotproof.toml)"
    );
    let declaration = fs::read_to_string(r.join(".config/rotproof.toml")).unwrap();
    let with_unchecked = |path: &str| {
        declaration.replace(
            "ui_design = \"none\"\n",
            &format!("ui_design = \"none\"\nunchecked = [\"{path}\"]\n"),
        )
    };
    let write = |text: &str| fs::write(r.join(".config/rotproof.toml"), text).unwrap();
    write(&with_unchecked("modules/billing/helpers.py"));
    let out = run(&["--root", &root_arg(r), "check"]);
    assert!(out.status.success(), "{}", stdout(&out));
    write(&with_unchecked("modules/billing"));
    let (passed, found) = structure_findings(r);
    assert!(!passed);
    assert_eq!(
        found.lines().next().unwrap(),
        "  unchecked lists modules/billing, which holds the unit billing (modules/billing/): a unit cannot be \
         switched off"
    );
    write(&declaration);
    fs::remove_file(r.join("modules/billing/helpers.py")).unwrap();

    // A package's manifest belongs to a unit by its root: below a root and in a layer it does, and elsewhere it is a
    // package no unit says it is
    fs::create_dir_all(r.join("packages/core/docs")).unwrap();
    fs::write(r.join("packages/core/docs/pyproject.toml"), "").unwrap();
    fs::write(r.join("modules/billing/domain/pyproject.toml"), "").unwrap();
    let out = run(&["--root", &root_arg(r), "check"]);
    assert!(out.status.success(), "{}", stdout(&out));
    fs::create_dir_all(r.join("tools")).unwrap();
    fs::write(r.join("tools/pyproject.toml"), "").unwrap();
    let (_, found) = structure_findings(r);
    assert_eq!(
        found,
        "  tools/pyproject.toml is a manifest outside every unit's root: declare the unit whose root is tools (root \
         = \"tools\"), or list tools in unchecked in .config/rotproof.toml"
    );
    write(&with_unchecked("tools"));
    let out = run(&["--root", &root_arg(r), "check"]);
    assert!(out.status.success(), "{}", stdout(&out));
}

#[test]
fn init_drafts_the_document_of_the_units_once_and_the_check_passes_with_it() {
    let root = modular_monolith();
    let r = root.path();
    let arg = root_arg(r);
    // `billing` reads the shared `core`, and the app wires `billing` through its composition
    write_file(
        r,
        "modules/billing/application/play.py",
        "from core.domain import score\n",
    );
    write_file(r, "modules/billing/composition/build.py", "");
    write_file(
        r,
        "modules/app/composition/wire.py",
        "from billing.composition import build\n",
    );
    let doc = r.join("docs/knowledge/units.md");
    assert!(!doc.exists());

    let out = run(&["--root", &arg, "init"]);
    assert!(out.status.success(), "{}", stdout(&out));
    let said = stdout(&out);
    assert!(said.contains("drafted docs/knowledge/units.md: "), "{said}");
    assert!(said.contains("wrote docs/knowledge/index.md\n"), "{said}");
    let text = fs::read_to_string(&doc).unwrap();
    assert!(text.contains("\ntags: [\"a\"]\n"), "{text}");
    assert!(
        text.contains("\nflowchart LR\n    core([\"core (shared)\"])\n    billing[\"billing\"]\n    app[\"app (app)\"]\n"),
        "{text}"
    );
    for edge in ["billing --> core", "app --> billing"] {
        assert!(text.contains(&format!("\n    {edge}\n")), "{text}");
    }
    for unit in ["core", "billing", "app"] {
        assert!(text.contains(&format!("\n## {unit}\n")), "{text}");
    }
    // It passes the check as it was written, with the readers of the shared unit listed by the same run
    let out = run(&["--root", &arg, "check"]);
    assert!(out.status.success(), "{}", stdout(&out));

    // Written once: a second run says nothing of it and leaves it as it is, even rewritten by a person
    let again = run(&["--root", &arg, "init"]);
    assert!(again.status.success(), "{}", stdout(&again));
    assert!(!stdout(&again).contains("draft"), "{}", stdout(&again));
    assert_eq!(fs::read_to_string(&doc).unwrap(), text);

    // It pins the units: one that changes fails the check, naming the command that pins the document again
    let before = fs::read_to_string(r.join(".config/rotproof.toml")).unwrap();
    declare(
        r,
        &before.replace(
            "path = \"modules/billing\"",
            "path = \"modules/billing\"\nabsent = []",
        ),
    );
    let out = run(&["--root", &arg, "check"]);
    let said = stdout(&out);
    assert_eq!(out.status.code(), Some(1), "{said}");
    assert!(
        said.contains(
            "knowledge/units.md follows .config/rotproof.toml::unit, which changed since the document was last \
             reviewed"
        ),
        "{said}"
    );
}

#[test]
fn a_composition_reads_the_protocol_and_the_domain_of_a_shared_unit_and_builds_no_insides_of_another_unit()
 {
    let root = modular_monolith();
    let r = root.path();
    let arg = root_arg(r);
    // The shared `core` is a kernel of value types: the composition of the app and that of `billing`, which is no
    // app, read its `domain`, and its `protocol`
    write_file(
        r,
        "modules/app/composition/wire.py",
        "from core.domain import money\nfrom core.protocol.inbound import port\nfrom billing.composition import build\n",
    );
    write_file(
        r,
        "modules/billing/composition/build.py",
        "from core.domain import money\nfrom core.protocol.outbound import store\n",
    );
    let out = run(&["--root", &arg, "index"]);
    assert!(out.status.success(), "{}", stdout(&out));
    let out = run(&["--root", &arg, "check"]);
    assert!(out.status.success(), "{}", stdout(&out));

    // No composition builds the insides of another unit, a shared one included: that unit builds them in its own
    // composition, which the app wires. The application of a unit still reads no other's
    write_file(
        r,
        "modules/app/composition/wire.py",
        "from core.domain import money\nfrom core.application import pay\nfrom billing.domain import invoice\n",
    );
    write_file(
        r,
        "modules/billing/composition/build.py",
        "from core.infrastructure.outbound import db\n",
    );
    write_file(
        r,
        "modules/billing/application/pay.py",
        "from core.application import pay\n",
    );
    let out = run(&["--root", &arg, "check"]);
    let said = stdout(&out);
    assert!(!out.status.success(), "{said}");
    for line in [
        "modules/app/composition/wire.py:2: imports core.application.pay, in application of unit core; the \
         composition of an app imports only the composition, protocol and domain of a shared unit: core builds its \
         own insides in its own composition",
        "modules/app/composition/wire.py:3: imports billing.domain.invoice, in domain of unit billing; the \
         composition of an app imports only the composition and protocol of a unit that is not shared: billing \
         builds its own insides in its own composition",
        "modules/billing/composition/build.py:1: imports core.infrastructure.outbound.db, in infrastructure.outbound \
         of unit core; the composition of a unit imports only the protocol and domain of a shared unit: core builds \
         its own insides in its own composition",
        "modules/billing/application/pay.py:1: imports core.application.pay, in application of unit core; \
         application never imports another unit's application: composition passes a shared unit's use cases in",
    ] {
        assert!(said.contains(line), "{line}:\n{said}");
    }
    assert_eq!(said.matches(": imports ").count(), 4, "{said}");
}

#[test]
fn init_drafts_nothing_without_an_area_and_says_why_until_there_is_one() {
    let root = declared(&two_units("true").replace("areas = [\"a\"]", "areas = []"));
    let r = root.path();
    let arg = root_arg(r);
    assert!(
        create(&["--root", &arg, "create", "--yes"])
            .status
            .success()
    );
    let out = run(&["--root", &arg, "init"]);
    assert!(out.status.success(), "{}", stdout(&out));
    let said = stdout(&out);
    assert!(
        said.contains(
            "docs/knowledge/units.md is not drafted: .config/rotproof.toml declares no areas"
        ),
        "{said}"
    );
    assert!(!r.join("docs/knowledge/units.md").exists());

    declare(
        r,
        &fs::read_to_string(r.join(".config/rotproof.toml"))
            .unwrap()
            .replace("areas = []", "areas = [\"a\"]"),
    );
    let out = run(&["--root", &arg, "init"]);
    assert!(out.status.success(), "{}", stdout(&out));
    assert!(
        stdout(&out).contains("drafted docs/knowledge/units.md: "),
        "{}",
        stdout(&out)
    );
    assert!(!stdout(&out).contains("is not drafted"), "{}", stdout(&out));
}

#[test]
fn a_project_that_declares_no_unit_is_not_asked_for_manifests_or_given_the_name_of_a_unit() {
    let root = clean_repo();
    let r = root.path();
    fs::create_dir_all(r.join("tools")).unwrap();
    fs::write(r.join("tools/pyproject.toml"), "").unwrap();
    let out = run(&["--root", &root_arg(r), "check"]);
    assert!(out.status.success(), "{}", stdout(&out));
    fs::remove_dir_all(r.join("domain")).unwrap();
    let (passed, found) = structure_findings(r);
    assert!(!passed);
    assert!(
        found.starts_with(
            "  domain is missing: domain/ (run `rotproof create`, or declare it in absent in"
        ),
        "{found}"
    );
    assert!(
        !found.contains("unit") && !found.contains("manifest"),
        "{found}"
    );
}

/// The names of the packages `cargo metadata` finds in the workspace at `root`, sorted. Panics, saying why, when Cargo
/// refuses the workspace.
fn workspace_packages(root: &Path) -> Vec<String> {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let out = Command::new(cargo)
        .args([
            "metadata",
            "--format-version",
            "1",
            "--no-deps",
            "--offline",
        ])
        .current_dir(root)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let metadata = String::from_utf8_lossy(&out.stdout).into_owned();
    // A package is the one object that has a name and then a version: a dependency or a target has none
    let mut names: Vec<String> = metadata
        .split("{\"name\":\"")
        .skip(1)
        .filter_map(|part| {
            let (name, rest) = part.split_once('"')?;
            rest.starts_with(",\"version\":").then(|| name.to_string())
        })
        .collect();
    names.sort();
    names
}

const RUST_LAYERS: [&str; 5] = [
    "application",
    "composition",
    "domain",
    "infrastructure",
    "protocol",
];

/// The declaration of a Rust project of two units: `core`, shared, whose crates sit in `crates/core`, and `cli`, an
/// app that holds `composition` and `infrastructure` only, whose crates sit in `apps/cli/crates` and whose entry file
/// is `entry`, in the package at `apps/cli` (its `root`).
fn rust_units(entry: &str) -> String {
    format!(
        "stack = \"rust\"\nareas = [\"a\"]\nui_design = \"none\"\nfiles = \"{}\"\n\n[[unit]]\nname = \"core\"\n\
         path = \"crates/core\"\nshared = true\n\n[[unit]]\nname = \"cli\"\npath = \"apps/cli/crates\"\n\
         root = \"apps/cli\"\nentry = [\"{entry}\"]\nabsent = [\"application\", \"protocol\", \"domain\"]\n",
        env!("CARGO_PKG_VERSION")
    )
}

#[test]
fn a_rust_project_that_declares_units_gets_the_crates_of_each_named_for_its_unit_and_passes_its_check()
 {
    let root = declared(&rust_units("apps/cli/src/main.rs"));
    let r = root.path();
    let arg = root_arg(r);
    let out = create(&["--root", &arg, "create", "--yes"]);
    assert!(out.status.success(), "{}", stdout(&out));
    let layers = [
        ("core", "crates/core", &RUST_LAYERS[..]),
        (
            "cli",
            "apps/cli/crates",
            &["composition", "infrastructure"][..],
        ),
    ];
    for (unit, base, layers) in layers {
        for layer in layers {
            let manifest =
                fs::read_to_string(r.join(format!("{base}/{layer}/Cargo.toml"))).unwrap();
            assert!(
                manifest.contains(&format!("name = \"{unit}_{layer}\"\n")),
                "{unit} {layer}: {manifest}"
            );
            assert!(r.join(format!("{base}/{layer}/src/lib.rs")).is_file());
        }
        // The unit's directory holds crates, and is none itself
        assert!(!r.join(format!("{base}/Cargo.toml")).exists());
    }
    // The levels are modules of their crate, in the unit's directory
    assert!(r.join("crates/core/protocol/src/inbound/mod.rs").is_file());
    // The entry file of the app, in the package that holds it, which lists no dependency until its code names one
    assert!(r.join("apps/cli/src/main.rs").is_file());
    let package = fs::read_to_string(r.join("apps/cli/Cargo.toml")).unwrap();
    assert!(package.contains("name = \"cli\"\n"), "{package}");
    assert!(!package.contains("[dependencies]"), "{package}");
    // The units replace the implicit one: no crate of the project's own, and no root package or entry file
    assert!(!r.join("crates/domain").exists() && !r.join("src/main.rs").exists());
    // The workspace names each crate, which a glob over `crates/` could not: it would match `crates/core`
    let workspace = fs::read_to_string(r.join("Cargo.toml")).unwrap();
    assert!(
        !workspace.contains("[package]") && !workspace.contains('*'),
        "{workspace}"
    );
    let mut have: Vec<String> = workspace
        .lines()
        .filter_map(|line| line.trim().strip_prefix('"')?.strip_suffix("\","))
        .map(String::from)
        .collect();
    have.sort();
    let mut want: Vec<String> = layers
        .iter()
        .flat_map(|(_, base, layers)| layers.iter().map(move |layer| format!("{base}/{layer}")))
        .chain(["apps/cli".to_string()])
        .collect();
    want.sort();
    assert_eq!(have, want, "{workspace}");
    // Cargo reads it, and finds a crate for each layer of each unit and the app's package, no two named alike
    let mut crates: Vec<String> = layers
        .iter()
        .flat_map(|(unit, _, layers)| layers.iter().map(move |layer| format!("{unit}_{layer}")))
        .chain(["cli".to_string()])
        .collect();
    crates.sort();
    assert_eq!(workspace_packages(r), crates);
    // What it made is what the check expects of the units, with nothing a person has to move first
    fs::write(
        r.join("docs/log.md"),
        "# Log\n\n## 2026-10-02\n\n* Something\n",
    )
    .unwrap();
    let out = run(&["--root", &arg, "check"]);
    assert!(out.status.success(), "{}", stdout(&out));

    // A second run has nothing to make, and writes nothing
    let before = tree(r);
    let second = create(&["--root", &arg, "create"]);
    assert!(second.status.success(), "{}", stdout(&second));
    assert!(
        stdout(&second).contains("nothing to make"),
        "{}",
        stdout(&second)
    );
    assert_eq!(tree(r), before);
}

#[test]
fn a_rust_entry_file_with_no_place_for_its_package_fails_create_before_it_writes_anything() {
    for (entry, said) in [
        (
            "apps/cli/main.rs",
            "a Rust entry file is <dir>/src/main.rs, so that its package is <dir>/Cargo.toml",
        ),
        ("src/main.rs", "at the root, where the workspace is"),
    ] {
        let root = declared(&rust_units(entry));
        let r = root.path();
        let out = create(&["--root", &root_arg(r), "create", "--yes"]);
        assert_eq!(out.status.code(), Some(2), "{entry}");
        let said_by = String::from_utf8_lossy(&out.stderr).into_owned();
        assert!(said_by.contains(said), "{entry}: {said_by}");
        // Nothing was written: only the declaration and the test's answer are there
        assert_eq!(
            tree(r)
                .iter()
                .map(|(path, _)| path.as_str())
                .collect::<Vec<_>>(),
            [".config/rotproof.toml", POINTER_FILE],
            "{entry}"
        );
    }
}

#[test]
fn a_rust_workspace_that_exists_gets_the_crates_of_a_unit_and_loses_the_glob_that_would_match_it() {
    // A project made by an older Rotproof: its workspace is the glob over `crates/`, and it declares no unit
    let older = "[package]\nname = \"older\"\nversion = \"0.1.0\"\nedition = \"2024\"\npublish = false\n\n\
                 [dependencies]\ncomposition = { path = \"crates/composition\" }\n\n\
                 [workspace]\nmembers = [\"crates/*\"]\nresolver = \"3\"\n";
    let root = declared(
        "stack = \"rust\"\nareas = [\"a\"]\nui_design = \"none\"\nabsent = [\"application\", \"infrastructure\", \"protocol\"]\n",
    );
    let r = root.path();
    let arg = root_arg(r);
    fs::write(r.join("Cargo.toml"), older).unwrap();
    fs::create_dir_all(r.join("src")).unwrap();
    fs::write(r.join("src/main.rs"), "fn main() {}\n").unwrap();
    let out = create(&["--root", &arg, "create", "--yes"]);
    assert!(out.status.success(), "{}", stdout(&out));
    // It keeps working, and untouched: the glob matches the crates it made
    assert_eq!(fs::read_to_string(r.join("Cargo.toml")).unwrap(), older);
    assert!(
        !stdout(&out).contains("changed Cargo.toml"),
        "{}",
        stdout(&out)
    );
    assert_eq!(workspace_packages(r), ["composition", "domain", "older"]);

    // The project nests its crates in a unit: the glob would match `crates/billing`, which holds no manifest
    declare(
        r,
        concat!(
            "stack = \"rust\"\nareas = [\"a\"]\nui_design = \"none\"\nfiles = \"",
            env!("CARGO_PKG_VERSION"),
            "\"\n\n[[unit]]\nname = \"billing\"\npath = \"crates/billing\"\n\
             absent = [\"application\", \"infrastructure\", \"protocol\"]\n"
        ),
    );
    let out = create(&["--root", &arg, "create", "--yes"]);
    let said = stdout(&out);
    assert!(out.status.success(), "{said}");
    assert!(
        said.contains(
            "changed Cargo.toml: replaced \"crates/*\" in members with the crates it matched, since it would match \
             crates/billing, which has no Cargo.toml"
        ),
        "{said}"
    );
    assert!(
        said.contains(
            "changed Cargo.toml: added to members: crates/billing/composition, crates/billing/domain"
        ),
        "{said}"
    );
    let workspace = fs::read_to_string(r.join("Cargo.toml")).unwrap();
    assert!(!workspace.contains("crates/*"), "{workspace}");
    // The rest of the manifest is as it was written
    assert!(
        workspace.starts_with(&older[..older.find("members").unwrap()]),
        "{workspace}"
    );
    assert!(workspace.ends_with("resolver = \"3\"\n"), "{workspace}");
    assert_eq!(
        workspace_packages(r),
        [
            "billing_composition",
            "billing_domain",
            "composition",
            "domain",
            "older"
        ]
    );
    // Said once: a second run changes nothing more
    let out = create(&["--root", &arg, "create"]);
    assert!(
        !stdout(&out).contains("changed Cargo.toml"),
        "{}",
        stdout(&out)
    );
    assert_eq!(fs::read_to_string(r.join("Cargo.toml")).unwrap(), workspace);
}

#[test]
fn a_python_or_typescript_project_that_declares_units_gets_layers_and_no_manifest() {
    for (stack, unit_path) in [
        ("python", "packages/billing/src/billing"),
        ("typescript", "packages/billing/src"),
    ] {
        let root = unlocked(&format!(
            "stack = \"{stack}\"\nareas = [\"a\"]\nui_design = \"none\"\nfiles = \"{version}\"\n\n[[unit]]\nname = \"billing\"\n\
             path = \"{unit_path}\"\n",
            version = env!("CARGO_PKG_VERSION")
        ));
        let r = root.path();
        let out = create(&["--root", &root_arg(r), "create", "--yes"]);
        assert!(out.status.success(), "{stack}: {}", stdout(&out));
        assert!(r.join(unit_path).join("domain").is_dir(), "{stack}");
        // A unit's manifest is the project's. The one at the root of a Python project is the pin of Rotproof, which
        // the workflow installs
        for manifest in [
            "package.json",
            "Cargo.toml",
            "packages/billing/pyproject.toml",
            "packages/billing/package.json",
        ] {
            assert!(!r.join(manifest).exists(), "{stack}: {manifest}");
        }
        assert_eq!(
            r.join("pyproject.toml").exists(),
            stack == "python",
            "{stack}"
        );
    }
}

/// A TypeScript monorepo as npm workspaces lay it out, with no `npm install`: the `package.json` files are written,
/// and nothing is in `node_modules`. `db` is shared and exports what it lets other units import, `billing` is a
/// package with no `exports`, and `web` is an app that holds only `composition` and `infrastructure`, its entry file
/// made by `rotproof create`.
fn npm_workspaces() -> tempfile::TempDir {
    let root = declared(&format!(
        "stack = \"typescript\"\nareas = [\"a\"]\nui_design = \"none\"\nfiles = \"{}\"\n\n[[unit]]\nname = \"db\"\n\
         path = \"packages/db/src\"\nroot = \"packages/db\"\nshared = true\n\n[[unit]]\nname = \"billing\"\n\
         path = \"packages/billing/src\"\nroot = \"packages/billing\"\n\n[[unit]]\nname = \"web\"\n\
         path = \"apps/web/src\"\nroot = \"apps/web\"\nentry = [\"apps/web/src/main.ts\"]\n\
         absent = [\"domain\", \"application\", \"protocol\"]\n\n[dependencies]\n\
         without_import = [\"@acme/db\", \"@acme/billing\", \"react\"]\n",
        env!("CARGO_PKG_VERSION")
    ));
    let r = root.path();
    let out = create(&["--root", &root_arg(r), "create", "--yes"]);
    assert!(out.status.success(), "{}", stdout(&out));
    fs::write(
        r.join("docs/log.md"),
        "# Log\n\n## 2026-10-02\n\n* Something\n",
    )
    .unwrap();
    write_file(
        r,
        "package.json",
        "{ \"name\": \"acme\", \"private\": true, \"workspaces\": [\"packages/*\", \"apps/*\"] }\n",
    );
    write_file(
        r,
        "packages/db/package.json",
        "{\n  \"name\": \"@acme/db\",\n  \"exports\": {\n    \"./protocol/*\": \"./src/protocol/*/index.ts\",\n    \
         \"./client\": { \"types\": \"./src/infrastructure/outbound/client.ts\", \"default\": \"./dist/client.js\" },\n    \
         \"./internal/*\": null\n  }\n}\n",
    );
    write_file(
        r,
        "packages/db/src/infrastructure/outbound/client.ts",
        "export const client = 1;\n",
    );
    write_file(
        r,
        "packages/billing/package.json",
        "{ \"name\": \"@acme/billing\", \"private\": true, \"dependencies\": { \"@acme/db\": \"*\" } }\n",
    );
    write_file(
        r,
        "apps/web/package.json",
        "{ \"name\": \"@acme/web\", \"private\": true, \"dependencies\": { \"@acme/billing\": \"*\", \"react\": \"*\" } }\n",
    );
    root
}

#[test]
fn an_npm_workspaces_monorepo_is_judged_through_the_names_of_its_packages() {
    let root = npm_workspaces();
    let r = root.path();
    let arg = root_arg(r);
    let out = run(&["--root", &arg, "check"]);
    assert!(out.status.success(), "{}", stdout(&out));

    // What passes: the use cases of a unit import the `protocol.inbound` of a shared one, by the name of its package
    write_file(
        r,
        "packages/billing/src/application/invoice.ts",
        "import type { Run } from '@acme/db/protocol/inbound';\nimport { f } from '../domain';\nexport const x = 1;\n",
    );
    write_file(
        r,
        "packages/db/src/protocol/inbound/index.ts",
        "export type Run = number;\n",
    );
    // What billing reads of the shared unit is listed, which the check keeps in step with the code
    let out = run(&["--root", &arg, "check"]);
    assert_eq!(out.status.code(), Some(1), "{}", stdout(&out));
    assert!(
        stdout(&out).contains("out of date, run `rotproof index`: .rotproof/readers.md"),
        "{}",
        stdout(&out)
    );
    let out = run(&["--root", &arg, "index"]);
    assert!(out.status.success(), "{}", stdout(&out));
    assert!(
        stdout(&out).contains("wrote .rotproof/readers.md"),
        "{}",
        stdout(&out)
    );
    // Another unit reading the name is said under the finding, with the shared unit it is of
    write_file(
        r,
        "apps/web/src/composition/more.ts",
        "import type { Run } from '@acme/db/protocol/inbound';\nexport type M = Run;\n",
    );
    let out = run(&["--root", &arg, "check"]);
    let said = stdout(&out);
    assert_eq!(out.status.code(), Some(1), "{said}");
    assert!(
        said.contains(
            "  out of date, run `rotproof index`: .rotproof/readers.md\n    db: \
             packages/db/src/protocol/inbound/index: Run is read by billing, web (was billing)\n"
        ),
        "{said}"
    );
    fs::remove_file(r.join("apps/web/src/composition/more.ts")).unwrap();
    let listed = fs::read_to_string(r.join(".rotproof/readers.md")).unwrap();
    assert!(
        listed.ends_with(
            "## db\n\n`packages/db/src`\n\n### Read by one unit\n\n| Name | Read by |\n|---|---|\n\
             | `packages/db/src/protocol/inbound/index: Run` | billing |\n"
        ),
        "{listed}"
    );
    let out = run(&["--root", &arg, "check"]);
    assert!(out.status.success(), "{}", stdout(&out));

    // The domain of a unit reaching the infrastructure of another, which a package of a third party it was taken for
    // let through: `@acme/db/client` is exported as src/infrastructure/outbound/client.ts
    write_file(
        r,
        "packages/billing/src/domain/song.ts",
        "import { client } from '@acme/db/client';\nexport const y = client;\n",
    );
    let out = run(&["--root", &arg, "check"]);
    let said = stdout(&out);
    assert!(!out.status.success(), "{said}");
    assert!(
        said.contains(
            "packages/billing/src/domain/song.ts:1: imports @acme/db/client \
             (packages/db/src/infrastructure/outbound/client.ts), in infrastructure.outbound of unit db; domain imports \
             no other layer"
        ),
        "{said}"
    );
    assert_eq!(said.matches(": imports ").count(), 1, "{said}");

    // A subpath the package does not export fails, and says what to do: it is no file Node or tsc finds either
    write_file(
        r,
        "packages/billing/src/domain/song.ts",
        "import { a } from '@acme/db/domain';\nimport { b } from '@acme/db/internal/x';\nexport const y = 1;\n",
    );
    let out = run(&["--root", &arg, "check"]);
    let said = stdout(&out);
    assert!(!out.status.success(), "{said}");
    for line in [
        "packages/billing/src/domain/song.ts:1: imports @acme/db/domain, which packages/db/package.json does not \
         export, so Node and tsc fail it and its layer cannot be judged: export the subpath in \
         packages/db/package.json, or import one that is exported",
        "packages/billing/src/domain/song.ts:2: imports @acme/db/internal/x, which packages/db/package.json does \
         not export, so Node and tsc fail it and its layer cannot be judged: export the subpath in \
         packages/db/package.json, or import one that is exported",
    ] {
        assert!(said.contains(line), "{line}:\n{said}");
    }

    // The composition of the app wires a unit that is not shared through that unit's composition, and a name that is no
    // unit's package is a package
    write_file(
        r,
        "packages/billing/src/domain/song.ts",
        "export const y = 1;\n",
    );
    write_file(
        r,
        "packages/billing/src/composition/index.ts",
        "export const y = 1;\n",
    );
    write_file(
        r,
        "apps/web/src/composition/index.ts",
        "import { y } from '@acme/billing/src/composition/index';\nimport React from 'react';\nexport const z = y;\n",
    );
    let out = run(&["--root", &arg, "check"]);
    assert!(out.status.success(), "{}", stdout(&out));
    // and builds none of its insides, which that unit's own composition builds
    write_file(
        r,
        "apps/web/src/composition/index.ts",
        "import { y } from '@acme/billing/src/domain/song';\nexport const z = y;\n",
    );
    let out = run(&["--root", &arg, "check"]);
    let said = stdout(&out);
    assert!(!out.status.success(), "{said}");
    assert!(
        said.contains(
            "apps/web/src/composition/index.ts:1: imports @acme/billing/src/domain/song \
             (packages/billing/src/domain/song), in domain of unit billing; the composition of an app imports only \
             the composition and protocol of a unit that is not shared: billing builds its own insides in its own \
             composition"
        ),
        "{said}"
    );
    write_file(
        r,
        "apps/web/src/composition/index.ts",
        "import { y } from '@acme/billing/src/composition/index';\nexport const z = y;\n",
    );
    // The entry file reads the composition of its own unit alone
    write_file(
        r,
        "apps/web/src/main.ts",
        "import { y } from '@acme/billing/src/composition/index';\nexport const w = y;\n",
    );
    let out = run(&["--root", &arg, "check"]);
    let said = stdout(&out);
    assert!(!out.status.success(), "{said}");
    assert!(
        said.contains("apps/web/src/main.ts:1: imports @acme/billing/src/composition/index"),
        "{said}"
    );
}

fn error_of(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

/// A records-only repository with a file of code to follow, whose index and rules are written.
fn repo_with_code() -> tempfile::TempDir {
    let root = declared("stack = \"none\"\nareas = [\"operations\"]\n");
    let arg = root_arg(root.path());
    assert!(run(&["--root", &arg, "create"]).status.success());
    write_file(
        root.path(),
        "domain/model.py",
        "def play():\n    return 1\n",
    );
    root
}

/// The text of the knowledge document `slug`.
fn started_text(root: &Path, slug: &str) -> String {
    fs::read_to_string(root.join(format!("docs/knowledge/{slug}.md"))).unwrap()
}

#[test]
fn new_starts_a_document_that_fails_the_check_until_it_is_written() {
    let root = repo_with_code();
    let arg = root_arg(root.path());
    let out = run(&["--root", &arg, "new", "rates-are-cached"]);
    assert!(out.status.success(), "{}", error_of(&out));
    let said = stdout(&out);
    assert!(
        said.starts_with("wrote docs/knowledge/rates-are-cached.md (area: operations)\n"),
        "{said}"
    );
    // The questions to ask are the command's output: where an agent reads what to do next
    for question in [
        "Why is it so, and not another way?",
        "What was considered and dropped, and why?",
        "How should it behave, where the code says only how it does?",
        "What outside the code does one need to know",
    ] {
        assert!(said.contains(question), "{question}\n{said}");
    }
    assert!(said.contains("next: write the description"), "{said}");
    let text = started_text(root.path(), "rates-are-cached");
    assert_eq!(
        text,
        "---\ntype: Knowledge\ntitle: Rates are cached\ndescription: \"\"\ntags: [operations]\nstatus: stable\n---\n"
    );
    // A document half done cannot pass: the description is empty
    let out = run(&["--root", &arg, "check"]);
    assert_eq!(out.status.code(), Some(1), "{}", stdout(&out));
    assert!(
        stdout(&out).contains("rates-are-cached.md: description: empty"),
        "{}",
        stdout(&out)
    );
    // Written, indexed, and it passes
    fs::write(
        root.path().join("docs/knowledge/rates-are-cached.md"),
        text.replace(
            "description: \"\"",
            "description: Why the rates are cached.",
        ) + "\n# Why\n\nThe provider limits calls.\n",
    )
    .unwrap();
    assert!(run(&["--root", &arg, "index"]).status.success());
    let out = run(&["--root", &arg, "check"]);
    assert!(out.status.success(), "{}", stdout(&out));
    // The slug never changes and nothing is written over: a second run is refused, and the document is as it was
    let before = tree(root.path());
    let out = run(&["--root", &arg, "new", "rates-are-cached"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(
        error_of(&out).contains("is there already"),
        "{}",
        error_of(&out)
    );
    assert_eq!(tree(root.path()), before);
}

#[test]
fn new_pins_nothing_and_follows_pins_the_document_once_its_body_is_written() {
    let root = repo_with_code();
    let arg = root_arg(root.path());
    // A pin says the document was reviewed against the code, so a document with no body has none: --follows is gone
    let before = tree(root.path());
    let out = run(&[
        "--root",
        &arg,
        "new",
        "model",
        "--follows",
        "domain/model.py::play",
    ]);
    assert_eq!(out.status.code(), Some(2), "{}", stdout(&out));
    assert!(error_of(&out).contains("--follows"), "{}", error_of(&out));
    assert_eq!(tree(root.path()), before);
    let out = run(&["--root", &arg, "new", "model"]);
    assert!(out.status.success(), "{}", error_of(&out));
    assert!(!stdout(&out).contains("follows"), "{}", stdout(&out));
    let text = started_text(root.path(), "model");
    assert!(!text.contains("follows"), "{text}");
    // The body is written, then the keys it describes are pinned
    fs::write(
        root.path().join("docs/knowledge/model.md"),
        text.replace("description: \"\"", "description: How a song plays.")
            + "\n# Why\n\nA song plays once.\n",
    )
    .unwrap();
    let out = run(&[
        "--root",
        &arg,
        "follows",
        "knowledge/model.md",
        "domain/model.py::play",
        "domain/",
    ]);
    assert!(out.status.success(), "{}", error_of(&out));
    assert!(
        started_text(root.path(), "model").contains("follows:\n  domain/model.py::play: \""),
        "{}",
        started_text(root.path(), "model")
    );
    assert!(run(&["--root", &arg, "index"]).status.success());
    let out = run(&["--root", &arg, "check"]);
    assert!(out.status.success(), "{}", stdout(&out));
    // And it is a pin: the code changes, the check says so
    write_file(
        root.path(),
        "domain/model.py",
        "def play():\n    return 2\n",
    );
    let out = run(&["--root", &arg, "check"]);
    assert_eq!(out.status.code(), Some(1), "{}", stdout(&out));
    assert!(
        stdout(&out).contains("knowledge/model.md follows domain/model.py::play, which changed"),
        "{}",
        stdout(&out)
    );
}

#[test]
fn new_writes_an_alarm_whose_two_headings_and_description_are_left_to_write() {
    let root = repo_with_code();
    let arg = root_arg(root.path());
    let out = run(&[
        "--root",
        &arg,
        "new",
        "pypi-403",
        "--alarm",
        "--match",
        "403",
        "--match",
        "trusted publisher",
    ]);
    assert!(out.status.success(), "{}", error_of(&out));
    let said = stdout(&out);
    assert!(said.contains("# When") && said.contains("# Do"), "{said}");
    assert!(!said.contains("Why is it so"), "{said}");
    let text = started_text(root.path(), "pypi-403");
    assert!(
        text.ends_with("match: [\"403\", \"trusted publisher\"]\n---\n\n# When\n\n# Do\n"),
        "{text}"
    );
    // The description first, then each heading: the check names what is missing
    fs::write(
        root.path().join("docs/knowledge/pypi-403.md"),
        text.replace("description: \"\"", "description: What to do on a 403."),
    )
    .unwrap();
    let out = run(&["--root", &arg, "check"]);
    assert_eq!(out.status.code(), Some(1), "{}", stdout(&out));
    assert!(
        stdout(&out).contains("an alarm has both # When"),
        "{}",
        stdout(&out)
    );
    fs::write(
        root.path().join("docs/knowledge/pypi-403.md"),
        text.replace("description: \"\"", "description: What to do on a 403.")
            .replace("# When\n", "# When\n\nThe release job prints 403.\n")
            .replace("# Do\n", "# Do\n\nOpen an issue.\n"),
    )
    .unwrap();
    assert!(run(&["--root", &arg, "index"]).status.success());
    let out = run(&["--root", &arg, "check"]);
    assert!(out.status.success(), "{}", stdout(&out));
    let index = fs::read_to_string(root.path().join("docs/knowledge/index.md")).unwrap();
    assert!(
        index.contains("403") && index.contains("trusted publisher"),
        "{index}"
    );
}

#[test]
fn new_refuses_what_it_cannot_write_correctly_and_writes_nothing() {
    let root = declared("stack = \"none\"\nareas = [\"ops\", \"billing\"]\n");
    let arg = root_arg(root.path());
    // Before docs/ is made
    let out = run(&["--root", &arg, "new", "a", "--area", "ops"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(
        error_of(&out).contains("docs/knowledge/ is not there"),
        "{}",
        error_of(&out)
    );
    assert!(!root.path().join("docs").exists());
    assert!(run(&["--root", &arg, "create"]).status.success());
    write_file(
        root.path(),
        "domain/model.py",
        "def play():\n    return 1\n",
    );
    let before_new = tree(root.path());
    for (args, says) in [
        (vec!["new", "a"], "--area"),
        (vec!["new", "a", "--area", "sales"], "\"sales\""),
        (vec!["new", "Rates", "--area", "ops"], "not a slug"),
        (vec!["new", "a.md", "--area", "ops"], "without .md"),
        (vec!["new", "index", "--area", "ops"], "keeps for a file"),
        (vec!["new", "rules", "--area", "ops"], "keeps for a file"),
        (
            vec!["new", "a", "--area", "ops", "--alarm", "--match", "a\nb"],
            "control character",
        ),
        (
            vec![
                "new", "a", "--area", "ops", "--alarm", "--match", "x", "--match", "x",
            ],
            "named twice",
        ),
        (
            vec!["new", "a", "--area", "ops", "--match", "403"],
            "--alarm",
        ),
    ] {
        let mut all = vec!["--root", arg.as_str()];
        all.extend(args.iter().copied());
        let out = run(&all);
        assert_eq!(out.status.code(), Some(2), "{args:?}");
        assert!(
            error_of(&out).contains(says),
            "{args:?}: {}",
            error_of(&out)
        );
        assert_eq!(tree(root.path()), before_new, "{args:?}");
    }
    // Two areas, one named: tagged with it
    let out = run(&["--root", &arg, "new", "a", "--area", "billing"]);
    assert!(out.status.success(), "{}", error_of(&out));
    assert!(started_text(root.path(), "a").contains("tags: [billing]\n"));
}

/// Today as the machine's clock gives it, which is what `rotproof log` reads: the day in the machine's own zone.
fn today() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

#[test]
fn log_adds_an_entry_under_todays_heading_that_the_check_accepts() {
    let root = repo();
    let arg = root_arg(root.path());
    assert!(run(&["--root", &arg, "create"]).status.success());
    // A log as it is when the project starts, with the days of earlier work under it
    fs::write(
        root.path().join("docs/log.md"),
        "# Log\n\n## 2026-10-04\n\n* **Earlier**\n  * **Problems**: None\n  * **Lessons**: L\n",
    )
    .unwrap();
    let day = today();
    let out = run(&[
        "--root",
        &arg,
        "log",
        "--title",
        "Start the log",
        "--issue",
        "acme/app#4",
        "--problem",
        "None",
        "--lesson",
        "Write the entry with the command",
    ]);
    assert!(out.status.success(), "{}", error_of(&out));
    assert!(
        stdout(&out).contains(&format!("wrote docs/log.md: the entry is under ## {day}")),
        "{}",
        stdout(&out)
    );
    let log = fs::read_to_string(root.path().join("docs/log.md")).unwrap();
    assert_eq!(
        log,
        format!(
            "# Log\n\n## {day}\n\n* **Start the log**\n  * **Issue**: acme/app#4\n  * **Problems**: None\n  * \
             **Lessons**: Write the entry with the command\n\n## 2026-10-04\n\n* **Earlier**\n  * **Problems**: None\n  \
             * **Lessons**: L\n"
        )
    );
    // A second entry of the day goes above the first, under the same heading, and lists are lists
    let out = run(&[
        "--root",
        &arg,
        "log",
        "--title",
        "A second task",
        "--problem",
        "The first thing\nthat went wrong",
        "--problem",
        "The second",
        "--lesson",
        "One lesson",
    ]);
    assert!(out.status.success(), "{}", error_of(&out));
    let log = fs::read_to_string(root.path().join("docs/log.md")).unwrap();
    assert_eq!(log.matches(&format!("## {day}")).count(), 1, "{log}");
    assert!(
        log.contains(&format!(
            "## {day}\n\n* **A second task**\n  * **Problems**:\n    * The first thing that went wrong\n    * The \
             second\n  * **Lessons**: One lesson\n\n* **Start the log**\n"
        )),
        "{log}"
    );
    let out = run(&["--root", &arg, "check"]);
    assert!(out.status.success(), "{}", stdout(&out));
}

#[test]
fn log_keeps_the_line_endings_and_the_byte_order_mark_of_the_log() {
    let root = repo();
    let arg = root_arg(root.path());
    assert!(run(&["--root", &arg, "create"]).status.success());
    fs::write(
        root.path().join("docs/log.md"),
        "\u{feff}# Log\r\n\r\n## 2026-10-04\r\n\r\n* **Earlier**\r\n",
    )
    .unwrap();
    let out = run(&[
        "--root",
        &arg,
        "log",
        "--title",
        "T",
        "--problem",
        "None",
        "--lesson",
        "L",
    ]);
    assert!(out.status.success(), "{}", error_of(&out));
    let raw = fs::read(root.path().join("docs/log.md")).unwrap();
    let text = String::from_utf8(raw).unwrap();
    assert!(text.starts_with('\u{feff}'), "{text:?}");
    assert!(!text.replace("\r\n", "").contains('\n'), "{text:?}");
    assert!(
        text.contains(&format!("## {}\r\n\r\n* **T**\r\n", today())),
        "{text:?}"
    );
    let out = run(&["--root", &arg, "check"]);
    assert!(out.status.success(), "{}", stdout(&out));
}

#[test]
fn log_adds_to_the_heading_of_today_when_the_log_has_one() {
    let root = repo();
    let arg = root_arg(root.path());
    assert!(run(&["--root", &arg, "create"]).status.success());
    let day = today();
    fs::write(
        root.path().join("docs/log.md"),
        format!("# Log\n\n## {day}\n\n* **Earlier today**\n"),
    )
    .unwrap();
    let out = run(&[
        "--root",
        &arg,
        "log",
        "--title",
        "T",
        "--problem",
        "None",
        "--lesson",
        "L",
    ]);
    assert!(out.status.success(), "{}", error_of(&out));
    let log = fs::read_to_string(root.path().join("docs/log.md")).unwrap();
    assert_eq!(
        log,
        format!(
            "# Log\n\n## {day}\n\n* **T**\n  * **Problems**: None\n  * **Lessons**: L\n\n* **Earlier today**\n"
        )
    );
}

#[test]
fn log_refuses_what_the_log_does_not_have_and_writes_nothing() {
    let root = repo();
    let arg = root_arg(root.path());
    // No log yet
    let out = run(&[
        "--root",
        &arg,
        "log",
        "--title",
        "T",
        "--problem",
        "None",
        "--lesson",
        "L",
    ]);
    assert_eq!(out.status.code(), Some(2));
    assert!(
        error_of(&out).contains("docs/log.md is not there"),
        "{}",
        error_of(&out)
    );
    assert!(!root.path().join("docs/log.md").exists());
    assert!(run(&["--root", &arg, "create"]).status.success());
    let before = tree(root.path());
    for (args, says) in [
        // A field forgotten is not a field with nothing in it: --problem None says the second
        (vec!["log", "--title", "T", "--lesson", "L"], "--problem"),
        (vec!["log", "--title", "T", "--problem", "None"], "--lesson"),
        (vec!["log", "--problem", "None", "--lesson", "L"], "--title"),
        (
            vec!["log", "--title", " ", "--problem", "None", "--lesson", "L"],
            "--title is empty",
        ),
        (
            vec!["log", "--title", "T", "--problem", "", "--lesson", "L"],
            "--problem is empty",
        ),
        (
            vec![
                "log",
                "--title",
                "T",
                "--problem",
                "None",
                "--lesson",
                "a <!-- b",
            ],
            "--lesson holds `<!--`",
        ),
        (
            vec![
                "log",
                "--title",
                "T",
                "--issue",
                "4",
                "--problem",
                "None",
                "--lesson",
                "L",
            ],
            "--issue",
        ),
    ] {
        let mut all = vec!["--root", arg.as_str()];
        all.extend(args.iter().copied());
        let out = run(&all);
        assert_eq!(out.status.code(), Some(2), "{args:?}");
        assert!(
            error_of(&out).contains(says),
            "{args:?}: {}",
            error_of(&out)
        );
        assert_eq!(tree(root.path()), before, "{args:?}");
    }
    // A log whose heading is hidden in a comment leaves no place that can be told
    fs::write(
        root.path().join("docs/log.md"),
        format!("# Log\n\n<!--\n## {}\n-->\n", today()),
    )
    .unwrap();
    let out = run(&[
        "--root",
        &arg,
        "log",
        "--title",
        "T",
        "--problem",
        "None",
        "--lesson",
        "L",
    ]);
    assert_eq!(out.status.code(), Some(2));
    assert!(
        error_of(&out).contains("cannot be told"),
        "{}",
        error_of(&out)
    );
}

#[test]
fn a_python_manifest_is_compared_with_what_the_code_imports_either_way() {
    let root = repo_with_ui("python", "");
    let r = root.path();
    let arg = root_arg(r);
    write_file(
        r,
        "pyproject.toml",
        "[project]\nname = \"shop\"\ndependencies = [\"requests\", \"python-dotenv\"]\n",
    );
    write_file(
        r,
        "infrastructure/outbound/pay.py",
        "import os\nimport requests\nfrom dotenv import load_dotenv\n",
    );
    // `dotenv` is taken for the distribution named like it, and `python-dotenv` is imported by nothing
    let out = run(&["--root", &arg, "check"]);
    let said = stdout(&out);
    assert_eq!(out.status.code(), Some(1), "{said}");
    assert!(
        said.contains("the manifests list the dependencies the code imports"),
        "{said}"
    );
    assert!(
        said.contains(
            "infrastructure/outbound/pay.py:3: imports dotenv.load_dotenv, which needs the package dotenv, and pyproject.toml \
             does not list it: add it to dependencies in [project]"
        ),
        "{said}"
    );
    assert!(
        said.contains(
            "pyproject.toml:3: lists the dependency python-dotenv, which no code imports"
        ),
        "{said}"
    );
    // The declaration says which distribution the import is of, and the manifest agrees with the code
    let declaration = fs::read_to_string(r.join(".config/rotproof.toml")).unwrap();
    write_file(
        r,
        ".config/rotproof.toml",
        &format!("{declaration}\n[dependencies]\nimports = {{ dotenv = \"python-dotenv\" }}\n"),
    );
    let out = run(&["--root", &arg, "check"]);
    assert!(out.status.success(), "{}", stdout(&out));
    // A development dependency serves a test, and a layer may not use it
    write_file(
        r,
        "pyproject.toml",
        "[project]\nname = \"shop\"\ndependencies = [\"requests\", \"python-dotenv\"]\n\
         [dependency-groups]\ndev = [\"pytest\"]\n",
    );
    write_file(r, "tests/test_pay.py", "import pytest\n");
    let out = run(&["--root", &arg, "check"]);
    assert!(out.status.success(), "{}", stdout(&out));
    write_file(r, "domain/rule.py", "import pytest\n");
    let said = stdout(&run(&["--root", &arg, "check"]));
    assert!(
        said.contains(
            "domain/rule.py:1: imports pytest, which pyproject.toml:5 lists for development only"
        ),
        "{said}"
    );
    // A package only tests import and the manifest lacks is to be added for development, not to runtime
    fs::remove_file(r.join("domain/rule.py")).unwrap();
    write_file(
        r,
        "pyproject.toml",
        "[project]\nname = \"shop\"\ndependencies = [\"requests\", \"python-dotenv\"]\n",
    );
    let said = stdout(&run(&["--root", &arg, "check"]));
    assert!(
        said.contains(
            "tests/test_pay.py:1: imports pytest, which needs the package pytest, and pyproject.toml does not list \
             it: add it to a group of [dependency-groups]"
        ),
        "{said}"
    );
}

#[test]
fn a_typescript_manifest_is_compared_with_what_the_code_imports_either_way() {
    let root = repo_with_ui("typescript", "");
    let r = root.path();
    let arg = root_arg(r);
    write_file(
        r,
        "package.json",
        "{\n  \"name\": \"shop\",\n  \"dependencies\": {\n    \"react\": \"^18\",\n    \"lodash\": \"^4\"\n  }\n}\n",
    );
    write_file(
        r,
        "src/infrastructure/outbound/pay.ts",
        "import React from \"react\";\nimport z from \"zod\";\nimport { join } from \"node:path\";\n",
    );
    let out = run(&["--root", &arg, "check"]);
    let said = stdout(&out);
    assert_eq!(out.status.code(), Some(1), "{said}");
    assert!(
        said.contains(
            "src/infrastructure/outbound/pay.ts:2: imports zod, which needs the package zod, and package.json does not list it: \
             add it to dependencies"
        ),
        "{said}"
    );
    assert!(
        said.contains("package.json:5: lists the dependency lodash, which no code imports"),
        "{said}"
    );
    // The manifest says what the code does: the dependency used without an import is in the declaration
    write_file(
        r,
        "package.json",
        "{\n  \"name\": \"shop\",\n  \"dependencies\": {\n    \"react\": \"^18\",\n    \"zod\": \"^3\",\n    \"lodash\": \"^4\"\n  }\n}\n",
    );
    let declaration = fs::read_to_string(r.join(".config/rotproof.toml")).unwrap();
    write_file(
        r,
        ".config/rotproof.toml",
        &format!("{declaration}\n[dependencies]\nwithout_import = [\"lodash\"]\n"),
    );
    let out = run(&["--root", &arg, "check"]);
    assert!(out.status.success(), "{}", stdout(&out));
    // A name that no manifest lists is no use of the table
    write_file(
        r,
        ".config/rotproof.toml",
        &format!("{declaration}\n[dependencies]\nwithout_import = [\"lodash\", \"gone\"]\n"),
    );
    let said = stdout(&run(&["--root", &arg, "check"]));
    assert!(
        said.contains("without_import under [dependencies] lists gone, which no manifest lists as a dependency"),
        "{said}"
    );
}

#[test]
fn a_rust_dependency_the_crate_does_not_name_is_unused_and_fails() {
    let root = repo_with_ui("rust", "");
    let r = root.path();
    let arg = root_arg(r);
    let manifest = "[package]\nname = \"domain\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[dependencies]\n\
                    serde = \"1\"\nserde_json = \"1\"\nopenssl-sys = \"0.9\"\n";
    write_file(r, "crates/domain/Cargo.toml", manifest);
    write_file(
        r,
        "crates/domain/src/lib.rs",
        "//! Doc\n\nuse serde::Serialize;\n",
    );
    let out = run(&["--root", &arg, "check"]);
    let said = stdout(&out);
    assert_eq!(out.status.code(), Some(1), "{said}");
    for dependency in ["serde_json", "openssl-sys"] {
        assert!(
            said.contains(&format!(
                "lists the dependency {dependency} in [dependencies], and no `use` or path in the crate names it"
            )),
            "{said}"
        );
    }
    assert!(!said.contains("lists the dependency serde in"), "{said}");
    // A path names a crate as a `use` does, and what is only linked is said in the declaration
    write_file(
        r,
        "crates/domain/src/lib.rs",
        "//! Doc\n\nuse serde::Serialize;\n\npub fn f() -> String {\n    serde_json::to_string(&1).unwrap()\n}\n",
    );
    let declaration = fs::read_to_string(r.join(".config/rotproof.toml")).unwrap();
    write_file(
        r,
        ".config/rotproof.toml",
        &format!("{declaration}\n[dependencies]\nwithout_import = [\"openssl_sys\"]\n"),
    );
    let out = run(&["--root", &arg, "check"]);
    assert!(out.status.success(), "{}", stdout(&out));
}

#[test]
fn a_python_project_lists_its_dependencies_in_a_manifest_and_commits_a_lock_file() {
    let root = unlocked(&format!(
        "stack = \"python\"\nareas = [\"a\"]\nui_design = \"none\"\nfiles = \"{}\"\n",
        env!("CARGO_PKG_VERSION")
    ));
    let r = root.path();
    let arg = root_arg(r);
    let out = create(&["--root", &arg, "create", "--yes"]);
    assert!(out.status.success(), "{}", stdout(&out));
    fs::write(
        r.join("docs/log.md"),
        "# Log\n\n## 2026-10-02\n\n* Something\n",
    )
    .unwrap();
    let heading = "a Python project lists its dependencies in pyproject.toml, commits a lock file, and keeps its virtual \
                   environment in .venv beside it:\n";
    // `create` wrote the manifest, with Rotproof pinned in its dev group, and a tool makes the lock
    let out = run(&["--root", &arg, "check"]);
    let said = stdout(&out);
    assert_eq!(out.status.code(), Some(1), "{said}");
    assert!(
        said.contains(&format!("{heading}  pyproject.toml has no lock file in the root or above it: commit one (uv.lock")),
        "{said}"
    );
    // A requirements file is no declaration, with a lock file or without one
    fs::remove_file(r.join("pyproject.toml")).unwrap();
    fs::write(r.join("requirements.txt"), "requests\n").unwrap();
    let said = stdout(&run(&["--root", &arg, "check"]));
    assert!(
        said.contains("  pyproject.toml is missing: a Python project lists its dependencies in pyproject.toml (a requirements file does not count)"),
        "{said}"
    );
    // Each tool's lock file is accepted
    fs::write(r.join("pyproject.toml"), "[project]\nname = \"x\"\n").unwrap();
    for lock in [
        "uv.lock",
        "poetry.lock",
        "pdm.lock",
        "pylock.toml",
        "pylock.dev.toml",
    ] {
        fs::write(r.join(lock), "").unwrap();
        let out = run(&["--root", &arg, "check"]);
        assert!(out.status.success(), "{lock}: {}", stdout(&out));
        fs::remove_file(r.join(lock)).unwrap();
    }
    // One the project's .gitignore excludes is not committed
    fs::write(r.join("uv.lock"), "").unwrap();
    let ignore = fs::read_to_string(r.join(".gitignore")).unwrap();
    fs::write(r.join(".gitignore"), format!("{ignore}uv.lock\n")).unwrap();
    let said = stdout(&run(&["--root", &arg, "check"]));
    assert!(said.contains("pyproject.toml has no lock file"), "{said}");
}

#[test]
fn a_virtual_environment_is_the_dot_venv_beside_a_lock_file_that_the_gitignore_excludes() {
    let root = clean_repo();
    let r = root.path();
    let arg = root_arg(r);
    let ignore = fs::read_to_string(r.join(".gitignore")).unwrap();
    assert!(ignore.lines().any(|line| line == ".venv/"), "{ignore}");
    let check = || {
        let out = run(&["--root", &arg, "check"]);
        (out.status.code(), stdout(&out))
    };
    // The .venv `create` ignores, hidden and with all it holds, is no business of the other checks
    write_file(r, ".venv/pyvenv.cfg", "home = /usr/bin\n");
    write_file(
        r,
        ".venv/Lib/site-packages/dotenv/__init__.py",
        "import nothing\n",
    );
    let (code, said) = check();
    assert_eq!(code, Some(0), "{said}");
    // The .gitignore does not exclude it: it can be committed
    fs::write(r.join(".gitignore"), ignore.replace(".venv/\n", "")).unwrap();
    let (code, said) = check();
    assert_eq!(code, Some(1), "{said}");
    assert!(
        said.contains(
            "  .venv/ is a virtual environment that no .gitignore excludes, so it can be committed"
        ),
        "{said}"
    );
    // The `.gitignore` uv writes into its own .venv excludes it too, as it does for git
    write_file(r, ".venv/.gitignore", "*\n");
    let (code, said) = check();
    assert_eq!(code, Some(0), "{said}");
    fs::remove_file(r.join(".venv/.gitignore")).unwrap();
    fs::write(r.join(".gitignore"), &ignore).unwrap();

    // Another name is found by its `pyvenv.cfg`, though the .gitignore hides it; a directory of any other kind is not
    write_file(r, "venv/pyvenv.cfg", "");
    write_file(r, "env/notes.txt", "");
    write_file(r, "py312/pyvenv.cfg", "");
    fs::write(r.join(".gitignore"), format!("{ignore}venv/\npy312/\n")).unwrap();
    let (code, said) = check();
    assert_eq!(code, Some(1), "{said}");
    assert!(
        said.contains("  venv/ is a virtual environment (it holds pyvenv.cfg), and the virtual environment of a Rotproof project is named .venv alone"),
        "{said}"
    );
    assert!(said.contains("  py312/ is a virtual environment"), "{said}");
    assert!(
        !said.contains("\n  env/ is a virtual environment"),
        "{said}"
    );
    fs::remove_dir_all(r.join("venv")).unwrap();
    fs::remove_dir_all(r.join("py312")).unwrap();
    fs::remove_dir_all(r.join("env")).unwrap();
    fs::write(r.join(".gitignore"), &ignore).unwrap();

    // Below an ignored or hidden directory nothing is looked at, as for every check (a tox environment, a package
    // that brings its own)
    write_file(r, ".tox/py312/pyvenv.cfg", "");
    write_file(r, "node_modules/pkg/venv/pyvenv.cfg", "");
    fs::write(
        r.join(".gitignore"),
        format!("{ignore}.tox/\nnode_modules/\n"),
    )
    .unwrap();
    let (code, said) = check();
    assert_eq!(code, Some(0), "{said}");

    // A directory of the project, however deep, is looked at: the .venv of a package is a second environment beside
    // the lock file of the root, and the right one beside a lock file of its own
    write_file(r, "tools/notes.txt", "");
    write_file(r, "tools/.venv/pyvenv.cfg", "");
    fs::write(
        r.join(".gitignore"),
        format!("{ignore}.tox/\nnode_modules/\n"),
    )
    .unwrap();
    let (code, said) = check();
    assert_eq!(code, Some(1), "{said}");
    assert!(
        said.contains("  tools/.venv/ is a virtual environment in a directory with no lock file"),
        "{said}"
    );
    write_file(r, "tools/uv.lock", "");
    let (code, said) = check();
    assert_eq!(code, Some(0), "{said}");
}

#[test]
fn a_project_that_is_not_python_is_not_asked_for_a_lock_file_or_judged_by_its_environments() {
    let root = unlocked(&format!(
        "stack = \"rust\"\nareas = [\"a\"]\nui_design = \"none\"\nfiles = \"{}\"\n",
        env!("CARGO_PKG_VERSION")
    ));
    let r = root.path();
    let arg = root_arg(r);
    let out = create(&["--root", &arg, "create", "--yes"]);
    assert!(out.status.success(), "{}", stdout(&out));
    write_file(r, "venv/pyvenv.cfg", "");
    let out = run(&["--root", &arg, "check"]);
    assert!(
        !stdout(&out).contains("virtual environment"),
        "{}",
        stdout(&out)
    );
}

#[test]
fn a_python_project_with_screens_checks_its_templates_and_says_so_only_when_it_has_one() {
    let root = declared(
        "stack = \"python\"\nareas = [\"a\"]\nui_design = \"atomic\"\nabsent = [\"presentation\"]\n",
    );
    let arg = root_arg(root.path());
    let out = create(&["--root", &arg, "create", "--yes"]);
    assert!(out.status.success(), "{}", stdout(&out));
    fs::write(
        root.path().join("docs/log.md"),
        "# Log\n\n## 2026-10-02\n\n* Something\n",
    )
    .unwrap();
    assert!(root.path().join("ui/pages/__init__.py").is_file());
    assert!(!root.path().join("presentation").exists());
    // Nothing is said while the views are code alone
    let said = "templates: references are read as the template languages Jinja2 3 and Django 5/6\n";
    let out = run(&["--root", &arg, "check"]);
    assert!(out.status.success(), "{}", stdout(&out));
    assert!(!stdout(&out).contains("templates:"), "{}", stdout(&out));
    // Once a view is a template, it is said on every run
    let write = |path: &str, text: &str| fs::write(root.path().join(path), text).unwrap();
    write("ui/atoms/link.html", "");
    let out = run(&["--root", &arg, "check"]);
    assert!(out.status.success(), "{}", stdout(&out));
    assert!(stdout(&out).starts_with(said), "{}", stdout(&out));

    // A template in a level may refer to the levels below its own, and one that does not is a finding, as is a
    // template outside the levels
    write(
        "ui/pages/home.html",
        "{% extends \"templates/base.html\" %}\n",
    );
    write(
        "ui/templates/base.html",
        "{% include \"organisms/nav.html\" %}\n",
    );
    write(
        "ui/organisms/nav.html",
        "{% include \"atoms/link.html\" %}\n{% include \"templates/base.html\" %}\n",
    );
    let out = run(&["--root", &arg, "check"]);
    assert_eq!(out.status.code(), Some(1), "{}", stdout(&out));
    assert!(
        stdout(&out).contains(
            "ui/organisms/nav.html:2: includes templates/base.html, in ui.templates; ui.organisms may import"
        ),
        "{}",
        stdout(&out)
    );
    assert!(!stdout(&out).contains("home.html"), "{}", stdout(&out));
    write("stray.html", "");
    let out = run(&["--root", &arg, "check"]);
    assert!(
        stdout(&out).contains("code outside the layers: stray.html"),
        "{}",
        stdout(&out)
    );

    // A manifest that asks for another version of every language: nothing is judged, and the check says why
    write(
        "pyproject.toml",
        "[project]\nname = \"x\"\ndependencies = [\"jinja2>=4\", \"django>=7\"]\n",
    );
    let out = run(&["--root", &arg, "check"]);
    assert!(
        stdout(&out).contains(
            "templates: references are not judged: pyproject.toml asks for jinja2>=4, which is not Jinja2 3; \
             pyproject.toml asks for django>=7, which is not Django 5/6\n"
        ),
        "{}",
        stdout(&out)
    );
    assert!(!stdout(&out).contains("nav.html"), "{}", stdout(&out));
}

#[test]
fn a_python_project_with_no_screens_reads_no_template() {
    let root = declared("stack = \"python\"\nareas = [\"a\"]\nui_design = \"none\"\n");
    let arg = root_arg(root.path());
    let out = create(&["--root", &arg, "create", "--yes"]);
    assert!(out.status.success(), "{}", stdout(&out));
    fs::write(
        root.path().join("docs/log.md"),
        "# Log\n\n## 2026-10-02\n\n* Something\n",
    )
    .unwrap();
    fs::write(
        root.path().join("stray.html"),
        "{% include \"pages/x.html\" %}",
    )
    .unwrap();
    let out = run(&["--root", &arg, "check"]);
    assert!(out.status.success(), "{}", stdout(&out));
    assert!(!stdout(&out).contains("templates:"), "{}", stdout(&out));
}

/// What a finding of a package an inner layer may not import starts with: the way out, up to the place.
const PACKAGE_FIX: &str = "move the code that needs the package to infrastructure, behind a port the use case is \
                           given, or, when the package does no I/O, ask the person whether to allow it for";

#[test]
fn a_package_is_imported_in_an_inner_layer_only_where_it_is_allowed_and_each_form_planted_fails() {
    let root = repo_with_ui("python", "");
    let r = root.path();
    put(
        r,
        "pyproject.toml",
        "[project]\nname = \"shop\"\ndependencies = [\"requests\", \"pydantic\"]\n",
    );
    // What is not an inner layer is free: the layer that does the I/O, what wires and draws, the entry file and a test
    for path in [
        "infrastructure/outbound/o.py",
        "infrastructure/inbound/i.py",
        "composition/c.py",
        "ui/pages/p.py",
        "__main__.py",
        "tests/test_o.py",
    ] {
        put(r, path, "import requests\nimport pydantic\nimport os\n");
    }
    let (code, said) = checked(r);
    assert_eq!(code, Some(0), "{said}");
    // Every inner layer and level fails on a package that no list allows it
    for path in [
        "domain/x.py",
        "protocol/inbound/x.py",
        "protocol/outbound/x.py",
        "application/x.py",
        "presentation/models/x.py",
        "presentation/display/x.py",
    ] {
        put(r, path, "import os\nimport requests\n");
        let (code, said) = checked(r);
        assert_eq!(code, Some(1), "{path}\n{said}");
        assert!(said.contains(IO_HEADING), "{path}\n{said}");
        let place = path.rsplit_once('/').unwrap().0.replace('/', ".");
        assert!(
            said.contains(&format!("  {path}:2: {PACKAGE_FIX} {place} (allowed under [dependencies] in .config/rotproof.toml; an agent never adds it to pass the check): imports requests, of the package requests, which {place} may not import\n    import requests")),
            "{path}\n{said}"
        );
        fs::remove_file(r.join(path)).unwrap();
    }

    // A list allows it, by layer or by level, and the declaration is the person's: it is written, never made
    let declaration = fs::read_to_string(r.join(".config/rotproof.toml")).unwrap();
    assert!(!declaration.contains("allowed"), "{declaration}");
    let declare = |text: &str| {
        put(
            r,
            ".config/rotproof.toml",
            &format!("{declaration}\n{text}\n"),
        )
    };
    put(r, "domain/x.py", "import requests\n");
    put(r, "presentation/models/x.py", "import pydantic\n");
    put(r, "presentation/display/x.py", "import os\n");
    declare("[dependencies]\nallowed = { domain = [\"Requests\"], presentation = [\"pydantic\"] }");
    let (code, said) = checked(r);
    assert_eq!(code, Some(0), "{said}");
    // A level's key adds to that level alone
    put(r, "presentation/display/x.py", "import pydantic\n");
    declare(
        "[dependencies]\nallowed = { domain = [\"requests\"], \"presentation.models\" = [\"pydantic\"] }",
    );
    let (code, said) = checked(r);
    assert_eq!(code, Some(1), "{said}");
    assert!(
        said.contains("presentation/display/x.py:1: ")
            && !said.contains("presentation/models/x.py:1: "),
        "{said}"
    );
    put(r, "presentation/display/x.py", "import os\n");
    // The table form of the same list, with its keys in quotes
    declare(
        "[dependencies.allowed]\ndomain = [\"requests\"]\n\"presentation.models\" = [\"pydantic\"]",
    );
    let (code, said) = checked(r);
    assert_eq!(code, Some(0), "{said}");

    // Each form of a list that cannot be kept fails, said with what to do, and the declaration put back passes
    for (table, said_so) in [
        (
            "allowed = { domain = [\"requests\"], domane = [\"x\"] }",
            "allowed under [dependencies] has the key \"domane\", which is no layer or level of the table: write one of presentation, presentation.models",
        ),
        (
            "allowed = { domain = [\"requests\"], infrastructure = [\"x\"] }",
            "has the key \"infrastructure\", a layer that is free to import any package",
        ),
        (
            "allowed = { domain = [\"requests\"], composition = [\"x\"] }",
            "has the key \"composition\", a layer that is free to import any package",
        ),
        (
            "allowed = { domain = [\"requests\"], ui = [\"x\"] }",
            "has the key \"ui\", a layer that is free to import any package",
        ),
        (
            "allowed = { domain = [\"requests\"], presentation.models = [\"pydantic\"] }",
            "has the table presentation.models: TOML reads presentation.models = [...] written without quotes as the key models inside the table presentation, not as the key presentation.models. Write the key in quotes: \"presentation.models\" = [...]",
        ),
        (
            "allowed = { domain = \"requests\" }",
            "has domain = \"requests\", which is no list of names",
        ),
        (
            "allowed = { domain = [\"requests\", \"Requests\"] }",
            "lists requests twice for domain (as \"requests\" and \"Requests\"): allow it once",
        ),
        (
            "allowed = { domain = [\"requests\"], \"presentation.models\" = [\"pydantic\"], presentation = [\"pydantic\"] }",
            "pydantic is allowed twice for the same code, by allowed under [dependencies] (presentation) and by allowed under [dependencies] (presentation.models)",
        ),
        (
            "allowed = { domain = [\"requests\", \"pydantic\"], \"presentation.models\" = [\"pydantic\"] }",
            "lists pydantic for domain, which no file of domain imports: take it out, so that the list does not keep what is gone",
        ),
        (
            "allowed = { domain = [\"requests\", \"ghost\"], \"presentation.models\" = [\"pydantic\"] }",
            "lists ghost for domain, which no manifest of the project lists as a dependency: take it out",
        ),
    ] {
        declare(&format!("[dependencies]\n{table}"));
        let (code, said) = checked(r);
        assert_eq!(code, Some(1), "{table}\n{said}");
        assert!(said.contains(said_so), "{table}\n{said}");
    }
    // A layer the project lacks: left out by the UI, or declared absent
    let root = clean_repo();
    let r = root.path();
    put(r, "domain/x.py", "import os\n");
    let declaration = fs::read_to_string(r.join(".config/rotproof.toml")).unwrap();
    put(
        r,
        ".config/rotproof.toml",
        &format!("{declaration}\n[dependencies]\nallowed = {{ presentation = [\"pydantic\"] }}\n"),
    );
    let (code, said) = checked(r);
    assert_eq!(code, Some(1), "{said}");
    assert!(
        said.contains("has the key \"presentation\", and no unit of the project has it (declared absent, or left out by the ui): take it out"),
        "{said}"
    );
}

#[test]
fn a_unit_adds_its_own_packages_to_the_projects_and_a_key_for_a_layer_it_lacks_fails() {
    let unit = |name: &str, extra: &str| {
        format!("\n[[unit]]\nname = \"{name}\"\npath = \"{name}\"\n{extra}")
    };
    let declaration = format!(
        "stack = \"python\"\nareas = [\"a\"]\nui_design = \"none\"\nfiles = \"{}\"\n\n[dependencies]\nallowed = {{ domain = [\"requests\"] }}\n{}{}",
        env!("CARGO_PKG_VERSION"),
        unit("a", "allowed = { domain = [\"pydantic\"] }\n"),
        unit("b", ""),
    );
    let root = declared(&declaration);
    let r = root.path();
    let arg = root_arg(r);
    let out = create(&["--root", &arg, "create", "--yes"]);
    assert!(
        out.status.success(),
        "{}{}",
        stdout(&out),
        String::from_utf8_lossy(&out.stderr)
    );
    put(r, "docs/log.md", "# Log\n\n## 2026-10-02\n\n* Something\n");
    for dir in ["a", "b"] {
        put(
            r,
            &format!("{dir}/pyproject.toml"),
            "[project]\nname = \"x\"\ndependencies = [\"requests\", \"pydantic\"]\n",
        );
    }
    put(r, "a/domain/x.py", "import requests\nimport pydantic\n");
    put(r, "b/domain/x.py", "import requests\nimport pydantic\n");
    let (code, said) = checked(r);
    // The project's list is for both units, a unit's own for its code alone
    assert_eq!(code, Some(1), "{said}");
    assert!(
        said.contains("  b/domain/x.py:2: ") && !said.contains("a/domain/x.py:"),
        "{said}"
    );
    assert!(
        said.contains(
            "imports pydantic, of the package pydantic, which domain of unit b may not import"
        ),
        "{said}"
    );
    // The unit that lacks a layer has no list for it, and says so, with the unit named
    let lacking = declaration.replace(
        "name = \"b\"\npath = \"b\"\n",
        "name = \"b\"\npath = \"b\"\nabsent = [\"application\"]\nallowed = { application = [\"requests\"] }\n",
    );
    put(r, ".config/rotproof.toml", &up_to_date(&lacking));
    let (code, said) = checked(r);
    assert_eq!(code, Some(1), "{said}");
    assert!(
        said.contains(
            "allowed of unit \"b\" has the key \"application\", and the unit \"b\" lacks it"
        ),
        "{said}"
    );
}

#[test]
fn a_rust_project_has_no_list_and_its_crate_lists_what_its_layer_may_use() {
    let root = repo_with_ui("rust", "");
    let r = root.path();
    put(
        r,
        "crates/domain/Cargo.toml",
        "[package]\nname = \"domain\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[dependencies]\nserde = \"1\"\n",
    );
    put(
        r,
        "crates/domain/src/lib.rs",
        "//! Doc\n\nuse serde::Serialize;\n",
    );
    // A dependency of an inner layer's crate is that crate's own list, and passes
    let (code, said) = checked(r);
    assert_eq!(code, Some(0), "{said}");
    // An `allowed` there fails, with why, in the project's table and in a unit's
    let declaration = fs::read_to_string(r.join(".config/rotproof.toml")).unwrap();
    for table in [
        "[dependencies]\nallowed = { domain = [\"serde\"] }\n",
        "[[unit]]\nname = \"core\"\npath = \"crates\"\nallowed = { domain = [\"serde\"] }\n",
    ] {
        put(
            r,
            ".config/rotproof.toml",
            &format!("{declaration}\n{table}"),
        );
        let (code, said) = checked(r);
        assert_eq!(code, Some(1), "{table}\n{said}");
        assert!(
            said.contains("is set, and a Rust project has no such list: a crate's own Cargo.toml [dependencies] is the list of packages its layer may import")
                && said.contains("is a decision of the person: take allowed out"),
            "{table}\n{said}"
        );
    }
    // A repository of records has no code to list
    let root = repo();
    let declaration = fs::read_to_string(root.path().join(".config/rotproof.toml")).unwrap();
    put(
        root.path(),
        ".config/rotproof.toml",
        &format!("{declaration}\n[dependencies]\nallowed = {{ domain = [\"x\"] }}\n"),
    );
    let (code, said) = checked(root.path());
    assert_eq!(code, Some(1), "{said}");
    assert!(
        said.contains("stack = \"none\" has no code, so it has no dependencies to judge: take out [dependencies]"),
        "{said}"
    );
}

#[test]
fn a_typescript_layout_allows_react_in_the_models_and_nothing_else_and_the_list_is_the_persons() {
    let root = repo_with_ui("typescript", "");
    let r = root.path();
    put(
        r,
        "package.json",
        "{\n  \"name\": \"shop\",\n  \"dependencies\": { \"react\": \"^18\", \"react-dom\": \"^18\", \"zustand\": \"^5\" }\n}\n",
    );
    put(
        r,
        "src/main.tsx",
        "import { createRoot } from 'react-dom/client';\nimport create from 'zustand';\n",
    );
    put(
        r,
        "src/presentation/models/m.ts",
        "import { useState } from 'react';\nimport { useRef } from 'react/jsx-runtime';\n",
    );
    let (code, said) = checked(r);
    assert_eq!(code, Some(0), "{said}");
    // `react` is for the models alone, and `react-dom` for no inner layer: the layout lists the one, no more
    for (path, source, package, place) in [
        (
            "src/domain/x.ts",
            "import { useState } from 'react';\n",
            "react",
            "domain",
        ),
        (
            "src/presentation/display/x.ts",
            "import { useState } from 'react';\n",
            "react",
            "presentation.display",
        ),
        (
            "src/presentation/models/x.ts",
            "import { createRoot } from 'react-dom/client';\n",
            "react-dom",
            "presentation.models",
        ),
        (
            "src/application/x.ts",
            "import create from 'zustand';\n",
            "zustand",
            "application",
        ),
    ] {
        put(r, path, source);
        let (code, said) = checked(r);
        assert_eq!(code, Some(1), "{path}\n{said}");
        assert!(
            said.contains(&format!("  {path}:1: {PACKAGE_FIX} {place} (allowed under [dependencies] in .config/rotproof.toml; an agent never adds it to pass the check): imports")),
            "{path}\n{said}"
        );
        assert!(
            said.contains(&format!(
                "of the package {package}, which {place} may not import"
            )),
            "{path}\n{said}"
        );
        fs::remove_file(r.join(path)).unwrap();
    }
    // A test is free, and a list written by the person allows the rest: the name as `package.json` writes it
    put(r, "src/domain/x.test.ts", "import create from 'zustand';\n");
    put(r, "src/application/x.ts", "import create from 'zustand';\n");
    let declaration = fs::read_to_string(r.join(".config/rotproof.toml")).unwrap();
    put(
        r,
        ".config/rotproof.toml",
        &format!("{declaration}\n[dependencies]\nallowed = {{ application = [\"zustand\"] }}\n"),
    );
    let (code, said) = checked(r);
    assert_eq!(code, Some(0), "{said}");
    // What the layout allows is not written again, and a name npm writes in another case is another name
    for (table, said_so) in [
        (
            "allowed = { application = [\"zustand\"], \"presentation.models\" = [\"react\"] }",
            "lists react for presentation.models, which the typescript layout allows for presentation.models already: take it out",
        ),
        (
            "allowed = { application = [\"Zustand\"] }",
            "lists Zustand for application, which no manifest of the project lists as a dependency",
        ),
    ] {
        put(
            r,
            ".config/rotproof.toml",
            &format!("{declaration}\n[dependencies]\n{table}\n"),
        );
        let (code, said) = checked(r);
        assert_eq!(code, Some(1), "{table}\n{said}");
        assert!(said.contains(said_so), "{table}\n{said}");
    }
}

#[test]
fn an_approved_package_prints_the_imports_the_file_makes_of_it_and_a_stale_approval_fails() {
    let root = repo_with_ui("python", "");
    let r = root.path();
    put(
        r,
        "pyproject.toml",
        "[project]\nname = \"shop\"\ndependencies = [\"requests\"]\n",
    );
    put(
        r,
        "domain/x.py",
        "from requests import get, post\nimport requests\n\n\ndef f():\n    return requests.get\n",
    );
    let (code, said) = checked(r);
    assert_eq!(code, Some(1), "{said}");
    // The package is one finding of the file, with both lines the file imports it on
    assert!(
        said.contains(
            "which domain may not import, on lines 1, 2\n    from requests import get, post\n"
        ) && said.matches("domain/x.py:").count() == 1,
        "{said}"
    );
    let named = Some("[user]\n\tname = Someone Else\n");
    let out = approve_with_git(
        r,
        named,
        &[
            "domain/x.py",
            "requests",
            "--reason",
            "the report is fetched here until the port is made",
        ],
    );
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}{}",
        stdout(&out),
        String::from_utf8_lossy(&out.stderr)
    );
    // One approval for the file and the package covers every import of it
    let (code, said) = checked(r);
    assert_eq!(code, Some(0), "{said}");
    assert!(
        said.contains(
            "approved in .config/rotproof-approved.toml (1):\n  requests in domain/x.py: the report is fetched here until the port is made (Someone Else, "
        ) && said.contains("; now used: requests, requests.get, requests.post\n"),
        "{said}"
    );
    // Another package of the file is not covered
    put(r, "domain/x.py", "import requests\nimport pydantic\n");
    put(
        r,
        "pyproject.toml",
        "[project]\nname = \"shop\"\ndependencies = [\"requests\", \"pydantic\"]\n",
    );
    let (code, said) = checked(r);
    assert_eq!(code, Some(1), "{said}");
    assert!(
        said.contains("domain/x.py:2: ")
            && said.contains("of the package pydantic")
            && !said.contains("domain/x.py:1: "),
        "{said}"
    );
    // The import goes, and the approval that matches nothing fails
    put(r, "domain/x.py", "import os\n");
    put(
        r,
        "pyproject.toml",
        "[project]\nname = \"shop\"\ndependencies = []\n",
    );
    let (code, said) = checked(r);
    assert_eq!(code, Some(1), "{said}");
    assert!(
        said.contains("approved, but no such forbidden import: requests in domain/x.py"),
        "{said}"
    );
}

#[test]
fn the_guide_says_what_is_allowed_in_an_inner_layer_and_that_the_list_is_the_persons() {
    for stack in ["python", "typescript"] {
        // The guide is wrapped: its words are read as one line
        let guide = stdout(&run(&["guide", "--stack", stack]))
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        for said in [
            "**The inner layers import only the packages they are allowed.**",
            "`allowed = { domain = [\"pydantic\"], protocol = [\"pydantic\"], \"presentation.models\" = [\"zustand\"] }`",
            "**Write a dotted key in quotes:**",
            "**Allowing a package is a decision of the person.** Never add, change or take out a name of `allowed` to pass the check, as with `todo_link` and `approved_max`",
            "`rotproof init` and `rotproof create` never write it",
        ] {
            assert!(guide.contains(said), "{stack}: {said}");
        }
        assert_eq!(
            guide.contains("The layout allows `react` in `presentation.models`"),
            stack == "typescript"
        );
    }
    let guide = stdout(&run(&["guide", "--stack", "rust"]))
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    assert!(
        guide.contains("**A crate's own `Cargo.toml` is the list of the packages its layer may use, and adding one is a decision of the person.**"),
        "{guide}"
    );
    // Neither command writes it into a declaration
    for stack in ["python", "typescript", "rust"] {
        let ui = if stack == "rust" { "none" } else { "atomic" };
        let root = tempfile::tempdir().unwrap();
        let out = run(&[
            "--root",
            &root_arg(root.path()),
            "init",
            "--stack",
            stack,
            "--ui-design",
            ui,
        ]);
        assert!(out.status.success(), "{}", stdout(&out));
        let text = fs::read_to_string(root.path().join(".config/rotproof.toml")).unwrap();
        assert!(!text.contains("allowed"), "{stack}: {text}");
    }
}
