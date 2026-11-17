//! `rotproof check`: every rule of the structure and the records, run against one repository. The rules are listed
//! for a person in the help of `rotproof check` (`CHECK_HELP` in `cli.rs`) and in the specification ("What `rotproof
//! check` checks"); each is judged in the module of `domain` that holds it, and this use case reads what it needs and
//! runs them.
//!
//! Each check has a floor: when the scan finds nothing at all, it fails instead of passing with nothing checked.

use std::io;

use crate::finding::{under, warn};
use crate::tree::{exactly, read_text};
use domain::approvals::{over_limit, shown_all, sort, stale};
use domain::declaration::Declared;
use domain::direction::Rule;
use domain::guide::GUIDE;
use domain::markers::KNOWLEDGE_HEADING;
use domain::moving::Candidates;
use domain::outside::Outside;
use domain::project::MAP_HEADING;
use domain::schema::Time;
use domain::skill::{PATHS as SKILL_PATHS, SKILL};
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
    let of_rule = |direction: bool| {
        sorted
            .forbidden
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
/// (`files` in the declaration), and the guide and the skill are the ones this version writes.
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
    let guide = domain::guide::guide(&declared.declaration.stack, declared.layout.as_ref());
    let written = exactly(tree, GUIDE)
        .ok()
        .and_then(|(path, _)| read_text(tree, &path).ok());
    if written.as_ref() != Some(&guide) {
        let state = if written.is_some() {
            "out of date"
        } else {
            "missing"
        };
        found.extend(under(
            "Rotproof's guide is up to date",
            [format!(
                "{state}, {}: {GUIDE}",
                guide_command(declared.declaration.files.as_deref(), VERSION)
            )],
        ));
    }
    // Each copy on a line of its own, so that one missing is not read as both
    found.extend(under(
        "Rotproof's skill is up to date",
        SKILL_PATHS.iter().filter_map(|path| {
            let written = exactly(tree, path)
                .ok()
                .and_then(|(found, _)| read_text(tree, &found).ok());
            if written.as_deref() == Some(SKILL) {
                return None;
            }
            let state = if written.is_some() {
                "out of date"
            } else {
                "missing"
            };
            Some(format!(
                "{state}, {}: {path}",
                guide_command(declared.declaration.files.as_deref(), VERSION)
            ))
        }),
    ));
    found
}
