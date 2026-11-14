# AGENTS.md (Rotproof)

This file holds **the map of this repository** and **short rules that apply to every change**. What Rotproof checks,
and why, is in `README.md`.

## Map

| Path | Content |
|---|---|
| `crates/` | The tool, one crate per layer, each with its modules (below): `composition` (the one place that wires), `infrastructure` (the command line in `inbound`, the file system, git and the parsers in `outbound`), `application`, `protocol` (the input port and the output ports) and `domain` |
| `src/` | The entry file of the root package `rotproof`, the binary named for what it ships: `main.rs` calls `composition` alone |
| `tests/` | `cli.rs`: the tests that run the binary as a user runs it |
| `layers/` | The layer definitions, built into the binary: `table.toml` (what the layers are, in every stack) and one layout per stack (`python.toml`, `typescript.toml`, `rust.toml`: where each layer lives, the files that make it, and the files its toolchain needs at the root) |
| `records/` | The records skeleton, built into the binary: `knowledge-rules.md` (the rules of `docs/knowledge/` Rotproof writes into every project) and `log.md` (the log `rotproof create` starts) |
| `project/` | What Rotproof writes into a project outside `docs/`, built into the binary. Each file is named after the one it becomes, without a leading dot and with `.in` added, so no agent here reads a project's `AGENTS.md` as its own: the project's files at the top (`AGENTS.md.in`, `CLAUDE.md.in`, `README.md.in`, `gitattributes.in`), `gitignore/` (one per stack), `pypi/` (the pin, the workflow and the README's instructions for the stacks that install Rotproof from PyPI), `release/` (the workflow, which pins the version, and the README's instructions for the stacks that install it from the GitHub Release), `npm/` (the workflow and the README's instructions, with the pin the project adds to its own `package.json`, for the stacks that install it from npm), and `rotproof/` (the parts of `.rotproof/AGENTS.md`, the guide Rotproof generates for each stack) |
| `xtask/` | The CI entry point (`cargo xtask ci`), which also checks Rotproof's own layers with its library (`layers.rs`), that every crate takes its dependencies from the workspace (`manifests.rs`), that `domain` calls no port (`pure.rs`) and the width of comment lines (`width.rs`), and `cargo xtask licenses`, which writes `THIRD-PARTY-LICENSES.txt` (`--check`: checks it) |
| `.cargo/` | The `cargo xtask` alias |
| `.github/` | GitHub Actions: `ci.yml` runs `cargo xtask ci` in the container, and builds and installs the wheels on Linux and Windows and runs them through `smoke.sh`; `licenses.yml` checks `THIRD-PARTY-LICENSES.txt` with cargo-about; `release.yml` releases a pushed tag to PyPI, npm and GitHub Releases, after running each wheel and each npm package through `smoke.sh` and installing the assets as the CI of a rust or none project does (`project-ci.sh`) |
| `npm/` | The npm packages, made by `release.yml` from the archives of a release with `pack.py`, which fills in the version from `Cargo.toml` and the description from `pyproject.toml`: `rotproof/` (the package a project installs, whose `bin/rotproof.js` runs the binary of its platform's package) and one directory per platform (`linux-x64/`, `win32-x64/`), named as Node names the platform, whose package holds the binary |
| `Cargo.toml` | The root package `rotproof`, which depends on `composition` alone; the workspace of the crates, and the one place that names Rotproof's version (`[workspace.package]`: every crate takes it, and maturin takes the wheel version from it) and each dependency's version or path (`[workspace.dependencies]`: every crate takes them with `workspace = true`) |
| `rust-toolchain.toml` | The one place that names the toolchain. The Dockerfile and CI install from it |
| `Dockerfile`, `compose.yaml` | The development container: the Linux of CI, with the packaging tools and cargo-about |
| `pyproject.toml` | The pip package: a wheel that carries only the binary and the licenses (maturin, `bindings = "bin"`, built from the root `Cargo.toml`) |
| `requirements-build.txt` | The one place that names the packaging tools (maturin, cargo-xwin). The Dockerfile and CI install from it |
| `THIRD-PARTY-LICENSES.txt` | The licenses of every crate the binary links, which go with the binary: in the wheel next to Rotproof's own. Generated, never edited by hand |
| `about.toml`, `about.hbs` | What cargo-about writes `THIRD-PARTY-LICENSES.txt` from: the licenses Rotproof may ship, and the file's template |
| `README.md` | What Rotproof checks, how to use it, and how to build it |
| `LICENSE-MIT`, `LICENSE-APACHE` | The license: MIT OR Apache-2.0 |

Also tracked, as in most repositories: `.gitattributes`, `.gitignore`, `Cargo.lock`, `AGENTS.md`, and `CLAUDE.md`
(which only imports `AGENTS.md`).

| Module of `src/` | Content |
|---|---|
| `main.rs` | The entry file: `composition::run` |

| Module of `crates/composition/src/` | Content |
|---|---|
| `lib.rs` | The layer's role, and `run`: the machine of `infrastructure::outbound` given to the commands of `application`, and those to the command line of `infrastructure::inbound` |

| Module of `crates/infrastructure/src/` | Content |
|---|---|
| `lib.rs` | The layer's role, and its two levels |

| Module of `crates/infrastructure/src/inbound/` | Content |
|---|---|
| `mod.rs` | The level's role |
| `cli.rs` | The command line: `rotproof init`, `rotproof create`, `rotproof check`, `rotproof approve` (which alone asks a person on the terminal: an import kept, or the knowledge documents re-pinned with `--reviewed`), `rotproof guide`, `rotproof index` and `rotproof follows`, and the help that leads from one to the next. Each command is a call of `protocol::inbound::Commands`; what it prints and how it exits are decided here. A command line that names `stop-hook` or `time-hook`, the hooks Rotproof wrote into a project's Claude Code settings before 1.0, says in one line that they were removed and exits 1, never 2 (no command of that name is left) |

| Module of `crates/infrastructure/src/outbound/` | Content |
|---|---|
| `mod.rs` | The level's role, and `Local`: the `Machine` Rotproof runs on, from the modules below and the local clock |
| `python.rs` | Reading Python with Ruff's parser: imports, as the modules they name, comments, the reads of the environment, the functions and classes a file defines, and the outline of a file (every function and class with its names and range) |
| `typescript.rs` | Reading TypeScript and JavaScript with oxc: imports and comments, which it reads itself, with how a file is parsed and the text of a literal string, which the readers in `typescript/` share; and `read`, which gives the imports, the comments and the reads of the environment |
| `cargo.rs` | Reading a crate's `Cargo.toml` with toml_edit: the dependencies it declares on a path or its workspace, and its workspace |
| `readers.rs` | `Parsers` with the readers above |
| `disk.rs` | `Tree` and `Writer` on the file system, the project's `.gitignore` files kept, the project's name, and the project a directory sits in |
| `git.rs` | The name `rotproof approve` signs with, git's `user.name` |

| Module of `crates/infrastructure/src/outbound/typescript/` | Content |
|---|---|
| `environment.rs` | The reads of the environment in a TypeScript or JavaScript file: `process.env` (the global, `globalThis.process`, or a name the file imports `process` as) and `import.meta.env`, each as a member or taken apart, and `env` imported from `process`, with what Vite sets itself passing |
| `outline.rs` | The outline of a TypeScript or JavaScript file (a function, class, interface, type, enum or namespace, a variable, and their members, each with its names and range), and `Err` when oxc gives up on the file |
| `aliases.rs` | The aliases of the project: the `tsconfig*.json` files at the root and the local files they extend, read as JSON with comments, and each entry of `paths` made into an alias |

| Module of `crates/protocol/src/` | Content |
|---|---|
| `lib.rs` | The layer's role, and its two levels |

| Module of `crates/protocol/src/inbound/` | Content |
|---|---|
| `mod.rs` | The input port: `Commands`, one method per command, and the plain data each takes and returns (`Made`, `Initialized`, `Report`, `Indexed`, `Drift` and the rest) |

| Module of `crates/protocol/src/outbound/` | Content |
|---|---|
| `mod.rs` | The output ports: `Tree` and `Writer` (a project's files read and written, together `Files`), `Parsers` (the readers of code), and `Machine`, which gives each of them for a root, with git's `user.name` and the clock |

| Module of `crates/domain/src/` | Content |
|---|---|
| `lib.rs` | The layer's role, and its modules |
| `approvals.rs` | The approvals file, `.config/rotproof-approved.toml`: the imports the layer table forbids that a person approved, each keyed by its file and what it imports, matched against what the direction check found, and the file with an entry added or removed |
| `bundle.rs` | The part of `docs/` Rotproof keeps, as one OKF bundle: the knowledge documents sorted out once they are read, the files Rotproof generates (the index files and the rules), and the tags the knowledge documents use |
| `schema.rs` | The frontmatter of a knowledge document and of a guide |
| `follows.rs` | What a knowledge document follows: the forms of a key of `follows` (a file, a directory of code, a definition in a file of a language Rotproof reads), the language of a file, the hash a key is pinned at and the hash of a directory, what fails when one changed or is gone, the document `rotproof follows` names, and a hash replaced or a key added in the frontmatter |
| `definitions.rs` | The outline of a file as its definitions (names and range), and the one rule that selects the pieces of a name from it and joins them into the text a key of `follows` hashes, for every language |
| `records.rs` | The rules of the records `rotproof check` runs on what it read: the files under `docs/knowledge/` Rotproof would not read, the log's structure, its days that have not come (against the latest day it is anywhere), and the floor of the bundle |
| `tree.rs` | How Rotproof reads a file's text, how it writes the text back as the file was written (its line endings and byte order mark), and how Windows reads a name |
| `code.rs` | What the readers give, as plain values: the module a Python file is, which files are TypeScript source, the alias an entry of `paths` makes, and where a TypeScript import lands |
| `upgrade.rs` | Upgrading a project's files: the named updates a later version brings to the files `rotproof create` writes once, which of them a project takes between the version its files are up to and the running one, what each does to its file and what a person is told when it cannot, and what `rotproof check` finds in `files` and `declined` |
| `settings.rs` | The edits those updates make to Claude Code's settings, `.claude/settings.json`, as JSON: the deny rule added and the hooks taken out, each matched exactly, with the other keys and their order kept |
| `version.rs` | A version as SemVer writes and orders it (pre-releases included), and the version of Rotproof that runs |
| `crossing.rs` | What `rotproof init` says when an upgrade crosses a version that changed what the layers are (the onion table, the split of the screens into `ui` and `presentation`): the versions, what each moved and where each retired layer's code goes, and whether an upgrade crosses one. Nothing moves; a project does it by hand |
| `direction.rs` | The direction check: every layer imports only what `layers/table.toml` allows (Python, TypeScript, Rust), judged once an import is resolved to a place; and where Cargo looks for a workspace |
| `modules.rs` | The levels inside a Rust crate: the module each file of `src/` is, every path it names from `crate`, `super`, `self` or a dependency on another layer judged by the level it lands in, the crate's root holding only its levels' modules, and no `#[path]` |
| `layers.rs` | The layer definitions in `layers/`: the table, and one layout per stack with the places it makes, and the questions asked of them (a layer or a level of one, where a layer lives, whether the layout leaves a layer out, which files have comments to read) |
| `declaration.rs` | A project's declaration, `.config/rotproof.toml`: what it says, read as it is and against the layout it names (`Declared`, which also says whether a path is in `unchecked`), the fields `rotproof create` adds to it and the text `rotproof init` writes, and what an upgrade reads from it and sets in it (`files`, `declined`) |
| `links.rs` | Whether a link in a record resolves: by its target alone (no URL is checked, no path on one machine, no `\`, nothing above the repository), then by the exact names of its files, to a heading that is there, and to a definition in a `.py` file |
| `markers.rs` | The marker check: no comment in the code holds `TODO`, `FIXME`, `XXX`, `HACK` or `NOTE` (Python, TypeScript, Rust), judged once the comments of a file are read |
| `environment.rs` | The environment check: only `infrastructure` reads or writes the environment variables (the names of Python's `os`, TypeScript's `process.env` and `import.meta.env`, the functions of Rust's `std::env`), the code of every other layer and the entry file judged, the reads of a Rust file found by the scanner, and what the guide says of it |
| `suppressions.rs` | The suppression check: no comment switches the type checker off (`# type: ignore`, `# <checker>: ignore` and `ignore-errors` in Python, `@ts-ignore`, `@ts-expect-error` and `@ts-nocheck` in TypeScript), and a warning for a comment that may make a Python checker check less, judged once the comments of a file are read |
| `hook.rs` | The settings file that `rotproof create` writes for Claude Code (the rule that denies editing the approvals file), and the commands of the stop hook and the time hook Rotproof wrote before, which `rotproof init` takes out |
| `guide.rs` | The guide `.rotproof/AGENTS.md`, which `rotproof create` rewrites: how to run Rotproof, the section on the layers with their table and what each rule of the code says in the stack's language, and the records |
| `project.rs` | The project's files `rotproof create` writes once, outside `docs/` |
| `structure.rs` | The structure check: the tree agrees with `.config/rotproof.toml`, either way, judged from the paths it names and the code files of the directories it lists |
| `frontmatter.rs` | Splitting a document into frontmatter and the sections of its body |
| `markdown.rs` | What GitHub renders as text, headings and their anchors as GitHub computes them, and links |
| `sentence.rs` | The first sentence of a paragraph, as the index shows it: the paragraph read as markdown reads it, in any language, with no sentence ending inside parentheses or inline code |
| `rust.rs` | Reading Rust source with a scanner of Rotproof's own: its comments, past strings and character literals, the code alone with both blanked, and its tokens, the paths it names, the items of a module it names through its `use`, its modules with a body and its attributes, and, in `rust/`, its outline |
| `paths.rs` | Paths from the root with `/`: the parent, the name, a join, without `.` and `..`, whether one sits in another, and the paths in messages |
| `text.rs` | The line at a position in a text, for messages |

| Module of `crates/domain/src/rust/` | Content |
|---|---|
| `outline.rs` | The outline of a Rust file, read from the scanner's tokens: functions, types, traits, constants, macros and inline modules, the variants of an enum and the items of a trait and of an `impl` block, each from its first attribute or doc comment to its end, an `impl` block a definition of the type it is for |

| Module of `crates/application/src/` | Content |
|---|---|
| `lib.rs` | The layer's role, and its modules |
| `approvals.rs` | Reading the approvals file from the tree, and `rotproof approve`: an entry added for a forbidden import a person approved, and the entries that match nothing removed (`--prune`), the file written back as it was written |
| `bundle.rs` | Reading `docs/knowledge/` from the tree |
| `check.rs` | `rotproof check`: every check run against one repository, each with its floor, and what each found printed under its heading |
| `commands.rs` | `protocol::inbound::Commands` implemented: each command opens the project at its root through the `Machine` `composition` passes in, and runs its use case |
| `create.rs` | `rotproof create`: making the layers and the records skeleton a project lacks, and adding the fields the declaration lacks; and the same without making a layer, for an upgrade |
| `tree.rs` | Reading a project's files through `Tree`: a path read name by name and compared exactly (and whether it is a file), where an import's path lands as the operating system resolves it, the code files of a directory, and the text of a file, with how it was written |
| `code.rs` | Where a TypeScript import lands, the tree asked whether a module is under a `baseUrl` |
| `direction.rs` | The direction check, read from the tree: the code of each language read, and every import resolved to a place: for Rust, each dependency of a crate resolved to the path it comes from (through its workspace too) and then the levels inside each crate |
| `layers.rs` | Reading a project's `.config/rotproof.toml` |
| `links.rs` | Whether a link in a record resolves, read from the tree: the file it names looked up and read |
| `follows.rs` | What a knowledge document follows, read from the tree and hashed as it is now, the re-pin of `rotproof approve --reviewed`, and the pin of `rotproof follows` |
| `environment.rs` | The environment check, read from the tree: the code of every layer but `infrastructure`, and the entry file, read by the reader of its language |
| `markers.rs` | The checks of the comments, read from the tree: every code file walked, and its comments read by the reader of its language, for the marker check and the suppression check |
| `index.rs` | `rotproof index`: every index file written from the frontmatter |
| `init.rs` | `rotproof init`: writing a project's declaration, or, when it exists, upgrading the project's files to the running version |
| `project.rs` | `rotproof guide`: the guide for a stack, named or declared |
| `finding.rs` | A finding or a warning of `rotproof check`, made from what a check found and the heading it is printed under |
| `records.rs` | The checks of the records, read from the tree: the areas, the floor, the links of `docs/knowledge/` and the log, the log, the knowledge documents and the generated files |
| `structure.rs` | The structure check, read from the tree |

## How this repository differs from what Rotproof keeps

- **No `docs/` here.** Rotproof's own records (its knowledge, its log, and the specs and work items it kept before 1.0)
  are kept outside this repository.
- **In layers, and no `.config/rotproof.toml`.** Every layer of the Rust layout Rotproof keeps (README) is a crate of
  its own in `crates/`, with the levels of `infrastructure` and `protocol` as modules, the entry file is `src/main.rs`
  of the root package, and it declares `ui = "none"`, the one UI that layout offers. `cargo xtask ci` checks them as
  `rotproof check` checks a project's (structure, direction, markers), with the declaration written in
  `xtask/src/layers.rs` in place of the file, and reads the comments of `xtask/` for markers too, which the layout
  leaves out; the records are kept outside, so they are not checked here.
  `domain` calls no port and does no I/O: a rule takes the values `application` has read, stricter than what
  `rotproof check` asks of a project. The ports are in `protocol`, which `domain` cannot depend on; `cargo xtask ci`
  checks the rest in `xtask/src/pure.rs` by the forms in the code outside the tests and the trait blocks: anywhere, a
  port taken (every trait of `protocol::outbound`, and every trait `domain` declares) and I/O
  (`io::Result`, `std::fs` and the like, printing); and, since whether a function passed in reads cannot be told from
  its type, no function, closure or iterator taken where `application` can pass one (a `pub` signature, field, struct,
  enum or type alias). Private and `pub(crate)` functions are called by `domain` alone, and may take them.

## Rules for every change

- **A layer changes in `layers/`, not in code; a stack, in `layers/` and in three tables of code.** The table says
  what the layers are once; a layout says only where they live in one stack. A new stack is also named in `STACKS`
  (`crates/domain/src/layers.rs`, which builds its layout into the binary), in `GITIGNORE` with its file in
  `project/gitignore/`, and in `CHANNEL` with where it installs Rotproof from (both in `crates/domain/src/project.rs`).
  The tests read every definition and every one of those tables, so a layout that names a layer the table lacks
  fails, and so does a stack left out of a table.
- **Write Rotproof where prose names the tool, and `rotproof` for the command, the package, the crate and file
  names,** as Ruff and `ruff` are written. A sentence that starts with the command still writes it in backticks.
- **Wrap a line that holds a comment at 120 characters.** rustfmt wraps code, not comments; `cargo xtask ci` fails on
  a longer one in any tracked `.rs` file (`xtask/src/width.rs`).

- **Run CI with `cargo xtask ci`, on the host or in the container (`docker compose run --rm dev cargo xtask ci`).** The
  container is the Linux of CI and needs only Docker. On Windows, the host needs Visual Studio's C++ tools and the
  Windows SDK; without the SDK, linking fails (`kernel32.lib` not found), and Git Bash's own `link` gets in the way.
  To run the tests alone, `cargo test --workspace --no-fail-fast`: a bare `cargo test` at the root runs only the root
  package's tests, and stops at the first suite that fails, so a failing test of a crate passes unseen.
- **Change the toolchain version only in `rust-toolchain.toml`,** and the base image (`Dockerfile`, the `container`
  in `.github/workflows/ci.yml`) to the same version. Change the packaging tools only in `requirements-build.txt`.
- **Add or bump a dependency in `[workspace.dependencies]` of the root `Cargo.toml`,** and take it in a crate with
  `<name>.workspace = true`. `cargo xtask ci` fails on a crate that names a dependency's version or path itself (in
  any table of dependencies, under `[target.*]` too), so a dependency two crates use cannot drift to two versions.
- **After a change to the dependencies, run `cargo xtask licenses` and commit `THIRD-PARTY-LICENSES.txt` with it.**
  It needs cargo-about, at the version the `Dockerfile` installs (the container has it; on the host, the command
  prints how to install it). `cargo xtask ci` fails while the file misses a crate `cargo tree` says the binary links,
  or lists one it does not; it does not run cargo-about, which takes minutes to build. `licenses.yml` checks the text
  with `cargo xtask licenses --check`, only on a push that changes what the text is made from. A license outside
  `accepted` in `about.toml` fails the generation: adding one is a decision of its own, not part of a bump.
- **cargo-about is pinned in the `Dockerfile` and `.github/workflows/licenses.yml`, to the same version,** and
  `cargo xtask ci` fails when they differ; `cargo xtask licenses` fails when the installed one is another: another
  version may write another text. After changing it, run `cargo xtask licenses`.
- **The `oxc_*` crates are pinned to one exact version, and move together,** for the same reason as the Ruff crates
  below: oxc's API changes between minor versions. Bump them when TypeScript gains syntax Rotproof fails to read.
- **`ruff_python_parser` and `ruff_python_ast` are pinned to one exact version, and move together.** They are
  internal crates of Ruff, whose API changes between any two versions. Bump them by hand when the toolchain changes
  (a new Ruff may need a newer Rust) and when Python gains syntax Rotproof fails to read.
- **Try a wheel as a user gets it:** build it (README) and `pip install` it into a fresh venv, outside the build tree.
- **While Rotproof is `0.x`, a release that can make something that worked under the previous version fail or behave
  otherwise raises the minor version; anything else raises the patch version.** Minor: a new or stricter rule, a
  command, flag or field removed, renamed or given another meaning, a command line that succeeded and now stops or
  exits otherwise, a fix that rejects what was wrongly accepted. Patch: a fix that accepts what was wrongly rejected or
  stops a crash, the words of a message, a new command or optional flag, documentation. When in doubt, minor. Projects
  pin the exact version, so they take the change in a commit of their own.
- **Raise the version in `Cargo.toml` and every `pip install rotproof==<version>` and `npm install -D -E
  rotproof@<version>` in `README.md` together;** `cargo xtask ci` fails while they differ (the README is the page on
  PyPI and on npm). Release by pushing the tag `v<version>` of that
  commit. Before the tag, `gh workflow run release.yml` builds and tests the release without publishing it.
