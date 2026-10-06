//! Keeps a project's structure: makes and checks its layers (as `.config/rotproof.toml` declares them) and the records
//! an agent works from (backlog, specs and log in `docs/`, written in OKF 0.2), and writes their index files.

use std::io::{IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use application::create::Made;
use application::init::Initialized;
use chrono::{Local, SecondsFormat};
use clap::{CommandFactory, FromArgMatches, Parser, Subcommand};
use domain::approvals::{APPROVALS, Approved, Kept};
use domain::hook::Changes;
use domain::layers::{DECLARATION, RECORDS_ONLY};
use domain::markers::{MARKERS, either};
use domain::upgrade::VERSION;
use infrastructure::disk::{Disk, project_root};
use infrastructure::git::Git;
use infrastructure::readers::Readers;

/// What every help says after the commands: how to start, where the rules are, and the exit codes. An agent with only
/// the binary reads its way from here to a checked project.
const START: &str = "\
Start a project:
  1. rotproof init --stack <stack>   write .config/rotproof.toml (python, typescript, rust, or none for records only)
  2. edit it: list in absent the layers the project does not have
  3. rotproof create --yes           make the layers, docs/ and the project's files (without --yes, it lists the
                                     layers it would make and writes nothing)
  4. rotproof check                  check them; this is the project's CI

Upgrade Rotproof: change the pinned version, install it, run `rotproof init` (it updates the project's files), then
`rotproof check`.

The rules Rotproof keeps are in .rotproof/AGENTS.md once `rotproof create` has run; `rotproof guide --stack <stack>`
prints them before. `rotproof <command> --help` says what a command reads, writes and never does.

Exit codes: 0 when the rules are kept and the command did its work; 1 when `rotproof check` finds a rule broken; 2
when a file cannot be read or written, the command line is wrong, `rotproof create` would make layers without --yes,
`rotproof init` leaves an update of the project's files to a person, or `rotproof approve` is not run on a terminal
or not answered y.";

/// The long help of `rotproof check`. `{markers}` becomes the words the marker check fails on, taken from the check:
/// written here, they would be a comment that holds them, and a list that could drift from the check.
const CHECK_HELP: &str = "\
Check the layers and the records, and exit non-zero when one breaks the rules

Checks that the tree matches .config/rotproof.toml (every layer present or declared absent, no code outside the \
layers), that each layer imports only what the layer table allows or a person approved in \
.config/rotproof-approved.toml (each approval printed on every run, and one that matches nothing failing), that no \
comment holds {markers}, that the project's files are up to this version (files in the declaration; `rotproof init` \
updates them), that \
.rotproof/AGENTS.md is up to date, and that every record in docs/ keeps its rules (the rules.md of each directory), \
a knowledge document matching the code it follows among them. \
Prints every broken rule under the check that found it, and what was not checked and why. Writes nothing. Exits 1 when \
a rule is broken, 2 when a file cannot be read.";

/// Keeps a project's structure from drifting while LLMs and people change it: the layers (which part of the code may
/// import which) and the records an agent works from (backlog, specs, knowledge and log in docs/).
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
    /// the declaration to this version. It never changes the declaration's other values. What an update cannot do
    /// without a person, it says, leaves files as it was, and exits 2. Exits 2 too for an unknown stack, a stack
    /// other than the declared one, or a file that cannot be read or written.
    Init {
        /// python, typescript, rust, or none (records only). Needed when there is no declaration yet
        #[arg(long)]
        stack: Option<String>,
    },
    /// Make the layers that .config/rotproof.toml declares and the tree lacks, and the records skeleton in docs/
    ///
    /// Makes only what is missing: each layer neither present nor declared absent, the directories of docs/ and
    /// docs/log.md, and the project's files (AGENTS.md, CLAUDE.md, README.md, .gitignore, .gitattributes, Claude Code's
    /// settings with the hook and the rule that denies editing the approvals file, for python and none the pin of
    /// Rotproof and a CI workflow, and for rust the workspace's Cargo.toml), each when it does not exist.
    /// Rewrites the files Rotproof generates: .rotproof/AGENTS.md (the rules it keeps) and the index files and rules in
    /// docs/. Adds the fields the declaration lacks, keeping its comments and values. Never overwrites another file,
    /// and never moves or deletes one. Run it when a project starts, and after editing the declaration; after upgrading
    /// Rotproof, `rotproof init` does what it does, without making a layer.
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
    /// Approve on a terminal a forbidden import, or the knowledge documents whose followed code changed (--follows);
    /// or remove the approvals that match nothing (--prune)
    ///
    /// `rotproof approve <file> <import>` keeps an import the layer table forbids, as `rotproof check` names it: the
    /// file from the root, and the import as the check writes it (a Python module, a TypeScript specifier, a crate's
    /// dependency). It shows the forbidden import, asks why it is kept and for a y, signs with git's user.name (or a
    /// name it asks for), and adds the entry to .config/rotproof-approved.toml. `rotproof approve --follows`, after a
    /// refactoring that kept what the code does, lists every knowledge document whose followed code changed and what
    /// changed under it, asks for one y, writes the new hashes in their follows, and prints the log lines to write;
    /// what is gone is left to edit by hand. Both run only when stdin is a terminal, so an agent's shell cannot
    /// approve: a person does. `rotproof approve --prune` removes every entry of .config/rotproof-approved.toml that
    /// matches no forbidden import, needs no terminal, and never lets anything pass that failed before. Exits 2 when
    /// stdin is not a terminal, the import is not forbidden or is approved already, the answer is not y, something
    /// followed is gone, or a file cannot be read or written.
    Approve {
        /// The file that imports, from the root, as `rotproof check` names it
        #[arg(
            required_unless_present_any = ["prune", "follows"],
            conflicts_with_all = ["prune", "follows"]
        )]
        file: Option<String>,
        /// What it imports, as `rotproof check` names it
        #[arg(
            required_unless_present_any = ["prune", "follows"],
            conflicts_with_all = ["prune", "follows"]
        )]
        import: Option<String>,
        /// Remove the entries that match no forbidden import
        #[arg(long, conflicts_with = "follows")]
        prune: bool,
        /// Re-pin, in one act on a terminal, every knowledge document whose followed code changed (after a
        /// refactoring that kept what the code does): each hash in follows set to what the code is now
        #[arg(long)]
        follows: bool,
    },
    /// Write every index.md in docs/ from the frontmatter
    ///
    /// Rewrites docs/index.md and the index.md of docs/backlog/, docs/specs/ and docs/knowledge/, and the rules.md
    /// Rotproof keeps there, from the frontmatter of the documents. Run it after a record changes; never edit an
    /// index.md by hand. Lists the documents it left out and why, and the backlog items past their stale_after. Exits
    /// 2 when the declaration or a document cannot be read.
    Index,
    /// Run as Claude Code's Stop hook: send the agent back once when its last message leaves something open and
    /// nothing in docs/ changed. Reads the hook input on stdin
    ///
    /// Not run by hand: `rotproof create` writes the .claude/settings.json that runs it. When the agent's last message
    /// holds a phrase that leaves something open (such as "not checked") and git shows no change in docs/, it asks the
    /// agent once to record the finding or say where it is. Exits 1, never 2, when it fails, so a broken hook never
    /// keeps the agent from stopping.
    StopHook,
}

/// The command line as clap reads it, with the long help of `rotproof check` naming the markers of the check.
fn command() -> clap::Command {
    Cli::command().mut_subcommand("check", |check| {
        check.long_about(CHECK_HELP.replace("{markers}", &either(&MARKERS)))
    })
}

fn main() -> ExitCode {
    let cli = Cli::from_arg_matches(&command().get_matches()).unwrap_or_else(|e| e.exit());
    // A root that does not exist fails here. Read as an empty tree, it would pass every check with nothing checked
    if !cli.root.is_dir() {
        eprintln!("the root is not a directory: {}", cli.root.display());
        return ExitCode::from(2);
    }
    if let Command::StopHook = cli.command {
        return stop_hook(&cli.root);
    }
    let result = match cli.command {
        Command::Init { stack } => init(&cli.root, stack.as_deref()).map(|()| true),
        Command::Create { yes } => create(&cli.root, yes).map(|()| true),
        Command::Check => check(&cli.root),
        Command::Guide { stack } => {
            application::project::guide_for(&Disk::new(&cli.root), stack.as_deref()).map(|text| {
                print!("{text}");
                true
            })
        }
        Command::Index => index(&cli.root).map(|()| true),
        Command::Approve {
            file,
            import,
            prune,
            follows,
        } => match (prune, follows, file, import) {
            (true, _, _, _) => approve_prune(&cli.root).map(|()| true),
            (_, true, _, _) => approve_follows(&cli.root).map(|()| true),
            (false, false, Some(file), Some(import)) => {
                approve(&cli.root, &file, &import).map(|()| true)
            }
            _ => unreachable!("clap requires the file and the import without --prune or --follows"),
        },
        Command::StopHook => unreachable!("answered above"),
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

/// Write the declaration, or upgrade the project's files, and say what to do next.
fn init(root: &Path, stack: Option<&str>) -> Result<(), String> {
    let disk = Disk::new(root);
    let upgraded = match application::init::init(&disk, &disk, stack, &disk.name()?, VERSION)? {
        Initialized::Declared(path) => {
            println!("wrote {path}");
            if stack == Some(RECORDS_ONLY) {
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
    report_made(&upgraded.made);
    if !upgraded.by_hand.is_empty() {
        for why in &upgraded.by_hand {
            println!("needs a person: {why}");
        }
        return Err(format!(
            "the project's files stay up to {} in {DECLARATION} until these are done or declined: then run \
             `rotproof init` again",
            upgraded.from
        ));
    }
    if upgraded.files_set {
        println!(
            "the project's files are up to {VERSION} (from {}): files = \"{VERSION}\" in {DECLARATION}",
            upgraded.from
        );
    } else {
        println!("the project's files are up to {VERSION} already");
    }
    println!("next: run `rotproof check`");
    Ok(())
}

/// Say what `rotproof create`, or the refresh of an upgrade, wrote and added.
fn report_made(made: &Made) {
    for path in &made.written {
        println!("wrote {path}");
    }
    for field in &made.added {
        println!("added to {DECLARATION}: {field}");
    }
    if let Some(why) = &made.not_written {
        println!("not written: {why}");
    }
    for (name, why) in &made.left_out {
        println!("left out of the index, fix it: {name}: {why}");
    }
}

/// Make what is missing, and say what was written. A second run with nothing changed writes nothing.
fn create(root: &Path, yes: bool) -> Result<(), String> {
    let disk = Disk::new(root);
    let made = application::create::create(&disk, &disk, &disk.name()?, yes)?;
    report_made(&made);
    if made.written.is_empty() {
        println!(
            "nothing to make: the tree has what {} declares",
            DECLARATION
        );
    }
    if !made.written.is_empty() {
        println!("next: run `rotproof check`; the rules it keeps are in .rotproof/AGENTS.md");
    }
    Ok(())
}

/// Print every broken rule under the check that found it. `true` when there are none.
fn check(root: &Path) -> Result<bool, String> {
    let report =
        application::check::check(&Disk::new(root), &Readers).map_err(|e| e.to_string())?;
    if let Some(why) = &report.skipped {
        println!("{why}");
    }
    // An approval never hides: every one is printed, on every run, before what fails
    if !report.approved.is_empty() {
        println!(
            "approved by a person in {APPROVALS} ({}):",
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

/// Answer the agent's stop hook. An error exits 1, never 2: Claude Code takes exit code 2 from it for "do not
/// stop", and a broken hook would keep the agent from stopping instead of being shown.
fn stop_hook(root: &Path) -> ExitCode {
    let mut input = String::new();
    let answer = std::io::stdin()
        .read_to_string(&mut input)
        .map_err(|e| format!("the hook input could not be read: {e}"))
        .and_then(|_| {
            // git runs only when the decision asks for it
            let git = project_root(root).map(|root| Git::new(&root));
            application::hook::run(&input, git.as_ref().map(|git| git as &dyn Changes))
        });
    match answer {
        Ok(Some(out)) => {
            println!("{out}");
            ExitCode::SUCCESS
        }
        Ok(None) => ExitCode::SUCCESS,
        Err(why) => {
            eprintln!("rotproof stop-hook: {why}");
            ExitCode::FAILURE
        }
    }
}

/// Ask a person on the terminal to approve the forbidden import of `import` by `file`, and add the entry. Every answer
/// is read from stdin, which must be a terminal: an agent's shell tool has none.
fn approve(root: &Path, file: &str, import: &str) -> Result<(), String> {
    if !std::io::stdin().is_terminal() {
        return Err(
            "rotproof approve asks a person, and stdin is not a terminal (an agent's shell has none): run it in your \
             own terminal"
                .into(),
        );
    }
    let disk = Disk::new(root);
    let pending = application::approvals::pending(&disk, &Readers, file, import)?;
    println!("forbidden by the layer table:");
    for forbidden in &pending {
        println!("  {}", forbidden.detail);
    }
    let by = match Git::new(root).user_name() {
        Some(name) => name,
        None => ask("git's user.name is not set; the name to sign with: ")?,
    };
    let reason = ask(&format!("why is {import} in {file} kept? "))?;
    let answer = ask(&format!(
        "approve it as {by}, in {APPROVALS}, printed on every run? [y/N] "
    ))?;
    if !answer.eq_ignore_ascii_case("y") {
        return Err("not approved: nothing written".into());
    }
    let entry = Kept {
        from: file.to_string(),
        import: import.to_string(),
        reason,
        approved: Approved {
            by,
            at: Local::now()
                .fixed_offset()
                .to_rfc3339_opts(SecondsFormat::Secs, true),
        },
    };
    let path = application::approvals::approve(&disk, &disk, &entry)?;
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
fn approve_follows(root: &Path) -> Result<(), String> {
    if !std::io::stdin().is_terminal() {
        return Err(
            "rotproof approve --follows asks a person, and stdin is not a terminal (an agent's shell has none): run \
             it in your own terminal, or review each document and write the hash `rotproof check` prints"
                .into(),
        );
    }
    let disk = Disk::new(root);
    let drifts = application::follows::changed(&disk, &Readers)?;
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
        let answer = ask(&format!(
            "re-pin these {changed} as reviewed, as if each document was read against its code? [y/N] "
        ))?;
        if !answer.eq_ignore_ascii_case("y") {
            return Err("not re-pinned: nothing written".into());
        }
        for (doc, hash) in application::follows::repin(&disk, &disk, &drifts)? {
            println!("wrote docs/knowledge/{doc}");
            println!(
                "  in the log entry of this change, write: * **Knowledge**: knowledge/{doc}@{hash}"
            );
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
fn approve_prune(root: &Path) -> Result<(), String> {
    let disk = Disk::new(root);
    let removed = application::approvals::prune(&disk, &Readers, &disk)?;
    if removed.is_empty() {
        println!("nothing to prune: every approval in {APPROVALS} matches a forbidden import");
    }
    for entry in &removed {
        println!(
            "removed from {APPROVALS}: {} in {}",
            entry.import, entry.from
        );
    }
    Ok(())
}

/// Write every index file, then list what was left out of them and the items to measure again.
fn index(root: &Path) -> Result<(), String> {
    let disk = Disk::new(root);
    let indexed = application::index::index(&disk, &disk, Local::now().fixed_offset())?;
    for path in indexed.written {
        println!("wrote {path}");
    }
    for (name, why) in indexed.left_out {
        println!("left out of the index, fix it: {name}: {why}");
    }
    for (name, at) in indexed.stale {
        println!("past stale_after, measure the state again: {name} ({at})");
    }
    Ok(())
}
