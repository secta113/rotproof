//! `rotproof guide`: the guide for a stack, named or declared; and whether the map in the project's own `AGENTS.md`
//! still lists the layers the declaration has, which `rotproof check` and `rotproof init` say.

use crate::layers::declaration;
use crate::tree::{exactly, read_text};
use domain::declaration::{DECLARATION, Declared, unreadable};
use domain::guide::guide_named;
use domain::layers::known_stacks;
use domain::project::stale_map;
use protocol::outbound::Tree;

/// The guide `rotproof guide` prints: for `stack` when it is named, otherwise for the stack the project in `tree`
/// declares. `Err` when the stack is unknown, or none is named and the declaration cannot be read.
pub fn guide_for(tree: &dyn Tree, stack: Option<&str>) -> Result<String, String> {
    let stack = match stack {
        Some(stack) => stack.to_string(),
        None => {
            let stacks = known_stacks().join(", ");
            match declaration(tree).map_err(|e| e.to_string())? {
                Some(Ok(declaration)) => declaration.stack,
                None => {
                    return Err(format!(
                        "no {DECLARATION} here: name a stack with --stack ({stacks})"
                    ));
                }
                Some(Err(why)) => {
                    return Err(format!(
                        "{}; or name a stack with --stack ({stacks})",
                        unreadable(&why)
                    ));
                }
            }
        }
    };
    guide_named(&stack)
}

/// Where the project's own map is, from the root
const AGENTS: &str = "AGENTS.md";

/// What is out of date in the map of the project's `AGENTS.md` against the layers `declared` has (`domain::project`),
/// each as a line to say. Nothing when the file is not there, cannot be read, or has no map: it is the project's.
pub fn map_problems(tree: &dyn Tree, declared: &Declared) -> Vec<String> {
    if declared.layout.is_none() {
        return Vec::new();
    }
    let Some(text) = exactly(tree, AGENTS)
        .ok()
        .and_then(|(path, _)| read_text(tree, &path).ok())
    else {
        return Vec::new();
    };
    stale_map(declared, &text)
}
