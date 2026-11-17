//! Whether the project points its agent at Rotproof's guide: the one question Rotproof asks of a project, and how it
//! tells that the question was answered.
//!
//! - **Rotproof never writes where an agent reads by itself.** Its text is in `.rotproof/`, and the line that sends an
//!   agent there goes in a file the project owns (its `AGENTS.md`, its `CLAUDE.md`, or whatever its agent reads
//!   first). Whether to add it is the person's decision: what an agent is told belongs to the person, and a line
//!   Rotproof added would change what the agent obeys whenever Rotproof is upgraded.
//! - **The question stays until it is answered, either way.** It is answered when the root's `AGENTS.md` or
//!   `CLAUDE.md` names the guide's path, in any words, or when `declined` names [`NAME`]. Until then `rotproof create`
//!   and `rotproof init` ask it and exit 2, and `rotproof check` fails; a project that declined is never asked again,
//!   and its agent then knows only the rules `rotproof check` enforces.
//! - **Which of the files the agent reads, and how a file brings in another, is not looked at.** That is the agent's
//!   own, and changes with its version; the agent that adds the line knows which file it reads.

use crate::declaration::DECLARATION;
use crate::guide::GUIDE;

/// The name a project declines the question by, in `declined`
pub const NAME: &str = "guide-pointer";

/// The files at the root that are read for the answer, in the order they are named
pub const FILES: [&str; 2] = ["AGENTS.md", "CLAUDE.md"];

/// The line to add, as a sentence with the path bare: an agent that brings files in by `@path` (Claude Code) brings the
/// guide in, and one that does not reads the path as where to look.
pub fn line() -> String {
    format!("The rules Rotproof keeps: @{GUIDE}")
}

/// Whether the question was answered: one of `texts` (the files of [`FILES`] that are there) names the guide's path,
/// or `declined` names [`NAME`].
pub fn answered(texts: &[&str], declined: &[String]) -> bool {
    declined.iter().any(|name| name == NAME) || texts.iter().any(|text| text.contains(GUIDE))
}

/// The question, as the lines `rotproof create` and `rotproof init` print, `present` being the files of [`FILES`] that
/// are there. Written so that a person who reads it can answer it by hand too.
pub fn question(present: &[&str]) -> Vec<String> {
    let files = if present.is_empty() {
        format!(
            "there is no {}: ask the person which file to make",
            FILES.join(" or ")
        )
    } else {
        format!("here: {}", present.join(", "))
    };
    vec![
        format!(
            "ask the person: should this project's agent read the rules Rotproof keeps, in {GUIDE}? The person decides; \
             an agent asks before it adds the line or declines"
        ),
        format!("  yes: add this line to the file the agent reads first ({files}):"),
        format!("    {}", line()),
        format!("  no: add \"{NAME}\" to declined in {DECLARATION}"),
    ]
}

/// What `rotproof check` says while the question is unanswered, as one line.
pub fn unanswered() -> String {
    format!(
        "no {} names {GUIDE}, and declined in {DECLARATION} does not name \"{NAME}\": ask the person whether to add \
         \"{}\" to the file the agent reads first, or to decline it; `rotproof create` asks it in full",
        FILES.join(" or "),
        line()
    )
}

/// The heading `rotproof check` says it under
pub const HEADING: &str = "the project answered whether its agent reads Rotproof's guide";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_that_names_the_guide_in_any_words_or_a_decline_answers_it() {
        assert!(!answered(&[], &[]));
        assert!(!answered(&["# Mine\n\nNo pointer.\n"], &[]));
        assert!(answered(&["@AGENTS.md\n@.rotproof/AGENTS.md\n"], &[]));
        assert!(answered(&["Read `.rotproof/AGENTS.md` too.\n"], &[]));
        assert!(answered(&[&line()], &[]));
        assert!(answered(&[], &[NAME.to_string()]));
        // Another name declined is no answer
        assert!(!answered(&[], &["claude-remove-stop-hook".to_string()]));
    }

    #[test]
    fn the_line_names_the_guide_bare_and_answers_the_question() {
        assert!(!line().contains('`'));
        assert!(line().ends_with(&format!(" @{GUIDE}")));
        assert!(answered(&[&line()], &[]));
    }

    #[test]
    fn the_question_names_the_files_there_or_asks_which_to_make() {
        let asked = question(&["AGENTS.md"]).join("\n");
        assert!(asked.contains("(here: AGENTS.md)"), "{asked}");
        assert!(asked.contains(&line()) && asked.contains(&format!("\"{NAME}\"")));
        let asked = question(&[]).join("\n");
        assert!(
            asked.contains("there is no AGENTS.md or CLAUDE.md: ask the person which file to make"),
            "{asked}"
        );
    }
}
