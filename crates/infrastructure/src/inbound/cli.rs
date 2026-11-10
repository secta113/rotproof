//! The command line: `rotproof init`, `rotproof create`, `rotproof check`, `rotproof approve` (which alone asks a
//! person on the terminal), `rotproof guide`, `rotproof index`, `rotproof follows` and `rotproof time-hook`, and the
//! help that leads from one to the next. Each command is a call of an input port of `protocol.inbound`; what it
//! prints and how it exits are decided here. `rotproof before` and `rotproof stop-hook`, hidden, only say that they do
//! nothing since 1.0, and call no port.

use std::io::{IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{CommandFactory, FromArgMatches, Parser, Subcommand};
use protocol::inbound::{Commands, Initialized, Made, Names};

/// What every help says after the commands: how to start, where the rules are, and the exit codes. An agent with only
/// the binary reads its way from here to a checked project.
const START: &str = "\
Start a project:
  1. rotproof init --stack <stack>   write .config/rotproof.toml (python, typescript, rust, or none for records only)
  2. edit it: list in absent the layers the project does not have
  3. rotproof create --yes           make the layers, docs/ and the project's files (without --yes, a run that
                                     would make a layer lists the layers and writes nothing)
  4. rotproof check                  check them; this is the project's CI

Upgrade Rotproof: change the pinned version, install it, run `rotproof init` (it updates the project's files), then
`rotproof check`.

The rules Rotproof keeps are in .rotproof/AGENTS.md once `rotproof create` has run; `rotproof guide --stack <stack>`
prints them before. `rotproof <command> --help` says what a command reads, writes and never does.

Exit codes: 0 when the rules are kept and the command did its work; 1 when `rotproof check` finds a rule broken, or
`rotproof time-hook` fails (it reads no --root; a wrong command line that names a hook fails with 1 too); 2 when there is no
.config/rotproof.toml to work from, a file cannot be read or written, the command line of another command is wrong or
its --root is not a directory, `rotproof create` would make layers
without --yes, `rotproof init` leaves an update of the project's files to a person,
or `rotproof approve` is not run on a terminal or not answered y.";

/// The long help of `rotproof check`. `{markers}` becomes the words the marker check fails on, taken from the check:
/// written here, they would be a comment that holds them, and a list that could drift from the check.
const CHECK_HELP: &str = "\
Check the layers and the records, and exit non-zero when one breaks the rules

Checks that the tree matches .config/rotproof.toml (every layer present or declared absent, no code outside the \
layers), that each layer imports only what the layer table allows or a person approved in \
.config/rotproof-approved.toml (each approval printed on every run, and one that matches nothing failing), that no \
comment holds {markers}, that the project's files are up to this version (files in the declaration; `rotproof init` \
updates them), that .rotproof/AGENTS.md is up to date, that every knowledge document in docs/knowledge/ keeps its \
rules (docs/knowledge/rules.md), has one of the areas the declaration lists as its tag and matches the code it \
follows, that every link in them and in the log resolves, and that the log keeps its structure, with no day later \
than today. The rest of docs/ is the project's, and is not read. \
Prints every broken rule under the check that found it, and what was not checked and why. Writes nothing. Exits 1 when \
a rule is broken, 2 when there is no .config/rotproof.toml or a file cannot be read.";

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
    /// `rotproof create` wrote once (such as a rule added to .claude/settings.json), except the ones the declaration
    /// lists in declined, does what `rotproof create` does after an upgrade without making a layer, and sets files in
    /// the declaration to this version, adding declined = [] when the declaration lacks it. It names every field it
    /// adds to the declaration, as `rotproof create` does, and never changes its other values. What an update cannot do
    /// without a person, it says, leaves files as it was, and exits 2. It never moves or rewrites a record. Crossing
    /// 0.4.0, which builds the layer table on the onion architecture, it says where the code of the layers that are
    /// gone (handler, utils) and the ports go, and moves no code: `rotproof check` names what is left until it is
    /// moved. Exits 2 too for an unknown stack, a stack other than the declared one, or a file that cannot be read or
    /// written.
    Init {
        /// python, typescript, rust, or none (records only). Needed when there is no declaration yet
        #[arg(long)]
        stack: Option<String>,
    },
    /// Make the layers that .config/rotproof.toml declares and the tree lacks, and the records skeleton in docs/
    ///
    /// Makes only what is missing: each layer neither present nor declared absent, docs/knowledge/, docs/log.md, and
    /// the project's files
    /// (AGENTS.md, CLAUDE.md, README.md, .gitignore, .gitattributes, Claude Code's settings with the time hook and
    /// the rule that denies editing the approvals file,
    /// for python and none the pin of Rotproof and a CI workflow, and for rust the workspace's Cargo.toml and the
    /// entry file src/main.rs), each when it does not exist.
    /// Rewrites the files Rotproof generates: .rotproof/AGENTS.md (the rules it keeps), and docs/index.md and the
    /// index and the rules of docs/knowledge/. Adds the fields the declaration lacks, keeping its comments and values.
    /// Never overwrites another file, and never moves or deletes one. Run it when a project starts, and after editing
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
    /// Prints the guide of the stack the project declares, or of the stack named with --stack, which needs no project:
    /// how to run Rotproof, the layers of the stack with where each lives and what it may import, and the rules of the
    /// records. The same text `rotproof create` writes to .rotproof/AGENTS.md. Exits 2 for an unknown stack, or when
    /// no stack is named and the declaration cannot be read.
    Guide {
        /// python, typescript, rust, or none (records only). Without it, the stack in .config/rotproof.toml
        #[arg(long)]
        stack: Option<String>,
    },
    /// Approve on a terminal a forbidden import, or the knowledge documents whose followed code changed (--reviewed);
    /// or remove the approvals that match nothing (--prune)
    ///
    /// `rotproof approve <file> <import>` keeps an import the layer table forbids, as `rotproof check` names it: the
    /// file from the root, and the import as the check writes it (a Python dotted name, `m.n` for each name of
    /// `from m import n`; a TypeScript specifier; a crate's dependency). It shows the forbidden import, asks why it is
    /// kept and for a y, signs with git's user.name (or a name it asks for), and adds the entry to
    /// .config/rotproof-approved.toml, which keeps a byte order mark and its line endings (CRLF on every line when one
    /// line had it), as does --prune. `rotproof approve --reviewed`, after a refactoring that kept what the code does,
    /// lists every knowledge document whose followed code changed and what changed under it, says to read the changes
    /// first, asks for one y, writes the new hashes in their follows (each document keeps a byte order mark and its
    /// line endings, as with rotproof follows), and names each document it wrote; what is gone is left to edit by
    /// hand. Both run only when stdin is a terminal, so an agent's shell cannot approve: a person does. `rotproof
    /// approve --prune` removes every entry of .config/rotproof-approved.toml that matches no forbidden import, needs
    /// no terminal, and never lets anything pass that failed before. Exits 2 when stdin is not a terminal, the import
    /// is not forbidden or is approved already, the answer is not y, something followed is gone, or a file cannot be
    /// read or written.
    Approve {
        /// The file that imports, from the root, as `rotproof check` names it
        #[arg(
            required_unless_present_any = ["prune", "reviewed"],
            conflicts_with_all = ["prune", "reviewed"]
        )]
        file: Option<String>,
        /// What it imports, as `rotproof check` names it
        #[arg(
            required_unless_present_any = ["prune", "reviewed"],
            conflicts_with_all = ["prune", "reviewed"]
        )]
        import: Option<String>,
        /// Remove the entries that match no forbidden import
        #[arg(long, conflicts_with = "reviewed")]
        prune: bool,
        /// Re-pin, in one act on a terminal, every knowledge document whose followed code changed (after a
        /// refactoring that kept what the code does): each hash in follows set to what the code is now
        #[arg(long)]
        reviewed: bool,
    },
    /// Write docs/index.md and docs/knowledge/index.md from the frontmatter, and the rules.md Rotproof keeps
    ///
    /// Rewrites docs/index.md and docs/knowledge/index.md from the frontmatter of the knowledge documents, and
    /// docs/knowledge/rules.md. Run it after a knowledge document changes; never edit these by hand. Writes nothing
    /// in docs/work/, which Rotproof no longer reads. Lists the documents it left out and why, also under `# Left out`
    /// in the index. Exits 2 when the declaration or a document cannot be read.
    Index,
    /// Does nothing since 1.0, which no longer manages tasks: says so, and exits 2
    ///
    /// Before 1.0 it printed the open records of docs/work/ that close before a record. Rotproof no longer reads
    /// docs/work/: keep open work in issues, and run `rotproof init` to update .rotproof/AGENTS.md. Takes any
    /// arguments, as an old guide or an agent's memory may still pass them.
    #[command(hide = true)]
    Before {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true, hide = true)]
        _args: Vec<String>,
    },
    /// Pin keys in a knowledge document's follows, at the hash of the code as it is now
    ///
    /// Run it once the document is reviewed against the code: when it starts to follow something, or after what it
    /// follows changed. The document is named as rotproof check names it (knowledge/api.md) or from the root
    /// (docs/knowledge/api.md, or docs\knowledge\api.md). Each key is what follows names: a file from the root
    /// (src/api/routes.py), a definition of a Python file (src/api/routes.py::create_user, or Class.method), or every
    /// .py file under a directory (src/api/). A key in follows gets its new hash, and a key not in it is added at its
    /// end, with the hash quoted (follows is made at the end of the frontmatter when the document has none); nothing
    /// else in the document changes, and it keeps a byte order mark and its line endings (CRLF on every line when one
    /// line had it). Prints each key with what was done, then whether the document was written. Writes all the keys or
    /// none, and only the keys named: re-pinning every changed document in one act is `rotproof approve --reviewed`,
    /// which a person runs. Needs no terminal: pinning one document is an edit of it, which git records. A key gone
    /// from the code is left to edit by hand. Exits
    /// 2, writing nothing, when the first argument is not a knowledge document (as before 0.4.0, when follows took the
    /// keys alone), the document is not there, cannot be read, is a guide or is deprecated, or a key is not a form
    /// follows takes, names what is not there, or cannot be read.
    Follows {
        /// The knowledge document: knowledge/<slug>.md, or docs/knowledge/<slug>.md
        document: String,
        /// The keys, as follows writes them
        #[arg(required = true)]
        keys: Vec<String>,
    },
    /// Does nothing since 1.0: says so to the person when Claude Code runs it as the Stop hook Rotproof wrote before
    ///
    /// Before 1.0 it sent the agent back when its last message left something open and nothing in docs/ changed.
    /// `rotproof init` takes it out of the project's .claude/settings.json (the update claude-remove-stop-hook). Until
    /// then it reads the hook input on stdin, prints a systemMessage, which keeps no agent from stopping, and exits 0.
    /// A command line that is wrong exits 1, never 2, as Claude Code takes 2 for "do not stop".
    #[command(hide = true)]
    StopHook,
    /// Run as Claude Code's SessionStart and UserPromptSubmit hook: tell the agent the time now. Reads the hook input
    /// on stdin
    ///
    /// Not run by hand: `rotproof create` writes the .claude/settings.json that runs it. An agent has no clock, and a
    /// day it writes into docs/ from a guess runs ahead of the clock, which `rotproof check` fails on; this
    /// gives it the time on every prompt and at the start of every session, to write from. It reads no --root, so one
    /// that is not a directory changes nothing. Exits 1, never 2, when it fails (a command line that is wrong among the
    /// failures), so a broken hook never blocks the prompt.
    TimeHook,
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
    // The hooks answer before the root is checked below, whose exit code 2 Claude Code takes from a hook for "block":
    // neither reads a root
    if let Command::StopHook = cli.command {
        return stop_hook();
    }
    if let Command::TimeHook = cli.command {
        return time_hook(commands);
    }
    // Whatever its root: it reads none
    if let Command::Before { .. } = cli.command {
        eprintln!("{BEFORE_REMOVED}");
        return ExitCode::from(2);
    }
    // A root that does not exist fails here. Read as an empty tree, it would pass every check with nothing checked
    if !cli.root.is_dir() {
        eprintln!("the root is not a directory: {}", cli.root.display());
        return ExitCode::from(2);
    }
    let root = cli.root.as_path();
    let result = match cli.command {
        Command::Init { stack } => init(commands, &names, root, stack.as_deref()).map(|()| true),
        Command::Create { yes } => create(commands, &names, root, yes).map(|()| true),
        Command::Check => check(commands, &names, root),
        Command::Guide { stack } => commands.guide(root, stack.as_deref()).map(|text| {
            print!("{text}");
            true
        }),
        Command::Index => index(commands, root).map(|()| true),
        Command::Approve {
            file,
            import,
            prune,
            reviewed,
        } => match (prune, reviewed, file, import) {
            (true, _, _, _) => approve_prune(commands, &names, root).map(|()| true),
            (_, true, _, _) => approve_reviewed(commands, root).map(|()| true),
            (false, false, Some(file), Some(import)) => {
                approve(commands, &names, root, &file, &import).map(|()| true)
            }
            _ => {
                unreachable!("clap requires the file and the import without --prune or --reviewed")
            }
        },
        Command::Follows { document, keys } => {
            follows(commands, root, &document, &keys).map(|()| true)
        }
        Command::StopHook | Command::TimeHook | Command::Before { .. } => {
            unreachable!("answered above")
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

/// Say what is wrong with the command line, and exit. Exit 2 for a wrong command line, as clap does, and 0 for the
/// help and the version, which clap reports as errors too; but 1 when the command line names a hook (`stop-hook` or
/// `time-hook`, whatever else is wrong with it), as a hook exits 1, never 2: Claude Code takes exit code 2 from one
/// for "do not stop" or "block the prompt", and a hook set up wrongly would keep the agent or the prompt from going on.
fn wrong_command_line(e: &clap::Error, names: &Names) -> ExitCode {
    if e.use_stderr() && names_a_hook(names) {
        // `print` writes to stderr for an error that uses it, as `exit` does
        let _ = e.print();
        return ExitCode::FAILURE;
    }
    e.exit()
}

/// Whether the command line names `stop-hook` or `time-hook` as its subcommand, though it did not parse. clap is asked
/// again with its errors ignored, so that it finds the subcommand as it always does, past the global `--root` and its
/// value, rather than a guess here at which word is the subcommand. When clap finds none (a flag it does not know
/// before the subcommand stops it there), a word that is a hook's name is enough: a hook set up with a mistyped flag
/// must not exit 2 either, and any other command line that holds the word fails with 1 instead of 2, which no hook
/// is run by.
fn names_a_hook(names: &Names) -> bool {
    const HOOKS: [&str; 2] = ["stop-hook", "time-hook"];
    let found = command(names).ignore_errors(true).try_get_matches();
    match found
        .as_ref()
        .ok()
        .and_then(|matches| matches.subcommand_name())
    {
        Some(name) => HOOKS.contains(&name),
        None => std::env::args_os().any(|arg| HOOKS.iter().any(|hook| arg == *hook)),
    }
}

/// Write the declaration, or upgrade the project's files, and say what to do next.
fn init(
    commands: &dyn Commands,
    names: &Names,
    root: &Path,
    stack: Option<&str>,
) -> Result<(), String> {
    let Names {
        declaration,
        version,
        ..
    } = names;
    let upgraded = match commands.init(root, stack)? {
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
    for field in &made.added {
        println!("added to {}: {field}", names.declaration);
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
    if made.written.is_empty() {
        println!(
            "nothing to make: the tree has what {} declares",
            names.declaration
        );
    }
    if !made.written.is_empty() {
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
            "approved by a person in {} ({}):",
            names.approvals,
            report.approved.len()
        );
        for line in &report.approved {
            println!("  {line}");
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

/// What `rotproof stop-hook` says, as the JSON Claude Code reads from a hook: a `systemMessage`, shown to the person,
/// which keeps no agent from stopping. Its text holds no `"` or `\`, so it is JSON as written.
const STOP_HOOK_REMOVED: &str = "{\"systemMessage\": \"rotproof stop-hook does nothing since Rotproof 1.0, which \
    no longer manages tasks. In a project, run `rotproof init`, which takes this hook out of .claude/settings.json; \
    elsewhere, remove the Stop hook that runs rotproof stop-hook from your Claude Code settings.\"}";

/// Answer a stop hook Rotproof wrote before 1.0, which a project's settings still run: say that it does nothing now,
/// and exit 0. The input is read and not looked at, so Claude Code writes it whole; reading it fails nothing, and
/// --root is not read, as a hook that fails is shown with less than the message says.
fn stop_hook() -> ExitCode {
    let _ = std::io::stdin().read_to_string(&mut String::new());
    println!("{STOP_HOOK_REMOVED}");
    ExitCode::SUCCESS
}

/// Answer the agent's time hook. An error exits 1, never 2: Claude Code takes exit code 2 from it for "block the
/// prompt", and a broken hook would keep the person's prompt from the agent instead of being shown. It reads no root,
/// so --root, whatever it names, changes nothing here.
fn time_hook(commands: &dyn Commands) -> ExitCode {
    let mut input = String::new();
    let answer = std::io::stdin()
        .read_to_string(&mut input)
        .map_err(|e| format!("the hook input could not be read: {e}"))
        .and_then(|_| commands.time_hook(&input));
    match answer {
        Ok(out) => {
            println!("{out}");
            ExitCode::SUCCESS
        }
        Err(why) => {
            eprintln!("rotproof time-hook: {why}");
            ExitCode::FAILURE
        }
    }
}

/// Ask a person on the terminal to approve the forbidden import of `import` by `file`, and add the entry. Every answer
/// is read from stdin, which must be a terminal: an agent's shell tool has none.
fn approve(
    commands: &dyn Commands,
    names: &Names,
    root: &Path,
    file: &str,
    import: &str,
) -> Result<(), String> {
    if !std::io::stdin().is_terminal() {
        return Err(
            "rotproof approve asks a person, and stdin is not a terminal (an agent's shell has none): run it in your \
             own terminal"
                .into(),
        );
    }
    let pending = commands.pending(root, file, import)?;
    println!("forbidden by the layer table:");
    for detail in &pending {
        println!("  {detail}");
    }
    let by = match commands.user_name(root) {
        Some(name) => name,
        None => ask("git's user.name is not set; the name to sign with: ")?,
    };
    let reason = ask(&format!("why is {import} in {file} kept? "))?;
    let answer = ask(&format!(
        "approve it as {by}, in {}, printed on every run? [y/N] ",
        names.approvals
    ))?;
    if !answer.eq_ignore_ascii_case("y") {
        return Err("not approved: nothing written".into());
    }
    let path = commands.approve(root, file, import, &reason, &by)?;
    println!("wrote {path}");
    Ok(())
}

/// Print `question` and read one line of answer from stdin, trimmed. An empty answer is asked again; the end of the
/// input is an error.
fn ask(question: &str) -> Result<String, String> {
    loop {
        print!("{question}");
        std::io::stdout()
            .flush()
            .map_err(|e| format!("the question could not be shown: {e}"))?;
        let mut line = String::new();
        let read = std::io::stdin()
            .read_line(&mut line)
            .map_err(|e| format!("the answer could not be read: {e}"))?;
        if read == 0 {
            return Err("no answer: nothing written".into());
        }
        let line = line.trim();
        if !line.is_empty() {
            return Ok(line.to_string());
        }
    }
}

/// Ask a person on the terminal, once, to re-pin every knowledge document whose followed code changed, and write the
/// new hashes. What is gone, or cannot be read, is left for the person to edit.
fn approve_reviewed(commands: &dyn Commands, root: &Path) -> Result<(), String> {
    if !std::io::stdin().is_terminal() {
        return Err(
            "rotproof approve --reviewed asks a person, and stdin is not a terminal (an agent's shell has none): run \
             it in your own terminal, or review each document and pin it with the `rotproof follows` that `rotproof \
             check` names"
                .into(),
        );
    }
    let drifts = commands.changed(root)?;
    if drifts.is_empty() {
        println!("nothing to re-pin: every knowledge document matches the code it follows");
        return Ok(());
    }
    let mut left = Vec::new();
    println!("the code changed under these knowledge documents since they were last reviewed:");
    for d in &drifts {
        match &d.now {
            Ok(Some(now)) => println!(
                "  knowledge/{} follows {}: {} -> {now}",
                d.doc, d.key, d.pinned
            ),
            Ok(None) => left.push(format!(
                "knowledge/{} follows {}, which is not there",
                d.doc, d.key
            )),
            Err(why) => left.push(format!("knowledge/{} follows {}: {why}", d.doc, d.key)),
        }
    }
    let changed = drifts.len() - left.len();
    if changed > 0 {
        // A hash says only that the code changed; the refactoring may be committed already, so no diff is named
        println!(
            "the hashes do not say what changed: read the changes first (git diff, or the commits since each document \
             was last reviewed)"
        );
        let answer = ask(&format!(
            "re-pin these {changed} as reviewed, as if each document was read against its code? [y/N] "
        ))?;
        if !answer.eq_ignore_ascii_case("y") {
            return Err("not re-pinned: nothing written".into());
        }
        for doc in commands.repin(root, &drifts)? {
            println!("wrote docs/knowledge/{doc}");
        }
    }
    if !left.is_empty() {
        for why in &left {
            println!("needs a person: {why}");
        }
        return Err(
            "what is gone was not re-pinned: edit follows in those documents by hand".into(),
        );
    }
    Ok(())
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

/// Write every index file, then list what was left out of them.
fn index(commands: &dyn Commands, root: &Path) -> Result<(), String> {
    let indexed = commands.index(root)?;
    for path in indexed.written {
        println!("wrote {path}");
    }
    for (name, why) in indexed.left_out {
        println!("left out of the index, fix it: {name}: {why}");
    }
    Ok(())
}

/// What `rotproof before` says, on stderr, before it exits 2.
const BEFORE_REMOVED: &str = "rotproof before does nothing since Rotproof 1.0, which no longer manages tasks: keep \
    open work in issues. Run `rotproof init` to update .rotproof/AGENTS.md, which no longer tells you to run it.";
