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

/// The line of the declaration that gives `stack` its fullest UI: every layer the stack can have. Empty for a
/// repository of records only, which has no UI.
fn ui_line(stack: &str) -> &'static str {
    match stack {
        "python" | "typescript" => "ui = \"atomic\"\n",
        "none" => "",
        _ => "ui = \"none\"\n",
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
    let root = declared("stack = \"python\"\nareas = [\"a\"]\nui = \"none\"\n");
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
            "stack = \"python\"\nareas = [\"a\", \"A\"]\nui = \"none\"\n".into(),
        ),
        // Without the areas, no document can be judged: the check says so instead of passing them
        (
            "the bundle is seen",
            ".config/rotproof.toml",
            "stack = \"python\"\nui = \"none\"\n".into(),
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
fn before_says_it_does_nothing_since_1_0_whatever_it_is_given() {
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
            said.contains("rotproof before does nothing since Rotproof 1.0")
                && said.contains("rotproof init"),
            "{args:?}: {said}"
        );
    }
    // Hidden from the help
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
const PROJECT_FILES: [&str; 4] = [
    "AGENTS.md",
    "CLAUDE.md",
    "README.md",
    "requirements-dev.txt",
];

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
            agents.contains("composition/` | The composition root"),
            stack != "none",
            "{stack}: {agents}"
        );
        assert!(!agents.contains("application/` |"), "{stack}: {agents}");
        let requirements = root.join("requirements-dev.txt");
        let workflow = root.join(".github/workflows/ci.yml");
        let readme = fs::read_to_string(root.join("README.md")).unwrap();
        if channel == "pypi" {
            let pin = format!("rotproof=={}\n", env!("CARGO_PKG_VERSION"));
            assert!(fs::read_to_string(&requirements).unwrap().contains(&pin));
            let workflow = fs::read_to_string(&workflow).unwrap();
            assert!(
                workflow.contains("timeout-minutes:") && workflow.contains("- run: rotproof check")
            );
            assert!(!stdout(&out).contains("not written"), "{}", stdout(&out));
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
        "stack = \"python\"\nareas = [\"a\"]\nui = \"atomic\"\nabsent = [\"ui.templates\", \"infrastructure\"]\n",
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
    let root = declared("stack = \"python\"\nareas = [\"a\"]\nui = \"none\"\n");
    let arg = root_arg(root.path());
    assert!(
        create(&["--root", &arg, "create", "--yes"])
            .status
            .success()
    );
    assert!(create(&["--root", &arg, "create"]).status.success());
    declare(
        root.path(),
        "stack = \"python\"\nareas = [\"a\"]\nui = \"atomic\"\n",
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
        "stack = \"python\"\nareas = [\"a\"]\nui = \"atomic\"\nabsent = [\"ui.templates\"]\n",
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
    assert!(stdout(&out).contains("ui = \"atomic\""), "{}", stdout(&out));
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
                "missing field `ui`: run `rotproof init` after an upgrade of Rotproof, `rotproof create` otherwise; \
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
        now.contains("ui = ")
            && now.contains(&format!("files = \"{}\"", env!("CARGO_PKG_VERSION"))),
        "{now}"
    );
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
            Some("stack = \"rust\"\nareas = [\"a\"]\nui = \"none\"\nabsent = [\"ui\"]\n"),
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
        // With ui = "none", ui/ is no layer
        (
            "code outside the layers: ui",
            Box::new(|r| plant_file(r, "ui/__init__.py")),
        ),
        (
            "absent names \"ui\", which ui = \"none\" leaves out already",
            Box::new(|r| {
                declare(
                    r,
                    "stack = \"python\"\nareas = [\"a\"]\nui = \"none\"\nabsent = [\"ui\"]\n",
                )
            }),
        ),
        (
            "domain is declared absent, but domain/ exists",
            Box::new(|r| {
                declare(
                    r,
                    "stack = \"python\"\nareas = [\"a\"]\nui = \"none\"\nabsent = [\"domain\"]\n",
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
                    "stack = \"python\"\nareas = [\"a\"]\nui = \"none\"\nabsent = [\"service\"]\n",
                )
            }),
        ),
        (
            "unknown field",
            Box::new(|r| {
                declare(
                    r,
                    "stack = \"python\"\nareas = [\"a\"]\nui = \"none\"\nextra = 1\n",
                )
            }),
        ),
        // A layer cannot be switched off by listing it, or a directory holding it
        (
            "which holds or sits in the layer domain",
            Box::new(|r| {
                declare(
                    r,
                    "stack = \"python\"\nareas = [\"a\"]\nui = \"none\"\nunchecked = [\"domain\"]\n",
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
                    "stack = \"python\"\nareas = [\"a\"]\nui = \"none\"\nunchecked = [\"./scripts\"]\n",
                );
            }),
        ),
        (
            "unchecked lists scripts, which does not exist",
            Box::new(|r| {
                declare(
                    r,
                    "stack = \"python\"\nareas = [\"a\"]\nui = \"none\"\nunchecked = [\"scripts\"]\n",
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
                    "stack = \"python\"\nareas = [\"a\"]\nui = \"none\"\nabsent = [\"composition\", \
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
        // A test with a wrapper in JSX is a view all the same, and so is any case
        "src/presentation/models/useForm.test.tsx",
        "src/presentation/models/Form.TSX",
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
    ] {
        plant_file(root.path(), path);
    }
    let out = run(&["--root", &root_arg(root.path()), "check"]);
    assert!(out.status.success(), "{}", stdout(&out));

    // Python has no such file in its code, and a project with ui = "none" has no presentation to judge
    let root = repo_with_ui("python", "");
    plant_file(root.path(), "presentation/models/view.tsx");
    let out = run(&["--root", &root_arg(root.path()), "check"]);
    assert!(out.status.success(), "{}", stdout(&out));
    let root = declared("stack = \"typescript\"\nareas = [\"a\"]\nui = \"none\"\n");
    let out = create(&["--root", &root_arg(root.path()), "create", "--yes"]);
    assert!(out.status.success(), "{}", stdout(&out));
    plant_file(root.path(), "src/presentation/Form.tsx");
    let said = stdout(&run(&["--root", &root_arg(root.path()), "check"]));
    assert!(!said.contains("a view in"), "{said}");
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
         [workspace.dependencies], so it is not checked",
        "crates/domain/Cargo.toml:15: depends on composition (crates/composition), in composition; ",
        "crates/domain/Cargo.toml:21: depends on app (crates/application), in application; ",
        "crates/protocol/Cargo.toml:8: depends on application (crates/application, from the workspace), in \
         application; protocol may import `domain`",
        "crates/infrastructure/Cargo.toml:8: core comes from the workspace, and .., which [package] workspace names, \
         is no workspace, so it is not checked",
        "crates/application/Cargo.toml:1: cannot be read as TOML (",
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
            "[dependencies]\n",
            "[dependencies]\napplication = { path = \"crates/application\" }\n",
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
         export const load = () => import('../composition/main');\nconst x = require(`../vendor/x`);\n",
    );
    write(
        "src/ui/molecules/Row.tsx",
        "import { Button } from '../atoms/Button';\nimport { List } from '../organisms/List';\nimport '../index.css';\n\
         import logo from '../../domain/logo.svg?url';\nexport const Row = () => <p>{/* ../pages */}</p>;\n",
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
        "src/ui/molecules/Row.tsx:2: imports ../organisms/List (src/ui/organisms/List), in ui.organisms; \
         ui.molecules may import the levels below it",
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
        "stack = \"typescript\"\nareas = [\"a\"]\nui = \"atomic\"\nabsent = [\"composition\"]\n",
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
    // The entry file, outside the layers, is read too
    write(
        "__main__.py",
        "import composition\nfrom application import play\n",
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
    // A layer declared absent is a module the project does not have: a package of the same name is not judged
    fs::write(r.join("domain/model.py"), "import infrastructure.db\n").unwrap();
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
        said.contains("domain/latin.py: cannot be read as UTF-8"),
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
        "stack = \"python\"\nareas = [\"a\"]\nui = \"none\"\nunchecked = [\"scripts/\"]\n",
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
        "stack = \"typescript\"\nareas = [\"a\"]\nui = \"none\"\n",
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
        "--ui <ui>",
        "list in absent",
        "rotproof create   ",
        "rotproof create --yes",
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
        "--root", &arg, "init", "--stack", "python", "--ui", "atomic",
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
    // Nothing is left for a person to write before the first check passes: no area is needed until the first
    // knowledge document
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
            args.extend(["--ui", "atomic"]);
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
        let out = create(&["--root", &arg, "create", "--yes"]);
        assert!(out.status.success(), "{stack}: {}", stdout(&out));
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
        said.contains("name the UI with --ui (atomic, none)"),
        "{said}"
    );
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
    // Every field it adds to the declaration is named, as `rotproof create` names those it adds
    for field in [format!("files = \"{version}\""), "declined = []".into()] {
        assert!(
            stdout(&out).contains(&format!("added to .config/rotproof.toml: {field}\n")),
            "{}",
            stdout(&out)
        );
    }
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
fn init_asks_no_person_for_the_settings_it_writes_itself() {
    // A project of the first release without .claude/settings.json: the deny rule's update finds no file, and the
    // refresh of the same run writes one that has the rule
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
        stdout(&out).contains("wrote .claude/settings.json"),
        "{}",
        stdout(&out)
    );
    assert!(!stdout(&out).contains("needs a person"), "{}", stdout(&out));
    let settings = fs::read_to_string(r.join(".claude/settings.json")).unwrap();
    assert!(
        settings.contains("Edit(/.config/rotproof-approved.toml)"),
        "{settings}"
    );
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
        "stack = \"python\"\nareas = [\"a\"]\nui = \"none\"\nunchecked = [\"scripts\"]\n",
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
            "stack = \"rust\"\nareas = [\"a\"]\nui = \"none\"\nabsent = []\nunchecked = [\"crates/generated\"]\n",
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
    }
}

/// Run the stop hook from `root` with `input` on stdin.
fn stop_hook(root: &Path, input: &str) -> Output {
    hook(root, "stop-hook", input)
}

/// Run the hook `command` from `root` with `input` on stdin.
fn hook(root: &Path, command: &str, input: &str) -> Output {
    use std::io::Write;
    use std::process::Stdio;
    let mut child = Command::new(env!("CARGO_BIN_EXE_rotproof"))
        .args(["--root", &root_arg(root), command])
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

#[test]
fn create_writes_no_stop_hook_and_the_old_one_says_it_does_nothing() {
    let root = declared("stack = \"none\"\nareas = [\"a\"]\n");
    let r = root.path();
    assert!(create(&["--root", &root_arg(r), "create"]).status.success());
    // create wrote the settings that run no hook, for Claude Code only
    let settings = fs::read_to_string(r.join(".claude/settings.json")).unwrap();
    assert!(!settings.contains("\"Stop\""), "{settings}");
    assert!(!settings.contains("\"hooks\""), "{settings}");
    // and that deny Claude Code editing the approvals file, which only a person adds to
    assert!(
        settings.contains("\"deny\": [\n      \"Edit(/.config/rotproof-approved.toml)\"\n    ]"),
        "{settings}"
    );
    assert!(!r.join(".gemini").exists());

    // The command a project's settings may still run: a message for the person and exit 0, whatever the input and
    // the root, inside a project or not, as Claude Code takes 2 for "do not stop"
    let outside = tempfile::tempdir().unwrap();
    let open = r#"{"hook_event_name": "Stop", "stop_hook_active": false, "last_assistant_message": "未確認"}"#;
    for (from, input) in [
        (r.to_path_buf(), open),
        (outside.path().to_path_buf(), "not json"),
        (r.join("no/such/directory"), ""),
    ] {
        let out = stop_hook(&from, input);
        assert_eq!(out.status.code(), Some(0), "{from:?}");
        let said: serde_json::Value = serde_json::from_str(&stdout(&out)).unwrap();
        assert!(
            said["systemMessage"]
                .as_str()
                .unwrap()
                .contains("run `rotproof init`, which takes this hook out"),
            "{said}"
        );
        assert!(said.get("hookSpecificOutput").is_none(), "{said}");
    }
    // Hidden from the help
    let help = stdout(&run(&["--help"]));
    assert!(!help.contains("stop-hook"), "{help}");
}
#[test]
fn the_old_time_hook_says_it_does_nothing_at_the_start_of_a_session_and_never_fails() {
    let root = declared("stack = \"none\"\nareas = [\"a\"]\n");
    let r = root.path();
    assert!(create(&["--root", &root_arg(r), "create"]).status.success());
    // create wrote no hook of any event: nothing runs `rotproof time-hook`
    let settings = fs::read_to_string(r.join(".claude/settings.json")).unwrap();
    assert!(!settings.contains("hooks"), "{settings}");
    assert!(!settings.contains("time-hook"), "{settings}");

    // The command a project's settings may still run: at the start of a session, a message for the person and exit 0
    let out = hook(r, "time-hook", "{\"hook_event_name\": \"SessionStart\"}");
    assert_eq!(out.status.code(), Some(0));
    let said: serde_json::Value = serde_json::from_str(&stdout(&out)).unwrap();
    assert!(
        said["systemMessage"]
            .as_str()
            .unwrap()
            .contains("run `rotproof init`, which takes this hook out"),
        "{said}"
    );
    assert!(said.get("hookSpecificOutput").is_none(), "{said}");
    // On a prompt, and on any input, nothing: the notice would come back on every prompt. It fails on none, and
    // reads no root, inside a project or not, as Claude Code takes 2 for "block the prompt"
    let outside = tempfile::tempdir().unwrap();
    for (from, input) in [
        (
            r.to_path_buf(),
            "{\"hook_event_name\": \"UserPromptSubmit\", \"prompt\": \"go on\"}",
        ),
        (r.to_path_buf(), "{\"hook_event_name\": \"Stop\"}"),
        (outside.path().to_path_buf(), "not json"),
        (r.join("no/such/directory"), ""),
        (r.join("no/such/directory"), "{}"),
    ] {
        let out = hook(&from, "time-hook", input);
        assert_eq!(out.status.code(), Some(0), "{from:?} {input}");
        assert_eq!(stdout(&out), "", "{from:?} {input}");
        assert_eq!(String::from_utf8_lossy(&out.stderr), "", "{from:?} {input}");
    }
    // Hidden from the help
    let help = stdout(&run(&["--help"]));
    assert!(!help.contains("time-hook"), "{help}");
}

/// Run the binary with `args` as given, with `{}` on stdin.
fn run_with_input(args: &[&str]) -> Output {
    use std::io::Write;
    use std::process::Stdio;
    let mut child = Command::new(env!("CARGO_BIN_EXE_rotproof"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the binary runs");
    // The binary may exit on its command line before it reads any input
    let _ = child.stdin.take().unwrap().write_all(b"{}");
    child.wait_with_output().unwrap()
}

#[test]
fn a_hook_with_a_wrong_command_line_exits_1_never_2() {
    // Claude Code reads exit code 2 from a Stop hook as "do not stop" and from a UserPromptSubmit hook as "block the
    // prompt": a hook set up wrongly fails with 1 and says so, as clap does for the other commands, with 2
    let root = tempfile::tempdir().unwrap();
    let arg = root_arg(root.path());
    for args in [
        vec!["stop-hook", "extra"],
        vec!["time-hook", "--bogus"],
        vec!["--root", &arg, "stop-hook", "extra"],
        vec!["--root", &arg, "time-hook", "--bogus"],
        vec!["--bogus", "stop-hook"],
    ] {
        let out = run_with_input(&args);
        assert_eq!(out.status.code(), Some(1), "{args:?}");
        let said = String::from_utf8_lossy(&out.stderr);
        assert!(said.contains("unexpected argument"), "{args:?}: {said}");
        assert_eq!(stdout(&out), "", "{args:?}");
    }
    // The help is not a failure
    for args in [
        vec!["stop-hook", "--help"],
        vec!["time-hook", "-h"],
        vec!["--root", &arg, "stop-hook", "--help"],
        vec!["--version", "time-hook"],
    ] {
        let out = run_with_input(&args);
        assert_eq!(out.status.code(), Some(0), "{args:?}");
        assert!(!stdout(&out).is_empty(), "{args:?}");
    }
    // Any other command is as it was: 2
    for args in [
        vec!["check", "extra"],
        vec!["index", "--bogus"],
        vec!["--bogus", "check"],
        vec!["bogus-command"],
        vec!["stop-hook-not", "x"],
        vec!["check", "stop-hook"],
    ] {
        let out = run_with_input(&args);
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
    fs::write(r.join("domain/model.py"), "import os\n").unwrap();
    let out = run(&["--root", &root_arg(r), "approve", "--prune"]);
    assert_eq!(out.status.code(), Some(0), "{}", stdout(&out));
    assert!(!fs::read_to_string(&path).unwrap().contains("[[kept]]"));
    let out = run(&["--root", &root_arg(r), "check"]);
    assert_eq!(out.status.code(), Some(0), "{}", stdout(&out));
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
