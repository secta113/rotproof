//! Rotproof's guide, `.rotproof/AGENTS.md`, which every `rotproof create` rewrites and `rotproof check` compares, as it
//! does the generated files in `docs/`. It says the rules Rotproof keeps in the project's stack: how to run Rotproof,
//! the layers (their table is written from `layers/table.toml`, so the two cannot disagree) and the records. It names
//! the version that wrote it, so an upgrade fails `rotproof check` until `rotproof init` has run.
//!
//! The texts are in `project/rotproof/`, built into the binary. A rule that says something of its own in the section on
//! the layers keeps its text beside it, and gives it for a language (`environment::guide`, `suppressions::guide`): this
//! module puts the section together, and knows no rule but the layers'.

use crate::layers::{
    Language, Layout, RECORDS_ONLY, fullest_ui, importable, known_stacks, layout, listed, table,
};

/// Rotproof's guide, from the root
pub const GUIDE: &str = ".rotproof/AGENTS.md";

const GUIDE_TEXT: &str = include_str!("../../../project/rotproof/AGENTS.md.in");
const LAYERS_TEXT: &str = include_str!("../../../project/rotproof/layers.md.in");
const UI_TEXT: &str = include_str!("../../../project/rotproof/ui.md.in");
/// How the direction is read: the imports of a source file, or the dependencies of a crate
const DIRECTION_IMPORTS: &str = include_str!("../../../project/rotproof/direction-imports.md.in");
const DIRECTION_CARGO: &str = include_str!("../../../project/rotproof/direction-cargo.md.in");

/// The guide for the stack named `stack`. `Err` when the stack is unknown.
pub fn guide_named(stack: &str) -> Result<String, String> {
    if stack == RECORDS_ONLY {
        return Ok(guide(stack, None));
    }
    match layout(stack, &fullest_ui(stack)) {
        Some(layout) => Ok(guide(stack, Some(&layout))),
        None => Err(format!(
            "unknown stack {stack:?} (known: {})",
            known_stacks().join(", ")
        )),
    }
}

/// The guide for a project of `stack`, whose layout is `layout` (`None` for a repository of records only).
pub fn guide(stack: &str, layout: Option<&Layout>) -> String {
    let (kept, checked, layers) = match layout {
        None => (
            format!(
                "Rotproof keeps the records in `docs/` in this project. It has no layers: its stack is \"{stack}\"."
            ),
            "the records",
            String::new(),
        ),
        Some(layout) => (
            "Rotproof keeps two things in this project: the layers (where code lives and what it may import) and the\n\
             records in `docs/`."
                .to_string(),
            "the layers and the records",
            layers_section(layout),
        ),
    };
    GUIDE_TEXT
        .replace("{version}", env!("CARGO_PKG_VERSION"))
        .replace("{stack}", stack)
        .replace("{kept}", &kept)
        .replace("{checked}", checked)
        .replace("{layers}", &layers)
}

/// The section on the layers: the rules, the table of where each layer lives in this stack and what it may import, and
/// what each rule of the code says in this stack's language.
fn layers_section(layout: &Layout) -> String {
    let direction = match layout.language {
        Language::Python | Language::TypeScript => DIRECTION_IMPORTS,
        Language::Rust => DIRECTION_CARGO,
    };
    let rules = [
        crate::environment::guide(layout.language),
        crate::suppressions::guide(layout.language).to_string(),
    ]
    .concat();
    LAYERS_TEXT
        .replace("{direction}\n", direction)
        .replace("{rules}", &rules)
        .replace("{not_layers}", &not_layers(layout))
        .replace("{table}", &rows(layout))
        .replace("{ui}", &ui(layout))
}

/// The table of the layers: each place of the layout, where it lives and what it may import, and the entry file.
fn rows(layout: &Layout) -> String {
    let mut rows = vec![
        "| Layer | Where | May import |".to_string(),
        "|---|---|---|".to_string(),
    ];
    let places = layout.places(&table());
    for place in &places {
        let importable = importable(place);
        let has_levels = places.iter().any(|p| p.is_level_of(place));
        let imports = if has_levels {
            "Nothing: it holds only its levels".to_string()
        } else if importable.is_empty() {
            "No other layer".to_string()
        } else {
            capitalized(&importable)
        };
        rows.push(format!(
            "| `{}` | `{}/` | {imports} |",
            place.name, place.path
        ));
    }
    if let Some(entry) = layout.entry(&table()) {
        rows.push(format!(
            "| The entry file, no layer | `{}` | {} |",
            entry.path,
            listed(&entry.imports)
        ));
    }
    format!("{}\n", rows.join("\n"))
}

/// `text` with its first letter in upper case.
fn capitalized(text: &str) -> String {
    let mut chars = text.chars();
    chars
        .next()
        .into_iter()
        .flat_map(char::to_uppercase)
        .chain(chars)
        .collect()
}

/// The paths of the layout that are no layer, on a line of their own in the list item; nothing when there are none.
fn not_layers(layout: &Layout) -> String {
    if layout.not_layers.is_empty() {
        return String::new();
    }
    // Paths from the root, as `unchecked` writes them
    format!(
        "\n  {} {}, and may hold code.",
        listed(&layout.not_layers),
        if layout.not_layers.len() == 1 {
            "is not a layer"
        } else {
            "are not layers"
        }
    )
}

/// The rules of the screens, when the layout has `ui`: atomic design, and where a view may not go.
fn ui(layout: &Layout) -> String {
    let has = |name: &String| !layout.leaves_out(name);
    let mut ui = if !layout.leaves_out("ui") {
        UI_TEXT
    } else {
        ""
    }
    .to_string();
    let no_view = layout.no_view.as_ref().filter(|n| n.layers.iter().any(has));
    if let Some(no_view) = no_view {
        let names: Vec<String> = no_view.layers.iter().filter(|n| has(n)).cloned().collect();
        let files: Vec<String> = no_view
            .extensions
            .iter()
            .map(|e| format!("`.{e}`"))
            .collect();
        ui.push_str(&format!(
            "- **{} holds no view, and `rotproof check` fails on a {} file there.** Rotproof tells a view by the \
             extension alone, so a view written without JSX (`createElement` in a `.ts` file) passes: do not write \
             one. To fix a finding, {}.\n",
            listed(&names),
            files.join(" or "),
            no_view.goes
        ));
    }
    ui
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layers::{NO_UI, STACKS, uis};

    #[test]
    fn every_stack_has_a_guide_with_every_placeholder_filled() {
        let mut guides: Vec<String> = STACKS
            .iter()
            .flat_map(|(stack, _)| {
                uis(stack)
                    .unwrap()
                    .into_iter()
                    .map(move |ui| guide(stack, layout(stack, &ui).as_ref()))
            })
            .collect();
        guides.push(guide(RECORDS_ONLY, None));
        for text in &guides {
            for placeholder in [
                "{version}",
                "{stack}",
                "{kept}",
                "{checked}",
                "{layers}",
                "{not_layers}",
                "{table}",
                "{ui}",
                "{direction}",
                "{rules}",
                "{reads}",
            ] {
                assert!(
                    !text.contains(placeholder),
                    "{placeholder} left in:\n{text}"
                );
            }
            assert!(text.contains(env!("CARGO_PKG_VERSION")));
        }
    }

    #[test]
    fn each_rule_of_the_code_is_said_in_the_language_of_the_stack() {
        let text = |stack: &str| guide(stack, layout(stack, NO_UI).as_ref());
        let (python, typescript, rust) = (text("python"), text("typescript"), text("rust"));
        assert!(python.contains("`os.environ`") && python.contains("# type: ignore"));
        assert!(typescript.contains("`process.env`") && typescript.contains("@ts-ignore"));
        // Rust's compiler has no comment that switches it off
        assert!(rust.contains("`std::env::var`") && !rust.contains("switch the type checker off"));
    }

    #[test]
    fn the_guide_says_where_a_view_is_not_allowed_only_in_the_stack_that_checks_it() {
        let said = "`presentation` holds no view, and `rotproof check` fails on a `.tsx` or `.jsx` file there";
        let ts = |ui: &str| guide("typescript", layout("typescript", ui).as_ref());
        assert!(ts("atomic").contains(said));
        assert!(ts("atomic").contains("move the component into src/ui"));
        assert!(!ts(NO_UI).contains("holds no view"));
        assert!(!guide("python", layout("python", "atomic").as_ref()).contains("holds no view"));
        assert!(!guide("rust", layout("rust", NO_UI).as_ref()).contains("holds no view"));
    }

    #[test]
    fn the_table_lists_every_place_of_the_layout() {
        let every = STACKS.iter().flat_map(|(stack, _)| {
            uis(stack)
                .unwrap()
                .into_iter()
                .map(move |ui| (*stack, layout(stack, &ui).unwrap()))
        });
        for (stack, layout) in every {
            let text = guide(stack, Some(&layout));
            let places = layout.places(&table());
            assert!(!places.is_empty());
            for place in places {
                let row = format!("| `{}` | `{}/` |", place.name, place.path);
                assert!(text.contains(&row), "{stack}: no row {row}");
            }
            assert_eq!(
                text.contains("atomic design"),
                !layout.leaves_out("ui"),
                "{stack}: the rule on ui goes with the layer"
            );
        }
    }

    #[test]
    fn the_table_says_what_each_place_may_import_as_its_documentation_does() {
        let layout = layout("python", "atomic").unwrap();
        let text = guide("python", Some(&layout));
        for row in [
            "| `ui` | `ui/` | Nothing: it holds only its levels |",
            "| `ui.pages` | `ui/pages/` | The levels below it, and `presentation.models`, `presentation.display` and \
             `protocol.inbound` |",
            "| `ui.organisms` | `ui/organisms/` | The levels below it, and `presentation.display` |",
            "| `ui.molecules` | `ui/molecules/` | The levels below it |",
            "| `ui.atoms` | `ui/atoms/` | No other layer |",
            "| `presentation` | `presentation/` | Nothing: it holds only its levels |",
            "| `presentation.models` | `presentation/models/` | The levels below it, and `protocol.inbound` |",
            "| `presentation.display` | `presentation/display/` | No other layer |",
            "| `infrastructure` | `infrastructure/` | Nothing: it holds only its levels |",
            "| `infrastructure.inbound` | `infrastructure/inbound/` | `protocol.inbound` |",
            "| `protocol.outbound` | `protocol/outbound/` | `domain` |",
            "| `domain` | `domain/` | No other layer |",
            "| The entry file, no layer | `__main__.py` | `composition` |",
        ] {
            assert!(text.contains(row), "no row {row}:\n{text}");
        }
    }

    #[test]
    fn a_repository_of_records_only_has_no_section_on_layers() {
        let text = guide(RECORDS_ONLY, None);
        assert!(!text.contains("## The layers"));
        assert!(text.contains("## The records"));
    }
}
