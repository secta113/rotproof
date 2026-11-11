# Rotproof

Rotproof keeps a project's structure from drifting while LLMs and people change it, so that it does not rot. It keeps
two structures:

- **The layers:** which part of the code may import which, on the onion architecture (`composition`,
  `infrastructure`, `application`, `protocol`, `domain`, and an optional `ui`). Code that cannot be split into these
  layers mixes responsibilities, so the layers
  are how an LLM, or a person, is made to split it: every piece of code has to land in a layer whose role and allowed
  imports are written down. Rotproof makes them when a project starts and checks them on every run, so the direction
  of dependencies stays what it was meant to be.
- **The records:** how things are now, and why, as far as the code cannot tell it, in knowledge documents that
  follow the code they describe, so that `rotproof check` fails once the code changes under them (and alarms, for what
  matters only when it is seen); and a log of what each task left behind. Open work is kept in issues, not here.
  Knowledge does not stay outside them, nor does a finding: a `TODO` or `NOTE` in a code comment fails the check.

## Principles

- **The declaration is the truth, and Rotproof does not repair.** The structure a project declares is the structure. A
  tree that differs from it fails, either way, and someone decides whether the tree or the declaration is wrong.
  `rotproof check` only reports. What writes is a command a person or an agent runs, and it says what it did. Such a
  command adds what is missing and leaves the values a person wrote as they are: a layer or a project's file that is
  not there, a field the declaration lacks, an entry a person approved, what a named update of `rotproof init` brings
  (see "Upgrading Rotproof"), and the files Rotproof generates, which nobody writes by hand. A field the declaration
  lacks takes its first value from the tree, once (`areas` gets the tags the knowledge documents use, so the documents
  that kept the rules keep them); from then on a document with a tag the declaration does not list fails. An update of
  `.claude/settings.json` keeps its keys, their order and their values (it takes out only the stop hook Rotproof wrote
  before 1.0, matched exactly), but writes the whole file again in Rotproof's layout (two spaces of indentation),
  keeping its line endings and byte order mark. These change what a person wrote, each in a command run for that.
  Rotproof never moves or rewrites a record: from 1.0 it reads no records of work, and those of an older project stay
  where they are, as they are. Once a knowledge
  document is reviewed against its code, `rotproof follows` (the keys named) and `rotproof approve --reviewed` (a
  person, on a terminal) write the hashes in its `follows`: the review is the person's or the agent's, and pinning
  only records it. And `rotproof approve --prune` removes the approvals that match no forbidden import, which
  `rotproof check` fails on: it only takes away, so nothing passes that failed before. Code it never moves: when the
  layer table changes (0.4.0), `rotproof init` says where each layer's code goes, and the person moves it.
- **Every rule has a check that can fail.** A rule without a check is only a label, and soon drifts.
- **A check with nothing to check fails.** A check that passes on an empty tree protects nothing.
- **The rules come with the tool.** A project pins one version of Rotproof, and takes improvements to the rules by
  upgrading it, in a commit of its own.
- **The same structure in every language.** Rotproof is one binary with no language runtime, so a Rust or TypeScript
  project keeps the same layers and records as a Python one.
- **Whoever picks up the work next reads one index, not every file.** The index files are generated, never written by
  hand.

Rotproof makes the layer directories, checks that they are where the project declares them, and checks their
direction: from the imports in Python and TypeScript, and in Rust from the dependencies each crate declares and, for
the levels inside a crate, from the paths its code names.

The records live in `docs/`, which is a bundle in [OKF 0.2](https://github.com/GoogleCloudPlatform/open-knowledge-format)
(its `SPEC.md` as of commit `ad30107`): every document has YAML frontmatter with a `type`, `index.md` and `log.md` are
reserved names, and the log's headings are dates. An OKF reader can read the records as a bundle. The rules on top of
that format (a knowledge document pinned to the code it follows, a deprecated one opening with its resolution, an
alarm's two headings, the shape of the log) are Rotproof's own, stricter than OKF, and not part of it.
Rotproof is not an OKF validator. In one place the records depart from a SHOULD of OKF, which asks that every
timestamp be a datetime with a UTC offset (section 5): the `at` of `verified` and `stale_after` are written as a day
(`YYYY-MM-DD`), and Rotproof reads only the day of either, of a datetime too. An agent has no clock, and no check can
tell a time of day it guessed a few minutes early from one it measured, so the records hold no time of day rather than
one that may be untrue; OKF's `verified` is written for a person or a process with a clock to confirm. A reader that
knows only OKF may not read a day there as a datetime.

## Install

Rotproof is released for Windows x86_64 and Linux x86_64 (glibc 2.17 or newer). Pin the exact version, so that an
upgrade, which can bring new rules, is a commit of its own:

```sh
pip install rotproof==1.0.0-rc.2
```

The wheel carries only the binary; no Python code runs. Without Python, take the archive for your platform from
[GitHub Releases](https://github.com/secta113/rotproof/releases) and check it against `SHA256SUMS` there. Each archive
holds the binary with `LICENSE-MIT`, `LICENSE-APACHE` and `THIRD-PARTY-LICENSES.txt`.

On another platform, pip finds no Rotproof to install. Build it from source with [rustup](https://rustup.rs/) installed
(the toolchain version comes from `rust-toolchain.toml`):

```sh
cargo build --release   # the binary is target/release/rotproof (rotproof.exe on Windows)
```

## Usage

```sh
rotproof --root <repository> init --stack python  # write .config/rotproof.toml, when there is none
rotproof --root <repository> init    # after upgrading Rotproof: update the project's files (see "Upgrading Rotproof")
rotproof --root <repository> create --yes  # make the layers .config/rotproof.toml declares, the records and the guide
rotproof --root <repository> check   # check the layers and the records; exits 1 when a rule is broken
rotproof --root <repository> index   # write every generated file in docs/ (the index files and the knowledge rules)
rotproof --root <repository> approve <file> <import>  # keep a forbidden import, asked on a terminal (see below)
rotproof --root <repository> approve --prune  # remove the approvals that match no forbidden import
rotproof --root <repository> approve --reviewed  # re-pin the knowledge documents whose followed code changed
rotproof --root <repository> follows <document> <key>...  # pin keys in a knowledge document's follows, as the code is now
rotproof guide --stack python        # print the rules Rotproof keeps for a stack, with or without a project
rotproof time-hook                   # run by Claude Code on each prompt, to tell the agent the time (see "The time hook")
```

`--root` defaults to the current directory. The declaration is read from `<repository>/.config/rotproof.toml`, and the
records from `<repository>/docs`.

The binary leads the way without this README, for an agent that has only Rotproof: `rotproof --help` says what it is,
the order to start in and the exit codes, `rotproof <command> --help` what a command reads, writes and never does, and
each command's output names the next step. A test follows that path from an empty directory to a passing check.

Start a project with `rotproof init --stack <stack>` (`python`, `typescript`, `rust`, or `none` for a repository
that keeps records only). It writes only the declaration, so you declare in `absent` the layers you do not want before
anything is made. Then run `rotproof create --yes`. Run on a project that has a declaration, `rotproof init` upgrades
the project's files instead (see "Upgrading Rotproof"), and never changes the declaration's values but `files`.

`rotproof create` makes layers only with `--yes`. Without it, a run that would make one lists the layers, writes
nothing and exits 2: run straight after `rotproof init`, it would otherwise make every layer of the stack, `ui` and its
five levels in a command-line tool too, and it never deletes them again. A run that makes no layer, as after an
upgrade, needs no `--yes`.

A TypeScript project with React starts from Vite: run `npm create vite@latest <name> -- --template react-ts` first,
then `rotproof init --stack typescript` and `rotproof create --yes` in it, which keep what Vite wrote. Vite's entry point,
`src/main.tsx`, is the entry file where it is (`index.html` loads it), and may import `composition` alone: what Vite
has it import (`./App.tsx`, `./index.css`) moves into `src/composition/`, which `main.tsx` calls. `rotproof check`
names the rest of the starter (`App.tsx`, the styles, `assets/`) as code outside the layers, with where each goes.

Run `rotproof create` when a project starts, and again after you change `.config/rotproof.toml` on purpose. It makes
only what is missing: a layer that is neither present nor declared absent, the project's files when they do not exist
(below), `docs/knowledge/`, and the files Rotproof generates (`.rotproof/AGENTS.md`, and the index files and the
knowledge rules in `docs/`), so a new project passes its first check with nothing for a person to write. It never
overwrites a file it does not generate, apart from adding the fields the declaration lacks (below), and never moves or
deletes one. Nothing runs it on its own, so a layer removed by mistake fails `rotproof check` instead of coming back.

The project's files are written once, as a starting point, and are the project's from then on:

| File | Content |
|---|---|
| `AGENTS.md` | The map (the layers present, with their roles to rewrite), the differences from what Rotproof keeps, and the project's own rules. It points at `.rotproof/AGENTS.md` |
| `CLAUDE.md` | `@AGENTS.md` and `@.rotproof/AGENTS.md` |
| `README.md` | The project's name (its root directory's) and how to run Rotproof |
| `.gitignore`, `.gitattributes` | For the stack; line endings as LF |
| `docs/log.md` | The log, with its title |
| `.claude/settings.json` | The time hook (see "The time hook"), and the rule that denies Claude Code editing the approvals file (see "Approving a forbidden import") |
| `requirements-dev.txt` | `python` and `none`: Rotproof pinned with `==` |
| `.github/workflows/ci.yml` | `python` and `none`: installs `requirements-dev.txt` and runs `rotproof check`, with a time limit |
| `Cargo.toml` | `rust`: the workspace, whose members are the crates in `crates/`, so `cargo build` builds every layer present, and its root package, which depends on `composition` |
| `src/main.rs` | `rust`: the entry file, which calls `composition` and starts the program |

How a `typescript` or `rust` project pins Rotproof is not decided yet, so for those stacks `rotproof create` writes
neither the pin nor the workflow, and says so.

When a newer Rotproof requires a field the declaration lacks, `rotproof check` fails and says so, and
`rotproof create` adds the field under a comment that says what it is and where its first value came from (`areas`
gets the tags the knowledge documents use, sorted by name), keeping every comment and value already there. A value that is present
is never changed, so an upgrade fails only on what the new rules find.

### Upgrading Rotproof

Upgrade in a commit of its own: change the pinned version, install it, run `rotproof init`, then `rotproof check`.

The files `rotproof create` writes once are the project's from then on, so a later version cannot simply rewrite
them. What it adds to them comes as named updates instead, which `rotproof init` applies when it runs on a project
that has a declaration:

- **`files` in the declaration is the version the project's files are up to.** `rotproof init` writes it; a
  declaration without it is up to 0.1.0. `rotproof check` fails while it is older than the running Rotproof, whether
  or not an update applies, so every upgrade runs `rotproof init` once.
- **`rotproof init` applies every update between `files` and the running version,** except those `declined` lists,
  does what `rotproof create` does after an upgrade without making a layer (the fields the declaration lacks,
  `.rotproof/AGENTS.md`, the generated files in `docs/`, and `docs/knowledge/`, the log and the project's files where
  they are missing), and sets `files` to the running version, adding `declined = []` when the declaration lacks it; it
  names every field it adds to the declaration, as `rotproof create` does. Crossing 0.4.0, which builds the layer table
  on the onion architecture, it says where the code of each layer that is gone goes (`handler`, `utils`) and that the
  ports move from `domain` to `protocol.outbound`, as "move by hand" lines; it moves no code, and keeps nothing behind:
  `rotproof check` names what is not moved yet. It never changes the declaration's other values, and once it has set
  `files`, running it again changes nothing.
- **An update only adds, or takes out what Rotproof itself wrote and no longer keeps, matched exactly,** and does
  nothing when its file is as the update leaves it already. It writes the file back with the line endings and the
  byte order mark it had. What it cannot do without a person (its file is not in a form Rotproof can read),
  `rotproof init` says, exits 2 and leaves `files` as it was: do it by hand, or decline the update by its name in
  `declined`, and run `rotproof init` again. When its file is missing, the same run writes the file as
  `rotproof create` does now, with what every update adds, and the update asks for nothing.

| Update | Version | File | What it does |
|---|---|---|---|
| `claude-deny-approvals` | 0.2.0 | `.claude/settings.json` | Adds `"Edit(/.config/rotproof-approved.toml)"` to `permissions.deny` (see "Approving a forbidden import"). The file is written again with its keys in their order and two spaces of indentation |
| `claude-stop-hook-on-path` | 0.4.0 | `.claude/settings.json` | Adds a second `Stop` hook that said when `rotproof` was not on the `PATH`, as a group of its own after the others, while a `Stop` hook runs `rotproof stop-hook`. Written again as above. Not applied in an upgrade that also crosses `claude-remove-stop-hook`, which would take it out again |
| `claude-time-hook` | 0.4.0 | `.claude/settings.json` | Adds the time hook on `SessionStart` and `UserPromptSubmit` (see "The time hook"), each as a group of its own after the others there, while a `Stop` hook runs `rotproof stop-hook`. Written again as above |
| `claude-remove-stop-hook` | 1.0.0-rc.1 | `.claude/settings.json` | Takes out each `Stop` hook whose command is exactly `rotproof stop-hook` or the second hook of `claude-stop-hook-on-path`, a group left with no hook, and `Stop` when no group is left (see "The time hook"). Written again as above: an untouched file ends as `rotproof create` writes it now |

Among the files Rotproof generates is `.rotproof/AGENTS.md`: the rules Rotproof keeps, written for the project's
stack. It says how to run Rotproof, lists the layers with where each lives and what it may import (from
`layers/table.toml`), and gives the rules of the records, starting with reading the knowledge index before work, and
where open work is kept (an issue). It names
the Rotproof version that wrote it, so after an upgrade `rotproof check` fails until `rotproof init` (or
`rotproof create`) has rewritten it. Edited by hand, it fails too: a project's own rules go in its own `AGENTS.md`.

The guide only helps if your agent reads it. The `AGENTS.md` and `CLAUDE.md` that `rotproof create` writes point at it;
a project that has its own points at it from its `AGENTS.md` (or whatever file its agent reads first), and imports it
in `CLAUDE.md` for Claude Code:

```markdown
@AGENTS.md
@.rotproof/AGENTS.md
```

## The time hook

An agent has no clock of its own. It writes the days in the records from what it was last told, and a guess runs ahead
of the clock: `rotproof check` fails on a day that has not come (see "What `rotproof check` checks").
`rotproof time-hook` is [Claude Code's](https://code.claude.com/docs/en/hooks) `SessionStart` and `UserPromptSubmit`
hook: at the start of every session (a resumed or a compacted one too) and on every prompt, it gives the agent the
time now, as `additionalContext`, to write from. Within a long turn the time goes on, so it says to read the clock for
a later one. `rotproof create` writes it into `.claude/settings.json`; a project made before 0.4.0 gets it from
`rotproof init` (the update `claude-time-hook`). A project that has its own settings adds it:

```json
{
  "hooks": {
    "SessionStart": [{ "hooks": [{ "type": "command", "command": "rotproof time-hook" }] }],
    "UserPromptSubmit": [{ "hooks": [{ "type": "command", "command": "rotproof time-hook" }] }]
  }
}
```

`rotproof` has to be on the `PATH` the agent runs hooks with (for a venv, start the agent with the venv active). The
hook fails (an input from another hook, a command line that is wrong) with exit 1, which Claude Code shows while the
prompt goes on; exit code 2 would block the prompt. It reads no `--root`, so one that is not a directory changes
nothing.

Before 1.0, Rotproof also wrote a `Stop` hook, `rotproof stop-hook`, which sent the agent back when its last message
left something open and nothing in `docs/` changed. It went with the records of work it asked for. `rotproof init`
takes it out of `.claude/settings.json` (the update `claude-remove-stop-hook`); until then, the command reads the hook
input, says in a `systemMessage` that it does nothing now, which keeps no agent from stopping, and exits 0.

## Approving a forbidden import

Sometimes a project means to break the layer table: a framework forces an import, or a fix cannot wait for the
refactoring. Such an import passes only as an entry in `.config/rotproof-approved.toml`, which a person adds on a
terminal:

```sh
rotproof approve domain/model.py infrastructure.db
```

The file and the import are named as `rotproof check` names them: the file from the root, and the import as the check
writes it (a Python dotted name, a TypeScript specifier, a crate's dependency). In Python, each name of
`from m import n` is its own import, `m.n`, whether `n` is a module or not: approving `m` approves none of them.
`rotproof approve` shows the forbidden import, asks why it is kept and for a `y`, signs with git's `user.name` (or a
name it asks for when that is not set), and adds the entry (the file keeps a byte order mark and its line endings,
CRLF on every line when one line had it, and so it does when `--prune` removes entries):

```toml
[[kept]]
from = "domain/model.py"
import = "infrastructure.db"
reason = "the ORM's session factory must be built where the model is"
approved = { by = "someone", at = "2026-10-06T09:00:00+09:00" }
```

- **It runs only when stdin is a terminal,** so an agent's shell cannot approve: a person does. There is no comment
  that silences the check on the import's line: it would be the easiest thing for an agent to write.
- **Claude Code is denied editing the file.** The `.claude/settings.json` that `rotproof create` writes holds
  `"permissions": { "deny": ["Edit(/.config/rotproof-approved.toml)"] }`, which refuses Claude Code's edit tools and
  its shell's file commands and redirects (`>>`, `sed -i`) on the file, with no prompt. A person edits it in an
  editor, or through `rotproof approve`.
- **An approval never hides.** `rotproof check` passes the approved import on every line of its file, and prints every
  approval on every run.
- **An approval follows the code.** An entry that matches no forbidden import fails: the import moved away, was
  fixed, or became allowed. `rotproof approve --prune` removes those entries. It needs no terminal, since it only
  takes approvals away and never lets anything pass that failed before.
- **This raises the cost of an exception; it does not prevent everything.** A script that writes the file passes. The
  point is that an exception is a deliberate act that a reviewer sees in the diff, never a side effect of fixing a
  failure.

## The layers

A project declares its structure in `.config/rotproof.toml`, the directory tools share for their configuration.
`rotproof init` writes it, and from then on it is the project's file: Rotproof only adds a field it lacks and sets
`files` on an upgrade. pip installs only the Rotproof binary: the layer
definitions are built into it, and the declaration is never shipped with it.

```toml
stack = "python"         # python | typescript | rust | none
areas = ["billing", "records"]  # the areas knowledge is grouped by, in this order (see The records)
absent = ["ui"]          # layers this project does not have
unchecked = ["scripts"]  # paths outside the layers that Rotproof does not look into
```

What the layers are is written once, in `layers/table.toml`, on the onion architecture:

| Layer | Its levels | May import |
|---|---|---|
| `composition` | | every layer: the one place that wires the program |
| `ui` | `pages` > `templates` > `organisms` > `molecules` > `atoms` (a level imports the levels below it) | `application`, `protocol`, `domain`; only `pages` call `application` |
| `infrastructure` | `inbound` (entry points: the command line, HTTP, events, jobs), `outbound` (connections: databases, clients, files) | `inbound`: `protocol.inbound` alone. `outbound`: `protocol.outbound` and `domain` |
| `application` | | `protocol`, `domain`: a use case implements an input port and calls output ports |
| `protocol` | `inbound` (input ports, with plain data), `outbound` (output ports) | `inbound`: nothing. `outbound`: `domain` |
| `domain` | | nothing |

The levels of a layer other than `ui` never import each other. The entry file is no layer, and may import
`composition` alone. No layer holds general-purpose parts: a part goes in the layer whose concept it speaks.

Where they live is written once per stack:

| Stack | A layer is | A level is | The entry file | Code Rotproof looks at |
|---|---|---|---|---|
| `python` | `<layer>/__init__.py`, the role as its docstring | `<layer>/<level>/__init__.py` | `__main__.py` | `.py` files anywhere (`.PY` too), except `tests/` |
| `typescript` | `src/<layer>/index.ts`, the role as a doc comment | `src/<layer>/<level>/index.ts` | `src/main.tsx` (Vite's entry point) | every file in `src/` |
| `rust` | a crate, `crates/<layer>/`, the role as `//!` | a module, `crates/<layer>/src/<level>/mod.rs` (no `ui` yet) | `src/main.rs` of the root package | every crate in `crates/`, and the root `Cargo.toml` |

`stack = "none"` declares a repository that keeps records only: `rotproof create` makes no layer, only `docs/`, the
guide and the project's files, and `rotproof check` checks only the records and prints that it did not check the
layers. It is a line in the declaration, not a flag, so the structure check is never switched off where the
declaration still declares layers.

A level is declared absent by its dotted name (`absent = ["ui.templates"]`). Files `.gitignore` excludes and
hidden files are not looked at, so a virtual environment or a build directory is not code. A code file is matched in any
case (`stray.PY`, `cargo.toml`): Windows runs or reads it all the same.

## The records

```
docs/
  index.md          generated
  log.md            what each task left behind, newest first
  knowledge/        how things are now, and why (type: Knowledge, status: stable or deprecated)
    rules.md        generated: the knowledge rules (type: Guide)
    index.md        generated
    <slug>.md       one document per file
```

Rotproof keeps two records, and nothing that is open. Open work (what is left, how far a task has come, the questions
it raised) is kept in the project's issues: it opens, moves and closes, and is no description of how things are. What
goes stale is a description of how things are once the code changes, and that is what Rotproof keeps from going stale.
The rest of `docs/` is the project's, and Rotproof reads none of it: the `docs/work/` of a project made before 1.0
stays as it is, no longer checked or written.

`knowledge/` holds how things are now, and why, as far as the code cannot tell it: why the code is so and what was
rejected, how it should behave where the code says only how it does, and what lies outside the repository (the
promises of a service it calls, the data in production, how to run or operate it). A summary of how the code works is
left out: the code says that, and a copy goes stale. A document is edited in place, git keeping its history, and is
deprecated, opening with `# Resolution`, only when it no longer holds. A document that describes code names it in
`follows`, each with its hash when the document was last reviewed: a file in any language; in Python, TypeScript,
JavaScript and Rust also a definition, written `<file>::<Name>.<Name>` in all of them (`src/api.py::Router.add`,
`src/api.ts::Router.add`, `crates/x/src/cli.rs::Command.Before`); or every file Rotproof reads as code under a
directory (`src/api/`: `.py`, `.ts`, `.tsx`, `.mts`, `.cts`, `.js`, `.jsx`, `.mjs`, `.cjs` and `.rs`, in any case,
whatever the stack of the project). A definition is hashed on its own source, so a change elsewhere in its file does
not fail it. A name that is more than one piece in its file (overloads, a getter and its setter, a Python property and
its `@x.setter`, an interface merged with a class, the `impl` blocks of a Rust type) is hashed on all of them, in the
order they appear, joined with a line break. Every definition counts, exported or not. In Python, a function or class
and its methods, from the first decorator; in TypeScript and JavaScript, a function, class, interface, type, enum or
namespace, a variable (`export const f = () => ...`, the whole statement), the members of a class, an interface and an
enum, and `default` for an `export default` that has no name; in Rust, a `fn`, `struct`, `enum` and its variants,
`union`, `trait` and its items, `type`, `const`, `static`, `macro_rules!` and an inline `mod` with its items
(`tests.helper`). A Rust type's key covers its definition and every `impl` block for it in the file, `Type.method` is
the method of that name in any of them, and a Rust definition starts at its first attribute or `///` doc comment. What
a macro defines is not seen. Once the document is reviewed against the code, `rotproof follows knowledge/api.md
<key>...` (or `docs/knowledge/api.md`) pins each key at its hash as the code is now, adding a key `follows` lacks with
its hash quoted, and writes all the keys or none: pinning changes the document, so its commit records the review.
Nothing else in the document changes: it keeps a byte order mark and its line endings (CRLF on every line when one
line had it). Run it when a document starts to follow something, and when `rotproof check` fails because what it
follows changed since: the check names the command to run. It pins only the keys named, needs no terminal, and leaves
a key gone from the code to edit by hand. After a refactoring that moved code without changing what it does, a person
can re-pin every changed hash in one act: `rotproof approve --reviewed` lists each document and what changed under it,
says to read the changes first (a hash says only that the code changed), asks for one `y` on a terminal (an agent's
shell has none), writes the new hashes (each document keeping a byte order mark and its line endings, as `rotproof
follows` does), and names the documents it wrote. What is gone is left for the person to edit. No LLM judges whether
the meaning changed: the agent that refactors is usually one.

Some knowledge matters only when something is seen: a service refuses an upload, a CI image moves. Such a document is
an alarm, with `# When` (what is seen and where, in the words of the output) and `# Do` (what to do then, usually
opening an issue), and optionally `match`, the short strings of the output that stay when its wording changes. It is
knowledge and not work: nothing waits yet, and it may never come, so an issue for it would stay open forever. The
knowledge index lists the alarms first, under `# When something happens`, with what is seen and the strings: whoever
meets a message searches `docs/` for it, and reading the index before work watches for them.

Every knowledge document belongs to exactly one area: its only tag, one of the `areas` the declaration lists. The
index groups by area, in the order of `areas`, so the project puts the largest or most active area first. An area says
where a document belongs (the layers, billing), and never closes. A declared area that no document uses passes, so an
area is declared before its first document. Renaming an area is editing `areas` and the tag of every document in it,
deprecated ones included. Guides keep their optional tags, which name no area.

A knowledge document has this frontmatter; its body is free, apart from `# Resolution` and an alarm's two headings:

```markdown
---
type: Knowledge
title: Uploads are tried three times
description: 'The storage service drops about one upload in a thousand: why three tries, and why not more.'
tags: [storage]
status: stable
follows:
  src/infrastructure/outbound/storage.py::upload: "3e1f0a9c"
verified: {by: human:someone, at: 2026-10-01}
stale_after: 2027-01-01
---
```

- `tags`: exactly one, declared in `areas`; the index groups documents by it.
- `status`: `stable` = holds, `deprecated` = no longer holds, with `# Resolution` first: what replaced it.
- `follows`, optional: the code it describes, each key with its hash when the document was last reviewed.
- `match`, optional, an alarm's only: the strings of the output it is found by.
- `verified`, optional: who reviewed it, and the day (`at`). A datetime with a time zone passes too, read as its day
  there.
- `stale_after`, optional: the day from which to look at it again, for what it says of outside the code. From that
  day `rotproof check` warns of it, and fails nothing.

The log holds one entry per task, under the day's `## YYYY-MM-DD` heading, newest first: the task, its issue, what went
wrong and what was learned, which is found nowhere else, and searchable in the repository when something breaks. What
changed is in the commits, what was measured and decided along the way in the task's issue, and what still holds in
`knowledge/`.

## What `rotproof check` checks

- **The tree matches `.config/rotproof.toml`, either way:** the declaration names a known stack, and only layers that
  stack has in `absent` (a misspelled field fails). Without a declaration nothing is checked, and `rotproof check`
  exits 2, not 1: CI tells a project that has not adopted Rotproof from one that breaks a rule. Every layer of the stack is present or declared
  absent, and no layer declared absent is present. No code sits outside the layers, the stack's own paths (`tests/`)
  and `unchecked`. A path in `unchecked` exists and neither holds nor sits in a layer, so a layer cannot be switched
  off by listing it. At least one layer is present: with every layer declared absent, nothing would be checked.
  A layer with levels holds only its levels: code beside them fails, apart from the layer's own file
  (`ui/__init__.py`). What the whole UI shares goes in a level: a part that knows no project concept, visible or not
  (a design value, one behaviour, a provider of a theme), is an atom. A directory of a layer of the table before
  0.4.0 (`handler`, `utils`) is named with where its code goes now.
- **The layers import only what the table allows** (Python): every `import` and `from ... import` in the layers,
  relative ones and those inside functions or under `if TYPE_CHECKING:` included, and those of the entry file. A
  layer may import itself and what its `imports` name (a layer with its levels, or one level); a level what its own
  `imports` name, and in `ui` the levels below it. Only direct imports
  are judged: what the table allows is closed under chaining, so a chain of allowed imports never reaches a forbidden
  layer. A module is placed where its parts, as a path, land as the operating system that runs the check resolves
  them: on Windows, `Infrastructure.db` lands in `infrastructure`, as Python imports it with `PYTHONCASEOK` set.
  Imports of modules in no layer (the standard library, packages) are not judged, and imports built at run time
  (`importlib`) are not seen. A file that is not UTF-8 or has a syntax error fails, since its imports cannot all be
  read.
- **The layers import only what the table allows** (TypeScript): every `import` (`import type` too), `export ...
  from`, `import x = require(...)`, and `import(...)` and `require(...)` with a literal string, in the `.ts`, `.tsx`,
  `.js` and `.jsx` files (and `.mts`, `.cts`, `.mjs`, `.cjs`) of the layers, read with
  [oxc](https://oxc.rs/). The place of an import is the path it lands on, whether or not a file is there: a relative
  specifier from the file, one starting with `/` from the root (as Vite reads it), and any other through
  `compilerOptions.paths` of the `tsconfig*.json` files at the root (and the local files they extend), then through
  their `baseUrl` when a module is there; otherwise it names a package, which is not judged. The path is placed where
  it lands as the operating system that runs the check resolves it: on Windows, `../Infrastructure/db`,
  `../infrastructure./db` and a short name (`../INFRAS~1/db`) land in `infrastructure`, as Vite builds them, and
  links are followed everywhere. A `tsconfig*.json` that
  cannot be read, or a `paths` entry with more than one `*`, fails rather than leaving its aliases unjudged. Two
  limits: only the first target of a `paths` entry is used, and a config extended from a package is not read, so an
  alias defined only there is taken for a package.
- **The layers import only what the table allows** (Rust): a crate is compiled only against the crates its
  `Cargo.toml` declares, so the declarations are read instead of `use`: every dependency in `[dependencies]` and
  `[build-dependencies]` (and `build_dependencies`), under `[target.<cfg>]` too, of the `Cargo.toml` files in the
  layers. The place of a dependency is where the path it comes from lands: its `path` from the crate, or with
  `workspace = true` the `path` of its entry in `[workspace.dependencies]` from the workspace, which is the directory
  `[package] workspace` names, or the nearest one up to the root that declares `[workspace]`. A path lands where the
  operating system that runs the check resolves it, as Cargo's does: on Windows, a path in another case, with dots or
  spaces at its end or with a short name (`DOMAIN~1`) lands in `domain`, and links are followed everywhere. A path
  that lands nowhere, where Cargo fails too, or outside the root is not judged. A renamed dependency (`package`) is
  placed by its path all the same. `[dev-dependencies]` serve the tests and are not judged; dependencies from a
  registry or git are not judged. A `Cargo.toml` that is not UTF-8 or
  TOML fails, and so does a dependency taken from a workspace that does not declare it. The root `Cargo.toml` is read
  for the entry file, `src/main.rs` of the root package. Not seen: `[patch]` and `[replace]`, and source files taken
  from another crate's directory (`include!`, `[lib] path`).
- **The levels of a crate import only what the table allows** (Rust): a level is a module of its layer's crate,
  which the manifest cannot tell apart, so the code of `src/` is read. Every path from `crate`, `super`, `self` or a
  dependency on another layer, in a `use`, an expression or the tokens of a macro, is judged by the level it lands in
  (`crate::outbound::Db` from `src/inbound/` fails, and so does `domain::Song` there). The crate's `src/lib.rs` only
  declares its levels' modules, so no re-export lets one level reach another; every other file of `src/` sits in a
  level, and `#[path]` fails. Not seen: a path a macro builds from pieces.
- **Every approval matches a forbidden import** (see "Approving a forbidden import"): a forbidden import with an entry
  in `.config/rotproof-approved.toml` for its file and import passes, and is printed on every run; an entry that
  matches none fails, and so does a file that is not TOML, an entry with a field missing, empty or unknown, an
  `approved.at` that is not a datetime with a time zone, and two entries for one import. A file named otherwise
  (`Rotproof-Approved.toml`) approves nothing, and fails.
- **The project's files are up to this version of Rotproof** (see "Upgrading Rotproof"): `files` in the declaration
  (0.1.0 when it is missing) is a version as `major.minor.patch`, neither older nor newer than the running Rotproof,
  and every name in `declined` is an update.
- **`.rotproof/AGENTS.md` is up to date:** it equals what `rotproof create` writes for the stack with this version of
  Rotproof (see "Upgrading Rotproof"), so it fails after an upgrade until `rotproof init` has run, and when edited by
  hand.
- **No comment holds `TODO`, `FIXME`, `XXX`, `HACK` or `NOTE`**: in upper case, as whole words, in any code file
  outside `unchecked` (a path there that holds or sits in a layer skips nothing), `tests/` included; in TypeScript, the `//` and `/* */` comments of the source files in `src/`,
  JSX text not counted; in Rust, the `//` and `/* */` comments (doc comments too) of every `.rs` file in `crates/`,
  a crate's `tests/` and `build.rs` included, read by Rotproof's own scanner, which skips strings, raw strings and
  character literals as Rust's lexer does. The first four put off a finding: it is fixed now, written as one line in
  the task's issue, or let go, and the comment taken out. `NOTE` keeps knowledge where only someone reading that code
  finds it, and is printed under a heading of its own: what explains only the code beside it stays as a plain comment
  without the word, and what spans files, says how to run or operate something, or lies outside the repository moves
  to `docs/knowledge/`, tied to the code by `follows`. A line that holds both is named under the first heading alone,
  as taking the comment out settles both. Only comments count: `Status.TODO` and `"XXX-XXXX"` are not markers, and docstrings are
  strings. Comments in other files (a stylesheet, a `Cargo.toml`) are not read.

- **The bundle is there:** `docs/knowledge/`, `docs/index.md` and `docs/knowledge/rules.md` exist, and the declaration
  with its `areas` can be read. Without them every other check would pass with nothing checked.
- **Every name is compared exactly,** wherever Rotproof looks for a file or a directory: a link's target, the files
  above, the layers and the declaration. Rotproof reads the names the directories hold instead of asking the
  operating system, which on Windows also finds `README.md` for `readme.md`, `README.md.`, `README.md `, a stream
  (`README.md:secret`) or a short name (`README~1.MD`). Linux and GitHub find none of them, so each fails, and the
  message says what the disk has.
- **The areas are distinct headings:** none is empty, has a space at either end or a line break, and no two differ only in case.
- **Every knowledge document keeps the format:** `type`, `title`, `description`, `status` (`stable` or `deprecated`)
  and exactly one declared area in `tags`, and, once it is deprecated, a non-empty `# Resolution` as the first
  heading. A field Rotproof does not know passes as an extension, as OKF allows, unless it looks like a misspelling of
  a field a knowledge document has (`stale_afer`, `staleAfter`, `Title`): that fails, as a misspelled optional field
  would otherwise be silently dropped. The `at` of `verified` and `stale_after` are a day, or a datetime with a time
  zone read as its day there. A required text is not blank (spaces alone are empty), and no tag is empty. What the
  index lists (`title`, `description` and the tags) is on one line: a line break would end the entry and start a
  heading or an entry of its own. A title is written into the index with `[`, `]` and `\` escaped, so it stays the text
  of its own link. The one tag is a declared area, and the message lists the declared ones. An alarm has both
  `# When` and `# Do`, neither empty, and only an alarm has `match`: a list of strings, each on one line and named
  once. A document that breaks one of these is left out of the index, which lists it first, with why.
- **Every knowledge document matches the code it follows:** each key of `follows` names a path from the root with
  `/` (a file; a directory with a `/` at its end; a definition of a Python, TypeScript, JavaScript or Rust file after
  `::`, as `f`, `Class` or `Class.method`) with a hash of 8 lower-case hex digits, and that hash is what it names now:
  the file's text, every line ending as `\n`; the definition's source from its first decorator or attribute to its
  end, all the pieces of a name defined more than once in the order they appear, joined with `\n`; or every file read
  as code under the directory (`.py`, `.rs` and the TypeScript and JavaScript extensions, in any case, whatever the
  stack; outside what the project's `.gitignore` files exclude, and hidden ones), each with its path. What is not
  there fails. A deprecated document no longer holds, so what it follows is not read.
- **Every link in `docs/knowledge/` and the log resolves:** each read from the document's own directory, with heading
  anchors computed as GitHub computes them, a link from the log into the `docs/work/` of an older project included. A
  path with a drive letter (`C:/...`) or a `file:` URL fails: it names a file on one machine. Only a URL (`https:`,
  `mailto:`) is not checked, and a file name with a line number (`check.rs:104`) is a path, not a URL. A path is
  separated with `/`: only Windows reads a backslash as a separator, so a path with one fails. A path that climbs
  above the repository fails: GitHub serves only the repository. Links are read as a CommonMark reader reads them:
  with a title (`[a](b.md "title")`), in angle brackets, as reference links defined in the document, as images, and as
  `href` in HTML, and not inside code or comments. A link to a `.py` file names, in its text, a function or class
  defined there, at any depth. The file is parsed as Python, so a `def` line inside a string or a docstring does not
  count. The text is one name exactly as defined (``[`render_index`](../tests/bundle.py)``), not a call
  (`render_index()`) or a dotted path (`Bundle.render`); when it names nothing, the message says which name to write
  or lists the names the file defines.
- **The log exists, and its second-level headings are dates, newest first.** Below the title, the log is a flat list
  of entries grouped under the dates (OKF 0.2, section 9): every entry is a list item, and its indented lines (wrapped
  text, nested items) belong to it. A task heading (`### ...`), a paragraph, or an entry before the first date fails.
  A new log, with only its title and HTML comments, passes. Its entries' fields are not checked: an entry written
  before 1.0 keeps the fields it had.
- **No day in the log is later than today:** its date headings are read from the clock when written, and one that
  has not come was guessed. Each is compared with the latest day anywhere (the day in UTC+14), so CI in UTC passes a
  day written in Tokyo before 09:00. Time alone only makes one pass, so this never turns CI red with no change. Claude
  Code is given the time by the time hook (see "The time hook").
- **Every markdown file under `docs/knowledge/` is a document Rotproof reads:** it sits in `docs/knowledge/` itself (a
  markdown file in a directory under it fails, as Rotproof would not read it; a picture there passes), is named `.md`
  in lowercase (GitHub shows a `.MD` file, but Rotproof would not read it), is not `log.md`, a name OKF reserves for
  the log in `docs/`, and is a knowledge document or a guide. The fields OKF defines for a document pass as OKF writes
  them (`generated` needs only `by`; every entry of `sources` needs a `resource`; `usage_window` is a `{from, to}`
  range).
- **Every file Rotproof generates equals what `rotproof index` writes:** `docs/index.md` and `docs/knowledge/index.md`,
  so nobody maintains a list by hand, and `docs/knowledge/rules.md`, so the rules a project reads are the rules its
  Rotproof checks. A project's own rules go in another guide beside it.

One thing is a warning and fails nothing: **a knowledge document whose `stale_after` has come** (the day in the time
zone of the machine that runs the check) is printed on every run from that day, with what to do: measure again what it
says of outside the code, which `follows` cannot watch, then write the day in `verified` and the next day in
`stale_after`. Failing would turn CI red with no change, from the clock alone. A deprecated document is not read for
it.

The frontmatter is read as YAML 1.2: quoting a date or a datetime never changes whether it passes. It holds no comment: a `#`
after a space or at the start of a line starts one, which YAML reads as nothing, so an unquoted `title: PR #1` would
be the title `PR`; quote a value that holds one. Its closing `---` may end the file. The body is read as GitHub
renders it: an HTML comment or a fenced code block (``` or ~~~, indented by up to 3 spaces, running to the end when
it is not closed) is not text, so a heading, a link or the only text of a section inside one counts for nothing. A
heading may be indented by up to 3 spaces, and a closing run of `#` (`## Notes ##`) is not part of it.

## Development

`cargo xtask ci` runs format, lint and tests, the same checks as CI. It also fails when the map in `AGENTS.md`
misses a tracked top-level path or a module of a crate in `crates/` (or names one that is gone), when
`rust-toolchain.toml`, the `Dockerfile` and the CI workflow name different toolchain versions, and when
`THIRD-PARTY-LICENSES.txt` does not list the crates the binary links (below), and when Rotproof's own crates break
the rules of the Rust layout it keeps: a crate outside the layers, a dependency the table does not allow, or a marker
in a comment of `crates/` or `xtask/`. On Windows the host needs Visual
Studio's C++ tools and the Windows SDK. Everything also runs in the container (`compose.yaml`), where the host needs
only Docker: `docker compose run --rm dev cargo xtask ci`.

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

The pip wheels (Windows x86_64 and Linux x86_64) are built with maturin, in the container:

```sh
docker compose run --rm dev maturin build --release --out dist                                   # Linux
docker compose run --rm dev maturin build --release --target x86_64-pc-windows-msvc --out dist  # Windows
```

Each wheel carries `LICENSE-MIT`, `LICENSE-APACHE` and `THIRD-PARTY-LICENSES.txt` in its `.dist-info/licenses/`
(`license-files` in `pyproject.toml`). The container's Linux wheel is tagged `manylinux_2_34`, after the container's
glibc, and its Windows wheel is cross-compiled with cargo-xwin, which downloads the MSVC runtime and the Windows SDK
under Microsoft's license. Both serve for trying the wheels, not for release.

A release is a pushed tag `v<version>` that matches `Cargo.toml`. `.github/workflows/release.yml` checks the licenses,
builds the Linux wheel in the manylinux2014 image (glibc 2.17) and the Windows wheel on Windows, installs each into a
fresh venv on its platform and runs it (`.github/smoke.sh`), and installs the Linux wheel in the manylinux2014 image
too, so the oldest glibc the tag claims is tested. It then publishes the wheels to PyPI and makes the GitHub Release
with the wheels, an archive of the binary per platform taken out of the tested wheel, and `SHA256SUMS`. Run by hand
(`gh workflow run release.yml`), it does everything but publish. CI builds and tests the Windows wheel natively on
Windows too. Other platforms (macOS, Linux aarch64) are welcome as contributions.

## License

Licensed under either of [MIT](https://github.com/secta113/rotproof/blob/main/LICENSE-MIT) or
[Apache-2.0](https://github.com/secta113/rotproof/blob/main/LICENSE-APACHE), at your option. The crates the binary
includes are under their own licenses, listed with their texts in
[THIRD-PARTY-LICENSES.txt](https://github.com/secta113/rotproof/blob/main/THIRD-PARTY-LICENSES.txt). The links are full
URLs so that they work on PyPI, where this README is the project's page.
