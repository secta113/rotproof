//! The approvals file, `.config/rotproof-approved.toml`, read from the tree and judged by the rules in `domain`.

use std::io;

use crate::tree::{Lookup, lookup, read_text};
use domain::approvals::{APPROVALS, Kept, parse, problems};
use domain::tree::Tree;

/// What the approvals file holds.
#[derive(Debug, Default)]
pub struct Approvals {
    /// Its text, with every line ending as `\n`: `None` when there is no file
    pub text: Option<String>,
    pub entries: Vec<Kept>,
    /// What is wrong with it: a name spelled otherwise, a file that is not TOML, an entry broken
    pub problems: Vec<String>,
}

/// The approvals of the project `tree` holds. An error is a file that is there and could not be read.
pub fn read(tree: &dyn Tree) -> io::Result<Approvals> {
    let path = match lookup(tree, "", APPROVALS) {
        Lookup::Found {
            path,
            is_dir: false,
        } => path,
        Lookup::Found { is_dir: true, .. } => {
            return Ok(Approvals {
                problems: vec![format!("{APPROVALS} is a directory")],
                ..Approvals::default()
            });
        }
        // An approvals file Linux would not find approves nothing there, so it approves nothing anywhere
        Lookup::Spelled(on_disk) => {
            return Ok(Approvals {
                problems: vec![format!(
                    "{on_disk} is not read: the approvals file is {APPROVALS}, by its exact name"
                )],
                ..Approvals::default()
            });
        }
        Lookup::Missing => return Ok(Approvals::default()),
    };
    let text = read_text(tree, &path)?;
    let (entries, problems) = match parse(&text) {
        Ok(entries) => {
            let found = problems(&entries);
            (entries, found)
        }
        Err(why) => (Vec::new(), vec![why]),
    };
    Ok(Approvals {
        text: Some(text),
        entries,
        problems,
    })
}
