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
| `records/` | The records skeleton, built into the binary: `work-rules.md` and `knowledge-rules.md` (the rules of `docs/work/` and `docs/knowledge/` Rotproof writes into every project), `log.md` (the log `rotproof create` starts) and `milestone.md` (the milestone it writes when `docs/work/` has none) |
| `project/` | What Rotproof writes into a project outside `docs/`, built into the binary. Each file is named after the one it becomes, without a leading dot and with `.in` added, so no agent here reads a project's `AGENTS.md` as its own: the project's files at the top (`AGENTS.md.in`, `CLAUDE.md.in`, `README.md.in`, `gitattributes.in`), `gitignore/` (one per stack), `pypi/` (the pin, the workflow and the README's instructions for the stacks that install Rotproof from PyPI), `unpinned/` (the README's instructions for the others), and `rotproof/` (the parts of `.rotproof/AGENTS.md`, the guide Rotproof generates for each stack) |
| `xtask/` | The CI entry point (`cargo xtask ci`), which also checks Rotproof's own layers with its library (`layers.rs`), that every crate takes its dependencies from the workspace (`manifests.rs`), that `domain` calls no port (`pure.rs`) and the width of comment lines (`width.rs`), and `cargo xtask licenses`, which writes `THIRD-PARTY-LICENSES.txt` (`--check`: checks it) |
| `.cargo/` | The `cargo xtask` alias |
| `.github/` | GitHub Actions: `ci.yml` runs `cargo xtask ci` in the container, and builds and installs the wheels on Linux and Windows and runs them through `smoke.sh`; `licenses.yml` checks `THIRD-PARTY-LICENSES.txt` with cargo-about; `release.yml` releases a pushed tag to PyPI and GitHub Releases, after running each wheel through `smoke.sh` |
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
| `cli.rs` | The command line: `rotproof init`, `rotproof create`, `rotproof check`, `rotproof approve` (which alone asks a person on the terminal: an import kept, or the knowledge documents re-pinned with `--reviewed`), `rotproof guide`, `rotproof index`, `rotproof follows` and `rotproof stop-hook`, and the help that leads from one to the next. Each command is a call of `protocol::inbound::Commands`; what it prints and how it exits are decided here |

| Module of `crates/infrastructure/src/outbound/` | Content |
|---|---|
| `mod.rs` | The level's role, and `Local`: the `Machine` Rotproof runs on, from the modules below and the local clock |
| `python.rs` | Reading Python with Ruff's parser: imports, as the modules they name, comments, the functions and classes a file defines, and the source of one of them |
| `typescript.rs` | Reading TypeScript and JavaScript with oxc: imports and comments, and the aliases of the `tsconfig*.json` files |
| `cargo.rs` | Reading a crate's `Cargo.toml` with toml_edit: the dependencies it declares on a path or its workspace, and its workspace |
| `readers.rs` | `Parsers` with the readers above |
| `disk.rs` | `Tree` and `Writer` on the file system, the project's `.gitignore` files kept, the project's name, and the project a directory sits in |
| `git.rs` | `Changes` with `git status`, and the name `rotproof approve` signs with, git's `user.name` |

| Module of `crates/protocol/src/` | Content |
|---|---|
| `lib.rs` | The layer's role, and its two levels |

| Module of `crates/protocol/src/inbound/` | Content |
|---|---|
| `mod.rs` | The input port: `Commands`, one method per command, and the plain data each takes and returns (`Made`, `Initialized`, `Report`, `Indexed`, `Drift` and the rest) |

| Module of `crates/protocol/src/outbound/` | Content |
|---|---|
| `mod.rs` | The output ports: `Tree` and `Writer` (a project's files read and written, together `Files`), `Parsers` (the readers of code), `Changes` (what version control shows), and `Machine`, which gives each of them for a root, with git's `user.name` and the clock |

| Module of `crates/domain/src/` | Content |
|---|---|
| `lib.rs` | The layer's role, and its modules |
| `approvals.rs` | The approvals file, `.config/rotproof-approved.toml`: the imports the layer table forbids that a person approved, each keyed by its file and what it imports, matched against what the direction check found, and the file with an entry added or removed |
| `bundle.rs` | `docs/` as one OKF bundle: its documents sorted out once they are read, the files Rotproof generates in it (the index files and the rules), and the tags the records use |
| `schema.rs` | The frontmatter of each document type |
| `arrows.rs` | The arrows between the records of `docs/work/` (`after` and `until`): the records they name, the order they keep with how each record closed, the cycles they make with the parents, and what each open record waits for |
| `follows.rs` | What a knowledge document follows: the forms of a key of `follows` (a file, a directory, a Python definition), the hash of a directory, and what fails when one changed or is gone |
| `records.rs` | The rules of the records `rotproof check` runs on what it read: the types of documents and their places, the files Rotproof would not read, the log's structure and what it points at, the hash of a knowledge document, and the floor of the bundle |
| `tree.rs` | How Rotproof reads a file's text and how Windows reads a name |
| `migrate.rs` | Moving the records of a project made before 0.3.0 from `docs/backlog/` and `docs/specs/` into `docs/work/`: when it is due, the move planned whole (each record, every link that reached one, the frontmatter a machine can rewrite) or why it cannot be made, and what it leaves to a person |
| `upgrade.rs` | Upgrading a project's files: the version they are up to (`files` in the declaration), the named updates a later version brings to the files `rotproof create` writes once, and what each does to its file |
| `code.rs` | What the readers give, as plain values: the module a Python file is, which files are TypeScript source, and where a TypeScript import lands |
| `direction.rs` | The direction check: every layer imports only what `layers/table.toml` allows (Python, TypeScript, Rust), judged once an import is resolved to a place; and where Cargo looks for a workspace |
| `modules.rs` | The levels inside a Rust crate: the module each file of `src/` is, every path it names from `crate`, `super`, `self` or a dependency on another layer judged by the level it lands in, the crate's root holding only its levels' modules, and no `#[path]` |
| `layers.rs` | The layer definitions in `layers/`, what a project's `.config/rotproof.toml` says, and the fields `rotproof create` adds to it |
| `links.rs` | Whether a link in a record resolves: by its target alone (no URL is checked, no path on one machine, no `\`, nothing above the repository), then by the exact names of its files, to a heading that is there, and to a definition in a `.py` file |
| `markers.rs` | The marker check: no comment in the code holds `TODO`, `FIXME`, `XXX`, `HACK` or `NOTE` (Python, TypeScript, Rust), judged once the comments of a file are read |
| `hook.rs` | What `rotproof stop-hook` finds open in the agent's last message and what it answers, and the settings file that `rotproof create` writes for Claude Code: the hook, and the rule that denies editing the approvals file |
| `project.rs` | The files Rotproof writes outside `docs/`: the guide `.rotproof/AGENTS.md`, and the project's files, once |
| `structure.rs` | The structure check: the tree agrees with `.config/rotproof.toml`, either way, judged from the paths it names and the code files of the directories it lists |
| `frontmatter.rs` | Splitting a document into frontmatter and the sections of its body |
| `markdown.rs` | What GitHub renders as text, headings and their anchors as GitHub computes them, and links |
| `rust.rs` | Reading Rust source with a scanner of Rotproof's own: its comments, past strings and character literals, the code alone with both blanked, and its tokens, the paths it names, its modules with a body and its attributes |
| `paths.rs` | Paths from the root with `/`: the parent, the name, a join, without `.` and `..`, whether one sits in another, and the paths in messages |
| `text.rs` | The line at a position in a text, for messages |

| Module of `crates/application/src/` | Content |
|---|---|
| `lib.rs` | The layer's role, and its modules |
| `approvals.rs` | Reading the approvals file from the tree, and `rotproof approve`: an entry added for a forbidden import a person approved, and the entries that match nothing removed (`--prune`) |
| `before.rs` | `rotproof before`: the open records (with `--all`, the closed ones too) that close before a record of `docs/work/`, earliest first |
| `bundle.rs` | Reading `docs/` from the tree, directory by directory |
| `check.rs` | `rotproof check`: every check run against one repository, each with its floor, and `docs/` walked and read for the rules of the records |
| `commands.rs` | `protocol::inbound::Commands` implemented: each command opens the project at its root through the `Machine` `composition` passes in, and runs its use case |
| `create.rs` | `rotproof create`: making the layers and the records skeleton a project lacks, and adding the fields the declaration lacks; and the same without making a layer, for an upgrade |
| `tree.rs` | Reading a project's files through `Tree`: a path read name by name and compared exactly, where an import's path lands as the operating system resolves it, the code files of a directory, and the text of a file |
| `code.rs` | Where a TypeScript import lands, the tree asked whether a module is under a `baseUrl` |
| `direction.rs` | The direction check, read from the tree: the code of each language read, and every import resolved to a place |
| `layers.rs` | Reading a project's `.config/rotproof.toml` |
| `links.rs` | Whether a link in a record resolves, read from the tree: the file it names looked up and read |
| `follows.rs` | What a knowledge document follows, read from the tree and hashed as it is now, and the re-pin of `rotproof approve --reviewed` |
| `markers.rs` | The marker check, read from the tree: every code file walked, and its comments read by the reader of its language |
| `hook.rs` | `rotproof stop-hook`, the hook Claude Code runs when the agent stops |
| `index.rs` | `rotproof index`: every index file written from the frontmatter, and the open items to measure again |
| `init.rs` | `rotproof init`: writing a project's declaration, or, when it exists, upgrading the project's files to the running version |
| `project.rs` | `rotproof guide`: the guide for a stack, named or declared |
| `structure.rs` | The structure check, read from the tree |

## How this repository differs from what Rotproof keeps

- **No `docs/` here.** Rotproof's own specs, work items and log are kept outside this repository.
- **In layers, and no `.config/rotproof.toml`.** Every layer of the Rust layout Rotproof keeps (README) is a crate of
  its own in `crates/`, with the levels of `infrastructure` and `protocol` as modules, the entry file is `src/main.rs`
  of the root package, and `ui` is left out as that layout leaves it out. `cargo xtask ci` checks them as
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
  `project/gitignore/`, and in `PYPI` when it installs Rotproof from PyPI (both in `crates/domain/src/project.rs`).
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
- **Raise the version in `Cargo.toml` and the `pip install rotproof==<version>` in `README.md` together;** `cargo xtask
  ci` fails while they differ (the README is the page on PyPI). Release by pushing the tag `v<version>` of that
  commit. Before the tag, `gh workflow run release.yml` builds and tests the release without publishing it.
