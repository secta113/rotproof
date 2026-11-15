//! The draft of the knowledge document of the units, `docs/knowledge/units.md`, which `rotproof init` writes once for a
//! project that declares units and has no document that follows them.
//!
//! The document is the intended picture of the units, written by a person or with an agent's help: what each is
//! responsible for, why the code is split this way, why each edge between them exists, and which edges are left out on
//! purpose. It follows the `[[unit]]` tables of the declaration, so `rotproof check` fails when a unit changes until
//! the picture is read against it again. What the draft holds is what the code says now, as a start to write over: the
//! units as nodes (a shared one and an app marked), an edge for each import found from one unit to another, and a
//! section for each unit with its responsibility left to write.

use std::collections::BTreeSet;

use crate::declaration::DECLARATION;
use crate::direction::Landing;
use crate::layers::Place;
use crate::units::Unit;

/// Where the draft is written, from the root
pub const FILE: &str = "docs/knowledge/units.md";

/// The document as `rotproof check` and `rotproof follows` name it
pub const NAME: &str = "knowledge/units.md";

/// The name of its file in `docs/knowledge/`
pub const DOCUMENT: &str = "units.md";

/// What the document follows: every `[[unit]]` table of the declaration, and not its other fields
pub const KEY: &str = ".config/rotproof.toml::unit";

/// What the draft says under each unit, until a person writes it
const TO_WRITE: &str = "_Not written yet: what this unit is responsible for, why the code is split this way, why \
                        each of its edges exists, and which edges are left out on purpose._";

/// Words Mermaid's flowchart reads as its own where it expects a node's name, which a unit of that name is drawn
/// without
const RESERVED: [&str; 11] = [
    "call",
    "class",
    "click",
    "default",
    "direction",
    "end",
    "flowchart",
    "graph",
    "href",
    "style",
    "subgraph",
];

/// One import from a unit to another, by their names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    /// The unit that imports
    pub from: String,
    /// The unit imported
    pub to: String,
}

/// The import by a file that sits in `from` of what lands in `to`, when `to` is in another unit. It lands in a place of
/// the unit or in its code outside its layers: either is an edge between the two, whether or not the rules allow it.
pub fn link<'a>(from: &Place, to: impl Into<Landing<'a>>) -> Option<Link> {
    let to = to.into().unit_name().to_string();
    (from.unit.name != to).then(|| Link {
        from: from.unit.name.clone(),
        to,
    })
}

/// The text of the draft for `units` and the imports `links` found between them, tagged with `area` and pinned at
/// `hash`, the hash of what [`KEY`] names now (`rotproof follows` writes it the same way). A link that names a unit
/// that is not in `units` is left out, and so is a link repeated.
pub fn draft(units: &[Unit], links: &[Link], area: &str, hash: &str) -> String {
    let ids: Vec<String> = units
        .iter()
        .map(|unit| node_id(&unit.name, units))
        .collect();
    let index = |name: &str| units.iter().position(|unit| unit.name == name);
    let edges: BTreeSet<(usize, usize)> = links
        .iter()
        .filter_map(|link| Some((index(&link.from)?, index(&link.to)?)))
        .filter(|(from, to)| from != to)
        .collect();
    let mut text = format!(
        "---\n\
         type: Knowledge\n\
         title: Units\n\
         description: The units of the code as intended, with what each is for, why the code is split into them and \
         why each edge between them exists.\n\
         tags: [{}]\n\
         status: stable\n\
         follows:\n\
         \x20 {KEY}: \"{hash}\"\n\
         ---\n\
         \n\
         # Units\n\
         \n\
         Drafted by `rotproof init` from the units that `{DECLARATION}` declares and from the imports between them as \
         the code has them now: write over it with the picture you intend. An edge below is an import found, not a \
         decision, so one that should not be there is a defect to fix, and one that stays is to be explained. The \
         document follows `{KEY}`, every `[[unit]]` table of the declaration, so `rotproof check` fails when one \
         changes, until the document is read against it again and pinned with `rotproof follows {NAME} {KEY}`.\n\
         \n\
         An arrow runs from the unit that imports to the unit it imports. A unit that is shared, which any unit may \
         import, and an app, which has an entry, are marked.\n\
         \n\
         ```mermaid\n\
         flowchart LR\n",
        quoted(area)
    );
    for (unit, id) in units.iter().zip(&ids) {
        let node = match (unit.shared, unit.entries.is_empty()) {
            (true, _) => format!("{id}([\"{} (shared)\"])", unit.name),
            (false, false) => format!("{id}[\"{} (app)\"]", unit.name),
            (false, true) => format!("{id}[\"{}\"]", unit.name),
        };
        text.push_str(&format!("    {node}\n"));
    }
    for (from, to) in edges {
        text.push_str(&format!("    {} --> {}\n", ids[from], ids[to]));
    }
    text.push_str("```\n");
    for unit in units {
        text.push_str(&format!("\n## {}\n\n{TO_WRITE}\n", unit.name));
    }
    text
}

/// The name of the node for the unit `name`: the name itself, unless Mermaid reads it as its own word or another unit
/// has the name that would be made, then the name with `_` added until it is neither.
fn node_id(name: &str, units: &[Unit]) -> String {
    let mut id = name.to_string();
    while RESERVED.contains(&id.as_str())
        || (id != name && units.iter().any(|unit| unit.name == id))
    {
        id.push('_');
    }
    id
}

/// `value` as a YAML double-quoted scalar, so that whatever an area holds reads back as it is.
fn quoted(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::follows::{added, followed};
    use crate::schema::{KnowledgeDoc, Status, knowledge_doc};

    fn unit(name: &str, shared: bool, entry: bool) -> Unit {
        Unit {
            name: name.into(),
            path: name.into(),
            root: name.into(),
            shared,
            entries: if entry {
                vec![format!("{name}/main.py")]
            } else {
                Vec::new()
            },
            absent: Vec::new(),
            declared: true,
        }
    }

    fn link(from: &str, to: &str) -> Link {
        Link {
            from: from.into(),
            to: to.into(),
        }
    }

    fn world() -> Vec<Unit> {
        vec![
            unit("core", true, false),
            unit("billing", false, false),
            unit("cli", false, true),
        ]
    }

    #[test]
    fn the_key_is_the_unit_tables_of_the_declaration() {
        assert_eq!(KEY, format!("{DECLARATION}::unit"));
        assert!(followed(KEY).is_ok());
        assert_eq!(FILE, format!("docs/{NAME}"));
        assert_eq!(NAME, format!("knowledge/{DOCUMENT}"));
    }

    #[test]
    fn the_draft_is_a_knowledge_document_in_the_area_that_follows_the_units_at_the_hash() {
        let text = draft(&world(), &[], "design", "0a1b2c3d");
        let Ok(KnowledgeDoc::Knowledge(doc, sections)) = knowledge_doc(&text) else {
            panic!("not a knowledge document: {text}");
        };
        assert_eq!(doc.tag, "design");
        assert_eq!(doc.status, Status::Stable);
        assert_eq!(doc.follows, [(KEY.to_string(), "0a1b2c3d".to_string())]);
        assert!(sections.contains_key("Units"));
        // The pin is written the way `rotproof follows` writes it
        let without = text.replacen(&format!("follows:\n  {KEY}: \"0a1b2c3d\"\n"), "", 1);
        assert_ne!(without, text);
        assert_eq!(added(&without, &[(KEY, "0a1b2c3d")]).unwrap(), text);
    }

    #[test]
    fn an_area_that_yaml_would_read_otherwise_reads_back_as_it_is() {
        for area in [
            "design",
            "a: b",
            "x # y",
            "say \"hi\"",
            "back\\slash",
            "[a]",
            "12",
            "true",
        ] {
            let text = draft(&world(), &[], area, "00000000");
            let Ok(KnowledgeDoc::Knowledge(doc, _)) = knowledge_doc(&text) else {
                panic!("{area}: {text}");
            };
            assert_eq!(doc.tag, area);
        }
    }

    #[test]
    fn the_units_are_nodes_marked_by_what_they_are_and_the_imports_are_edges_once_each() {
        let links = [
            link("cli", "billing"),
            link("billing", "core"),
            link("cli", "core"),
            link("billing", "core"),
            // Itself, and a unit that is not declared, are no edge
            link("core", "core"),
            link("core", "gone"),
            link("gone", "core"),
        ];
        let text = draft(&world(), &links, "design", "00000000");
        let diagram: Vec<&str> = text
            .split("```mermaid\n")
            .nth(1)
            .unwrap()
            .split("```")
            .next()
            .unwrap()
            .lines()
            .collect();
        assert_eq!(
            diagram,
            [
                "flowchart LR",
                "    core([\"core (shared)\"])",
                "    billing[\"billing\"]",
                "    cli[\"cli (app)\"]",
                // In the order the units are declared, by the unit that imports and then the unit imported
                "    billing --> core",
                "    cli --> core",
                "    cli --> billing",
            ]
        );
    }

    #[test]
    fn each_unit_has_a_section_with_its_responsibility_to_write_in_the_order_it_is_declared() {
        let text = draft(&world(), &[], "design", "00000000");
        let headings: Vec<&str> = text.lines().filter(|l| l.starts_with("## ")).collect();
        assert_eq!(headings, ["## core", "## billing", "## cli"]);
        assert_eq!(text.matches(TO_WRITE).count(), 3);
    }

    #[test]
    fn a_unit_named_like_a_word_of_mermaid_is_a_node_of_another_name() {
        let units = [unit("end", false, false), unit("end_", false, false)];
        let text = draft(&units, &[link("end_", "end")], "design", "00000000");
        // `end_` is a unit of its own, so the node of `end` is not named that
        assert!(
            text.contains("    end__[\"end\"]\n    end_[\"end_\"]\n"),
            "{text}"
        );
        assert!(text.contains("    end_ --> end__\n"), "{text}");
    }
}
