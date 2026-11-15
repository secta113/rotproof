//! The command line: `rotproof init`, `rotproof create`, `rotproof check`, `rotproof approve` (which keeps a
//! forbidden import for a reason it is given), `rotproof guide`, `rotproof index`, `rotproof follows`, `rotproof new`
//! (which starts a knowledge document) and `rotproof log` (which adds an entry to the log), and the help that leads
//! from one to the next. Each command is a call of an input port of `protocol.inbound`; what it prints and how it
//! exits are decided here. A command line that names `stop-hook` or `time-hook`, the hooks Rotproof wrote into a
//! project's Claude Code settings before 1.0, says they are gone and exits 1; no command of that name is left.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{CommandFactory, FromArgMatches, Parser, Subcommand};
use protocol::inbound::{
    Commands, Initialized, LinkedTodo, LogEntry, Made, Moving, MovingFile, Names, NewDocument,
    Warning,
};

/// What every help says after the commands: how to start, where the rules are, and the exit codes. An agent with only
/// the binary reads its way from here to a checked project.
const START: &str = "\
Start a project:
  1. rotproof init --stack <stack>   write .config/rotproof.toml (python, typescript, rust, or none for records only),
     --ui <ui>                       with python and typescript: atomic for screens, none for no screens
  2. edit it: list in absent the layers the project does not have
  3. rotproof create                 list the layers it would make, writing nothing (with none, it makes docs/ and
                                     the project's files: go on to 5)
  4. rotproof create --yes           once absent lists every layer the project lacks: make the layers, docs/ and the
                                     project's files
  5. rotproof check                  check them; this is the project's CI

Upgrade Rotproof: change the pinned version, install it, run `rotproof init` (it updates the project's files), then
`rotproof check`.

The rules Rotproof keeps are in .rotproof/AGENTS.md once `rotproof create` has run; `rotproof guide --stack <stack>`
prints them before. `rotproof <command> --help` says what a command reads, writes and never does.

Exit codes: 0 when the rules are kept and the command did its work; 1 when `rotproof check` finds a rule broken; 2 when there is no
.config/rotproof.toml to work from, a file cannot be read or written, the command line of another command is wrong or
its --root is not a directory, `rotproof create` would make layers
without --yes, `rotproof init` leaves an update of the project's files to a person,
or `rotproof approve` is given no reason, or an import that is not forbidden.";

/// What `rotproof new` asks, for a document of knowledge: what the code cannot tell, which only a person or the task
/// can. The text is here and not in a file Rotproof writes, so that what an agent is told to ask is what the command it
/// ran printed.
const QUESTIONS: &str = "\
Write the body from what the person or the task has said, not from what the code does (the code says that). Ask for
each of these, answer in your own words, and leave out a question nobody can answer rather than a heading with nothing
under it:
  1. Why is it so, and not another way?
  2. What was considered and dropped, and why?
  3. How should it behave, where the code says only how it does?
  4. What outside the code does one need to know (what a service promises, the data in production, how to run or
     operate it)?";

/// What `rotproof new --alarm` asks: the two headings an alarm has, in the words they are searched by.
const ALARM_QUESTIONS: &str = "\
Write the two headings from what was seen, as it was seen, not in other words (whoever meets the message searches
docs/ for it):
  # When  What is seen, and where: the message, the exit code or the status as they were observed, and the job or the
          command whose output shows them. Say each wording when a version of a tool changes it.
  # Do    What to do then. Usually: open an issue for the work, and say what to look at first.";

/// The long help of `rotproof check`. `{markers}` becomes the words the marker check fails on, taken from the check:
/// written here, they would be a comment that holds them, and a list that could drift from the check.
const CHECK_HELP: &str = "\
Check the layers and the records, and exit non-zero when one breaks the rules

Checks that the tree matches .config/rotproof.toml (every layer present or declared absent, no code outside the \
layers, and in TypeScript no .tsx or .jsx file in presentation, which holds no view; in each unit, when the \
declaration has [[unit]], with no manifest outside the root of every unit), that each layer imports only \
what the layer table allows, without what is declared absent, or was approved in .config/rotproof-approved.toml (each approval printed on every run, one that matches nothing failing, and so more approvals than approved_max in the declaration allows, when it has one), that the manifest of each package \
(the pyproject.toml of a Python unit, the package.json of a TypeScript one, the Cargo.toml of a crate, whose [dependencies] no path of its code names) lists the dependencies its code imports (a package imported and not \
listed fails, and so does a runtime dependency no code imports; development dependencies are never judged unused, and \
in Python the code of a layer uses runtime dependencies only; the distribution an import is of and a dependency used without an \
import are said in [dependencies] of the declaration; nothing is installed, and what a .venv provides only words a \
message), that only \
infrastructure reads or writes the environment variables (os.environ and os.getenv in Python, process.env and \
import.meta.env in TypeScript, std::env::var in Rust; not in another layer or the entry file), that no \
comment holds {markers}, that no comment switches the type checker off (# type: ignore, # pyright: ignore and the \
like in Python, @ts-ignore, @ts-expect-error and @ts-nocheck in TypeScript), that the project's files are up to this version (files in the declaration; `rotproof init` \
updates them), that .rotproof/AGENTS.md and the two copies of the skill rotproof (.claude/skills/rotproof/SKILL.md and .agents/skills/rotproof/SKILL.md) are up to date, that .rotproof/readers.md lists who reads each shared unit as `rotproof index` \
would write it (there with a shared unit, not there without), that every knowledge document in docs/knowledge/ keeps its \
rules (docs/knowledge/rules.md), has one of the areas the declaration lists as its tag and matches the code it \
follows, that every link in them and in the log resolves, and that the log keeps its structure, with no day later \
than today. The rest of docs/ is the project's, and is not read. \
Prints every broken rule under the check that found it, and what was not checked and why. Warns of every knowledge \
document whose stale_after has come, on every run from that day, and fails nothing for it: time alone never turns \
the check red. Warns, too, of every comment that may make a Python type checker check less (# pyright: basic, \
# mypy: allow-untyped-defs), and fails nothing for it. Warns, too, of a layer the map in AGENTS.md has no row for, and \
of a row for a layer declared absent, and fails nothing for it: the file is the project's. With todo_link in the declaration (\"number\" or \"key\"), a \
comment that holds TODO(#123): what is left (\"number\") or TODO(ABC-123): what is left (\"key\"), on one line, with the \
number or key of the issue that tracks the work, is not a finding: every one is printed on every run under `linked \
TODO`, as `path:line` and the reference, and fails nothing. Whether the issue exists or is open is not checked. With \
paths in moving in the declaration (the project's code that is not in a layer yet, which no check reads), prints on \
every run how many files are left and which to move next, those that import no other file still moving first, each \
with the layers it could go in (where the direction check would pass it), and fails nothing for it. Writes nothing. Exits 1 when a rule is broken, 2 when there is no .config/rotproof.toml or a file \
cannot be read.";

/// Keeps a project's structure from drifting while LLMs and people change it: the layers (which part of the code may
/// import which) and the records that keep the code's reasons (knowledge pinned to the code, and the log, in docs/).
#[derive(Parser)]
#[command(name = "rotproof", version, about, after_help = START)]
struct Cli {
    /// The repository root; the records are in `<root>/docs`
    #[arg(long, default_value = ".")]
    root: PathBuf,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Write .config/rotproof.toml for a stack; run again after upgrading Rotproof, to update the project's files
    ///
    /// Without a declaration, writes only the declaration, with every field and what it means, so the layers the
    /// project does not want are listed in absent before anything is made; then edit it and run `rotproof create`.
    /// With one, it is the whole upgrade of Rotproof: it applies every update a newer Rotproof brings to the files
    /// `rotproof create` wrote once (such as a hook or a rule taken out of .claude/settings.json), except the ones the
    /// declaration lists in declined, does what `rotproof create` does after an upgrade without making a layer, and
    /// sets files in the declaration to this version, adding declined = [] when the declaration lacks it. It names
    /// every field it adds to the declaration, as `rotproof create` does, and never changes its other values. What an
    /// update cannot do without a person, it says, leaves files as it was, and exits 2. It never moves or rewrites a
    /// record. Crossing 0.4.0, which builds the layer table on the onion architecture, it says where the code of the
    /// layers that are gone (handler, utils) and the ports go, and moves no code: `rotproof check` names what is left
    /// until it is moved. Crossing 1.0.0-rc.3, which splits the UI into the view (ui) and the state of the screens
    /// (presentation) and declares the UI, the refresh adds ui to the declaration ("atomic" when ui/ is there, "none"
    /// otherwise), and it says to make presentation, what moves out of ui, and to take ui out of absent once ui is
    /// "none", and moves no code either. Exits 2 too for an unknown stack or UI, a stack or UI other than the declared
    /// one, or a file that cannot be read or written.
    Init {
        /// python, typescript, rust, or none (records only). Needed when there is no declaration yet
        #[arg(long)]
        stack: Option<String>,
        /// The UI, which picks the layers of the screens: atomic (the view in ui, by atomic design, and the state of
        /// the screens in presentation) or none (no screens). Needed with python and typescript when there is no
        /// declaration yet; rust offers only none
        #[arg(long)]
        ui: Option<String>,
    },
    /// Make the layers that .config/rotproof.toml declares and the tree lacks, and the records skeleton in docs/
    ///
    /// Makes only what is missing: each layer neither present nor declared absent, docs/knowledge/, docs/log.md, and
    /// the project's files
    /// (AGENTS.md, CLAUDE.md, README.md, .gitignore, .gitattributes,
    /// a CI workflow that installs the pinned Rotproof (from PyPI with requirements-dev.txt for python, from npm with
    /// the pin package.json holds for typescript, which the project adds, and from the GitHub Release for rust and
    /// none), and for rust the workspace's Cargo.toml and the entry file src/main.rs), each when it does not exist.
    /// Makes, too, each entry file that a [[unit]] declares and the tree lacks.
    /// Rewrites the files Rotproof generates: .rotproof/AGENTS.md (the rules it keeps), the skill rotproof (how to
    /// upgrade Rotproof and when to run each command; the same text in .claude/skills/rotproof/SKILL.md and
    /// .agents/skills/rotproof/SKILL.md), .rotproof/readers.md (who reads
    /// each shared unit, when a unit is shared), and docs/index.md and the
    /// index and the rules of docs/knowledge/. Adds the fields the declaration lacks, keeping its comments and values.
    /// Never overwrites another file, and never moves or deletes one, except that in a rust project it adds the
    /// crates it makes to the members of the workspace in Cargo.toml when none names them, and replaces the member
    /// `crates/*` with the crates it matched when it would match a directory that has no Cargo.toml, as a unit's
    /// does, and says each change. Run it when a project starts, and after editing
    /// the declaration; after upgrading Rotproof, `rotproof init` does what it does, without making a layer.
    /// Makes layers only with --yes: without it, a run that would make one lists them, writes nothing and exits 2, so
    /// the layers the project does not have are declared absent first. Exits 2 too when the declaration cannot be read
    /// or a file cannot be written.
    Create {
        /// Make the layers that are neither present nor declared absent
        #[arg(long)]
        yes: bool,
    },
    /// Check the layers and the records, and exit non-zero when one breaks the rules
    // Its long help is CHECK_HELP, which `command` puts in
    Check,
    /// Print the rules Rotproof keeps for a stack, the text of .rotproof/AGENTS.md, writing nothing
    ///
    /// Prints the guide of the stack and the UI the project declares, or of the stack named with --stack, which needs
    /// no project: the layers of the stack with where each lives and what it may import, the rules of the records and
    /// where the skill for running Rotproof is. The same text `rotproof create` writes to .rotproof/AGENTS.md. With
    /// --stack, --ui picks the UI, as for `rotproof init` (with python and typescript, atomic or none; the layers of
    /// the fullest UI the stack offers without it). Exits 2 for an unknown stack or UI, or when no stack is named and
    /// the declaration cannot be read.
    Guide {
        /// python, typescript, rust, or none (records only). Without it, the stack in .config/rotproof.toml
        #[arg(long)]
        stack: Option<String>,
        /// The UI of the layers to show, as for `rotproof init`: atomic or none with python and typescript. Needs
        /// --stack
        #[arg(long, requires = "stack")]
        ui: Option<String>,
    },
    /// Keep a forbidden import for a reason (--reason), or remove the approvals that match nothing (--prune)
    ///
    /// `rotproof approve <file> <import> --reason "<why>"` keeps an import the layer table forbids, as `rotproof check`
    /// names it: the file from the root, and the import as the check writes it (a Python dotted name, `m.n` for each
    /// name of `from m import n`; a TypeScript specifier; a crate's dependency). It shows the forbidden import, signs
    /// with git's user.name, and adds the entry, with the reason, to .config/rotproof-approved.toml, which keeps a byte
    /// order mark and its line endings (CRLF on every line when one line had it), as does --prune. It asks nothing and
    /// needs no terminal, but it takes no approval without a reason, and `rotproof check` prints every approval on
    /// every run. Run it only when the user asked for the exception, and say that you ran it.
    /// `rotproof approve --prune` removes every entry of .config/rotproof-approved.toml that matches no forbidden
    /// import, and never lets anything pass that failed before. Exits 2 when --reason is empty, git's user.name is not
    /// set, the import is not forbidden or is approved already, or a file cannot be read or written.
    Approve {
        /// The file that imports, from the root, as `rotproof check` names it
        #[arg(required_unless_present = "prune", conflicts_with = "prune")]
        file: Option<String>,
        /// What it imports, as `rotproof check` names it
        #[arg(required_unless_present = "prune", conflicts_with = "prune")]
        import: Option<String>,
        /// Why the import is kept: it is written in the entry and printed on every run
        #[arg(
            long,
            value_name = "WHY",
            required_unless_present = "prune",
            conflicts_with = "prune"
        )]
        reason: Option<String>,
        /// Remove the entries that match no forbidden import
        #[arg(long)]
        prune: bool,
    },
    /// Write docs/index.md and docs/knowledge/index.md from the frontmatter, and the rules.md Rotproof keeps
    ///
    /// Rewrites docs/index.md and docs/knowledge/index.md from the frontmatter of the knowledge documents, and
    /// docs/knowledge/rules.md, each only when it differs from what it would write, and names each it wrote (when none
    /// differs, it says so). Run it after a knowledge document changes; never edit these by hand. Writes nothing
    /// in docs/work/, which Rotproof no longer reads. Lists the documents it left out and why, also under `# Left out`
    /// in the index. In a project that declares a shared unit it writes, too, .rotproof/readers.md, the names the other
    /// units read of each shared unit with the units that read them (run it after the imports between units change),
    /// and takes it away once no unit is shared. Exits 2 when the declaration or a document cannot be read.
    Index,
    /// Pin keys in a knowledge document's follows, at the hash of the code as it is now
    ///
    /// Run it once the document is reviewed against the code: when it starts to follow something, or after what it
    /// follows changed. The document is named as rotproof check names it (knowledge/api.md) or from the root
    /// (docs/knowledge/api.md, or docs\knowledge\api.md). Each key is what follows names: a file from the root
    /// (src/api/routes.py), a definition of a Python, TypeScript, JavaScript or Rust file
    /// (src/api/routes.py::create_user, src/api.ts::Router.add, src/cli.rs::Command.Before: Name, or Name.Name; a Rust
    /// type covers its impl blocks), a table of a .toml file (.config/rotproof.toml::unit: every [[unit]] in it, and
    /// not the fields above them; a table in another is Name.Name, and a name holds the tables below it), or every file
    /// read as code under a directory (src/api/: .py, .rs and the TypeScript and JavaScript extensions). A name defined
    /// more than once in a file is hashed on all its pieces. A key in follows gets its new hash, and a key not in it is
    /// added at its end, with the hash quoted (follows is made at the end of the frontmatter when the document has
    /// none); nothing else in the document changes, and it keeps a byte order mark and its line endings (CRLF on every
    /// line when one line had it). Prints each key with what was done, then whether the document was written. Writes
    /// all the keys or none, and only the keys named. Needs no terminal: pinning one document is an edit of it, which
    /// git records. A key gone from the code is left to edit by hand. Exits 2, writing nothing, when the first
    /// argument is not a knowledge document (as before 0.4.0, when follows took the keys alone), the document is not
    /// there, cannot be read, is a guide or is deprecated, or a key is not a form follows takes, names what is not
    /// there, or cannot be read.
    Follows {
        /// The knowledge document: knowledge/<slug>.md, or docs/knowledge/<slug>.md
        document: String,
        /// The keys, as follows writes them
        #[arg(required = true)]
        keys: Vec<String>,
    },
    /// Start a knowledge document: write docs/knowledge/<slug>.md with its frontmatter, and print what to ask
    ///
    /// Writes the frontmatter of a new document, which is the part that is easy to write wrongly (`type`, a `title`
    /// made from the slug, the one area it is tagged with, `status: stable`), and nothing under it: the body is yours
    /// to write from what a person or the task says. The `description` is left empty, so `rotproof check` fails until
    /// it is written, and the document cannot be committed half done; change the title too when the slug says less
    /// than a title would. The slug is the file's name without .md (lower-case words and digits joined by hyphens,
    /// never index, log or rules), and it never changes once the file exists, as commit messages and other documents
    /// refer to it. --area names the area the document is tagged with, one of the areas of .config/rotproof.toml, and
    /// may be left out when it lists only one. Each --follows KEY, repeated for more, is a key as `rotproof follows`
    /// takes it (a file, a definition `file::Name.Name`, a table of a .toml file, or a directory), pinned at its hash
    /// now exactly as `rotproof follows` would pin it: run it once the document says what the code does, as the pin
    /// says it was reviewed; a key that is not there writes nothing. --alarm writes what an alarm needs: `# When` and
    /// `# Do`, empty so that the check fails until each is written, and --match STRING, repeated for more, the strings
    /// of `match` (each as it appears in what is seen; with none, the field is left out). Prints the file written, each
    /// key with its hash, and the questions to ask to write the body. It never writes over a document, and does not
    /// write the index: run `rotproof index` once the description is written. Exits 2, writing nothing, when the slug
    /// is not one, a document of that name is there already (by Windows' reading of names too), the area is not one the
    /// declaration lists or is left out when it lists more than one, docs/knowledge/ is not there, a key is not a form
    /// follows takes, names what is not there or cannot be read, or --match is given without --alarm or holds a string
    /// that cannot be written.
    New {
        /// The file's name in docs/knowledge/ without .md: lower-case words joined by hyphens
        slug: String,
        /// The area the document is tagged with. Needed when the declaration lists more than one
        #[arg(long)]
        area: Option<String>,
        /// A key the document follows, pinned at its hash now; repeat it for more
        #[arg(long = "follows", value_name = "KEY")]
        follows: Vec<String>,
        /// The document is an alarm: it gets `# When` and `# Do`
        #[arg(long)]
        alarm: bool,
        /// A string of an alarm's `match`, as it appears in what is seen; repeat it for more
        #[arg(long = "match", value_name = "STRING", requires = "alarm")]
        matches: Vec<String>,
    },
    /// Add an entry to docs/log.md: the task, its issue, what went wrong and what was learned
    ///
    /// Run it when a task is done, in place of writing the entry by hand: it reads today from the clock of the machine
    /// (the day there, in its own zone), puts the entry at the top under the heading `## YYYY-MM-DD` of that day, makes
    /// the heading in its place when the log has none (the days stay newest first), so that the log need not be read
    /// first to place the entry. The entry has the form the guide gives:
    /// `* **<task>**`, and under it `**Issue**`, `**Problems**` and `**Lessons**`. --title is the task; --issue, which
    /// may be left out when the task has none, is its issue as `owner/repo#n`. --problem and --lesson are each needed
    /// at least once, so that a field forgotten is told from one with nothing to say: write `--problem None` when
    /// nothing went wrong. Repeat either to give a list. A line break in a value becomes a space, so a value never
    /// opens a line of its own; a value that is empty or opens an HTML comment is refused. The log keeps its line
    /// endings and its byte order mark, and the rest of it is as it was. Exits 2, writing nothing, when a value is not
    /// as above or --problem or --lesson is not given, docs/log.md is not there, or the entry would break a rule the
    /// log kept (`rotproof check` finds the same).
    Log {
        /// The task
        #[arg(long)]
        title: String,
        /// The task's issue, as `owner/repo#n`; leave it out when there is none
        #[arg(long, value_name = "OWNER/REPO#N")]
        issue: Option<String>,
        /// What went wrong, the cause and the workaround; `None` when nothing did. Repeat it for a list
        #[arg(long = "problem", value_name = "TEXT", required = true)]
        problems: Vec<String>,
        /// What to do differently next time, and where it was recorded if it became a rule. Repeat it for a list
        #[arg(long = "lesson", value_name = "TEXT", required = true)]
        lessons: Vec<String>,
    },
}

/// The command line as clap reads it, with the long help of `rotproof check` naming the markers of the check.
fn command(names: &Names) -> clap::Command {
    Cli::command().mut_subcommand("check", |check| {
        check.long_about(CHECK_HELP.replace("{markers}", &names.markers))
    })
}

/// Run the command the command line names, with `commands` doing the work, and say how it went.
pub fn run(commands: &dyn Commands) -> ExitCode {
    let names = commands.names();
    let parsed = command(&names)
        .try_get_matches()
        .and_then(|matches| Cli::from_arg_matches(&matches));
    let cli = match parsed {
        Ok(cli) => cli,
        Err(e) => return wrong_command_line(&e, &names),
    };
    // A root that does not exist fails here. Read as an empty tree, it would pass every check with nothing checked
    if !cli.root.is_dir() {
        eprintln!("the root is not a directory: {}", cli.root.display());
        return ExitCode::from(2);
    }
    let root = cli.root.as_path();
    let result = match cli.command {
        Command::Init { stack, ui } => {
            init(commands, &names, root, stack.as_deref(), ui.as_deref()).map(|()| true)
        }
        Command::Create { yes } => create(commands, &names, root, yes).map(|()| true),
        Command::Check => check(commands, &names, root),
        Command::Guide { stack, ui } => {
            commands
                .guide(root, stack.as_deref(), ui.as_deref())
                .map(|text| {
                    print!("{text}");
                    true
                })
        }
        Command::Index => index(commands, root).map(|()| true),
        Command::Approve {
            file,
            import,
            reason,
            prune,
        } => match (prune, file, import, reason) {
            (true, _, _, _) => approve_prune(commands, &names, root).map(|()| true),
            (false, Some(file), Some(import), Some(reason)) => {
                approve(commands, root, &file, &import, &reason).map(|()| true)
            }
            _ => {
                unreachable!(
                    "clap requires the file and the import when --prune is not given, and no more"
                )
            }
        },
        Command::Follows { document, keys } => {
            follows(commands, root, &document, &keys).map(|()| true)
        }
        Command::New {
            slug,
            area,
            follows,
            alarm,
            matches,
        } => {
            let document = NewDocument {
                slug,
                area,
                follows,
                alarm,
                matches,
            };
            new_document(commands, root, &document).map(|()| true)
        }
        Command::Log {
            title,
            issue,
            problems,
            lessons,
        } => {
            let entry = LogEntry {
                title,
                issue,
                problems,
                lessons,
            };
            log(commands, root, &entry).map(|()| true)
        }
    };
    match result {
        Ok(true) => ExitCode::SUCCESS,
        // A broken rule: 1. A file that could not be read or written: 2
        Ok(false) => ExitCode::FAILURE,
        Err(why) => {
            eprintln!("{why}");
            ExitCode::from(2)
        }
    }
}

/// The hooks Rotproof wrote into a project's Claude Code settings, which a settings file may still run: the commands
/// that were removed, and are named here only to be told apart from a mistyped command.
const REMOVED_HOOKS: [&str; 2] = ["stop-hook", "time-hook"];

/// What to do about a removed hook, as the one line a command line that names it says.
const TAKE_IT_OUT: &str = "in a project, run `rotproof init`, which takes the hook out of .claude/settings.json; \
    elsewhere, remove the hook that runs it from your Claude Code settings";

/// What a command line that names a removed hook says, on stderr, once.
fn hook_removed(hook: &str) -> String {
    format!("rotproof {hook} was removed in Rotproof 1.0 and does nothing: {TAKE_IT_OUT}")
}

/// Say what is wrong with the command line, and exit. Exit 2 for a wrong command line, as clap does, and 0 for the
/// help and the version, which clap reports as errors too; but 1 when the command line names a removed hook
/// (`stop-hook` or `time-hook`, whatever else is wrong with it), with one line that says so in place of clap's: a
/// settings file that runs the binary by path keeps the hook through any upgrade, and Claude Code takes exit code 2
/// from a `Stop` hook as "send Claude back to work" (up to 8 times in a row) and from a `UserPromptSubmit` hook as
/// "block the prompt", where 1 is an error that is only shown. Stdin is not read: the hook input is not looked at, and
/// a hook that exits before its input is written is ordinary to Claude Code.
fn wrong_command_line(e: &clap::Error, names: &Names) -> ExitCode {
    if e.use_stderr()
        && let Some(hook) = names_a_hook(names)
    {
        eprintln!("{}", hook_removed(hook));
        return ExitCode::FAILURE;
    }
    e.exit()
}

/// The removed hook the command line names as its subcommand, though it did not parse: none of them is a command
/// now, so clap rejects it. clap is asked again with its errors ignored, so that it finds the subcommand as it always
/// does, past the global `--root` and its value, rather than a guess here at which word is the subcommand. When clap
/// finds none (a hook's name is not a command, and a flag it does not know before the subcommand stops it there), a
/// word that is a hook's name is enough: a hook set up with a mistyped flag must not exit 2 either, and any other
/// command line that holds the word fails with 1 instead of 2, which no hook is run by.
fn names_a_hook(names: &Names) -> Option<&'static str> {
    let found = command(names).ignore_errors(true).try_get_matches();
    match found
        .as_ref()
        .ok()
        .and_then(|matches| matches.subcommand_name())
    {
        Some(name) => REMOVED_HOOKS.into_iter().find(|hook| *hook == name),
        None => REMOVED_HOOKS
            .into_iter()
            .find(|hook| std::env::args_os().any(|arg| arg == *hook)),
    }
}

/// Write the declaration, or upgrade the project's files, and say what to do next.
fn init(
    commands: &dyn Commands,
    names: &Names,
    root: &Path,
    stack: Option<&str>,
    ui: Option<&str>,
) -> Result<(), String> {
    let Names {
        declaration,
        version,
        ..
    } = names;
    let upgraded = match commands.init(root, stack, ui)? {
        Initialized::Declared(path) => {
            println!("wrote {path}");
            if stack == Some(names.records_only) {
                println!("next: run `rotproof create` to make docs/");
            } else {
                println!(
                    "next: declare in absent the layers this project does not have, then run `rotproof create`, \
                     which lists the layers it would make, and `rotproof create --yes` to make them"
                );
            }
            return Ok(());
        }
        Initialized::Upgraded(upgraded) => upgraded,
    };
    for (name, path) in &upgraded.applied {
        println!("updated {path}: {name}");
    }
    for how in &upgraded.moves {
        println!("move by hand: {how}");
    }
    report_made(names, &upgraded.made);
    if let Some(path) = &upgraded.drafted {
        println!(
            "drafted {path}: the units as the code has them, to write over with the picture you intend; it follows \
             the units of {declaration}, so `rotproof check` fails when one changes until it is read again"
        );
    }
    if let Some(why) = &upgraded.not_drafted {
        println!("{why}");
    }
    // The map is the project's file, so it is told of and never written over
    for stale in &upgraded.stale_map {
        println!("note: {stale}");
    }
    if !upgraded.by_hand.is_empty() {
        for why in &upgraded.by_hand {
            println!("needs a person: {why}");
        }
        return Err(format!(
            "the project's files stay up to {} in {declaration} until these are done, or declined where the line \
             says how: then run `rotproof init` again",
            upgraded.from
        ));
    }
    if upgraded.files_set {
        println!(
            "the project's files are up to {version} (from {}): files = \"{version}\" in {declaration}",
            upgraded.from
        );
    } else {
        println!("the project's files are up to {version} already");
    }
    println!("next: run `rotproof check`");
    Ok(())
}

/// Say what `rotproof create`, or an upgrade (its refresh, and `files` set), wrote and added.
fn report_made(names: &Names, made: &Made) {
    for path in &made.written {
        println!("wrote {path}");
    }
    for path in &made.removed {
        println!("removed {path}");
    }
    for field in &made.added {
        println!("added to {}: {field}", names.declaration);
    }
    for (path, what) in &made.changed {
        println!("changed {path}: {what}");
    }
    if let Some(why) = &made.not_written {
        println!("not written: {why}");
    }
    for (name, why) in &made.left_out {
        println!("left out of the index, fix it: {name}: {why}");
    }
}

/// Make what is missing, and say what was written. A second run with nothing changed writes nothing.
fn create(commands: &dyn Commands, names: &Names, root: &Path, yes: bool) -> Result<(), String> {
    let made = commands.create(root, yes)?;
    report_made(names, &made);
    // A changed file is written too, and told of above
    let wrote = !made.written.is_empty() || !made.removed.is_empty() || !made.changed.is_empty();
    if !wrote {
        println!(
            "nothing to make: the tree has what {} declares",
            names.declaration
        );
    }
    if wrote {
        println!("next: run `rotproof check`; the rules it keeps are in .rotproof/AGENTS.md");
    }
    Ok(())
}

/// Print every broken rule under the check that found it. `true` when there are none.
fn check(commands: &dyn Commands, names: &Names, root: &Path) -> Result<bool, String> {
    let report = commands.check(root)?;
    if let Some(why) = &report.skipped {
        println!("{why}");
    }
    // An approval never hides: every one is printed, on every run, before what fails
    if !report.approved.is_empty() {
        println!(
            "approved in {} ({}):",
            names.approvals,
            report.approved.len()
        );
        for line in &report.approved {
            println!("  {line}");
        }
    }
    // A comment tied to an issue is listed apart, on every run, before the warnings and what fails, and fails nothing
    print_linked(&report.linked);
    // The code that is moving is counted on every run, and nothing fails for it
    if let Some(moving) = &report.moving {
        for line in moving_lines(moving, names.declaration) {
            println!("{line}");
        }
    }
    // A warning: printed on every run, under the heading of its kind, and failing nothing
    for (heading, details) in warnings(&report.warnings) {
        println!(
            "{heading} (a warning, which fails nothing) ({}):",
            details.len()
        );
        for detail in details {
            println!("  {}", detail.replace('\n', "\n    "));
        }
    }
    let found = report.findings;
    let mut last = "";
    for finding in &found {
        if finding.check != last {
            println!("{}:", finding.check);
            last = &finding.check;
        }
        // A detail of several lines (an example to write, the line a marker sits on) stays under its finding
        println!("  {}", finding.detail.replace('\n', "\n    "));
    }
    if found.is_empty() {
        if report.layers_checked {
            println!("the layers and every record keep the rules");
        } else {
            println!("every record keeps the rules");
        }
    }
    Ok(found.is_empty())
}

/// What `print_linked` prints for `linked`: the heading with the count, then a line for each, indented by two spaces,
/// with `path:line`, padded to the longest, two spaces and the reference. Nothing when there are none. A CI step reads
/// these lines (the specification says their form is kept): the first field of a line is the place, the last the
/// reference.
fn linked_lines(linked: &[LinkedTodo]) -> Vec<String> {
    if linked.is_empty() {
        return Vec::new();
    }
    let places: Vec<String> = linked
        .iter()
        .map(|todo| format!("{}:{}", todo.path, todo.line))
        .collect();
    let width = places
        .iter()
        .map(|place| place.chars().count())
        .max()
        .unwrap_or_default();
    let mut lines = vec![format!("linked TODO ({})", linked.len())];
    lines.extend(
        places
            .iter()
            .zip(linked)
            .map(|(place, todo)| format!("  {place:<width$}  {}", todo.reference)),
    );
    lines
}

/// What `check` prints for the code that is moving: how much there is and that none of it is checked, then the files
/// to move next, each group on a line of its own, and how many more wait for no other file. Nothing is said of a
/// date: the count falls when a file is moved.
fn moving_lines(moving: &Moving, declaration: &str) -> Vec<String> {
    let total = moving.total;
    if total == 0 {
        return vec![format!(
            "moving: no code is left in the paths `moving` lists in {declaration}: take them out"
        )];
    }
    let count = match total {
        1 => "1 file is".to_string(),
        _ => format!("{total} files are"),
    };
    let head = format!("moving: {count} not in a layer yet, and none is checked");
    if !moving.ordered {
        return vec![format!(
            "{head}. Rotproof does not read what a Rust file imports, so it names none to move first"
        )];
    }
    let mut lines = vec![format!(
        "{head}. Move these next (they import no other file still moving), each with the layers it could go in:"
    )];
    for group in &moving.next {
        match group.as_slice() {
            [file] => lines.push(format!("  {}", moving_file(file))),
            files => {
                let paths: Vec<&str> = files.iter().map(|file| file.path.as_str()).collect();
                lines.push(format!(
                    "  {} import each other, so move them together:",
                    paths.join(", ")
                ));
                lines.extend(
                    files
                        .iter()
                        .map(|file| format!("    {}", moving_file(file))),
                );
            }
        }
    }
    if moving.more > 0 {
        lines.push(format!(
            "  and {} more that import no other file still moving",
            moving.more
        ));
    }
    lines
}

/// A file to move with the layers it could go in: every one, or those named, or none.
fn moving_file(file: &MovingFile) -> String {
    let layers = match &file.candidates {
        None => "any layer".to_string(),
        Some(places) if places.is_empty() => {
            "no layer passes the direction check, for what it imports and what imports it"
                .to_string()
        }
        Some(places) => places.join(", "),
    };
    format!("{}: {layers}", file.path)
}

/// Print the comments tied to an issue under their heading, apart from the warnings and the findings.
fn print_linked(linked: &[LinkedTodo]) {
    for line in linked_lines(linked) {
        println!("{line}");
    }
}

/// The details of each kind of warning, under its heading, in the order the headings first come.
fn warnings(all: &[Warning]) -> Vec<(&str, Vec<&str>)> {
    let mut kinds: Vec<(&str, Vec<&str>)> = Vec::new();
    for warning in all {
        match kinds
            .iter_mut()
            .find(|(heading, _)| *heading == warning.check)
        {
            Some((_, details)) => details.push(&warning.detail),
            None => kinds.push((&warning.check, vec![&warning.detail])),
        }
    }
    kinds
}

/// Keep the forbidden import of `import` by `file` for `reason`, signed with git's `user.name`, and say what was
/// written. Nothing is asked: the reason is what the entry holds, and it is printed on every run of `rotproof check`.
fn approve(
    commands: &dyn Commands,
    root: &Path,
    file: &str,
    import: &str,
    reason: &str,
) -> Result<(), String> {
    let reason = reason.trim();
    if reason.is_empty() {
        return Err(format!(
            "--reason is empty: say why {import} in {file} is kept"
        ));
    }
    let pending = commands.pending(root, file, import)?;
    let by = signer(commands, root)?;
    println!("forbidden by the layer table:");
    for detail in &pending {
        println!("  {detail}");
    }
    let path = commands.approve(root, file, import, reason, &by)?;
    println!(
        "wrote {path}: {import} in {file} kept by {by}; `rotproof check` prints it on every run"
    );
    Ok(())
}

/// The name an approval is signed with: git's `user.name`.
fn signer(commands: &dyn Commands, root: &Path) -> Result<String, String> {
    commands.user_name(root).ok_or_else(|| {
        "git's user.name is not set, so the approval would carry no name: set it (git config user.name <name>) and \
         run the command again"
            .to_string()
    })
}

/// Remove the approvals that match no forbidden import, and say which.
fn approve_prune(commands: &dyn Commands, names: &Names, root: &Path) -> Result<(), String> {
    let removed = commands.prune(root)?;
    if removed.is_empty() {
        println!(
            "nothing to prune: every approval in {} matches a forbidden import",
            names.approvals
        );
    }
    for entry in &removed {
        println!(
            "removed from {}: {} in {}",
            names.approvals, entry.import, entry.from
        );
    }
    Ok(())
}

/// Pin the keys in the document, then say what was done to each.
fn follows(
    commands: &dyn Commands,
    root: &Path,
    document: &str,
    keys: &[String],
) -> Result<(), String> {
    let pinned = commands.pin(root, document, keys)?;
    for (key, before, now) in &pinned.keys {
        match before {
            None => println!("  {key}: added {now}"),
            Some(before) if before == now => println!("  {key}: unchanged"),
            Some(before) => println!("  {key}: {before} -> {now}"),
        }
    }
    if pinned.written {
        println!("wrote docs/knowledge/{}", pinned.doc);
    } else {
        println!(
            "nothing written: docs/knowledge/{} pins these keys as the code is now",
            pinned.doc
        );
    }
    Ok(())
}

/// Write the new document, say what was written, and print what to ask to write it.
fn new_document(
    commands: &dyn Commands,
    root: &Path,
    document: &NewDocument,
) -> Result<(), String> {
    let started = commands.start(root, document)?;
    println!("wrote {} (area: {})", started.path, started.area);
    for (key, hash) in &started.follows {
        println!("  follows {key}: {hash}");
    }
    println!();
    println!(
        "{}",
        if document.alarm {
            ALARM_QUESTIONS
        } else {
            QUESTIONS
        }
    );
    println!();
    println!(
        "next: write the description (one sentence: what the document tells), and the title if the slug says less, then \
         run `rotproof index`; `rotproof check` fails until the description is written{}",
        if document.alarm {
            ", and both headings"
        } else {
            ""
        }
    );
    Ok(())
}

/// Add the entry to the log, and say under which day it went.
fn log(commands: &dyn Commands, root: &Path, entry: &LogEntry) -> Result<(), String> {
    let logged = commands.log(root, entry)?;
    println!(
        "wrote {}: the entry is under ## {}",
        logged.path, logged.day
    );
    println!("next: run `rotproof check`");
    Ok(())
}

/// Write each index file that is out of date, or say that none is, then list what was left out of them.
fn index(commands: &dyn Commands, root: &Path) -> Result<(), String> {
    let indexed = commands.index(root)?;
    if indexed.written.is_empty() && indexed.removed.is_empty() {
        println!("nothing written: every index file is up to date");
    }
    for path in indexed.written {
        println!("wrote {path}");
    }
    for path in indexed.removed {
        println!("removed {path}");
    }
    for (name, why) in indexed.left_out {
        println!("left out of the index, fix it: {name}: {why}");
    }
    Ok(())
}
