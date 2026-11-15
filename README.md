# Rotproof

English | [日本語](https://github.com/secta113/rotproof/blob/main/README.ja.md)

Rotproof keeps a project's structure from drifting while LLMs and people change it, so that it does not rot. It is one
binary with no language runtime, and `rotproof check` is the command a project's CI runs: it fails when the code breaks
the layers the project declared, or when a document about the code is left behind after the code changed.

## What you gain

If LLM agents write the code of a repository, these are the things nobody has to catch in review:

- **A layer an agent crosses fails the build.** Every import is checked against a layer table (the onion architecture:
  `composition`, `infrastructure`, `application`, `protocol`, `domain`, and `ui` and `presentation` for screens), in
  Python, TypeScript and Rust. Code that cannot be split into the layers mixes responsibilities, so the table is also
  how an agent is made to split it. An exception is an entry in an approvals file, with the reason it was given, which a
  reviewer sees in the diff.
- **Each part of a monorepo keeps its own layers.** The packages of a monorepo, or the modules of a modular monolith,
  are declared as units: every import between them is checked, so a part reaches another only through the layers that
  may be reached, and a shared part that collects what no one needs shows in a list that the diff displays (see
  "Units").
- **A manifest lists what the code imports.** The `pyproject.toml` of a Python unit and the `package.json` of a
  TypeScript one are compared with its imports, without installing anything: a package imported and not listed
  fails, and so does a runtime dependency nothing imports. In Python the code of a layer uses runtime dependencies
  only, and a test both; the distribution an import is of (`dotenv` is `python-dotenv`) and a dependency used without an
  import are said in the declaration, and the verdict is the same on every machine.
- **What the code cannot say stays true.** The reasons behind the code are kept in knowledge documents that name the
  code they describe, each pinned to it by a hash. When that code changes, `rotproof check` fails until someone reads
  the document against it and pins it again.
- **Put-off work does not hide in a comment.** A `TODO`, `FIXME`, `XXX`, `HACK` or `NOTE` in a code comment fails the
  check: the finding is fixed now, written in an issue, or let go. A project that declares `todo_link` may keep a
  `TODO` that names the issue tracking it, which every run lists. No comment switches the type checker off, and only
  `infrastructure` reads the environment variables.
- **An agent that has only the binary finds its way.** `rotproof --help` gives the order to start in, and the rules
  are written for the project's stack in `.rotproof/AGENTS.md`, which the `AGENTS.md` and `CLAUDE.md` that
  `rotproof create` writes point the agent at. How to upgrade Rotproof and when to run each command is in the skill
  `rotproof`, which `rotproof create` writes to `.claude/skills/` and `.agents/skills/`.
- **The rules are the ones you pinned.** A project pins one exact version of Rotproof, so the rules change only in a
  commit of the project's own (see "Compatibility").

What it leaves to you: `rotproof check` only reports, and Rotproof repairs nothing. It cannot tell whether a document is
still true, only that the code under it changed, so the review stays with a person or the agent that made the change.

## Install

Rotproof is released for Windows x86_64, Linux x86_64 (glibc 2.17 or newer) and macOS arm64 (Apple silicon, macOS 11 or
newer). Pin the exact version, so that an upgrade, which can bring new rules, is a commit of its own:

```sh
pip install rotproof==2.0.0-rc.1
```

In a Node project, pin it with npm, which installs the binary for your platform with it (the package `rotproof` and
one of `@rotproof/linux-x64`, `@rotproof/win32-x64` and `@rotproof/darwin-arm64`):

```sh
npm install -D -E rotproof@2.0.0-rc.1
```

The wheel carries only the binary; no Python code runs. Without Python or Node, take the archive for your platform from
[GitHub Releases](https://github.com/secta113/rotproof/releases) and check it against `SHA256SUMS` there. Each archive
holds the binary with `LICENSE-MIT`, `LICENSE-APACHE` and `THIRD-PARTY-LICENSES.txt`. The releases are immutable from
the release after 1.0.0-rc.2: once published, their archives cannot be replaced, so a version names one binary. A
`rust` or `none` project made by `rotproof create` installs Rotproof this way in CI.

On another platform, pip finds no Rotproof to install. Build it from source with [rustup](https://rustup.rs/) installed
(the toolchain version comes from `rust-toolchain.toml`):

```sh
cargo build --release   # the binary is target/release/rotproof (rotproof.exe on Windows)
```

## Tutorial: five minutes

A new Python project, a layer broken on purpose, and a knowledge document that follows code. It needs `rotproof` on the
`PATH` (a `pip install` in an activated virtual environment puts it there) and a text editor. The project need not be a
Git repository, and no Python is run. The output shown is what Rotproof printed when this was written, with the lines
left out marked `...`.

### 1. Make the project

```sh
mkdir shop
cd shop
rotproof init --stack python --ui none
```

```text
wrote .config/rotproof.toml
next: declare in absent the layers this project does not have, then run `rotproof create`, which lists the layers it would make, and `rotproof create --yes` to make them
```

`init` writes only the declaration, `.config/rotproof.toml`, which is the structure this project declares: the stack,
and `--ui none` for a project with no screens (a command-line tool, a library, a server). Its `absent` lists the layers
the project does not have; this one has all five, so leave it as it is. `rotproof create` alone lists the layers it
would make and writes nothing, so that the list is read first; with `--yes` it makes them:

```sh
rotproof create --yes
```

```text
wrote composition/__init__.py
...
wrote domain/__init__.py
wrote docs/log.md
wrote .rotproof/AGENTS.md
...
wrote docs/knowledge/index.md
next: run `rotproof check`; the rules it keeps are in .rotproof/AGENTS.md
```

Each layer is a directory whose `__init__.py` says its role. `docs/` holds the records, and the rest is what an agent
reads (`AGENTS.md` and `CLAUDE.md`, which point at `.rotproof/AGENTS.md`, the rules written for the stack) and a CI
workflow that runs `rotproof check`. A new project passes its first check with nothing to write:

```sh
rotproof check
```

```text
the layers and every record keep the rules
```

### 2. Break a layer on purpose

`domain` is the innermost layer and imports no other. Make it import one. Create `infrastructure/outbound/db.py`:

```python
def load_rate(currency: str) -> float:
    return 1.0
```

and `domain/price.py`, which reaches for it:

```python
from infrastructure.outbound.db import load_rate


def convert(amount: float, currency: str) -> float:
    return amount * load_rate(currency)
```

```sh
rotproof check
```

```text
the layers import only what the layer table in .rotproof/AGENTS.md allows:
  domain/price.py:1: imports infrastructure.outbound.db.load_rate, in infrastructure.outbound; domain imports no other layer
```

It exits 1, naming the file, the line and the rule. The table it names is built into Rotproof, and `.rotproof/AGENTS.md`
lists it. Fix it the way the layers ask: `domain` takes the rate as an argument, and whoever calls it in an outer layer
reads the rate. Replace `domain/price.py`:

```python
def convert(amount: float, rate: float) -> float:
    return amount * rate
```

`rotproof check` prints `the layers and every record keep the rules` again.

### 3. Make a document follow the code

The reason `convert` takes a rate is not in the code. Knowledge like that goes in `docs/knowledge/`, in a document that
names the code it describes. A document belongs to one area, so first declare one: in `.config/rotproof.toml`, change
`areas = []` to

```toml
areas = ["pricing"]
```

Start `docs/knowledge/rates.md` with `rotproof new`, which writes the frontmatter, the part that is easy to write
wrongly, and nothing under it:

```sh
rotproof new rates
```

```text
wrote docs/knowledge/rates.md (area: pricing)
```

It then prints what to ask to write the body: why it is so, what was considered and dropped, how it should behave, and
what one must know that lies outside the code. The `description` is left empty, so `rotproof check` fails until it is
written. Write the description and the body, and change the title:

```markdown
---
type: Knowledge
title: A price is converted with the rate it is handed
description: 'convert does not look the rate up: the rate comes from the bank feed, which domain may not reach, so a caller passes it in.'
tags: [pricing]
status: stable
---

The rate is read from the bank's feed in `infrastructure`, and `domain` imports no other layer, so `convert` takes it as
an argument. Looking it up inside `convert` was tried first and was rejected by `rotproof check`.
```

The index of `docs/knowledge/` is generated, never written by hand; `rotproof check` fails until it is written again:

```sh
rotproof check
```

```text
every generated file is up to date:
  out of date, run `rotproof index`: docs/knowledge/index.md
```

Run `rotproof index`, which writes it. Now make the document follow `convert`: read the document against the code, and
pin it.

```sh
rotproof follows knowledge/rates.md domain/price.py::convert
```

```text
  domain/price.py::convert: added 08e2917d
wrote docs/knowledge/rates.md
```

The document now has `follows` in its frontmatter, with the hash of `convert`'s source:

```yaml
follows:
  domain/price.py::convert: "08e2917d"
```

`rotproof check` passes. Change the code, so that the result is rounded to cents, in `domain/price.py`:

```python
def convert(amount: float, rate: float) -> float:
    return round(amount * rate, 2)
```

```sh
rotproof check
```

```text
every knowledge document matches the code it follows:
  knowledge/rates.md follows domain/price.py::convert, which changed since the document was last reviewed: review the document against it, then run `rotproof follows knowledge/rates.md domain/price.py::convert`
```

It exits 1 again: the document may now be wrong, and nothing else would tell. Rotproof does not judge it; a hash says only
that the code changed. Read the document against the new code, add what it lacks (a line at the end of
`docs/knowledge/rates.md`: `The result is rounded to cents.`), and pin it again with the command the message names:

```sh
rotproof follows knowledge/rates.md domain/price.py::convert
```

```text
  domain/price.py::convert: 08e2917d -> d2e4eb3b
wrote docs/knowledge/rates.md
```

`rotproof check` passes. The commit that changes the code now holds the reviewed document beside it.

When a task is done, `rotproof log` adds its entry to `docs/log.md`. It reads today from the clock and writes the entry
at the top under that day's heading, making the heading when there is none:

```sh
rotproof log --title "Round the converted price" --issue acme/shop#12 --problem None \
  --lesson "The rate is passed in, so a test of convert needs no feed"
```

```text
wrote docs/log.md: the entry is under ## 2026-11-16
```

`--problem` and `--lesson` are each needed, and `None` says there was nothing to say; repeat either for a list.

### Where to go from here

Commit the project. The CI workflow it made runs `rotproof check` on every push, and `requirements-dev.txt` pins
Rotproof to an exact version. What `rotproof check` checks, field by field, the commands and the layers of each stack
are in the [specification](https://github.com/secta113/rotproof/blob/main/SPECIFICATION.md), and "Compatibility" says what an upgrade of Rotproof can change.

## Compatibility

From 1.0.0, a release raises exactly one part of the version, chosen by the most breaking change it holds since the
previous release:

- **Major** (`1.2.0` to `2.0.0`): something a project did under the previous version can fail, or do something else,
  under this one. A new rule, or a stricter one, in `rotproof check`. A command, a flag or a field of
  `.config/rotproof.toml` removed, renamed, or given another meaning. A command line that succeeded and now stops or
  exits with another code. A fix that makes `rotproof check` reject what it wrongly accepted.
- **Minor** (`1.1.3` to `1.2.0`): everything that worked still works the same way, and there is something new to use: a
  new command, a new optional flag, a new optional field of the declaration.
- **Patch** (`1.1.2` to `1.1.3`): everything that worked still works the same way, and nothing new is offered: a fix
  that makes Rotproof accept what it wrongly rejected, or stop crashing; the words of a message; documentation.

So whatever makes something that worked under 1.0.0 fail or behave otherwise waits for 2.0.0, and a new rule never
arrives in a minor or a patch. When in doubt, the release is a major. One thing is no sign of a breaking change: every
upgrade, a patch included, fails `rotproof check` until `rotproof init` has run, since `files` in the declaration names
the version the project's files are up to (see [Upgrading Rotproof](https://github.com/secta113/rotproof/blob/main/SPECIFICATION.md#upgrading-rotproof)).

A major goes out first as release candidates (`2.0.0-rc.1`), used in real work before the release follows; between two
candidates anything may break, so pin one only to try it. A minor or a patch goes out directly. pip installs a
pre-release only when it is pinned or asked for with `--pre`, and npm publishes it under the dist-tag `next`.

Projects pin the exact version, so an upgrade is always a commit of the project's own: it tells whoever makes it how much
to read, a patch taken without looking, a minor for what is new, a major with its release notes. Before 1.0, the minor
version took the major's role.

## What Rotproof keeps

It keeps two structures:

- **The layers:** which part of the code may import which, on the onion architecture (`composition`,
  `infrastructure`, `application`, `protocol`, `domain`, and `ui` and `presentation` for screens). Code that cannot
  be split into these
  layers mixes responsibilities, so the layers
  are how an LLM, or a person, is made to split it: every piece of code has to land in a layer whose role and allowed
  imports are written down. Rotproof makes them when a project starts and checks them on every run, so the direction
  of dependencies stays what it was meant to be. A project made of several parts that each hold the layers declares
  them as units (see "Units").
- **The records:** how things are now, and why, as far as the code cannot tell it, in knowledge documents that
  follow the code they describe, so that `rotproof check` fails once the code changes under them (and alarms, for what
  matters only when it is seen); and a log of what each task left behind. Open work is kept in issues, not here.
  Knowledge does not stay outside them, nor does a finding: a `TODO` or `NOTE` in a code comment fails the check,
  unless it is a `TODO` that names its issue in a project that declares `todo_link`.

## Units

A project is one unit unless it says otherwise: one set of layers, where its stack puts them. A project made of several
parts that each hold the layers, such as the packages of a monorepo or the modules of a modular monolith, declares each
part as a unit, one `[[unit]]` table in `.config/rotproof.toml`. Rotproof then keeps the layers inside each unit, and
judges the imports between units too. Each unit says in its own `absent` which layers it lacks, so the `absent` at the
top stays empty. `rotproof init` writes a commented example at the end of the declaration. A Python monorepo with a
shared unit, two units that are not shared, and an app:

```toml
[[unit]]
name = "core"
path = "packages/core/src/core"       # where its layers sit directly
shared = true                         # any unit may import it

[[unit]]
name = "billing"
path = "packages/billing/src/billing"
root = "packages/billing"             # where its manifest is (`path` when left out)

[[unit]]
name = "shipping"
path = "packages/shipping/src/shipping"
root = "packages/shipping"

[[unit]]
name = "cli"
path = "apps/cli/cli"
entry = ["apps/cli/__main__.py"]        # an app: its composition wires the other units
absent = ["application", "protocol", "domain"]   # an app holds only composition and infrastructure
```

`rotproof create --yes` then makes each unit's layers and each entry file that is missing, and the same checks hold in
each unit, with the unit named in a finding. The rules between units are few:

- A unit imports the layers of its own unit as the table says, and no other unit unless it is shared.
- A shared unit imports shared units only, never in a cycle. A layer imports the layer of a shared unit that its own
  layer may import by the table (`infrastructure.inbound` imports `protocol.inbound` of a shared unit), except that
  `application` never imports another unit's `application`: the use cases are passed in. The one addition is
  `protocol.inbound`, which also imports the `domain` of a shared unit, where the value types used across units live,
  so that an input port names them and does not copy their shape.
- A `composition` imports another unit only through that unit's `protocol`, and its `domain` when the unit is shared
  (the value types the ports take), since each unit, a shared one included, builds its own insides in its own
  `composition`. The `composition` of an app wires the units, so it imports their `composition` too, and never another
  app's. An entry file imports the `composition` of its own unit alone.
- An import of another unit names a module of one of its layers, never the unit as a whole: what the code reads from
  outside the layers cannot be seen.

A shared unit is imported by any unit, and nothing stops it from collecting what no one needs. A project that declares
a shared unit keeps `.rotproof/readers.md`: what each unit reads of each shared unit, by name, with a name that only one
unit reads set apart. `rotproof index` writes it from the imports and `rotproof check` fails while it is out of date.
No rule fails for what the list shows, since a library that one unit uses may be shared on purpose: it is for the
person who reads it, and for the diff that shows a name appearing or its readers changing.

Why the code is split into these units is for a person to say. `rotproof init`, run on a project that declares units,
has an area to tag it with and has no document that follows them, drafts `docs/knowledge/units.md` once: a Mermaid
diagram of the units and the imports between them as the code has them, and a section for each unit whose
responsibility is left to write. The
document follows `.config/rotproof.toml::unit`, so `rotproof check` fails when a unit is added, removed or changed
until the document is read against the declaration and pinned again. Every rule, and how each stack imports a unit, is
in [Units](https://github.com/secta113/rotproof/blob/main/SPECIFICATION.md#units).

## Principles

- **The declaration is the truth, and Rotproof does not repair.** The structure a project declares is the structure. A
  tree that differs from it fails, either way, and someone decides whether the tree or the declaration is wrong.
  `rotproof check` only reports. What writes is a command a person or an agent runs, and it says what it did. Such a
  command adds what is missing and leaves the values a person wrote as they are: a layer or a project's file that is
  not there, a field the declaration lacks, an approval, what a named update of `rotproof init` brings
  (see [Upgrading Rotproof](https://github.com/secta113/rotproof/blob/main/SPECIFICATION.md#upgrading-rotproof)), and the files Rotproof generates, which nobody writes by hand. A field the declaration
  lacks takes its first value from the tree, once (`areas` gets the tags the knowledge documents use, so the documents
  that kept the rules keep them); from then on a document with a tag the declaration does not list fails. An update of
  `.claude/settings.json` keeps its keys, their order and their values (it takes out only the stop hook, the time hook and the deny rule
  Rotproof wrote, matched exactly), but writes the whole file again in Rotproof's layout (two spaces of indentation),
  keeping its line endings and byte order mark. These change what a person wrote, each in a command run for that.
  Rotproof never moves or rewrites a record: from 1.0 it reads no records of work, and those of an older project stay
  where they are, as they are. Once a knowledge
  document is reviewed against its code, `rotproof follows` (the keys named) writes the hashes in its `follows`: the
  review is the person's or the agent's, and pinning only records it. `rotproof new` starts a document with the
  frontmatter, which is easy to write wrongly, and an empty description that the check fails on, and no body.
  `rotproof log` adds an entry to the log under today's heading, read from the clock. And `rotproof approve --prune` removes the approvals that match no forbidden import, which
  `rotproof check` fails on: it only takes away, so nothing passes that failed before. Code it never moves: when the
  layer table changes (0.4.0, 1.0.0-rc.3), `rotproof init` says where each layer's code goes, and the person moves
  it.
- **Every rule has a check that can fail.** A rule without a check is only a label, and soon drifts.
- **A check with nothing to check fails.** A check that passes on an empty tree protects nothing.
- **The rules come with the tool.** A project pins one version of Rotproof, and takes improvements to the rules by
  upgrading it, in a commit of its own.
- **The same structure in every language.** Rotproof is one binary with no language runtime, so a Rust or TypeScript
  project keeps the same layers and records as a Python one.
- **Whoever wants to know how things are now reads the index of the knowledge, not every file.** The index files are
  generated, never written by hand. Open work is in the issues.

## Specification

The reference is [SPECIFICATION.md](https://github.com/secta113/rotproof/blob/main/SPECIFICATION.md), one file, so that a section can quote another by its heading:

- [Overview][overview]: what is checked, and the format the records are in (OKF 0.2).
- [Usage][usage]: the commands, how a project starts, and the files `rotproof create` writes once.
- [Upgrading Rotproof][upgrading-rotproof]: what `rotproof init` does after the pinned version changes.
- [The hooks Rotproof no longer writes][the-hooks-rotproof-no-longer-writes]: what became of `stop-hook` and `time-hook`.
- [Approving a forbidden import][approving-a-forbidden-import]: the one way an import the table forbids stays.
- [The layers][the-layers]: the layer table, and where each layer lives in a stack.
- [Units][units]: `[[unit]]`, what a unit may import, who reads the shared units, and the document of the units.
- [The records][the-records]: knowledge documents and `follows`, alarms, areas and the log.
- [What `rotproof check` checks][what-rotproof-check-checks]: every rule, and how each fails.
- [Work put off that an issue tracks][work-put-off-that-an-issue-tracks]: `todo_link`, and a `TODO` that names its issue.

[overview]: https://github.com/secta113/rotproof/blob/main/SPECIFICATION.md#overview
[usage]: https://github.com/secta113/rotproof/blob/main/SPECIFICATION.md#usage
[upgrading-rotproof]: https://github.com/secta113/rotproof/blob/main/SPECIFICATION.md#upgrading-rotproof
[the-hooks-rotproof-no-longer-writes]: https://github.com/secta113/rotproof/blob/main/SPECIFICATION.md#the-hooks-rotproof-no-longer-writes
[approving-a-forbidden-import]: https://github.com/secta113/rotproof/blob/main/SPECIFICATION.md#approving-a-forbidden-import
[the-layers]: https://github.com/secta113/rotproof/blob/main/SPECIFICATION.md#the-layers
[units]: https://github.com/secta113/rotproof/blob/main/SPECIFICATION.md#units
[the-records]: https://github.com/secta113/rotproof/blob/main/SPECIFICATION.md#the-records
[what-rotproof-check-checks]: https://github.com/secta113/rotproof/blob/main/SPECIFICATION.md#what-rotproof-check-checks
[work-put-off-that-an-issue-tracks]: https://github.com/secta113/rotproof/blob/main/SPECIFICATION.md#work-put-off-that-an-issue-tracks

## Development

`cargo xtask ci` runs format, lint and tests, the same checks as CI. It also fails when the map in `AGENTS.md`
misses a tracked top-level path or a module of a crate in `crates/` (or names one that is gone), when
`rust-toolchain.toml`, the `Dockerfile` and the CI workflow name different toolchain versions, and when
`THIRD-PARTY-LICENSES.txt` does not list the crates the binary links (below), and when Rotproof's own crates break
the rules of the Rust layout it keeps: a crate outside the layers, a dependency the table does not allow, or a marker
in a comment of `crates/` or `xtask/`. On Windows the host needs Visual
Studio's C++ tools and the Windows SDK. Everything also runs in the container (`compose.yaml`), where the host needs
only Docker: `docker compose run --rm dev cargo xtask ci`.

The same run fails when a version the README or the specification installs is not the one in `Cargo.toml`, and when a
link in the README to another file is not a full URL (the README is the page on PyPI and npm, where a relative one
breaks) or a link, inside the page or into this repository, names a file or a heading that is not there. It fails too
when this README changed since `README.ja.md` was translated from it: the translation names the hash of the README it
was brought to.

The binary links the crates Rotproof depends on, and their licenses require their notices to go with it.
`THIRD-PARTY-LICENSES.txt` holds them: every crate the binary links on any platform, with the license text each one
ships. Rotproof's own crates in `crates/` (`publish = false`) are Rotproof, and are not listed. It is generated by
[cargo-about](https://github.com/EmbarkStudios/cargo-about) from `about.toml` and the template `about.hbs`, never
edited by hand:

```sh
cargo xtask licenses           # after any change to the dependencies; commit the file with the change
cargo xtask licenses --check   # fails where the file differs from what cargo-about writes now
```

Both need cargo-about at the version the `Dockerfile` installs (`cargo install --locked --features cli
cargo-about@<version>`); the container has it. The generation fails when a crate's license cannot be met from
`accepted` in `about.toml`, the licenses Rotproof agrees to ship. cargo-about takes minutes to build, so `cargo xtask
ci` checks only the list of crates, against `cargo tree`, which needs cargo alone: a new or bumped dependency fails
there until the file is written again. The text is checked by `.github/workflows/licenses.yml`, on a push that changes
the dependencies, `about.toml`, `about.hbs` or the file itself. The Dockerfile and that workflow install the same
version of cargo-about, and `cargo xtask ci` fails when they differ.

The pip wheels for Windows x86_64 and Linux x86_64 are built with maturin, in the container (the macOS wheel is built on
macOS, below):

```sh
docker compose run --rm dev maturin build --release --out dist                                   # Linux
docker compose run --rm dev maturin build --release --target x86_64-pc-windows-msvc --out dist  # Windows
```

Each wheel carries `LICENSE-MIT`, `LICENSE-APACHE` and `THIRD-PARTY-LICENSES.txt` in its `.dist-info/licenses/`
(`license-files` in `pyproject.toml`). The container's Linux wheel is tagged `manylinux_2_34`, after the container's
glibc, and its Windows wheel is cross-compiled with cargo-xwin, which downloads the MSVC runtime and the Windows SDK
under Microsoft's license. Both serve for trying the wheels, not for release.

A release is a pushed tag `v<version>` that matches `Cargo.toml`. `.github/workflows/release.yml` checks the licenses,
builds the Linux wheel in the manylinux2014 image (glibc 2.17), the Windows wheel on Windows and the macOS wheel
(`macosx_11_0_arm64`) on macOS, installs each into a fresh venv on its platform and runs it (`.github/smoke.sh`), and
installs the Linux wheel in the manylinux2014 image too, so the oldest glibc the tag claims is tested. The macOS
binary is checked to ask for no macOS newer than 11.0, which its tag claims; no macOS older than the runner's is run.
It packs an archive of the binary per platform, taken out of the
tested wheel, and `SHA256SUMS`, and installs them as the CI `rotproof create` writes for rust and none does
(`.github/project-ci.sh`), so a project's CI is known to find the archive before it is published: the releases are
immutable. From the archives it makes the npm packages (`npm/pack.py`): one per platform (`@rotproof/linux-x64`,
`@rotproof/win32-x64`, `@rotproof/darwin-arm64`) holding the binary and the licenses, and `rotproof`, whose script runs the binary of the one
npm installed (they are its optional dependencies, each for its own os and cpu). It installs them into a fresh
package on each platform and runs them through `smoke.sh`. It then publishes the wheels to PyPI and the packages to
npm (a version with a pre-release part under the dist-tag `next`), and, once both took the version, makes the GitHub
Release with the wheels, the archives and `SHA256SUMS`. A registry that has the version already is passed over, so
a run that failed after one of them took it can be made again on the same tag. Run by hand
(`gh workflow run release.yml`), it does everything but publish, and runs the publish to npm with `--dry-run`. CI
builds and tests the
Windows and macOS wheels natively on Windows and macOS too. Other platforms (macOS x86_64, Linux aarch64) are welcome
as contributions.

## License

Licensed under either of [MIT](https://github.com/secta113/rotproof/blob/main/LICENSE-MIT) or
[Apache-2.0](https://github.com/secta113/rotproof/blob/main/LICENSE-APACHE), at your option. The crates the binary
includes are under their own licenses, listed with their texts in
[THIRD-PARTY-LICENSES.txt](https://github.com/secta113/rotproof/blob/main/THIRD-PARTY-LICENSES.txt). The links are full
URLs so that they work on PyPI and npm, where this README is the project's page.
