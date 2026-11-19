//! What `rotproof check` and `rotproof sizes` print of the size of the code: the facts of `protocol::inbound::Sizes` as
//! lines. Nothing here says what a number means: a share is a share, and a layer with no code is said once, when it is
//! so.

use protocol::inbound::{Defined, Part, Size, UnitSizes};

/// The lines of `rotproof check` for the units of a project: for each, one line with the share of its lines of code
/// that each layer holds (the layers with code, the larger first, then `moving` and `unchecked` when they hold some),
/// and a line for what is nothing: the layers with no code at all, and a `domain` that defines types and no function
/// (or holds code and defines neither). A project that declares no unit has no unit's name in them.
pub fn summary(units: &[UnitSizes]) -> Vec<String> {
    let mut lines = Vec::new();
    for unit in units {
        let label = match unit.name.as_str() {
            "" => "sizes".to_string(),
            name => format!("sizes ({name})"),
        };
        let total = total(unit);
        let mut shares: Vec<String> = ranked(unit)
            .into_iter()
            .filter(|(size, _)| size.lines > 0)
            .map(|(size, _)| format!("{} {}", size.name, percent(size.lines, total)))
            .collect();
        shares.extend(
            unit.parts
                .iter()
                .filter(|size| {
                    matches!(size.part, Part::Moving | Part::Unchecked) && size.lines > 0
                })
                .map(|size| format!("{} {}", size.name, percent(size.lines, total))),
        );
        if !shares.is_empty() {
            lines.push(format!(
                "{label}: {} of the lines of code",
                shares.join(", ")
            ));
        }
        let empty: Vec<&str> = unit
            .parts
            .iter()
            .filter(|size| size.part == Part::Layer && size.lines == 0)
            .map(|size| size.name.as_str())
            .collect();
        if !empty.is_empty() {
            lines.push(format!("{label}: no code in {}", empty.join(", ")));
        }
        if let Some(fact) = domain_fact(unit) {
            lines.push(format!("{label}: {fact}"));
        }
    }
    lines
}

/// The lines of `rotproof sizes`: for each unit, a table of the layers (each followed by its levels, the layers with
/// the most lines first), the entry files, `moving` and `unchecked`, with files, lines and the share of the unit's
/// lines of code, then the files of each layer with the most lines. `none` says what is said of a project with no
/// layer to measure.
pub fn detail(units: &[UnitSizes], none: &str) -> Vec<String> {
    if units.is_empty() {
        return vec![none.to_string()];
    }
    let mut lines = vec![
        "lines of code (not counted: blank lines, comments, tests); share: of the total row"
            .to_string(),
    ];
    for unit in units {
        lines.push(String::new());
        if !unit.name.is_empty() {
            lines.push(format!("unit {}:", unit.name));
        }
        lines.extend(table(unit));
        if let Some(fact) = domain_fact(unit) {
            lines.push(fact);
        }
        let longest: Vec<&Size> = ranked(unit)
            .into_iter()
            .map(|(size, _)| size)
            .filter(|size| !size.longest.is_empty())
            .collect();
        if !longest.is_empty() {
            lines.push(String::new());
            lines.push("longest files:".to_string());
            for size in longest {
                lines.push(format!("  {}", size.name));
                let width = size.longest[0].lines.to_string().len();
                lines.extend(
                    size.longest
                        .iter()
                        .map(|file| format!("    {:>width$}  {}", file.lines, file.path)),
                );
            }
        }
    }
    lines
}

/// `lines` as a share of `total`, in whole percent: rounded to the nearest, and `<1%` for what is some but rounds to
/// none, so that no code is said to be nothing. `0%` for no line at all, and for a total of none.
fn percent(lines: usize, total: usize) -> String {
    if lines == 0 || total == 0 {
        return "0%".to_string();
    }
    // In 128 bits, so that no count of lines overflows
    let rounded = (lines as u128 * 100 + total as u128 / 2) / total as u128;
    match rounded {
        0 => "<1%".to_string(),
        n => format!("{n}%"),
    }
}

/// The lines of code of a unit, everything: its layers, the entry files, `moving` and `unchecked`. A level is in its
/// layer already.
fn total(unit: &UnitSizes) -> usize {
    unit.parts
        .iter()
        .filter(|size| size.part != Part::Level)
        .map(|size| size.lines)
        .sum()
}

/// The layers of a unit, the one with the most lines first and the others in the order they were given in, each with
/// its levels after it.
fn ranked(unit: &UnitSizes) -> Vec<(&Size, Vec<&Size>)> {
    let mut layers: Vec<(&Size, Vec<&Size>)> = Vec::new();
    for size in &unit.parts {
        match size.part {
            Part::Layer => layers.push((size, Vec::new())),
            Part::Level => {
                if let Some((_, levels)) = layers.last_mut() {
                    levels.push(size);
                }
            }
            _ => {}
        }
    }
    layers.sort_by_key(|(layer, _)| std::cmp::Reverse(layer.lines));
    layers
}

/// What is said of the `domain` of a unit, when it is so: that it defines types and no function, or that it holds code
/// and defines neither a function nor a type. Nothing says what it means.
fn domain_fact(unit: &UnitSizes) -> Option<String> {
    let domain = unit
        .parts
        .iter()
        .find(|size| size.part == Part::Layer && size.name == "domain")?;
    let Defined { functions, types } = unit.domain_defines?;
    match (functions, types) {
        (0, 0) if domain.lines > 0 => {
            let lines = match domain.lines {
                1 => "1 line".to_string(),
                n => format!("{n} lines"),
            };
            Some(format!(
                "domain holds {lines} of code and defines no function or type"
            ))
        }
        (0, 1) => Some("domain defines 1 type and no function".to_string()),
        (0, n) if n > 1 => Some(format!("domain defines {n} types and no function")),
        _ => None,
    }
}

/// The table of one unit: a header, a row for each part and a last one for the whole.
fn table(unit: &UnitSizes) -> Vec<String> {
    let total = total(unit);
    let mut rows: Vec<(String, &Size)> = Vec::new();
    for (layer, levels) in ranked(unit) {
        rows.push((layer.name.clone(), layer));
        rows.extend(
            levels
                .into_iter()
                .map(|level| (format!("  {}", level.name), level)),
        );
    }
    rows.extend(
        unit.parts
            .iter()
            .filter(|size| matches!(size.part, Part::Entry | Part::Moving | Part::Unchecked))
            .map(|size| (size.name.clone(), size)),
    );
    let files: usize = unit
        .parts
        .iter()
        .filter(|size| size.part != Part::Level)
        .map(|size| size.files)
        .sum();
    let name_width = rows
        .iter()
        .map(|(name, _)| name.chars().count())
        .chain(["total".len()])
        .max()
        .unwrap_or_default();
    let files_width = rows
        .iter()
        .map(|(_, size)| size.files)
        .chain([files])
        .map(|n| n.to_string().len())
        .max()
        .unwrap_or_default()
        .max("files".len());
    let lines_width = rows
        .iter()
        .map(|(_, size)| size.lines)
        .chain([total])
        .map(|n| n.to_string().len())
        .max()
        .unwrap_or_default()
        .max("lines".len());
    let mut out = vec![format!(
        "{:<name_width$}  {:>files_width$}  {:>lines_width$}  share",
        "", "files", "lines"
    )];
    for (name, size) in &rows {
        out.push(format!(
            "{name:<name_width$}  {:>files_width$}  {:>lines_width$}  {:>5}",
            size.files,
            size.lines,
            percent(size.lines, total)
        ));
    }
    out.push(format!(
        "{:<name_width$}  {files:>files_width$}  {total:>lines_width$}",
        "total"
    ));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use protocol::inbound::Longest;

    fn size(name: &str, part: Part, files: usize, lines: usize) -> Size {
        Size {
            name: name.to_string(),
            part,
            files,
            lines,
            longest: if part == Part::Layer && lines > 0 {
                vec![Longest {
                    path: format!("{name}/a.py"),
                    lines,
                }]
            } else {
                Vec::new()
            },
        }
    }

    fn defined(functions: usize, types: usize) -> Option<Defined> {
        Some(Defined { functions, types })
    }

    fn unit(name: &str, parts: Vec<Size>, domain_defines: Option<Defined>) -> UnitSizes {
        UnitSizes {
            name: name.to_string(),
            parts,
            domain_defines,
        }
    }

    #[test]
    fn a_share_rounds_to_the_nearest_percent_and_never_says_some_code_is_none() {
        assert_eq!(percent(1, 3), "33%");
        assert_eq!(percent(2, 3), "67%");
        assert_eq!(percent(1, 2), "50%");
        assert_eq!(percent(1, 1000), "<1%");
        assert_eq!(percent(5, 1000), "1%");
        assert_eq!(percent(0, 10), "0%");
        assert_eq!(percent(7, 0), "0%");
        assert_eq!(percent(10, 10), "100%");
    }

    #[test]
    fn the_summary_is_one_line_of_shares_the_larger_layer_first() {
        let units = [unit(
            "",
            vec![
                size("composition", Part::Layer, 1, 1),
                size("domain", Part::Layer, 3, 60),
                size("infrastructure", Part::Layer, 2, 30),
                size("infrastructure.inbound", Part::Level, 1, 10),
                size("moving", Part::Moving, 4, 9),
            ],
            defined(5, 1),
        )];
        assert_eq!(
            summary(&units),
            [
                "sizes: domain 60%, infrastructure 30%, composition 1%, moving 9% of the lines of code"
            ]
        );
    }

    #[test]
    fn a_layer_with_no_code_and_a_domain_that_defines_nothing_are_said_apart_and_only_when_so() {
        let units = [unit(
            "",
            vec![
                size("composition", Part::Layer, 1, 0),
                size("domain", Part::Layer, 3, 40),
                size("protocol", Part::Layer, 0, 0),
            ],
            defined(0, 0),
        )];
        assert_eq!(
            summary(&units),
            [
                "sizes: domain 100% of the lines of code",
                "sizes: no code in composition, protocol",
                "sizes: domain holds 40 lines of code and defines no function or type",
            ]
        );
        // With no code at all there is no share to say
        let none = [unit(
            "",
            vec![size("domain", Part::Layer, 1, 0)],
            defined(0, 0),
        )];
        assert_eq!(summary(&none), ["sizes: no code in domain"]);
    }

    #[test]
    fn a_domain_of_types_and_no_function_is_said_with_the_number_of_its_types() {
        let domain =
            |defines| summary(&[unit("", vec![size("domain", Part::Layer, 3, 40)], defines)]);
        assert_eq!(
            domain(defined(0, 14)),
            [
                "sizes: domain 100% of the lines of code",
                "sizes: domain defines 14 types and no function"
            ]
        );
        assert_eq!(
            domain(defined(0, 1))[1],
            "sizes: domain defines 1 type and no function"
        );
        // A function, or no domain to ask, says nothing
        assert_eq!(domain(defined(1, 14)).len(), 1);
        assert_eq!(domain(None).len(), 1);
    }

    #[test]
    fn each_unit_has_its_own_line_named_by_the_unit() {
        let units = [
            unit(
                "billing",
                vec![size("domain", Part::Layer, 1, 10)],
                defined(1, 0),
            ),
            unit(
                "shop",
                vec![size("domain", Part::Layer, 1, 20)],
                defined(1, 0),
            ),
        ];
        assert_eq!(
            summary(&units),
            [
                "sizes (billing): domain 100% of the lines of code",
                "sizes (shop): domain 100% of the lines of code"
            ]
        );
    }

    #[test]
    fn the_table_has_a_row_for_each_part_with_the_levels_under_their_layer_and_the_whole_last() {
        let units = [unit(
            "",
            vec![
                size("application", Part::Layer, 2, 25),
                size("domain", Part::Layer, 10, 70),
                size("domain.rules", Part::Level, 4, 30),
                size("entry", Part::Entry, 1, 5),
            ],
            defined(3, 2),
        )];
        let lines = detail(&units, "none");
        assert_eq!(
            lines,
            [
                "lines of code (not counted: blank lines, comments, tests); share: of the total row",
                "",
                "                files  lines  share",
                "domain             10     70    70%",
                "  domain.rules      4     30    30%",
                "application         2     25    25%",
                "entry               1      5     5%",
                "total              13    100",
                "",
                "longest files:",
                "  domain",
                "    70  domain/a.py",
                "  application",
                "    25  application/a.py",
            ]
        );
    }

    #[test]
    fn a_project_with_nothing_to_measure_says_so_once() {
        assert_eq!(detail(&[], "nothing here"), ["nothing here"]);
        assert!(summary(&[]).is_empty());
    }
}
