//! `rotproof guide`: the guide for a stack, named or declared.

use crate::application::layers::declaration;
use domain::layers::{DECLARATION, known_stacks};
use domain::project::guide_named;
use domain::tree::Tree;

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
                        "{DECLARATION}: {why}; or name a stack with --stack ({stacks})"
                    ));
                }
            }
        }
    };
    guide_named(&stack)
}
