//! The list of who reads each shared unit, `.rotproof/readers.md` (`domain::readers`), read from the tree: written by
//! `rotproof index` and `rotproof create` from the imports the direction check reads, and compared by `rotproof
//! check`.

use crate::direction::problems as direction;
use crate::layers::read;
use crate::tree::{exactly, read_text};
use domain::declaration::Declared;
use domain::readers::{FILE, Read, changes, render};
use protocol::outbound::{Parsers, Tree, Writer};

/// What the list needed done.
#[derive(Debug, PartialEq, Eq)]
pub enum Change {
    /// The list was written: new, or rewritten with a change
    Written,
    /// The list was taken away, since no unit is shared any more
    Removed,
}

/// Whether the list in `tree` is not what Rotproof writes for `declared` and `reads`: the finding, which says what to
/// run, and under it what changed in the names when there is a list to compare with. It is the list that is out of
/// date when it is missing, or there with no unit shared.
pub fn problems(tree: &dyn Tree, declared: &Declared, reads: &[Read]) -> Vec<String> {
    let expected = render(&declared.units, reads);
    let found = exactly(tree, FILE)
        .ok()
        .map(|(path, _)| read_text(tree, &path).ok());
    match (&expected, &found) {
        (None, None) => Vec::new(),
        (Some(text), Some(Some(found))) if text == found => Vec::new(),
        _ => {
            let mut said = format!("out of date, run `rotproof index`: {FILE}");
            // What changed in the names, under the finding, when there is a list to compare with and a list to write
            if let (Some(_), Some(Some(written))) = (&expected, &found) {
                for change in changes(&declared.units, reads, written) {
                    said.push('\n');
                    said.push_str(&change);
                }
            }
            vec![said]
        }
    }
}

/// Write the list for the project in `tree`, or take it away when no unit is shared, so that `rotproof check` finds it
/// as Rotproof writes it. `None` when it is as it should be, and when the declaration cannot be read or says no
/// layers: the other checks say why the project cannot be read.
pub fn refresh(
    tree: &dyn Tree,
    parsers: &dyn Parsers,
    out: &dyn Writer,
) -> Result<Option<Change>, String> {
    let Ok(declaration) = read(tree).map_err(|e| e.to_string())? else {
        return Ok(None);
    };
    let Ok(declared) = Declared::new(declaration) else {
        return Ok(None);
    };
    if declared.layout.is_none() {
        return Ok(None);
    }
    let direction = direction(tree, parsers, &declared).map_err(|e| e.to_string())?;
    if problems(tree, &declared, &direction.reads).is_empty() {
        return Ok(None);
    }
    match render(&declared.units, &direction.reads) {
        Some(text) => {
            out.write(FILE, &text).map_err(|e| format!("{FILE}: {e}"))?;
            Ok(Some(Change::Written))
        }
        None => {
            out.remove(FILE).map_err(|e| format!("{FILE}: {e}"))?;
            Ok(Some(Change::Removed))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::fake::Fake;
    use domain::declaration::{Declared, parse_declaration};
    use infrastructure::outbound::readers::Readers;

    /// A Python project of three units in directories of their names: `core` (shared, or not as `shared` says),
    /// `billing` and `steps`.
    fn declaration(shared: bool) -> String {
        format!(
            "stack = \"python\"\nareas = []\nui = \"none\"\n\
             [[unit]]\nname = \"core\"\npath = \"core\"\nshared = {shared}\n\
             [[unit]]\nname = \"billing\"\npath = \"billing\"\n\
             [[unit]]\nname = \"steps\"\npath = \"steps\"\n"
        )
    }

    const FILES: [(&str, &str); 6] = [
        ("core/domain/score.py", ""),
        ("core/domain/rank.py", ""),
        (
            "billing/application/play.py",
            "from core.domain.score import count_star6\nimport core.domain.rank\nimport os\n",
        ),
        (
            "steps/application/run.py",
            "from core.domain import score\nfrom core.domain.score import count_star6, other\n",
        ),
        ("steps/domain/own.py", "from steps.application import run\n"),
        ("billing/domain/own.py", ""),
    ];

    fn read_declared(text: &str) -> Declared {
        Declared::new(parse_declaration(text).unwrap()).unwrap()
    }

    fn reads(tree: &Fake, declared: &Declared) -> Vec<Read> {
        direction(tree, &Readers, declared).unwrap().reads
    }

    #[test]
    fn what_each_unit_reads_of_a_shared_unit_is_the_dotted_name_it_imports() {
        let text = declaration(true);
        let declared = read_declared(&text);
        let tree = Fake::new(&FILES);
        let mut found: Vec<(String, String)> = reads(&tree, &declared)
            .into_iter()
            .map(|read| (read.name, read.by))
            .collect();
        found.sort();
        let said = |name: &str, by: &str| (name.to_string(), by.to_string());
        // `from m import n` is `m.n`, `import m.n` is `m.n`; the unit's own imports and the standard library are no
        // read
        assert_eq!(
            found,
            [
                said("core.domain.rank", "billing"),
                said("core.domain.score", "steps"),
                said("core.domain.score.count_star6", "billing"),
                said("core.domain.score.count_star6", "steps"),
                said("core.domain.score.other", "steps"),
            ]
        );
        // Nothing is read of a unit that is not shared
        let text = declaration(false);
        assert!(reads(&tree, &read_declared(&text)).is_empty());
    }

    #[test]
    fn the_list_is_written_once_and_read_back_as_it_is_and_check_says_when_it_is_not() {
        let text = declaration(true);
        let declared = read_declared(&text);
        let mut files = FILES.to_vec();
        files.push((".config/rotproof.toml", &text));
        let tree = Fake::new(&files);
        let found = reads(&tree, &declared);
        // Missing: out of date
        let said = vec!["out of date, run `rotproof index`: .rotproof/readers.md".to_string()];
        assert_eq!(problems(&tree, &declared, &found), said);
        // Written once: the same code gives the same text, and nothing more is written
        assert_eq!(
            refresh(&tree, &Readers, &tree).unwrap(),
            Some(Change::Written)
        );
        let first = tree.text(FILE).unwrap();
        assert!(
            first.contains("| `core.domain.score.count_star6` | billing, steps |"),
            "{first}"
        );
        // A name read by one unit is in a section of its own
        assert!(
            first.contains(
                "### Read by one unit\n\n| Name | Read by |\n|---|---|\n| `core.domain.rank` | billing |\n\
                 | `core.domain.score` | steps |\n| `core.domain.score.other` | steps |\n"
            ),
            "{first}"
        );
        assert_eq!(problems(&tree, &declared, &found), Vec::<String>::new());
        assert_eq!(refresh(&tree, &Readers, &tree).unwrap(), None);
        assert_eq!(tree.text(FILE).unwrap(), first);
        // Stale: a name is read that the list does not have
        tree.write("steps/application/more.py", "import core.domain.rank\n")
            .unwrap();
        let found = reads(&tree, &declared);
        // The finding says which name changed, under it
        assert_eq!(
            problems(&tree, &declared, &found),
            [format!(
                "{}\ncore: core.domain.rank is read by billing, steps (was billing)",
                said[0]
            )]
        );
        assert_eq!(
            refresh(&tree, &Readers, &tree).unwrap(),
            Some(Change::Written)
        );
        assert!(
            tree.text(FILE)
                .unwrap()
                .contains("| `core.domain.rank` | billing, steps |")
        );
        // The file is not what is written, however it differs: with no table of its own, every name is new in it
        tree.write(FILE, "edited\n").unwrap();
        let edited = problems(&tree, &declared, &found);
        assert_eq!(edited.len(), 1);
        assert!(edited[0].starts_with(&said[0]), "{edited:?}");
        assert!(
            edited[0].contains("\ncore: core.domain.rank is read now, by billing, steps\n"),
            "{edited:?}"
        );
    }

    #[test]
    fn a_project_with_no_shared_unit_has_no_list_and_one_that_is_there_is_out_of_date_and_taken_away()
     {
        let text = declaration(false);
        let declared = read_declared(&text);
        let mut files = FILES.to_vec();
        files.push((".config/rotproof.toml", &text));
        let tree = Fake::new(&files);
        let found = reads(&tree, &declared);
        assert_eq!(problems(&tree, &declared, &found), Vec::<String>::new());
        assert_eq!(refresh(&tree, &Readers, &tree).unwrap(), None);
        assert_eq!(tree.found(FILE), None);
        // A list left from when a unit was shared
        tree.write(FILE, "old\n").unwrap();
        assert_eq!(
            problems(&tree, &declared, &found),
            ["out of date, run `rotproof index`: .rotproof/readers.md"]
        );
        assert_eq!(
            refresh(&tree, &Readers, &tree).unwrap(),
            Some(Change::Removed)
        );
        assert_eq!(tree.found(FILE), None);
        assert_eq!(refresh(&tree, &Readers, &tree).unwrap(), None);
    }

    #[test]
    fn a_shared_unit_that_nothing_reads_says_so_and_a_declaration_that_cannot_be_read_writes_nothing()
     {
        let text = declaration(true);
        let tree = Fake::new(&[
            (".config/rotproof.toml", &text),
            ("core/domain/score.py", ""),
            ("billing/domain/own.py", ""),
        ]);
        assert_eq!(
            refresh(&tree, &Readers, &tree).unwrap(),
            Some(Change::Written)
        );
        assert!(
            tree.text(FILE)
                .unwrap()
                .ends_with("## core\n\n`core`\n\nRead by no unit.\n"),
            "{}",
            tree.text(FILE).unwrap()
        );
        let broken = Fake::new(&[(".config/rotproof.toml", "stack = ")]);
        assert_eq!(refresh(&broken, &Readers, &broken).unwrap(), None);
        assert_eq!(broken.found(FILE), None);
        let nothing = Fake::default();
        assert_eq!(refresh(&nothing, &Readers, &nothing).unwrap(), None);
    }
}
