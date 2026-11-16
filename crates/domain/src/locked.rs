//! A Python project is locked: it lists its dependencies in its `pyproject.toml`, commits a lock file, and keeps its
//! virtual environment where the tools make it, in `.venv` beside that lock file.
//!
//! Whether a machine runs Python in a virtual environment is not in the tree (a `.venv` is ignored, and CI starts from
//! nothing), so the rule asks for what the tree shows: the manifest, the lock file, and the environments that are on
//! disk. A package installed globally lets an import pass that no manifest lists, and nobody sees it until a clean
//! machine runs the code; a lock file makes every machine install the same packages.
//!
//! The rules are on values read by `application`: the lock files of the tree and the directories that hold a
//! `pyvenv.cfg`, which names a virtual environment whatever its directory is called.

use crate::paths::join;

/// The heading the findings are printed under.
pub const HEADING: &str = "a Python project lists its dependencies in pyproject.toml, commits a lock file, and keeps \
                           its virtual environment in .venv beside it";

/// The only name of a virtual environment: what uv, PDM and Poetry (set to keep it in the project) make by default.
pub const VENV: &str = ".venv";

/// The file that makes a directory a virtual environment, as `venv` and `virtualenv` and uv write it.
pub const CONFIG: &str = "pyvenv.cfg";

/// The lock files, as a message names them.
pub const LOCK_NAMES: &str = "uv.lock, poetry.lock, pdm.lock or pylock.toml";

/// The tool a project locks with, which decides how its CI installs what the lock holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    /// uv: `uv.lock`
    Uv,
    /// Poetry: `poetry.lock`
    Poetry,
    /// PDM: `pdm.lock`
    Pdm,
    /// pip, which installs a `pylock.toml` (PEP 751)
    Pip,
}

/// The tools in the order that decides which one a project with more than one lock file has: the one the CI is written
/// for.
pub const TOOLS: [Tool; 4] = [Tool::Uv, Tool::Poetry, Tool::Pdm, Tool::Pip];

impl Tool {
    /// The tool whose lock file is named `name`: `uv.lock`, `poetry.lock`, `pdm.lock`, `pylock.toml` and
    /// `pylock.<name>.toml` (PEP 751), compared exactly.
    pub fn of_lock(name: &str) -> Option<Tool> {
        match name {
            "uv.lock" => Some(Tool::Uv),
            "poetry.lock" => Some(Tool::Poetry),
            "pdm.lock" => Some(Tool::Pdm),
            "pylock.toml" => Some(Tool::Pip),
            _ => {
                let middle = name.strip_prefix("pylock.")?.strip_suffix(".toml")?;
                (!middle.is_empty() && !middle.contains('.')).then_some(Tool::Pip)
            }
        }
    }

    /// The lock file the tool writes by default.
    pub fn lock(self) -> &'static str {
        match self {
            Tool::Uv => "uv.lock",
            Tool::Poetry => "poetry.lock",
            Tool::Pdm => "pdm.lock",
            Tool::Pip => "pylock.toml",
        }
    }
}

/// The lock a project has, which its CI installs from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Locked {
    pub tool: Tool,
    /// The name of the lock file: the tool's own, or the `pylock.<name>.toml` the project has
    pub file: String,
}

impl Locked {
    /// The lock of a project whose files in its root are `names`: the first of [`TOOLS`] one of them is the lock of,
    /// and uv's when none is, as the CI of a project that has no lock file yet is written for the tool that makes one
    /// with the least to install.
    pub fn of_names(names: &[String]) -> Locked {
        let found: Vec<(Tool, &str)> = names
            .iter()
            .filter_map(|name| Some((Tool::of_lock(name)?, name.as_str())))
            .collect();
        let tool = TOOLS
            .into_iter()
            .find(|tool| found.iter().any(|(of, _)| of == tool))
            .unwrap_or(Tool::Uv);
        let file = found
            .iter()
            .find(|(of, _)| *of == tool)
            .map_or(tool.lock(), |(_, name)| name);
        Locked {
            tool,
            file: file.to_string(),
        }
    }
}

/// Whether a file named `name` is a lock file.
pub fn is_lock(name: &str) -> bool {
    Tool::of_lock(name).is_some()
}

/// The finding for a directory whose manifest, `pyproject.toml` (`manifest`), is not there. `dir` is where it is
/// looked for, "" for the root.
pub fn no_manifest(dir: &str, manifest: &str) -> String {
    format!(
        "{} is missing: a Python project lists its dependencies in {manifest} (a requirements file does not count): \
         write them under dependencies in [project], and the development ones, Rotproof among them, in dev of \
         [dependency-groups]",
        join(dir, manifest)
    )
}

/// The finding for a manifest in `dir` with no lock file in the directory or above it, which `names` the files that
/// are one.
pub fn no_lock(dir: &str, manifest: &str) -> String {
    let at = if dir.is_empty() {
        "the root".to_string()
    } else {
        dir.to_string()
    };
    format!(
        "{} has no lock file in {at} or above it: commit one ({LOCK_NAMES}), made by the tool from {manifest}, so \
         that every machine installs the same packages (a lock file a .gitignore excludes is not committed)",
        join(dir, manifest)
    )
}

/// What `application` read of one virtual environment: the directory `path` that holds a `pyvenv.cfg`, whose name is
/// the last part of the path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Environment {
    pub path: String,
    /// Whether a lock file is in the directory that holds it, and not only above it
    pub beside_lock: bool,
    /// Whether the `.gitignore` files of the project exclude it, so that it cannot be committed
    pub ignored: bool,
}

/// The finding for a virtual environment that is not allowed, `None` for one that is: named `.venv`, beside a lock
/// file, and ignored. The first rule it breaks is the one said.
pub fn environment(found: &Environment) -> Option<String> {
    let path = &found.path;
    let name = path
        .rsplit_once('/')
        .map_or(path.as_str(), |(_, name)| name);
    if name != VENV {
        return Some(format!(
            "{path}/ is a virtual environment (it holds {CONFIG}), and the virtual environment of a Rotproof project \
             is named {VENV} alone, as uv, PDM and Poetry name it: delete {path}/ and use {VENV}"
        ));
    }
    if !found.beside_lock {
        return Some(format!(
            "{path}/ is a virtual environment in a directory with no lock file ({LOCK_NAMES}): a {VENV} sits beside \
             the lock file of the project it installs, and a workspace with one lock file at its root has one {VENV} \
             there, so delete {path}/"
        ));
    }
    if !found.ignored {
        return Some(format!(
            "{path}/ is a virtual environment that no .gitignore excludes, so it can be committed: write {VENV}/ in \
             the .gitignore"
        ));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_lock_file_is_named_by_one_of_the_tools_exactly() {
        assert_eq!(Tool::of_lock("uv.lock"), Some(Tool::Uv));
        assert_eq!(Tool::of_lock("poetry.lock"), Some(Tool::Poetry));
        assert_eq!(Tool::of_lock("pdm.lock"), Some(Tool::Pdm));
        assert_eq!(Tool::of_lock("pylock.toml"), Some(Tool::Pip));
        // PEP 751: `pylock.<name>.toml`, the name with no dot
        assert_eq!(Tool::of_lock("pylock.dev.toml"), Some(Tool::Pip));
        for not in [
            "Uv.lock",
            "uv.lock.bak",
            "pylock..toml",
            "pylock.a.b.toml",
            "pylock.dev",
            "requirements.txt",
            "Pipfile.lock",
            "package-lock.json",
            "Cargo.lock",
        ] {
            assert_eq!(Tool::of_lock(not), None, "{not}");
            assert!(!is_lock(not));
        }
    }

    #[test]
    fn the_tool_of_a_project_is_the_first_one_a_lock_file_names_and_uv_when_none_is() {
        let of = |names: &[&str]| {
            let names: Vec<String> = names.iter().map(|name| name.to_string()).collect();
            let locked = Locked::of_names(&names);
            (locked.tool, locked.file)
        };
        assert_eq!(
            of(&["a.py", "poetry.lock"]),
            (Tool::Poetry, "poetry.lock".to_string())
        );
        assert_eq!(
            of(&["pylock.toml", "pdm.lock"]),
            (Tool::Pdm, "pdm.lock".to_string())
        );
        assert_eq!(of(&["pdm.lock", "uv.lock"]).0, Tool::Uv);
        assert_eq!(of(&["requirements.txt"]), (Tool::Uv, "uv.lock".to_string()));
        assert_eq!(of(&[]), (Tool::Uv, "uv.lock".to_string()));
        // The file is the one the project has
        assert_eq!(of(&["pylock.dev.toml"]).1, "pylock.dev.toml");
    }

    #[test]
    fn a_virtual_environment_is_allowed_only_as_the_dot_venv_beside_a_lock_file_that_is_ignored() {
        let said = |path: &str, beside_lock, ignored| {
            environment(&Environment {
                path: path.to_string(),
                beside_lock,
                ignored,
            })
        };
        assert_eq!(said(".venv", true, true), None);
        assert_eq!(said("packages/api/.venv", true, true), None);
        let renamed = said("venv", true, true).unwrap();
        assert!(
            renamed.starts_with("venv/ is a virtual environment (it holds pyvenv.cfg)")
                && renamed.contains("named .venv alone"),
            "{renamed}"
        );
        // The name comes first, whatever else is wrong
        assert!(
            said("a/env", false, false)
                .unwrap()
                .starts_with("a/env/ is a virtual environment (it holds")
        );
        let alone = said("packages/api/.venv", false, true).unwrap();
        assert!(
            alone.starts_with(
                "packages/api/.venv/ is a virtual environment in a directory with no lock file"
            ),
            "{alone}"
        );
        let committed = said(".venv", true, false).unwrap();
        assert!(
            committed.starts_with(".venv/ is a virtual environment that no .gitignore excludes"),
            "{committed}"
        );
    }

    #[test]
    fn the_findings_of_a_missing_manifest_and_a_missing_lock_name_the_file_and_where_it_is_looked_for()
     {
        assert!(no_manifest("", "pyproject.toml").starts_with("pyproject.toml is missing: "));
        assert!(
            no_manifest("api", "pyproject.toml").starts_with("api/pyproject.toml is missing: ")
        );
        assert!(no_lock("", "pyproject.toml").starts_with(
            "pyproject.toml has no lock file in the root or above it: commit one (uv.lock"
        ));
        assert!(
            no_lock("api", "pyproject.toml")
                .starts_with("api/pyproject.toml has no lock file in api or above it")
        );
    }
}
