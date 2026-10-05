//! The CI entry point. Run `cargo xtask ci` both on a developer machine and in GitHub Actions, so that the two cannot
//! disagree about which checks ran. `cargo xtask licenses` writes `THIRD-PARTY-LICENSES.txt` with cargo-about, and
//! `cargo xtask licenses --check` checks it against what cargo-about writes (`licenses.rs` says why that is apart).
//!
//! Cargo is the one that runs this (`$CARGO`), never another version on `PATH`. **A missing tool is a failure.**
//! Skipping it would let CI pass with a check silently gone.

mod drift;
mod layers;
mod licenses;
mod manifests;
mod pure;

use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::io::Read;
use std::path::Path;
use std::process::{Child, Command, ExitCode, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};

/// Each check with how long it may run: about five times its slowest run in CI, and at least a minute. A check that
/// hangs fails with its name, instead of holding CI until the job's own limit
const CHECKS: &[(&str, Duration, &[&str])] = &[
    (
        "Format (rustfmt)",
        Duration::from_mins(1),
        &["fmt", "--all", "--check"],
    ),
    // Warnings fail too, so they cannot pile up behind a green CI
    (
        "Lint (clippy)",
        Duration::from_mins(2),
        &[
            "clippy",
            "--workspace",
            "--all-targets",
            "--",
            "-D",
            "warnings",
        ],
    ),
    // Every test binary runs even after one fails: by default a failing unit test hides the command-line tests
    (
        "Tests (cargo test)",
        Duration::from_mins(2),
        &["test", "--workspace", "--no-fail-fast"],
    ),
];

/// How long `git ls-files` may run. It takes milliseconds
const GIT_LIMIT: Duration = Duration::from_mins(1);

/// How long `cargo tree` may run. On its first run it downloads the crates of the other platforms
const TREE_LIMIT: Duration = Duration::from_mins(2);

/// How long `cargo fetch` may run before cargo-about reads the license files of the crates of every platform
const FETCH_LIMIT: Duration = Duration::from_mins(5);

/// How long cargo-about may run, offline
const ABOUT_LIMIT: Duration = Duration::from_mins(2);

fn run(cargo: &str, name: &str, limit: Duration, args: &[&str]) -> bool {
    println!("\n--- {name} ---\n$ cargo {}", args.join(" "));
    let status = Command::new(cargo)
        .args(args)
        .spawn()
        .map_err(|e| format!("cargo could not run: {e}"))
        .and_then(|mut child| wait(&mut child, name, limit));
    match status {
        Ok(status) => status.success(),
        Err(e) => {
            println!("{e}");
            false
        }
    }
}

/// Wait for a child, but no longer than `limit`; then it is stopped and the step fails. Only the child is stopped, not
/// what it started: the job's own limit stops the rest
fn wait(child: &mut Child, name: &str, limit: Duration) -> Result<ExitStatus, String> {
    let deadline = Instant::now() + limit;
    loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|e| format!("{name} could not be waited for: {e}"))?
        {
            return Ok(status);
        }
        if Instant::now() >= deadline {
            let secs = limit.as_secs();
            return Err(match child.kill().and_then(|()| child.wait()) {
                Ok(_) => format!("{name} did not finish within {secs} seconds, and was stopped"),
                Err(e) => format!(
                    "{name} did not finish within {secs} seconds, and could not be stopped: {e}"
                ),
            });
        }
        thread::sleep(Duration::from_millis(100));
    }
}

/// Read a pipe to its end on another thread, so that a child never stops on a full pipe while it is waited for
fn drain(mut pipe: impl Read + Send + 'static) -> thread::JoinHandle<Result<Vec<u8>, String>> {
    thread::spawn(move || {
        let mut bytes = Vec::new();
        pipe.read_to_end(&mut bytes)
            .map(|_| bytes)
            .map_err(|e| format!("cannot read the output of a command: {e}"))
    })
}

fn drained(reader: thread::JoinHandle<Result<Vec<u8>, String>>) -> Result<Vec<u8>, String> {
    reader.join().expect("reading a pipe does not panic")
}

/// Print what an in-process check found. An error reading its input is a failure, never a pass
fn report(name: &str, problems: Result<Vec<String>, String>) -> bool {
    println!("\n--- {name} ---");
    let problems = problems.unwrap_or_else(|e| vec![e]);
    for p in &problems {
        println!("{p}");
    }
    problems.is_empty()
}

fn read(root: &Path, file: &str) -> Result<String, String> {
    fs::read_to_string(root.join(file)).map_err(|e| format!("cannot read {file}: {e}"))
}

/// What a command prints, run in the repository, waited for no longer than `limit`. `name` names it in errors. A
/// command that fails, cannot run or runs past its limit is an error, with what it printed to standard error
fn output(
    root: &Path,
    name: &str,
    limit: Duration,
    program: &str,
    args: &[&str],
) -> Result<String, String> {
    let mut child = Command::new(program)
        .args(args)
        .current_dir(root)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("{name} could not run: {e}"))?;
    let stdout = drain(child.stdout.take().expect("stdout is piped"));
    let stderr = drain(child.stderr.take().expect("stderr is piped"));
    let status = wait(&mut child, name, limit)?;
    let (stdout, stderr) = (drained(stdout)?, drained(stderr)?);
    if !status.success() {
        return Err(format!(
            "{name} failed: {}",
            String::from_utf8_lossy(&stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&stdout).into_owned())
}

/// The tracked files, `/`-separated. In the CI container the checkout belongs to another user, and git refuses to read
/// such a repository unless it is marked safe; this only reads, so it is marked safe for this one command
fn tracked(root: &Path) -> Result<Vec<String>, String> {
    Ok(output(
        root,
        "git ls-files",
        GIT_LIMIT,
        "git",
        &["-c", "safe.directory=*", "ls-files"],
    )?
    .lines()
    .map(str::to_string)
    .collect())
}

fn map(root: &Path) -> Result<Vec<String>, String> {
    let map = drift::read_map(&read(root, "AGENTS.md")?);
    Ok(drift::map_problems(&map, &tracked(root)?))
}

fn toolchain(root: &Path) -> Result<Vec<String>, String> {
    Ok(drift::toolchain_problems(
        &read(root, "rust-toolchain.toml")?,
        &read(root, "Dockerfile")?,
        &read(root, ".github/workflows/ci.yml")?,
    ))
}

fn install_version(root: &Path) -> Result<Vec<String>, String> {
    Ok(drift::install_problems(
        &read(root, "Cargo.toml")?,
        &read(root, "README.md")?,
    ))
}

/// Where a member of the workspace names a dependency's source itself. The floor: at least one member is read
fn dependencies(root: &Path) -> Result<Vec<String>, String> {
    let members = manifests::members(&read(root, "Cargo.toml")?)?;
    if members.is_empty() {
        return Err("Cargo.toml lists no member, so no crate's dependencies were read".into());
    }
    let mut found = Vec::new();
    for member in members {
        let path = format!("{member}/Cargo.toml");
        found.extend(manifests::problems(&path, &read(root, &path)?)?);
    }
    Ok(found)
}

/// Where `domain` takes a port or does I/O, in its `.rs` files
fn domain_pure(root: &Path) -> Result<Vec<String>, String> {
    let dir = root.join(pure::DOMAIN);
    let mut names: Vec<String> = fs::read_dir(&dir)
        .map_err(|e| format!("cannot read {}: {e}", pure::DOMAIN))?
        .map(|entry| entry.map(|e| e.file_name().to_string_lossy().into_owned()))
        .collect::<Result<_, _>>()
        .map_err(|e| format!("cannot read {}: {e}", pure::DOMAIN))?;
    names.retain(|name| utils::rust::is_source(name));
    names.sort();
    let mut files = Vec::new();
    for name in names {
        let path = format!("{}/{name}", pure::DOMAIN);
        files.push((path.clone(), read(root, &path)?));
    }
    Ok(pure::problems(&files))
}

/// The version of cargo-about the Dockerfile and the licenses workflow install
fn pinned_about(root: &Path) -> Result<String, String> {
    licenses::pinned_version(&read(root, "Dockerfile")?, &read(root, licenses::WORKFLOW)?)
        .map_err(|problems| problems.join("\n"))
}

/// The crates the binary links: normal dependencies on every platform, as in about.toml
fn linked_crates(cargo: &str, root: &Path) -> Result<BTreeSet<String>, String> {
    let tree = output(
        root,
        "cargo tree",
        TREE_LIMIT,
        cargo,
        &[
            "tree", "--locked", "-p", "rotproof", "-e", "normal", "--target", "all", "--prefix",
            "none", "--format", "{p}",
        ],
    )?;
    Ok(licenses::tree_crates(&tree))
}

/// The check in `cargo xtask ci`: the file lists the crates the binary links, and the two files that install
/// cargo-about agree. cargo-about itself is not needed
fn licenses_list(cargo: &str, root: &Path) -> Result<Vec<String>, String> {
    pinned_about(root)?;
    Ok(licenses::list_problems(
        &read(root, licenses::FILE)?,
        &linked_crates(cargo, root)?,
    ))
}

/// What cargo-about writes now, with `\n` line endings and one at the end. The installed cargo-about has to be the
/// pinned one: another version may write another text, and the file would look stale for no reason. It reads the
/// license files of the crates of every platform, and `--frozen` keeps it off the network, so every crate is fetched
/// first
fn generate_licenses(cargo: &str, root: &Path) -> Result<String, String> {
    let pinned = pinned_about(root)?;
    let install = format!("cargo install --locked --features cli cargo-about@{pinned}");
    let installed = output(
        root,
        "cargo about --version",
        ABOUT_LIMIT,
        cargo,
        &["about", "--version"],
    )
    .map_err(|e| format!("{e}\ncargo-about is needed: {install}"))?;
    if installed.trim() != format!("cargo-about {pinned}") {
        return Err(format!(
            "{} is installed, but the Dockerfile pins cargo-about {pinned}: {install}",
            installed.trim()
        ));
    }
    output(
        root,
        "cargo fetch",
        FETCH_LIMIT,
        cargo,
        &["fetch", "--locked"],
    )?;
    let text = output(
        root,
        "cargo about generate",
        ABOUT_LIMIT,
        cargo,
        &["about", "generate", "--frozen", "--fail", "about.hbs"],
    )?;
    // cargo-about ends the template's last line with one more line break, which git reports as a blank line at the end
    Ok(format!(
        "{}\n",
        text.replace("\r\n", "\n").trim_end_matches('\n')
    ))
}

/// The check in `cargo xtask licenses --check`: the list, and the text cargo-about writes
fn licenses_text(cargo: &str, root: &Path) -> Result<Vec<String>, String> {
    let committed = read(root, licenses::FILE)?;
    let mut found = licenses::list_problems(&committed, &linked_crates(cargo, root)?);
    found.extend(licenses::text_problems(
        &committed,
        &generate_licenses(cargo, root)?,
    ));
    Ok(found)
}

/// `cargo xtask licenses`: write THIRD-PARTY-LICENSES.txt. `--check`: fail where it differs from what would be written
fn licenses(cargo: &str, root: &Path, check: bool) -> ExitCode {
    if check {
        return if report("Third-party licenses", licenses_text(cargo, root)) {
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        };
    }
    let written = generate_licenses(cargo, root).and_then(|text| {
        fs::write(root.join(licenses::FILE), &text)
            .map(|()| licenses::listed_crates(&text).len())
            .map_err(|e| format!("cannot write {}: {e}", licenses::FILE))
    });
    match written {
        Ok(crates) => {
            println!("wrote {} ({crates} crates)", licenses::FILE);
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let cargo = env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask sits inside the repository");
    match args.as_slice() {
        ["ci"] => {}
        ["licenses"] => return licenses(&cargo, root, false),
        ["licenses", "--check"] => return licenses(&cargo, root, true),
        _ => {
            eprintln!("usage: cargo xtask ci | cargo xtask licenses [--check]");
            return ExitCode::from(2);
        }
    }
    let mut results: Vec<(&str, bool)> = CHECKS
        .iter()
        .map(|(name, limit, args)| (*name, run(&cargo, name, *limit, args)))
        .collect();
    results.push(("Map (AGENTS.md)", report("Map (AGENTS.md)", map(root))));
    results.push((
        "Toolchain version",
        report("Toolchain version", toolchain(root)),
    ));
    results.push((
        "Install version (README)",
        report("Install version (README)", install_version(root)),
    ));
    results.push((
        "Dependencies (workspace)",
        report("Dependencies (workspace)", dependencies(root)),
    ));
    results.push((
        "Third-party licenses",
        report("Third-party licenses", licenses_list(&cargo, root)),
    ));
    results.push((
        "Layers (Rotproof)",
        report("Layers (Rotproof)", layers::problems(root)),
    ));
    results.push((
        "Domain calls no port",
        report("Domain calls no port", domain_pure(root)),
    ));
    println!("\n{}", "=".repeat(40));
    for (name, ok) in &results {
        println!(" {name:<24}: {}", if *ok { "PASSED" } else { "FAILED" });
    }
    if results.iter().all(|(_, ok)| *ok) {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
