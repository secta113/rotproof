//! `rotproof check`: every rule of the structure and the records, run against one repository. The rules are listed
//! for a person in the help of `rotproof check` (`CHECK_HELP` in `cli.rs`) and in the specification ("What `rotproof
//! check` checks"); each is judged in the module of `domain` that holds it, and this use case reads what it needs and
//! runs them.
//!
//! The structure, the checks of the comments and the records each have a floor: when the scan finds nothing at all,
//! it fails instead of passing with nothing checked. The checks of the environment, the I/O of the inner layers and
//! the dependencies have none of their own.

use std::io;

use crate::finding::{under, warn};
use crate::tree::{exactly, read_text};
use domain::approvals::{over_limit, shown_all, sort, stale};
use domain::baseline;
use domain::declaration::Declared;
use domain::direction::Rule;
use domain::guide::GUIDE;
use domain::markers::KNOWLEDGE_HEADING;
use domain::moving::Candidates;
use domain::outside::Outside;
use domain::project::MAP_HEADING;
use domain::running::{PATH as RUNNING_PATH, RUNNING};
use domain::schema::Time;
use domain::suppressions::WEAKENS_HEADING;
use domain::upgrade::guide_command;
use domain::version::VERSION;
use protocol::inbound::{Finding, LinkedTodo, Moving, MovingFile, Report, Templates, Warning};
use protocol::outbound::Parsers;
use protocol::outbound::Tree;

/// Every broken rule in the repository `tree` holds, at `now`. An error is a file that could not be read at all.
pub fn check(tree: &dyn Tree, parsers: &dyn Parsers, now: Time) -> io::Result<Report> {
    let structure = crate::structure::problems(tree)?;
    let mut report = Report {
        skipped: structure.skipped,
        tests: structure.tests,
        ..Report::default()
    };
    report.findings.extend(under(
        "the tree matches .config/rotproof.toml",
        structure.found,
    ));
    let mut weakened = Vec::new();
    if let Some(declared) = &structure.declared {
        report.layers_checked = declared.layout.is_some();
        weakened = code(tree, parsers, declared, &mut report)?;
        report.findings.extend(rotproof_files(tree, declared));
    }
    let mut stale_map = Vec::new();
    if let Some(declared) = &structure.declared {
        stale_map = warn(MAP_HEADING, crate::project::map_problems(tree, declared)).collect();
    }
    let records = crate::records::problems(tree, parsers, structure.areas, now)?;
    report.findings.extend(records.found);
    // The warnings are printed in this order: the documents that are due, then the comments of the code
    report.warnings.extend(records.warnings);
    report.warnings.extend(weakened);
    report.warnings.extend(stale_map);
    Ok(report)
}

/// Every broken rule of the code of `declared`, into `report`: the direction with its approvals, the markers, who
/// reads the environment, and the comments that switch the type checker off. The approvals go in `report` too, as they
/// are printed apart from what fails, and so do the comments tied to an issue; the comments that may make the checker
/// check less are returned, as warnings.
fn code(
    tree: &dyn Tree,
    parsers: &dyn Parsers,
    declared: &Declared,
    report: &mut Report,
) -> io::Result<Vec<Warning>> {
    let direction = crate::direction::problems(tree, parsers, declared)?;
    report.moving = direction.moving.map(|order| Moving {
        total: order.total,
        ordered: order.ordered,
        next: order
            .next
            .into_iter()
            .map(|group| {
                group
                    .into_iter()
                    .map(|file| MovingFile {
                        path: file.path,
                        candidates: match file.candidates {
                            Candidates::Any => None,
                            Candidates::Only(places) => Some(places),
                        },
                    })
                    .collect()
            })
            .collect(),
        more: order.more,
    });
    report.outside = direction.outside.iter().map(Outside::said).collect();
    report.sizes = Some(crate::sizes::measure(tree, parsers, declared)?);
    report.templates = direction.templates.map(|premise| Templates {
        read_as: premise.read_as,
        not_judged: premise.not_judged,
    });
    let listed = crate::readers::problems(tree, declared, &direction.reads);
    let approvals = crate::approvals::read(tree)?;
    // What the layer table forbids and the I/O of the inner layers are kept the same way: one approval for a file and
    // what it imports, or the entry of the table of I/O it uses
    let io = crate::inner_io::problems(tree, parsers, declared)?;
    report.io_net = io.excluded.line();
    let forbidden: Vec<_> = direction
        .forbidden
        .iter()
        .chain(&io.forbidden)
        .cloned()
        .collect();
    let sorted = sort(&forbidden, &approvals.entries);
    // What the approvals leave is judged by the baseline: a violation it froze passes, and any other fails
    let frozen = crate::baseline::read(tree)?;
    let rest = baseline::sort(&sorted.forbidden, frozen.entries());
    let of_rule = |direction: bool| {
        rest.forbidden
            .iter()
            .filter(move |f| (f.rule == Rule::Direction) == direction)
            .map(|f| f.detail.clone())
            .collect::<Vec<_>>()
    };
    report.findings.extend(under(
        "the layers import only what the layer table in .rotproof/AGENTS.md allows",
        direction.problems.into_iter().chain(of_rule(true)),
    ));
    report.findings.extend(under(
        domain::inner_io::HEADING,
        io.problems.into_iter().chain(of_rule(false)),
    ));
    report.findings.extend(under(
        crate::dependencies::HEADING,
        crate::dependencies::problems(tree, parsers, declared)?,
    ));
    report.findings.extend(under(
        domain::locked::HEADING,
        crate::locked::problems(tree, declared)?,
    ));
    report.findings.extend(under(
        "every approval matches a forbidden import",
        approvals
            .problems
            .into_iter()
            .chain(sorted.stale.iter().map(|entry| stale(entry))),
    ));
    report.findings.extend(under(
        "the approvals stay within approved_max",
        over_limit(approvals.entries.len(), declared.declaration.approved_max),
    ));
    report.findings.extend(under(
        baseline::HEADING,
        frozen
            .problems
            .iter()
            .cloned()
            .chain(rest.stale.iter().map(|entry| baseline::stale(entry))),
    ));
    report.findings.extend(under(
        baseline::COUNT_HEADING,
        frozen
            .baseline
            .as_ref()
            .and_then(|b| baseline::over_count(b.entries.len(), b.count)),
    ));
    report.baseline = frozen
        .baseline
        .as_ref()
        .map(|b| baseline::remaining(b.entries.len()));
    report
        .findings
        .extend(under(domain::readers::HEADING, listed));
    report.approved = shown_all(&sorted);
    report.approved.sort();
    report.approved.dedup();
    let comments = crate::markers::problems(tree, parsers, declared)?;
    let mut markers = comments.markers;
    report.linked = std::mem::take(&mut markers.linked)
        .into_iter()
        .map(|linked| LinkedTodo {
            path: linked.path,
            line: linked.line,
            reference: linked.reference,
        })
        .collect();
    // In the order of the path, then of the line, which a CI step reads, with no sort here: the tree gives the files in
    // the byte order of their paths (`Tree::files`), and a file's lines come in the order of the file
    report
        .findings
        .extend(under(markers.heading(declared.todo_link), markers.found));
    report
        .findings
        .extend(under(KNOWLEDGE_HEADING, markers.knowledge));
    report.findings.extend(under(
        domain::environment::HEADING,
        crate::environment::problems(tree, parsers, declared)?,
    ));
    report
        .findings
        .extend(under(domain::suppressions::HEADING, comments.silenced));
    Ok(warn(WEAKENS_HEADING, comments.weakened).collect())
}

/// Every broken rule of the files Rotproof writes into the project of `declared`: they are up to this version
/// (`files` in the declaration), the guide and the page on running Rotproof are the ones this version writes, and the
/// project has answered whether its agent reads the guide.
fn rotproof_files(tree: &dyn Tree, declared: &Declared) -> Vec<Finding> {
    let mut found: Vec<Finding> = under(
        "the project's files are up to this version of Rotproof",
        domain::upgrade::problems(
            declared.declaration.files.as_deref(),
            &declared.declaration.declined,
            VERSION,
        ),
    )
    .collect();
    let files = declared.declaration.files.as_deref();
    let guide = domain::guide::guide(&declared.declaration.stack, declared.layout.as_ref());
    found.extend(under(
        "Rotproof's guide is up to date",
        generated(tree, GUIDE, &guide, files),
    ));
    found.extend(under(
        "Rotproof's page on running it is up to date",
        generated(tree, RUNNING_PATH, RUNNING, files),
    ));
    // The same judgement as the question `rotproof create` and `rotproof init` ask, so that it cannot be skipped
    found.extend(under(
        domain::pointer::HEADING,
        crate::project::unanswered(tree, declared).map(|_| domain::pointer::unanswered()),
    ));
    found
}

/// What is wrong with the file Rotproof generates at `path`, whose text is to be `text`, with the command that writes
/// it again: nothing when it is that text.
fn generated(tree: &dyn Tree, path: &str, text: &str, files: Option<&str>) -> Option<String> {
    let written = exactly(tree, path)
        .ok()
        .and_then(|(found, _)| read_text(tree, &found).ok());
    if written.as_deref() == Some(text) {
        return None;
    }
    let state = if written.is_some() {
        "out of date"
    } else {
        "missing"
    };
    Some(format!(
        "{state}, {}: {path}",
        guide_command(files, VERSION)
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::fake::Fake;
    use domain::code::{Aliases, Import, Manifest, Source};
    use domain::definitions::Definition;
    use domain::dependencies::Listing;
    use domain::layers::Language;
    use infrastructure::outbound::readers::{Readers, Remembering};
    use std::cell::RefCell;
    use std::collections::BTreeMap;

    /// Readers that count how many times each path is asked of the readers of code.
    #[derive(Default)]
    struct Counting {
        asked: RefCell<BTreeMap<String, usize>>,
    }

    impl Counting {
        fn count(&self, path: &str) {
            *self.asked.borrow_mut().entry(path.to_string()).or_default() += 1;
        }
    }

    impl Parsers for Counting {
        fn python(&self, source: &str, path: &str) -> Source<Vec<String>> {
            self.count(path);
            Readers.python(source, path)
        }

        fn outline(
            &self,
            language: Language,
            source: &str,
            path: &str,
        ) -> Result<Vec<Definition>, (usize, String)> {
            Readers.outline(language, source, path)
        }

        fn typescript(&self, source: &str, path: &str) -> Source<Import> {
            self.count(path);
            Readers.typescript(source, path)
        }

        fn typescript_aliases(&self, tree: &dyn Tree, roots: &[String]) -> io::Result<Aliases> {
            Readers.typescript_aliases(tree, roots)
        }

        fn manifest(&self, source: &str) -> Result<Manifest, (usize, String)> {
            Readers.manifest(source)
        }

        fn listing(&self, language: Language, source: &str) -> Result<Listing, (usize, String)> {
            Readers.listing(language, source)
        }
    }

    /// How many times each file of code was asked of the readers of code when `rotproof check` ran over `files`, with
    /// readers that remember what they read or that do not.
    fn asked(stack: &str, files: &[(&str, &str)], remember: bool) -> BTreeMap<String, usize> {
        let declaration = format!(
            "stack = \"{stack}\"\nareas = []\nui_design = \"none\"\nmoving = [\"legacy\"]\nfiles = \"2.0.0\"\n"
        );
        let mut all = vec![(".config/rotproof.toml", declaration.as_str())];
        all.extend_from_slice(files);
        let tree = Fake::new(&all);
        let now = chrono::DateTime::parse_from_rfc3339("2026-11-15T12:00:00+09:00").unwrap();
        if remember {
            let readers = Remembering::new(Counting::default());
            check(&tree, &readers, now).unwrap();
            readers.inner().asked.take()
        } else {
            let readers = Counting::default();
            check(&tree, &readers, now).unwrap();
            readers.asked.take()
        }
    }

    #[test]
    fn a_run_reads_each_file_of_python_code_once_the_code_that_is_moving_included() {
        let files = [
            (
                "domain/a.py",
                "import os\nfrom legacy import old\n\n\nclass A:\n    def go(self):\n        return os.getcwd()\n",
            ),
            ("application/b.py", "import json\nfrom domain import a\n"),
            ("legacy/old.py", "import shutil\nimport legacy.older\n"),
            ("legacy/older.py", "import sys\n"),
            ("tests/test_a.py", "import domain.a\n"),
            ("__main__.py", "import application.b\n"),
        ];
        let once = asked("python", &files, true);
        for path in [
            "domain/a.py",
            "application/b.py",
            "legacy/old.py",
            "legacy/older.py",
        ] {
            assert_eq!(once.get(path), Some(&1), "{path}: {once:?}");
        }
        assert!(once.values().all(|times| *times == 1), "{once:?}");
        // The same run with readers that forget asks for some files again and again, which this test would see
        let forgetting = asked("python", &files, false);
        assert!(
            forgetting.values().any(|times| *times > 1),
            "{forgetting:?}"
        );
    }

    #[test]
    fn a_run_reads_each_file_of_typescript_code_once_the_code_that_is_moving_included() {
        let files = [
            (
                "src/domain/a.ts",
                "import fs from 'node:fs';\nimport { old } from '../../legacy/old';\n",
            ),
            ("src/application/b.ts", "import { a } from '../domain/a';\n"),
            (
                "legacy/old.ts",
                "import path from 'node:path';\nimport { older } from './older';\n",
            ),
            ("legacy/older.ts", "export const older = 1;\n"),
            ("package.json", "{\"name\":\"x\"}\n"),
        ];
        let once = asked("typescript", &files, true);
        for path in [
            "src/domain/a.ts",
            "src/application/b.ts",
            "legacy/old.ts",
            "legacy/older.ts",
        ] {
            assert_eq!(once.get(path), Some(&1), "{path}: {once:?}");
        }
        assert!(once.values().all(|times| *times == 1), "{once:?}");
        let forgetting = asked("typescript", &files, false);
        assert!(
            forgetting.values().any(|times| *times > 1),
            "{forgetting:?}"
        );
    }
}
