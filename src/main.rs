//! Keeps a project's structure: makes and checks its layers (as `.config/rotproof.toml` declares them) and the records
//! an agent works from (backlog, specs and log in `docs/`, written in OKF 0.2), and writes their index files.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use chrono::Local;
use clap::{CommandFactory, FromArgMatches, Parser, Subcommand};
use rotproof::application;
use rotproof::application::bundle::Bundle;
use rotproof::domain::bundle::{backlog, stale};
use rotproof::domain::hook::Changes;
use rotproof::domain::layers::{DECLARATION, RECORDS_ONLY};
use rotproof::domain::markers::{MARKERS, either};
use rotproof::domain::tree::Writer;
use rotproof::infrastructure::disk::{Disk, project_root};
use rotproof::infrastructure::git::Git;
use rotproof::infrastructure::readers::Readers;

/// What every help says after the commands: how to start, where the rules are, and the exit codes. An agent with only
/// the binary reads its way from here to a checked project.
const START: &str = "\
Start a project:
  1. rotproof init --stack <stack>   write .config/rotproof.toml (python, typescript, rust, or none for records only)
  2. edit it: list in absent the layers the project does not have
  3. rotproof create                 make the layers, docs/ and the project's files
  4. rotproof check                  check them; this is the project's CI

The rules Rotproof keeps are in .rotproof/AGENTS.md once `rotproof create` has run; `rotproof guide --stack <stack>`
prints them before. `rotproof <command> --help` says what a command reads, writes and never does.

Exit codes: 0 when the rules are kept and the command did its work; 1 when `rotproof check` finds a rule broken; 2
when a file cannot be read or written, or the command line is wrong.";

/// The long help of `rotproof check`. `{markers}` becomes the words the marker check fails on, taken from the check:
/// written here, they would be a comment that holds them, and a list that could drift from the check.
const CHECK_HELP: &str = "\
Check the layers and the records, and exit non-zero when one breaks the rules

Checks that the tree matches .config/rotproof.toml (every layer present or declared absent, no code outside the \
layers), that each layer imports only what the layer table allows, that no comment holds {markers}, that \
.rotproof/AGENTS.md is up to date, and that every record in docs/ keeps its rules (the rules.md of each directory). \
Prints every broken rule under the check that found it, and what was not checked and why. Writes nothing. Exits 1 when \
a rule is broken, 2 when a file cannot be read.";

/// Keeps a project's structure from drifting while LLMs and people change it: the layers (which part of the code may
/// import which) and the records an agent works from (backlog, specs, knowledge and log in docs/).
#[derive(Parser)]
#[command(version, about, after_help = START)]
struct Cli {
    /// The repository root; the records are in `<root>/docs`
    #[arg(long, default_value = ".")]
    root: PathBuf,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Write .config/rotproof.toml for a stack, once. Edit it, then run `rotproof create`
    ///
    /// Writes only the declaration, with every field and what it means, so the layers the project does not want are
    /// listed in absent before anything is made. Never overwrites a declaration that exists. Exits 2 for an unknown
    /// stack or a declaration that exists.
    Init {
        /// python, typescript, rust, or none (records only)
        #[arg(long)]
        stack: String,
    },
    /// Make the layers that .config/rotproof.toml declares and the tree lacks, and the records skeleton in docs/
    ///
    /// Makes only what is missing: each layer neither present nor declared absent, the directories of docs/ and
    /// docs/log.md, and the project's files (AGENTS.md, CLAUDE.md, README.md, .gitignore, .gitattributes, Claude Code's
    /// hook settings, for python and none the pin of Rotproof and a CI workflow, and for rust the workspace's
    /// Cargo.toml), each when it does not exist.
    /// Rewrites the files Rotproof generates: .rotproof/AGENTS.md (the rules it keeps) and the index files and rules in
    /// docs/. Adds the fields the declaration lacks, keeping its comments and values. Never overwrites another file,
    /// and never moves or deletes one. Run it when a project starts, after editing the declaration, and after
    /// upgrading Rotproof. Exits 2 when the declaration cannot be read or a file cannot be written.
    Create,
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
        Command::Init { stack } => init(&cli.root, &stack).map(|()| true),
        Command::Create => create(&cli.root).map(|()| true),
        Command::Check => check(&cli.root),
        Command::Guide { stack } => {
            application::project::guide_for(&Disk::new(&cli.root), stack.as_deref()).map(|text| {
                print!("{text}");
                true
            })
        }
        Command::Index => index(&cli.root).map(|()| true),
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

/// Write the declaration, and say what to do next.
fn init(root: &Path, stack: &str) -> Result<(), String> {
    let disk = Disk::new(root);
    let path = application::init::init(&disk, &disk, stack)?;
    println!("wrote {path}");
    if stack == RECORDS_ONLY {
        println!("next: run `rotproof create` to make docs/");
    } else {
        println!(
            "next: declare in absent the layers this project does not have, then run `rotproof create`"
        );
    }
    Ok(())
}

/// Make what is missing, and say what was written. A second run with nothing changed writes nothing.
fn create(root: &Path) -> Result<(), String> {
    let disk = Disk::new(root);
    let made = application::create::create(&disk, &disk, &disk.name()?)?;
    for path in &made.written {
        println!("wrote {path}");
    }
    for field in &made.added {
        println!("added to {DECLARATION}: {field}");
    }
    if made.written.is_empty() {
        println!(
            "nothing to make: the tree has what {} declares",
            DECLARATION
        );
    }
    if let Some(why) = &made.not_written {
        println!("not written: {why}");
    }
    for (name, why) in made.left_out {
        println!("left out of the index, fix it: {name}: {why}");
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
    for why in &report.skipped {
        println!("{why}");
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

/// Answer the agent's stop hook. An error exits 1, never 2: Claude Code takes exit code 2 from it for "do not stop", and a broken hook would keep the agent from stopping instead of being shown.
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

/// Write every index file, then list what was left out of them and the items to measure again.
fn index(root: &Path) -> Result<(), String> {
    let disk = Disk::new(root);
    let areas = application::layers::areas(&disk).map_err(|e| e.to_string())??;
    let bundle = Bundle::new(&disk, areas);
    let (files, problems) = bundle.expected().map_err(|e| e.to_string())?;
    for (path, text) in files {
        disk.write(&path, &text)
            .map_err(|e| format!("{path}: {e}"))?;
        println!("wrote {path}");
    }
    for (name, why) in problems {
        println!("left out of the index, fix it: {name}: {why}");
    }
    let docs = bundle.read_folder("backlog").map_err(|e| e.to_string())?;
    let parsed = backlog(&docs, &bundle.areas);
    for name in stale(&parsed.items, Local::now().fixed_offset()) {
        let at = parsed.items[&name].0.stale_after.unwrap();
        println!("past stale_after, measure the state again: {name} ({at})");
    }
    Ok(())
}
