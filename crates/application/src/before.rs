//! `rotproof before`: the open records that close before a record of `docs/work/`, earliest first.

use crate::bundle::Bundle;
use crate::layers::areas;
use domain::bundle::render_before;
use domain::schema::Time;
use protocol::inbound::Before;
use protocol::outbound::Tree;

/// What closes before `record`, a slug of `docs/work/` with or without `.md`, in the bundle in `tree`: the open
/// records, and the closed ones too when `closed`, with the milestones past their date at `now`. `Err` is a record that
/// is not there or is left out of the records, a declaration that cannot be read, or a file that cannot be read.
pub fn before(tree: &dyn Tree, record: &str, closed: bool, now: Time) -> Result<Before, String> {
    let areas = areas(tree).map_err(|e| e.to_string())??;
    let work = Bundle::new(tree, areas)
        .read_work()
        .map_err(|e| e.to_string())?;
    let name = if record.ends_with(".md") {
        record.to_string()
    } else {
        format!("{record}.md")
    };
    if let Some(why) = work.problems.get(&format!("work/{name}")) {
        return Err(format!(
            "{name} is left out of the records, fix it first: {why}"
        ));
    }
    let text = render_before(&work, &name, closed, now.date_naive())
        .ok_or_else(|| format!("names no record in docs/work/: {record}"))?;
    Ok(Before {
        text,
        left_out: work.problems.into_iter().collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::fake::Fake;

    const DECLARED: (&str, &str) = (
        ".config/rotproof.toml",
        "stack = \"none\"\nareas = [\"a\"]\n",
    );

    fn now() -> Time {
        chrono::DateTime::parse_from_rfc3339("2026-10-28T00:00:00+09:00").unwrap()
    }

    #[test]
    fn a_record_is_named_with_or_without_md() {
        let tree = Fake::new(&[
            DECLARED,
            (
                "docs/work/release.md",
                "---\ntype: Milestone\ntitle: Release\ndescription: D.\ntags: [a]\nstatus: stable\n---\n\n\
                 # Condition\n\nThe tag is pushed.\n",
            ),
        ]);
        for record in ["release", "release.md"] {
            let before = before(&tree, record, false, now()).unwrap();
            assert_eq!(
                before.text,
                "release.md: Release | nothing open closes before it\n"
            );
            assert!(before.left_out.is_empty());
        }
    }

    #[test]
    fn a_record_that_is_not_there_or_left_out_fails() {
        let tree = Fake::new(&[DECLARED, ("docs/work/broken.md", "# no frontmatter\n")]);
        let why = before(&tree, "nowhere", false, now()).unwrap_err();
        assert_eq!(why, "names no record in docs/work/: nowhere");
        let why = before(&tree, "broken", false, now()).unwrap_err();
        assert!(
            why.starts_with("broken.md is left out of the records, fix it first: "),
            "{why}"
        );
    }
}
