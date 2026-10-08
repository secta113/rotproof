# Rotproof

Rotproof keeps a project's structure from drifting while LLMs and people change it, so that it does not rot. It keeps
two structures:

- **The layers:** which part of the code may import which (`handler`, `application`, `domain`, `infrastructure`,
  `utils`, and an optional `ui`). Code that cannot be split into these layers mixes responsibilities, so the layers
  are how an LLM, or a person, is made to split it: every piece of code has to land in a layer whose role and allowed
  imports are written down. Rotproof makes them when a project starts and checks them on every run, so the direction
  of dependencies stays what it was meant to be.
- **The specs and records:** what is open, what is being changed, how things are now, and why. Specs and work items
  (open defects and postponed work, each with a trigger, a state and a deadline), knowledge documents (an API, a data
  model, a decision, edited in place and named in the log at every edit), and a log, each kept to strict rules. A
  finding does not stay outside them: a `TODO` or `NOTE` in a code comment fails the check, and a stop hook sends an
  agent back when its last message leaves something open that it did not record.

## Principles

- **The declaration is the truth, and Rotproof does not repair.** The structure a project declares is the structure. A
  tree that differs from it fails, either way, and someone decides whether the tree or the declaration is wrong;
  Rotproof changes the tree only when asked.
- **Every rule has a check that can fail.** A rule without a check is only a label, and soon drifts.
- **A check with nothing to check fails.** A check that passes on an empty tree protects nothing.
- **The rules come with the tool.** A project pins one version of Rotproof, and takes improvements to the rules by
  upgrading it, in a commit of its own.
- **The same structure in every language.** Rotproof is one binary with no language runtime, so a Rust or TypeScript
  project keeps the same layers and records as a Python one.
- **Whoever picks up the work next reads one index, not every file.** The index files are generated, never written by
  hand.

Rotproof makes the layer directories, checks that they are where the project declares them, and checks their
direction: from the imports in Python and TypeScript, and from the dependencies each crate declares in Rust.

The records live in `docs/`, which is a bundle in [OKF 0.2](https://github.com/GoogleCloudPlatform/open-knowledge-format)
(its `SPEC.md` as of commit `ad30107`): every document has YAML frontmatter with a `type`, `index.md` and `log.md` are
reserved names, and the log's headings are dates. An OKF reader can read the records as a bundle. The rules on top of
that format (a work item's trigger, state and deadline, a closed record opening with its resolution, a knowledge
document's hash in the log, the shape of a log entry) are Rotproof's own, stricter than OKF, and not part of it.
Rotproof is not an OKF validator.

## Install

Rotproof is released for Windows x86_64 and Linux x86_64 (glibc 2.17 or newer). Pin the exact version, so that an
upgrade, which can bring new rules, is a commit of its own:

```sh
pip install rotproof==0.2.0
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
rotproof --root <repository> index   # write every generated file in docs/ (the index files and the rules)
rotproof --root <repository> approve <file> <import>  # keep a forbidden import, asked on a terminal (see below)
rotproof --root <repository> approve --prune  # remove the approvals that match no forbidden import
rotproof --root <repository> approve --reviewed  # re-pin the knowledge documents whose followed code changed
rotproof --root <repository> follows <key>...  # print the hash to pin in follows, for each key
rotproof guide --stack python        # print the rules Rotproof keeps for a stack, with or without a project
rotproof stop-hook                   # run by Claude Code when the agent stops (see "The stop hook")
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
`src/main.tsx`, is `handler`'s where it is (`index.html` loads it); `rotproof check` names the rest of the starter
(`App.tsx`, the styles, `assets/`) as code outside the layers, with where each goes.

Run `rotproof create` when a project starts, and again after you change `.config/rotproof.toml` on purpose. It makes
only what is missing: a layer that is neither present nor declared absent, the project's files when they do not exist
(below), and the files Rotproof generates (`.rotproof/AGENTS.md`, and the index files and rules in `docs/`). It never
overwrites a file it does not generate, and never moves or deletes one. Nothing runs it on its own, so a layer removed
  `xtask/src/layers.rs` in place of the file, and reads the comments of `xtask/` for markers too, which the layout
  leaves out; the records are kept outside, so they are not checked here.

The project's files are written once, as a starting point, and are the project's from then on:

| File | Content |
|---|---|
| `AGENTS.md` | The map (the layers present, with their roles to rewrite), the differences from what Rotproof keeps, and the project's own rules. It points at `.rotproof/AGENTS.md` |
| `CLAUDE.md` | `@AGENTS.md` and `@.rotproof/AGENTS.md` |
| `README.md` | The project's name (its root directory's) and how to run Rotproof |
| `.gitignore`, `.gitattributes` | For the stack; line endings as LF |
| `docs/log.md` | The log, with its title |
| `.claude/settings.json` | The stop hook (see "The stop hook"), and the rule that denies Claude Code editing the approvals file (see "Approving a forbidden import") |
| `requirements-dev.txt` | `python` and `none`: Rotproof pinned with `==` |
| `.github/workflows/ci.yml` | `python` and `none`: installs `requirements-dev.txt` and runs `rotproof check`, with a time limit |
| `Cargo.toml` | `rust`: the workspace, whose members are the crates in `crates/`, so `cargo build` builds every layer present |

How a `typescript` or `rust` project pins Rotproof is not decided yet, so for those stacks `rotproof create` writes
neither the pin nor the workflow, and says so.

When a newer Rotproof requires a field the declaration lacks, `rotproof check` fails and says so, and
`rotproof create` adds the field under a comment that says what it is and where its first value came from (`areas`
gets the tags the records use, sorted by name), keeping every comment and value already there. A value that is present
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
  `.rotproof/AGENTS.md`, the generated files in `docs/`), and sets `files` to the running version. It never changes
  the declaration's other values, and run twice it changes nothing the second time.
- **An update only adds,** and does nothing when its file has it already. What it cannot do without a person (its file
  is missing, or not in a form Rotproof can read), `rotproof init` says, exits 2 and leaves `files` as it was: do it by
  hand, or decline the update by its name in `declined`, and run `rotproof init` again.

| Update | Version | File | What it adds |
|---|---|---|---|
| `claude-deny-approvals` | 0.2.0 | `.claude/settings.json` | `"Edit(/.config/rotproof-approved.toml)"` in `permissions.deny` (see "Approving a forbidden import"). The file is written again with its keys in their order and two spaces of indentation |

Among the files Rotproof generates is `.rotproof/AGENTS.md`: the rules Rotproof keeps, written for the project's
stack. It says how to run Rotproof, lists the layers with where each lives and what it may import (from
`layers/table.toml`), and gives the rules of the records, starting with reading the index files before work. It names
the Rotproof version that wrote it, so after an upgrade `rotproof check` fails until `rotproof init` (or
`rotproof create`) has rewritten it. Edited by hand, it fails too: a project's own rules go in its own `AGENTS.md`.

The guide only helps if your agent reads it. The `AGENTS.md` and `CLAUDE.md` that `rotproof create` writes point at it;
a project that has its own points at it from its `AGENTS.md` (or whatever file its agent reads first), and imports it
in `CLAUDE.md` for Claude Code:

```markdown
@AGENTS.md
@.rotproof/AGENTS.md
```

## The stop hook

An agent's findings are lost when it reports them and stops: "not checked", "out of scope" in its last message, and
nothing in the records. `rotproof stop-hook` is [Claude Code's `Stop` hook](https://code.claude.com/docs/en/hooks),
run when the agent stops, and reads that last message. When the message holds a phrase that leaves something open
and `git status` shows no change in `docs/`, the hook sends the agent back once, asking it to record the finding or to
say in one line where it already is. While the agent is
continuing because of a stop hook, the hook lets it stop, so it never loops. A line that points at the records (the
word `spec`, `backlog` or `knowledge`, the words `work item`, or a path through `work/`) is not read: what it leaves
open is recorded where it points. The phrases are built in (Japanese and English); the agent decides what each one meant.

`rotproof create` writes `.claude/settings.json` with the hook when it does not exist, together with the rule that
denies Claude Code editing the approvals file (see "Approving a forbidden import"). A project that has one adds the
hook to it:

```json
{
  "hooks": {
    "Stop": [{ "hooks": [{ "type": "command", "command": "rotproof stop-hook" }] }]
  }
}
```

`rotproof` has to be on the `PATH` the agent runs hooks with (for a venv, start the agent with the venv active). The
project is the nearest directory upwards that holds `.config/rotproof.toml`; outside one, the hook says nothing. A
hook that fails (not a git repository, an input from another hook) exits 1, which Claude Code shows without keeping
the agent from stopping; exit code 2 would keep it from stopping.

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
name it asks for when that is not set), and adds the entry:

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
areas = ["billing", "records"]  # the areas the records are grouped by, in this order (see The records)
absent = ["ui"]          # layers this project does not have
unchecked = ["scripts"]  # paths outside the layers that Rotproof does not look into
```

What the layers are (`handler`, `ui`, `application`, `infrastructure`, `domain`, `utils`, and the atomic levels of
`ui`: `pages`, `templates`, `organisms`, `molecules`, `atoms`) is written once, in `layers/table.toml`. Where they live
is written once per stack:

| Stack | A layer is | A `ui` level is | Code Rotproof looks at |
|---|---|---|---|
| `python` | `<layer>/__init__.py`, the role as its docstring | `ui/<level>/__init__.py` | `.py` files anywhere (`.PY` too), except `tests/` |
| `typescript` | `src/<layer>/index.ts`, the role as a doc comment | `src/ui/<level>/index.ts` (React) | every file in `src/`; `src/main.tsx` (Vite's entry point) is `handler`'s |
| `rust` | a crate, `crates/<layer>/` (`handler` a binary), the role as `//!` | none: Rust has no `ui` yet | every crate in `crates/` |

`stack = "none"` declares a repository that keeps records only: `rotproof create` makes only `docs/`, and `Rotproof
check` checks only the records and prints that it did not check the layers. It is a line in the declaration, not a
flag, so the structure check is never switched off where the declaration still declares layers.

A level of `ui` is declared absent by its dotted name (`absent = ["ui.templates"]`). Files `.gitignore` excludes and
hidden files are not looked at, so a virtual environment or a build directory is not code. A code file is matched in any
case (`stray.PY`, `cargo.toml`): Windows runs or reads it all the same.

## The records

```
docs/
  index.md          generated
  log.md            what was done, newest first
  work/             every spec and work item, open or closed
    rules.md        generated: the rules of both types (type: Guide)
    index.md        generated
    <slug>.md       one record per file (type: Spec or Work Item, status: draft, stable or deprecated)
  knowledge/        how things are now, and why (type: Knowledge, status: stable or deprecated)
    rules.md        generated: the knowledge rules (type: Guide)
    index.md        generated
```

The two directories hold records that are handled differently. `work/` holds what was decided and the work waiting on
it, closed once implemented, done or dropped, and never moved. Which type a sentence belongs to is one question: if
the work were finished today, would it be false? A spec holds what stays true (what was decided, why, what was
rejected); a work item holds what would then be false (what is not done or measured yet, and how far it has come).
Whether a record is decided is its `status`: a draft spec is not agreed yet, a draft work item is not sorted yet.
`knowledge/` holds how things are now, edited in place and deprecated only when it no longer holds. A closed spec is
history; what it built is described in `knowledge/`, an API or a data model, or why something was decided. Every edit of a knowledge document is named in the log by a hash of its
contents, under the label `**Knowledge**` (`* **Knowledge**: knowledge/api.md@a3f9c1d2`), and `rotproof check` fails
an edit the log does not name. A knowledge document that describes code names it in `follows`, each with its hash when
the document was last reviewed: a file in any language, and in Python also a function or class
(`src/api.py::Router.add`) or every `.py` file under a directory (`src/api/`). `rotproof follows <key>...` prints the
line to put in `follows` for each key, as the code is now, when a document starts to follow it. `rotproof check` fails
when one changed since, and prints the line to write once the document is reviewed; writing it changes the document,
so its log entry records the review. After a refactoring that moved code without changing what it does, a person can
re-pin every changed hash in one act: `rotproof approve --reviewed` lists each document and what changed under it,
says to read the changes first (a hash says only that the code changed), asks for one `y` on a terminal (an agent's
shell has none), writes the new hashes, and prints the log lines to write. What is gone is left for the person to
edit. No LLM judges whether the meaning changed: the agent that refactors is usually one.

A record stays where it was written when it closes: its status says it is closed, and the index lists it under
`# Closed`. Its path, and every link to it, never changes, so closing a record is a change to that record and its
index line only.

Every spec, work item and knowledge document belongs to exactly one area: its only tag, one of the `areas` the
declaration lists. The index files group by area, in the order of `areas`, so the project puts the largest or most
active area first. An area says where a record belongs (the layers, the records, billing), and never closes. A
declared area that no record uses passes, so an area is declared before its first record. Renaming an area is editing
`areas` and the tag of every record in it, closed ones included: the tag is frontmatter for the index, not history.
Guides keep their optional tags, which name no area.

The records form a tree: an epic, its parts, and the work items of each. A record names the spec it is a part of by
slug (its file name without `.md`) in `parent`. A large piece of work is split into specs that are parts of an epic,
which is a spec like any other, with goals and decisions of its own; the work a spec waits on is its work items, each
with its own state, instead of a list of steps in its body:

```yaml
type: Spec
title: Publish Rotproof for every stack
status: stable
tags: [rotproof]
parent: template-multi-stack
```

The area and the parent are independent: an area says where a record belongs and never closes, a parent says which
piece of work a record is part of and closes after its children. A part may be in another area than its epic. A spec
that cannot name one area mixes two, so it is split into one part per area, and the epic ties the parts back into one
piece of work. In an index, a record in the same area as its parent, and open or closed as its parent is, is listed
under it, indented; any other record with a parent is listed on its own with `Parent: [<title>](...)` after its line,
so each record appears once.

A work item has this frontmatter and these body headings:

```markdown
---
type: Work Item
title: Some problem
description: One sentence: what is waiting.
tags: [area]                  # exactly one, declared in areas; the index groups items by it
status: stable                # draft = open and not sorted yet, stable = open and sorted, deprecated = closed
parent: some-spec             # the spec it is a part of; required for stable
filed: 2026-10-01
verified: {by: human:someone, at: 2026-10-01T10:00:00+09:00}
deadline_kind: until          # until, or none with the reason in deadline
deadline: until the next deploy
stale_after: 2027-01-01T00:00:00+09:00   # optional: when to measure the state again
---

# Resolution    (only when closed, and then first)
# Trigger
# State
# Details
```

## What `rotproof check` checks

- **The tree matches `.config/rotproof.toml`, either way:** the declaration exists and names a known stack, and only
  layers that stack has in `absent` (a misspelled field fails). Every layer of the stack is present or declared
  absent, and no layer declared absent is present. No code sits outside the layers, the stack's own paths (`tests/`)
  and `unchecked`. A path in `unchecked` exists and neither holds nor sits in a layer, so a layer cannot be switched
  off by listing it. At least one layer is present: with every layer declared absent, nothing would be checked.
  `ui` holds only its levels: code in `ui` beside them fails, apart from the layer's own file (`ui/__init__.py`).
  What the whole UI shares goes in a level: a part that knows no project concept, visible or not (a design value, one
  behaviour, a provider of a theme), is an atom.
- **The layers import only what the table allows** (Python): every `import` and `from ... import` in the layers,
  relative ones and those inside functions or under `if TYPE_CHECKING:` included. A layer may import itself and the
  layers in its `imports`; a level of `ui` the levels below it and the layers in its `imports`. Only direct imports
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
  TOML fails, and so does a dependency taken from a workspace that does not declare it. Not seen: `[patch]` and
  `[replace]`, and source files taken from another crate's directory (`#[path]`, `include!`, `[lib] path`).
- **Every approval matches a forbidden import** (see "Approving a forbidden import"): a forbidden import with an entry
  in `.config/rotproof-approved.toml` for its file and import passes, and is printed on every run; an entry that
  matches none fails, and so does a file that is not TOML, an entry with a field missing, empty or unknown, an
  `approved.at` that is not a datetime with a time zone, and two entries for one import. A file named otherwise
  (`Rotproof-Approved.toml`) approves nothing, and fails.
- **The project's files are up to this version of Rotproof** (see "Upgrading Rotproof"): `files` in the declaration
  (0.1.0 when it is missing) is a version as `major.minor.patch`, neither older nor newer than the running Rotproof,
  and every name in `declined` is an update.
- **No comment holds `TODO`, `FIXME`, `XXX`, `HACK` or `NOTE`**: in upper case, as whole words, in any code file
  outside `unchecked` (a path there that holds or sits in a layer skips nothing), `tests/` included; in TypeScript, the `//` and `/* */` comments of the source files in `src/`,
  JSX text not counted; in Rust, the `//` and `/* */` comments (doc comments too) of every `.rs` file in `crates/`,
  a crate's `tests/` and `build.rs` included, read by Rotproof's own scanner, which skips strings, raw strings and
  character literals as Rust's lexer does. Work left to do belongs in a work item, where it is listed and closed, and
  a decision with its reason in the spec or the log entry of the change; a comment that explains how to read the code
  stays, without the word. Only comments count: `Status.TODO` and `"XXX-XXXX"` are not markers, and docstrings are
  strings. Comments in other files (a stylesheet, a `Cargo.toml`) are not read.

- **The bundle is there:** `docs/`, `docs/index.md`, `docs/work/`, `docs/work/rules.md`, `docs/knowledge/` and
  `docs/knowledge/rules.md` exist, and the declaration with its `areas`
  can be read. Without them every other check
  would pass with nothing checked.
- **Every name is compared exactly,** wherever Rotproof looks for a file or a directory: a link's target, the files
  above, the layers and the declaration. Rotproof reads the names the directories hold instead of asking the
  operating system, which on Windows also finds `README.md` for `readme.md`, `README.md.`, `README.md `, a stream
  (`README.md:secret`) or a short name (`README~1.MD`). Linux and GitHub find none of them, so each fails, and the
  message says what the disk has.
- **The areas are distinct headings:** none is empty, has a space at either end or a line break, and no two differ only in case.
- **Every work item keeps the format:** the fields above with their types, a `parent` once it is `stable`, and
  non-empty Trigger, State and Details (and Resolution when closed, as the first heading). A field Rotproof does not know passes as an extension, as OKF allows, unless it looks
  like a misspelling of a field the document type has (`stale_afer`, `staleAfter`, `Title`), or is a field
  other types have and this one does not (a work item's `deadline` on a spec, the `parent` of a spec or a work item on
  a knowledge document), or is a field Rotproof no longer reads (`epic`, which says to write `parent`): those fail, in
  every document type, as a misspelled optional field would otherwise be silently dropped. A deadline is an event or a reason, never only a date
  (`2026-10-31`, `2026/10/31`, `31.10.2026`, `2026年10月31日` or a month alone); an event may contain a date. Every
  time has a time zone. `stale_after` is later than the last `verified`. A required text is not blank (spaces alone
  are empty), in every document type, and no tag is empty. What the index lists (`title`, `description`, `deadline`
  and the tags) is on one line: a line break would end the entry and start a heading or an entry of its own. A
  title is written into the index with `[`, `]` and `\` escaped, so it stays the text of its own link. The one tag
  is a declared area, and the message lists the declared ones.
- **Every link in `# Details` resolves,** with heading anchors computed as GitHub computes them. A path with a drive
  letter (`C:/...`) or a `file:` URL fails: it names a file on one machine. Only a URL (`https:`, `mailto:`) is not
  checked, and a file name with a line number (`check.rs:104`) is a path, not a URL. A path is separated with `/`:
  only Windows reads a backslash as a separator, so a path with one fails. A path that climbs above the repository
  fails: GitHub serves only the repository. Links are read as a CommonMark reader reads them: with a title
  (`[a](b.md "title")`), in angle brackets, as reference links defined in `# Details`, as images, and as `href` in
  HTML, and not inside code or comments. A link to a `.py` file names, in its text, a function
  or class defined there, at any depth. The file is parsed as Python, so a `def` line inside a string or a docstring
  does not count. The text is one name exactly as defined (``[`render_index`](../tests/bundle.py)``), not a call
  (`render_index()`) or a dotted path (`Bundle.render`); when it names nothing, the message says which name to write
  or lists the names the file defines.
- **Every spec keeps the format:** `title`, `description`, `status` and exactly one declared area in `tags`, and a
  non-empty `# Resolution` as the first heading once it is closed. `parent`, when present, is the slug of another spec in `docs/work/`
  (not a path, not a work item or a guide, not the spec itself), and that spec has no `parent` of its own: one level
  only. The `parent` of a work item is the slug of any spec. A record that breaks one of these is left out of the index
  files.
- **A parent closes after its children:** no closed spec has an open part or an open work item. A child that is
  dropped closes as dropped, as any record does.
- **The log exists, points only at work items that exist** (`work/<slug>.md`, or `backlog/<slug>.md` as entries
  written before `docs/work/` name them, written with `/` or `\`, the slug percent-encoded or not), and its second-level headings are dates, newest first. Below the title, the log is
  a flat list of entries grouped under the dates (OKF 0.2, section 9): every entry is a list item, and its indented
  lines (wrapped text, nested items) belong to it. A task heading (`### ...`), a paragraph, or an entry before the
  first date fails. A new log, with only its title and HTML comments, passes.
- **Every knowledge document keeps the format,** as a spec does (without `parent`, and `status` `stable` or
  `deprecated`), and **the log names it as it is now:** some `**Knowledge**` field of a log entry, with its wrapped
  lines, names it with the first 8 hex digits of SHA-256 of the whole file (every line ending as `\n`). An edit the
  log does not name fails, and the failure prints the line to write. A `**Knowledge**` field that names a document
  `docs/knowledge/` does not have fails too.
- **Every knowledge document matches the code it follows:** each key of `follows` names a path from the root with
  `/` (a file; a directory with a `/` at its end; a definition of a `.py` file after `::`, as `f`, `Class` or
  `Class.method`) with a hash of 8 lower-case hex digits, and that hash is what it names now: the file's text, every
  line ending as `\n`; the definition's source from its first decorator to its end; or every `.py` file under the
  directory (outside what the project's `.gitignore` files exclude, and hidden ones), each with its path. What is not
  there fails. A deprecated document no longer holds, so what it follows is not read.
- **Every document is a known type in its directory.** The fields OKF defines for a document pass as OKF writes them
  (`generated` needs only `by`; every entry of `sources` needs a `resource`; `usage_window` is a `{from, to}` range).
  The names OKF reserves appear only where Rotproof writes and reads them: `index.md` in `docs/` and in each directory
  of documents, `log.md` in `docs/`. A markdown file is named `.md`, in lowercase: GitHub shows a `.MD` file, but
  Rotproof would not read it.
- **Every file Rotproof generates equals what `rotproof index` writes:** the index files, so nobody maintains a list by
  hand, and `docs/work/rules.md` and `docs/knowledge/rules.md`, so the rules a project reads are the rules its Rotproof
  checks. A project's own rules go in another guide in the same directory.
- **No spec sits at the repository root.**

The frontmatter is read as YAML 1.2: quoting a value never changes whether it passes. Its closing `---` may end the
file. The body is read as GitHub
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
