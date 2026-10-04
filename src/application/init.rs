//! `rotproof init`: write a project's declaration, `.config/rotproof.toml`, once.
//!
//! It writes only the declaration, so the project declares the layers it does not have before `rotproof create` makes
//! anything. The declaration is the project's from then on: `rotproof init` never overwrites it.

use crate::domain::layers::{DECLARATION, declaration_text, known_stacks};
use crate::domain::tree::{Tree, Writer};

/// Write the declaration for `stack` into `tree` through `out`, and return its path. `Err` when the stack is unknown,
/// the declaration exists, or it cannot be written.
pub fn init(tree: &dyn Tree, out: &dyn Writer, stack: &str) -> Result<&'static str, String> {
    if !known_stacks().contains(&stack) {
        return Err(format!(
            "unknown stack {stack:?} (known: {})",
            known_stacks().join(", ")
        ));
    }
    if tree.found(DECLARATION).is_some() {
        return Err(format!(
            "{DECLARATION} exists, and rotproof init never overwrites it: edit it, then run `rotproof create`"
        ));
    }
    out.write(DECLARATION, &declaration_text(stack))
        .map_err(|e| format!("{DECLARATION}: {e}"))?;
    Ok(DECLARATION)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::tree::fake::Fake;

    #[test]
    fn the_declaration_is_written_once() {
        let tree = Fake::default();
        assert_eq!(init(&tree, &tree, "python"), Ok(DECLARATION));
        assert_eq!(tree.text(DECLARATION), Some(declaration_text("python")));
        assert!(init(&tree, &tree, "rust").unwrap_err().contains("exists"));
        assert_eq!(tree.text(DECLARATION), Some(declaration_text("python")));
        assert!(
            init(&Fake::default(), &Fake::default(), "cobol")
                .unwrap_err()
                .starts_with("unknown stack")
        );
    }
}
