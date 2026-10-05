# AGENTS.md (Rotproof)

This file holds **the map of this repository** and **short rules that apply to every change**. What Rotproof checks,
and why, is in `README.md`.

## Map

| Path | Content |
|---|---|
| `crates/` | The tool, one crate per layer, each with its modules (below): `handler` (the command line: the package and the binary `rotproof`, named for what it ships, and in `tests/cli.rs` the tests that run it as a user runs it), `application`, `domain`, `infrastructure` and `utils` |
| `layers/` | The layer definitions, built into the binary: `table.toml` (what the layers are, in every stack) and one layout per stack (`python.toml`, `typescript.toml`, `rust.toml`: where each layer lives, the files that make it, and the files its toolchain needs at the root) |
| `records/` | The records skeleton, built into the binary: `rules.md`, `spec-rules.md` and `knowledge-rules.md` (the backlog, spec and knowledge rules Rotproof writes into every project), and `log.md` (the log `rotproof create` starts) |
| `project/` | What Rotproof writes into a project outside `docs/`, built into the binary. Each file is named after the one it becomes, without a leading dot and with `.in` added, so no agent here reads a project's `AGENTS.md` as its own: the project's files at the top (`AGENTS.md.in`, `CLAUDE.md.in`, `README.md.in`, `gitattributes.in`), `gitignore/` (one per stack), `pypi/` (the pin, the workflow and the README's instructions for the stacks that install Rotproof from PyPI), `unpinned/` (the README's instructions for the others), and `rotproof/` (the parts of `.rotproof/AGENTS.md`, the guide Rotproof generates for each stack) |
| `xtask/` | The CI entry point (`cargo xtask ci`), which also checks Rotproof's own layers with its library (`layers.rs`) and that every crate takes its dependencies from the workspace (`manifests.rs`), and `cargo xtask licenses`, which writes `THIRD-PARTY-LICENSES.txt` (`--check`: checks it) |
| `.cargo/` | The `cargo xtask` alias |
| `.github/` | GitHub Actions: `ci.yml` runs `cargo xtask ci` in the container, and builds and installs the wheels on Linux and Windows; `licenses.yml` checks `THIRD-PARTY-LICENSES.txt` with cargo-about; `release.yml` releases a pushed tag to PyPI and GitHub Releases, after running each wheel through `smoke.sh` |
| `Cargo.toml` | The workspace of the crates, and the one place that names Rotproof's version (`[workspace.package]`: every crate takes it, and maturin takes the wheel version from it) and each dependency's version or path (`[workspace.dependencies]`: every crate takes them with `workspace = true`) |
| `rust-toolchain.toml` | The one place that names the toolchain. The Dockerfile and CI install from it |
| `Dockerfile`, `compose.yaml` | The development container: the Linux of CI, with the packaging tools and cargo-about |
| `pyproject.toml` | The pip package: a wheel that carries only the binary and the licenses (maturin, `bindings = "bin"`, built from `crates/handler/Cargo.toml`) |
| `requirements-build.txt` | The one place that names the packaging tools (maturin, cargo-xwin). The Dockerfile and CI install from it |
| `THIRD-PARTY-LICENSES.txt` | The licenses of every crate the binary links, which go with the binary: in the wheel next to Rotproof's own. Generated, never edited by hand |
| `about.toml`, `about.hbs` | What cargo-about writes `THIRD-PARTY-LICENSES.txt` from: the licenses Rotproof may ship, and the file's template |
| `README.md` | What Rotproof checks, how to use it, and how to build it |
| `LICENSE-MIT`, `LICENSE-APACHE` | The license: MIT OR Apache-2.0 |

Also tracked, as in most repositories: `.gitattributes`, `.gitignore`, `Cargo.lock`, `AGENTS.md`, and `CLAUDE.md`
(which only imports `AGENTS.md`).

| Module of `crates/handler/src/` | Content |
|---|---|
| `main.rs` | The command line, which is the `handler`: `rotproof init`, `rotproof create`, `rotproof check`, `rotproof guide`, `rotproof index` and `rotproof stop-hook`, and the help that leads from one to the next. It alone builds the adapters in `infrastructure` |

| Module of `crates/domain/src/` | Content |
|---|---|
| `lib.rs` | The layer's role, and its modules |
| `bundle.rs` | `docs/` as one OKF bundle: its documents sorted out once they are read, the files Rotproof generates in it (the index files and the rules), and the tags the records use |
| `schema.rs` | The frontmatter of each document type |
| `records.rs` | The rules of the records `rotproof check` runs on what it read: the types of documents and their places, the files Rotproof would not read, the log's structure and what it points at, the hash of a knowledge document, and the floor of the bundle |
| `tree.rs` | The ports to a project's files (`Tree` to read them, `Writer` to write them), and how Rotproof reads a file's text and how Windows reads a name |
| `code.rs` | What the readers give, as plain values, and the port to them (`Parsers`): the module a Python file is, which files are TypeScript source, and where a TypeScript import lands |
| `direction.rs` | The direction check: every layer imports only what `layers/table.toml` allows (Python, TypeScript, Rust), judged once an import is resolved to a place; and where Cargo looks for a workspace |
| `layers.rs` | The layer definitions in `layers/`, what a project's `.config/rotproof.toml` says, and the fields `rotproof create` adds to it |
| `links.rs` | Whether a link in a record resolves: by its target alone (no URL is checked, no path on one machine, no `\`, nothing above the repository), then by the exact names of its files, to a heading that is there, and to a definition in a `.py` file |
| `markers.rs` | The marker check: no comment in the code holds `TODO`, `FIXME`, `XXX`, `HACK` or `NOTE` (Python, TypeScript, Rust), judged once the comments of a file are read |
| `hook.rs` | What `rotproof stop-hook` finds open in the agent's last message and what it answers, the port to the project's changes (`Changes`), and the settings file that `rotproof create` writes for the hook |
| `project.rs` | The files Rotproof writes outside `docs/`: the guide `.rotproof/AGENTS.md`, and the project's files, once |
| `structure.rs` | The structure check: the tree agrees with `.config/rotproof.toml`, either way, judged from the paths it names and the code files of the directories it lists |

| Module of `crates/application/src/` | Content |
|---|---|
| `lib.rs` | The layer's role, and its modules |
| `bundle.rs` | Reading `docs/` from the tree, directory by directory |
| `check.rs` | `rotproof check`: every check run against one repository, each with its floor, and `docs/` walked and read for the rules of the records |
| `create.rs` | `rotproof create`: making the layers and the records skeleton a project lacks, and adding the fields the declaration lacks |
| `tree.rs` | Reading a project's files through `Tree`: a path read name by name and compared exactly, the code files of a directory, and the text of a file |
| `code.rs` | Where a TypeScript import lands, the tree asked whether a module is under a `baseUrl` |
| `direction.rs` | The direction check, read from the tree: the code of each language read, and every import resolved to a place |
| `layers.rs` | Reading a project's `.config/rotproof.toml` |
| `links.rs` | Whether a link in a record resolves, read from the tree: the file it names looked up and read |
| `markers.rs` | The marker check, read from the tree: every code file walked, and its comments read by the reader of its language |
| `hook.rs` | `rotproof stop-hook`, the hook Claude Code runs when the agent stops |
| `index.rs` | `rotproof index`: every index file written from the frontmatter, and the open items to measure again |
| `init.rs` | `rotproof init`: writing a project's declaration, once |
| `project.rs` | `rotproof guide`: the guide for a stack, named or declared |
| `structure.rs` | The structure check, read from the tree |

| Module of `crates/infrastructure/src/` | Content |
|---|---|
| `lib.rs` | The layer's role, and its modules |
| `python.rs` | Reading Python with Ruff's parser: imports, as the modules they name, comments, and the functions and classes a file defines |
| `typescript.rs` | Reading TypeScript and JavaScript with oxc: imports and comments, and the aliases of the `tsconfig*.json` files |
| `cargo.rs` | Reading a crate's `Cargo.toml` with toml_edit: the dependencies it declares on a path or its workspace, and its workspace |
| `readers.rs` | `Parsers` with the readers above |
| `disk.rs` | `Tree` and `Writer` on the file system, the project's `.gitignore` files kept, the project's name, and the project a directory sits in |
| `git.rs` | `Changes` with `git status` |

| Module of `crates/utils/src/` | Content |
|---|---|
| `lib.rs` | The layer's role, and its modules |
| `frontmatter.rs` | Splitting a document into frontmatter and the sections of its body |
| `markdown.rs` | What GitHub renders as text, headings and their anchors as GitHub computes them, and links |
| `rust.rs` | Reading Rust source with a scanner of Rotproof's own: its comments, past strings and character literals |
| `source.rs` | The paths and lines in messages, and whether a path sits in another |

## How this repository differs from what Rotproof keeps

- **No `docs/` here.** Rotproof's own backlog, specs and log are kept outside this repository.
- **In layers, and no `.config/rotproof.toml`.** Every layer of the Rust layout Rotproof keeps (README) is a crate of
  its own in `crates/`, `handler` among them, and `ui` is left out as that layout leaves it out. `cargo xtask ci`
  checks them as `rotproof check` checks a project's (structure, direction, markers), with the declaration written in
  `xtask/src/layers.rs` in place of the file, and reads the comments of `xtask/` for markers too, which the layout
  leaves out; the records are kept outside, so they are not checked here.
  `domain` calls no port: a rule takes the values `application` has read. Nothing checks that yet.

## Rules for every change

- **A layer or a stack changes in `layers/`, not in code.** The table says what the layers are once; a layout says
  only where they live in one stack. The tests read every definition, so a layout that names a layer the table lacks
  fails.
- **Write Rotproof where prose names the tool, and `rotproof` for the command, the package, the crate and file
  names,** as Ruff and `ruff` are written. A sentence that starts with the command still writes it in backticks.

- **Run CI with `cargo xtask ci`, on the host or in the container (`docker compose run --rm dev cargo xtask ci`).** The
  container is the Linux of CI and needs only Docker. On Windows, the host needs Visual Studio's C++ tools and the
  Windows SDK; without the SDK, linking fails (`kernel32.lib` not found), and Git Bash's own `link` gets in the way.
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
- **A change that can fail records that passed before (a new rule, a stricter rule) raises the minor version** while
  Rotproof is `0.x`. Projects pin the exact version, so they take the change in a commit of their own.
- **Raise the version in `Cargo.toml` and the `pip install rotproof==<version>` in `README.md` together;** `cargo xtask
  ci` fails while they differ (the README is the page on PyPI). Release by pushing the tag `v<version>` of that
  commit. Before the tag, `gh workflow run release.yml` builds and tests the release without publishing it.
