//! Rotproof's skill, `rotproof`, which `rotproof create` writes to two places and `rotproof check` compares, as it
//! does the guide `.rotproof/AGENTS.md`. The guide holds the rules every session needs; the skill holds how to upgrade
//! Rotproof and when to run which command, which an agent needs only then, so it is read when the agent loads the
//! skill. It judges what the commands print and never says which command comes next: `rotproof check` and
//! `rotproof init` say that, and a second place that did would drift from them.
//!
//! The text is in `project/skill/`, built into the binary. It is the same for every project, whatever its stack, and
//! it carries no version: the project's `files` says which version wrote it, and a project upgrades through
//! `rotproof init` as for the guide.

/// The text of the skill, as it is written to each of its [`PATHS`].
pub const SKILL: &str = include_str!("../../../project/skill/SKILL.md.in");

/// The two places the skill is written, one copy in each: the places differ by agent, and a link between them cannot be
/// made on every system (Windows needs rights for it). `.claude/skills/` is where Claude Code looks; `.agents/skills/`
/// is where Codex and Antigravity look. Each directory is named as the skill is.
pub const PATHS: [&str; 2] = [
    ".claude/skills/rotproof/SKILL.md",
    ".agents/skills/rotproof/SKILL.md",
];

/// The name of the skill, which its directory has too.
pub const NAME: &str = "rotproof";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_skill_has_only_fields_of_the_specification_of_skills() {
        let rest = SKILL.strip_prefix("---\n").expect("frontmatter first");
        let (front, body) = rest.split_once("\n---\n").expect("frontmatter closed");
        let fields: Vec<&str> = front
            .lines()
            .filter(|line| !line.starts_with(' ') && line.contains(':'))
            .map(|line| line.split(':').next().unwrap())
            .collect();
        // Of the fields every agent that loads skills reads (name, description, license, compatibility, metadata and
        // allowed-tools), the two that are needed; any other field is one tool's own
        assert_eq!(fields, ["name", "description"]);
        assert!(front.contains(&format!("name: {NAME}\n")));
        let description = front
            .lines()
            .find_map(|line| line.strip_prefix("description: "))
            .unwrap();
        assert!(description.len() <= 1024, "{}", description.len());
        assert!(!body.contains("{version}") && !body.contains("{stack}"));
        assert!(!SKILL.contains('\r'));
    }

    #[test]
    fn each_copy_sits_in_a_directory_named_as_the_skill_is() {
        for path in PATHS {
            assert!(path.ends_with(&format!("/{NAME}/SKILL.md")), "{path}");
        }
        assert_ne!(PATHS[0], PATHS[1]);
    }

    #[test]
    fn the_skill_names_every_command_that_runs_in_a_session_and_none_that_is_gone() {
        for command in ["check", "init", "create", "index", "follows", "guide"] {
            assert!(SKILL.contains(&format!("`rotproof {command}")), "{command}");
        }
        for gone in ["stop-hook", "time-hook", "--reviewed"] {
            assert!(!SKILL.contains(gone), "{gone}");
        }
    }
}
