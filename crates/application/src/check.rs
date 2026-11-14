//! `rotproof check`: every rule of the structure and the records, run against one repository. The rules are listed
//! for a person in the help of `rotproof check` (`CHECK_HELP` in `cli.rs`) and in the README ("What `rotproof
//! check` checks"); each is judged in the module of `domain` that holds it, and this use case reads what it needs and
//! runs them.
//!
//! Each check has a floor: when the scan finds nothing at all, it fails instead of passing with nothing checked.

use std::io;

use crate::finding::{under, warn};
use crate::tree::{exactly, read_text};
use domain::approvals::{shown, sort, stale};
use domain::declaration::{Declared, INIT_OR_CREATE};
use domain::guide::GUIDE;
use domain::markers::KNOWLEDGE_HEADING;
use domain::schema::Time;
use domain::suppressions::WEAKENS_HEADING;
use domain::version::VERSION;
use protocol::inbound::{Finding, Report, Warning};
use protocol::outbound::Parsers;
use protocol::outbound::Tree;

/// Every broken rule in the repository `tree` holds, at `now`. An error is a file that could not be read at all.
pub fn check(tree: &dyn Tree, parsers: &dyn Parsers, now: Time) -> io::Result<Report> {
    let structure = crate::structure::problems(tree)?;
    let mut report = Report {
        skipped: structure.skipped,
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
    let records = crate::records::problems(tree, parsers, structure.areas, now)?;
    report.findings.extend(records.found);
    // The warnings are printed in this order: the documents that are due, then the comments of the code
    report.warnings.extend(records.warnings);
    report.warnings.extend(weakened);
    Ok(report)
}

/// Every broken rule of the code of `declared`, into `report`: the direction with its approvals, the markers, who
/// reads the environment, and the comments that switch the type checker off. The approvals go in `report` too, as they
/// are printed apart from what fails; the comments that may make the checker check less are returned, as warnings.
fn code(
    tree: &dyn Tree,
    parsers: &dyn Parsers,
    declared: &Declared,
    report: &mut Report,
) -> io::Result<Vec<Warning>> {
    let direction = crate::direction::problems(tree, parsers, declared)?;
    let approvals = crate::approvals::read(tree)?;
    let sorted = sort(&direction.forbidden, &approvals.entries);
    report.findings.extend(under(
        "the layers import only what layers/table.toml allows",
        direction
            .problems
            .into_iter()
            .chain(sorted.forbidden.iter().map(|f| f.detail.clone())),
    ));
    report.findings.extend(under(
        "every approval matches a forbidden import",
        approvals
            .problems
            .into_iter()
            .chain(sorted.stale.iter().map(|entry| stale(entry))),
    ));
    report.approved = sorted
        .approved
        .iter()
        .map(|(_, entry)| shown(entry))
        .collect();
    report.approved.sort();
    report.approved.dedup();
    let comments = crate::markers::problems(tree, parsers, declared)?;
    let markers = comments.markers;
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
/// (`files` in the declaration), and the guide is the one this version writes.
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
            [format!("{state}, {INIT_OR_CREATE}: {GUIDE}")],
        ));
    }
    found
}
