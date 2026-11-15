# Rotproof specification

What Rotproof checks, keeps and writes, in full: the commands, the project's files, the layers, the records and every
rule of `rotproof check`. The [README](https://github.com/secta113/rotproof#readme) says what Rotproof gives, how to
install it and how to try it in five minutes, and what a version promises. A change to what Rotproof checks, keeps or
writes changes this file in the same commit.

## Overview

Rotproof makes the layer directories, checks that they are where the project declares them, and checks their
direction: from the imports in Python and TypeScript, and in Rust from the dependencies each crate declares and, for
the levels inside a crate, from the paths its code names. From the same reading of the code it checks that only
`infrastructure` reads the environment variables, and that no comment switches the type checker off (see "What
`rotproof check` checks"). A project made of several parts that each hold the layers declares them as units, and all
of this holds in each unit, with the imports between units judged too (see "Units").

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

## Usage

```sh
rotproof --root <repository> init --stack python --ui atomic  # write .config/rotproof.toml, when there is none
rotproof --root <repository> init    # after upgrading Rotproof: update the project's files (see "Upgrading Rotproof")
rotproof --root <repository> create  # list the layers it would make, writing nothing (makes docs/ when it makes no layer)
rotproof --root <repository> create --yes  # make the layers .config/rotproof.toml declares, the records and the guide
rotproof --root <repository> check   # check the layers and the records; exits 1 when a rule is broken
rotproof --root <repository> index   # write the generated files that are out of date (docs/'s index files and the knowledge rules, and who reads the shared units)
rotproof --root <repository> approve <file> <import> --reason "<why>"  # keep a forbidden import (see below)
rotproof --root <repository> approve --prune  # remove the approvals that match no forbidden import
rotproof --root <repository> follows <document> <key>...  # pin keys in a knowledge document's follows, as the code is now
rotproof guide                       # print the guide `rotproof create` writes for the stack and the UI the declaration has
rotproof guide --stack python        # print the rules Rotproof keeps for a stack, with or without a project
rotproof guide --stack typescript --ui none  # the same for the layers of that UI (the values of init's --ui), with no project
```

`--root` defaults to the current directory. The declaration is read from `<repository>/.config/rotproof.toml`, and the
records from `<repository>/docs`.

The binary leads the way without this file, for an agent that has only Rotproof: `rotproof --help` says what it is,
the order to start in and the exit codes, `rotproof <command> --help` what a command reads, writes and never does, and
each command's output names the next step. A test follows that path from an empty directory to a passing check.

Start a project with `rotproof init --stack <stack>` (`python`, `typescript`, `rust`, or `none` for a repository
that keeps records only), and `--ui` for the stacks that offer a UI: `--ui atomic` for a project with screens, `--ui
none` for one without (a command-line tool, a library, a server). Python and TypeScript need it named, as a project
with screens and one without start from the same command; Rust offers `none` alone. It writes only the declaration,
so you declare in `absent` the layers you do not want before anything is made. Then run `rotproof create`, which
lists the layers it would make and writes nothing, and `rotproof create --yes` once `absent` lists every layer the
project lacks. Run on a project that has a declaration, `rotproof init` upgrades the project's files instead (see
"Upgrading Rotproof"), and never changes the declaration's values but `files`.

`rotproof create` makes layers only with `--yes`. Without it, a run that would make one lists the layers, writes
nothing and exits 2: run straight after `rotproof init`, it would otherwise make every layer the declaration names
before anyone read the list, and it never deletes them again. A run that makes no layer, as after an upgrade, needs no
`--yes`.

A TypeScript project with React starts from Vite: run `npm create vite@latest <name> -- --template react-ts` first,
pin Rotproof in it (`npm install -D -E rotproof@2.0.0-rc.1`), then run `rotproof init --stack typescript --ui atomic`
and `rotproof create --yes` there (`npx --no -- rotproof ...`), which keep what Vite wrote. Vite's entry point,
`src/main.tsx`, is the entry file where it is (`index.html` loads it), and may import `composition` alone: what Vite
has it import (`./App.tsx`, `./index.css`) moves into `src/composition/`, which `main.tsx` calls. `rotproof check`
names the rest of the starter (`App.tsx`, the styles, `assets/`) as code outside the layers, with where each goes.

Run `rotproof create` when a project starts, and again after you change `.config/rotproof.toml` on purpose. It makes
only what is missing: a layer that is neither present nor declared absent, the project's files when they do not exist
(below), `docs/knowledge/`, and the files Rotproof generates (`.rotproof/AGENTS.md`, `.rotproof/readers.md` when a unit
is shared, and the index files and the knowledge rules in `docs/`), so a new project passes its first check with nothing for a person to write. It never
overwrites a file it does not generate, apart from adding the fields the declaration lacks (below) and, in a Rust
project, adding the crates it makes to the workspace's `members` when none names them (see "Units"), and never moves
or deletes one. Nothing runs it on its own, so a layer removed by mistake fails `rotproof check` instead of coming back.
A project that declares units (see "Units") also gets each entry file its units declare that is missing, made from
one text for the stack (the entry file's docstring or doc comment, and in Rust `fn main() {}`), whatever the file is
named, and in Rust the package that holds it (`Cargo.toml` beside its `src/`); the entry file of a project of one unit
stays the project's to write, but in Rust, where it is the root package's `src/main.rs`.

The project's files are written once, as a starting point, and are the project's from then on:

| File | Content |
|---|---|
| `AGENTS.md` | The map (the layers present, each with the first sentence of its role to rewrite, and in a project that declares units a row for each layer of each unit, with the unit's name before the role), the differences from what Rotproof keeps, and the project's own rules. It points at `.rotproof/AGENTS.md` |
| `CLAUDE.md` | `@AGENTS.md` and `@.rotproof/AGENTS.md` |
| `README.md` | The project's name (its root directory's) and how to run Rotproof |
| `.gitignore`, `.gitattributes` | For the stack; line endings as LF |
| `docs/log.md` | The log, with its title |
| `.claude/settings.json` | The rule that denies Claude Code editing the approvals file (see "Approving a forbidden import") |
| `requirements-dev.txt` | `python`: Rotproof pinned with `==` |
| `.github/workflows/ci.yml` | `python`: installs `requirements-dev.txt` and runs `rotproof check`, with a time limit. `typescript`: installs what `package.json` and its lockfile pin (`npm ci`) and runs `npx --no -- rotproof check`, which never downloads another Rotproof, with a time limit. `rust` and `none`: pins Rotproof's version in `ROTPROOF_VERSION`, takes the Linux archive of that GitHub Release, checks it against the release's `SHA256SUMS`, and runs `rotproof check`, with a time limit |
| `Cargo.toml` | `rust`: the workspace, whose members are the crates of the layers present, each named on a line of its own (no glob: see "Units"), so `cargo build` builds every layer present, and its root package, which depends on `composition`. A project that declares units has the workspace alone |
| `src/main.rs` | `rust`: the entry file, which calls `composition` and starts the program. A project that declares units has none: an entry file is a unit's own |

A project that does not write Python needs no Python for Rotproof: `rust` and `none` pin only the version, as
Rotproof's GitHub Releases are immutable, so a version names one binary, as on PyPI. A binary cannot know the hash of
the archive it is in, so `rotproof create` writes no hash, and an upgrade changes only the version. A `typescript`
project pins Rotproof in `package.json` with `npm install -D -E` at an exact version, as it pins its other tools, and the
lockfile then names the binary of each platform with its hash. `package.json` exists before Rotproof runs and is the
project's, so `rotproof create` writes the workflow and the README's instructions, says that the pin is the project's
to add, and never writes it.

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
  `.rotproof/AGENTS.md`, `.rotproof/readers.md` when a unit is shared, the generated files in `docs/`, and
  `docs/knowledge/`, the log and the project's files where they are missing), and sets `files` to the running version, adding `declined = []` when the declaration lacks it; it
  names every field it adds to the declaration, as `rotproof create` does. Crossing 0.4.0, which builds the layer table
  on the onion architecture, it says where the code of each layer that is gone goes (`handler`, `utils`) and that the
  ports move from `domain` to `protocol.outbound`, as "move by hand" lines; it moves no code, and keeps nothing behind:
  `rotproof check` names what is not moved yet. Crossing 1.0.0-rc.3, which splits the UI into the view (`ui`) and the
  state of the screens (`presentation`) and declares the UI, the refresh adds `ui` to the declaration: `"atomic"` when
  `ui/` is there, `"none"` otherwise. With `"atomic"`, it says, as the same lines, that `presentation` is to be made
  (`rotproof create --yes`), and that the state of a page and its calls of the use cases move to
  `presentation.models` and the domain values the parts show to `presentation.display`; with `"none"`, that `ui` is
  to be taken out of `absent`, where `rotproof check` fails on it. For a project that declares units and has no
  document that follows them, it also drafts one, once ("The document of the units"). It never changes the
  declaration's other values, and once it has set `files`, running it again changes nothing. It never writes the
  project's `AGENTS.md`, but says on every run, as `note: ...` lines, what its map has gone behind on (see "The map in
  `AGENTS.md`").
- **An update only adds, or takes out what Rotproof itself wrote and no longer keeps, matched exactly,** and does
  nothing when its file is as the update leaves it already. It writes the file back with the line endings and the
  byte order mark it had. What it cannot do without a person (its file is not in a form Rotproof can read),
  `rotproof init` says, exits 2 and leaves `files` as it was: do it by hand, or decline the update by its name in
  `declined`, and run `rotproof init` again. When its file is missing, the same run writes the file as
  `rotproof create` does now, with what every update adds, and the update asks for nothing.

| Update | Version | File | What it does |
|---|---|---|---|
| `claude-deny-approvals` | 0.2.0 | `.claude/settings.json` | Adds `"Edit(/.config/rotproof-approved.toml)"` to `permissions.deny` (see "Approving a forbidden import"). The file is written again with its keys in their order and two spaces of indentation |
| `claude-remove-stop-hook` | 1.0.0-rc.1 | `.claude/settings.json` | Takes out each `Stop` hook whose command is exactly `rotproof stop-hook`, or the hook beside it that 0.4.0 added (it said when `rotproof` was not on the `PATH`), a group left with no hook, `Stop` when no group is left, and `hooks` when no event is left (see "The hooks Rotproof no longer writes"). Written again as above: an untouched file ends as `rotproof create` writes it now |
| `claude-remove-time-hook` | 1.0.0-rc.4 | `.claude/settings.json` | Takes out each hook on `SessionStart` and `UserPromptSubmit` whose command is exactly `rotproof time-hook`, a group left with no hook, an event left with no group, and `hooks` when no event is left (see "The hooks Rotproof no longer writes"). A hook with another command that runs it, such as a path to the binary, stays. Written again as above: an untouched file ends as `rotproof create` writes it now |

The two updates 0.4.0 brought, `claude-stop-hook-on-path` and `claude-time-hook`, which added the hooks the two updates
above take out again, are gone, as nothing adds those hooks now. A declaration that still names either in `declined` is
reported by `rotproof check` as naming no update: take the name out.

Among the files Rotproof generates is `.rotproof/AGENTS.md`: the rules Rotproof keeps, written for the project's
stack. It says how to run Rotproof, lists the layers with where each lives and what it may import (from
`layers/table.toml`), gives the rules of units, and gives the rules of the records, starting with reading the knowledge index before work, and
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

## The hooks Rotproof no longer writes

Until 1.0.0-rc.3, Rotproof wrote two hooks into `.claude/settings.json`, for Claude Code. Neither is written now, and
`rotproof init` takes each out of a project's settings. Neither command exists any more, but a settings file that runs
the binary by path (the update matches only the commands Rotproof wrote, exactly) keeps its hook through any upgrade, so
a command line that names `stop-hook` or `time-hook` says in one line that the hook was removed and what to do about
it, and exits 1, whatever else is wrong with it. It never exits 2: Claude Code takes exit code 2 from a `Stop` hook as
"send Claude back to work" (up to 8 times in a row) and from a `UserPromptSubmit` hook as "block the prompt", where exit
code 1 is an error that is only shown. The hook input on stdin is not read.

- **The `Stop` hook, `rotproof stop-hook`,** sent the agent back when its last message left something open and nothing
  in `docs/` changed. It went with the records of work it asked for. `rotproof init` takes it out (the update
  `claude-remove-stop-hook`).
- **The `SessionStart` and `UserPromptSubmit` hook, `rotproof time-hook`,** told the agent the time now (0.4.0 to
  1.0.0-rc.3). It was written when `rotproof check` failed on a time that had not come, and an agent wrote a guess a
  few minutes ahead of the clock. The records hold a day now, and Claude Code gives the agent the day itself.
  `rotproof init` takes it out (the update `claude-remove-time-hook`), matched exactly: a hook whose command is another
  text (a path to the binary) stays, and fails with the line above at the start of every session and on every prompt
  until it is removed by hand. The rule it repeated, to read a day from the clock and never from memory, is in the
  `.rotproof/AGENTS.md` that `rotproof create` writes; `rotproof check` still fails on a day that has not come.

## Approving a forbidden import

Sometimes a project means to break the layer table: a framework forces an import, or a fix cannot wait for the
refactoring. Such an import passes only as an entry in `.config/rotproof-approved.toml`, which `rotproof approve`
adds, with the reason it is given:

```sh
rotproof approve domain/model.py infrastructure.db --reason "the ORM's session factory must be built where the model is"
```

The file and the import are named as `rotproof check` names them: the file from the root, and the import as the check
writes it (a Python dotted name, a TypeScript specifier, a crate's dependency). In Python, each name of
`from m import n` is its own import, `m.n`, whether `n` is a module or not: approving `m` approves none of them.
`rotproof approve` shows the forbidden import, signs with git's `user.name`, and adds the entry (the file keeps a byte
order mark and its line endings, CRLF on every line when one line had it, and so it does when `--prune` removes
entries). It asks nothing, so an agent's shell can run it, but it takes no approval without `--reason` (an empty one
fails), and it fails when `user.name` is not set, as the entry would carry no name:

```toml
[[kept]]
from = "domain/model.py"
import = "infrastructure.db"
reason = "the ORM's session factory must be built where the model is"
approved = { by = "someone", at = "2026-10-06T09:00:00+09:00" }
```

- **Run it only when the user asked for the exception, and say that you ran it.** The guide in `.rotproof/AGENTS.md`
  says so. There is no comment that silences the check on the import's line: it would be the easiest thing for an
  agent to write.
- **Claude Code is denied editing the file.** The `.claude/settings.json` that `rotproof create` writes holds
  `"permissions": { "deny": ["Edit(/.config/rotproof-approved.toml)"] }`, which refuses Claude Code's edit tools and
  its shell's file commands and redirects (`>>`, `sed -i`) on the file, with no prompt. A person edits it in an
  editor, or through `rotproof approve`.
- **An approval never hides.** `rotproof check` passes the approved import on every line of its file, and prints every
  approval on every run.
- **An approval follows the code.** An entry that matches no forbidden import fails: the import moved away, was
  fixed, or became allowed. `rotproof approve --prune` removes those entries. It needs no terminal, since it only
  takes approvals away and never lets anything pass that failed before.
- **The number of approvals can have a limit.** `approved_max = 3` in `.config/rotproof.toml` is the most entries the
  approvals file may hold, an entry that matches nothing counted too; `rotproof check` fails above it, naming both
  files. `0` allows none. Without the key there is no limit, and the check prints every approval all the same.
  The limit is not a barrier but a count that cannot grow in silence: raising it is a change of the declaration, which
  the diff shows, and the guide tells an agent that raising it to pass the check is not its decision.
- **This does not prevent an exception; it makes one visible.** Nothing stops a run of `rotproof approve` that nobody
  asked for, or a script that writes the file. What stays is that an approval is a named line with its reason, printed
  on every run, which fails once it matches nothing, and which a reviewer sees in the diff.

## The layers

A project declares its structure in `.config/rotproof.toml`, the directory tools share for their configuration.
`rotproof init` writes it, and from then on it is the project's file: Rotproof only adds a field it lacks and sets
`files` on an upgrade. pip installs only the Rotproof binary: the layer
definitions are built into it, and the declaration is never shipped with it.

```toml
stack = "python"         # python | typescript | rust | none
areas = ["billing", "records"]  # the areas knowledge is grouped by, in this order (see The records)
ui = "atomic"            # the layers of the screens: atomic | none (rust: none; none for a stack of none)
absent = ["ui.templates"]  # layers or levels this project does not have
unchecked = ["scripts"]  # paths outside the layers that Rotproof does not look into
todo_link = "number"     # optional, number | key: a TODO may stay when it names its issue (see below)
approved_max = 3         # optional, a count: the most approvals the project holds (see "Approving a forbidden import")
```

`todo_link` and `approved_max` are the two keys of the declaration that `rotproof init` and `rotproof create` never
write: a project that wants one adds it by hand, and everything else is as without it. Without `todo_link` the marker
check below fails on every `TODO`; without `approved_max` the number of approvals has no limit. `todo_link` is
`"number"` for `TODO(#123): what is left`, or `"key"` for `TODO(ABC-123): what is left`; any other value fails, naming
the two, and so does the key with `stack = "none"`, which has no code to read. The declaration is read strictly, with
a field Rotproof does not know failing it, so a Rotproof older than the one that brought `todo_link` cannot read a
declaration that holds it: a project pins an exact version of Rotproof, and moves it before it adds the key. Whether a
project declares either is a person's decision, which an agent never makes to pass the check, and the guide says so.

What the layers are is written once, in `layers/table.toml`, on the onion architecture:

| Layer | Its levels | May import |
|---|---|---|
| `composition` | | every layer: the one place that wires the program |
| `ui` | `pages` > `templates` > `organisms` > `molecules` > `atoms` (a level imports the levels below it) | `pages`: `presentation.models`, `presentation.display`, `protocol.inbound`. `templates`, `organisms`: `presentation.display`. `molecules`, `atoms`: nothing. Never `application` or `domain` |
| `presentation` | `models` (what a screen holds and decides) > `display` (the plain data handed to the view) | `models`: `protocol.inbound`. `display`: nothing |
| `infrastructure` | `inbound` (entry points: the command line, HTTP, events, jobs), `outbound` (connections: databases, clients, files) | `inbound`: `protocol.inbound` alone. `outbound`: `protocol.outbound` and `domain` |
| `application` | | `protocol`, `domain`: a use case implements an input port and calls output ports |
| `protocol` | `inbound` (input ports, with plain data), `outbound` (output ports) | `inbound`: nothing. `outbound`: `domain` |
| `domain` | | nothing |

The levels of a layer other than `ui` and `presentation` never import each other. `ui` is the view and
`presentation` the state and behaviour of the screens apart from it, the Presentation Model (Fowler) held to the
inward direction of the onion: a model calls the use cases through `protocol.inbound`, as an entry point does, and
draws nothing. The two come with `ui = "atomic"`, and a project with `ui = "none"` has neither: `ui` is chosen as the
stack is, not switched off in `absent`, since a screen needs both or neither. In TypeScript, `presentation` also holds
no view: a `.tsx` or `.jsx` file there fails `rotproof check`, so the state and the hooks of a screen are `.ts` files
and a component goes in `ui` or `composition` (a provider keeps its context and state hook in `presentation` and its
thin component in `ui.pages`, or in `composition` for the whole program). The entry file is no layer, and may
import `composition` alone. No layer holds general-purpose parts: a part goes in the layer whose concept it speaks.

Where they live is written once per stack:

| Stack | A layer is | A level is | The entry file | Code Rotproof looks at |
|---|---|---|---|---|
| `python` | `<layer>/__init__.py`, the role as its docstring | `<layer>/<level>/__init__.py` | `__main__.py` | `.py` files anywhere (`.PY` too), except `tests/` |
| `typescript` | `src/<layer>/index.ts`, the role as a doc comment | `src/<layer>/<level>/index.ts` | `src/main.tsx` (Vite's entry point) | every file in `src/` |
| `rust` | a crate, `crates/<layer>/`, the role as `//!` | a module, `crates/<layer>/src/<level>/mod.rs` (no UI yet: `ui = "none"`) | `src/main.rs` of the root package | every crate in `crates/`, and every `.rs` file in `src/` of the root package but the entry file |

`stack = "none"` declares a repository that keeps records only: `rotproof create` makes no layer, only `docs/`, the
guide and the project's files, and `rotproof check` checks only the records and prints that it did not check the
layers. It is a line in the declaration, not a flag, so the structure check is never switched off where the
declaration still declares layers.

A level is declared absent by its dotted name (`absent = ["ui.templates"]`). A layer holds only its levels, so one
whose every level is in `absent` is not there either, and is declared absent as a layer: `absent = ["infrastructure"]`,
not both of its levels, which fails the declaration (it would otherwise make a directory with nothing in it, and
there would be two ways to write one thing). Files `.gitignore` excludes and
hidden files are not looked at, so a virtual environment or a build directory is not code. A code file is matched in any
case (`stray.PY`, `cargo.toml`): Windows runs or reads it all the same.

A project made of several parts that each hold the layers declares them as units, with `[[unit]]` tables (see
"Units"); without them the whole project is one unit, which is what the rest of this file describes.

## Units

A project is one unit unless it says otherwise: its layers sit where its stack puts them, and every rule above is
about them. A project made of several parts that each hold the layers, such as the packages of a monorepo or the
modules of a modular monolith, declares them in `.config/rotproof.toml`, one `[[unit]]` table for each. Each unit
then holds the layers by itself, and the project holds none of its own: the units replace the one implicit unit.

```toml
[[unit]]
name = "billing"                       # required: a lowercase letter, then lowercase letters, digits and _
path = "packages/billing/src/billing"  # required: where its layers sit directly, from the root ("." is the root)
shared = true                          # optional, false by default: any unit may import it
entry = ["apps/cli/__main__.py"]       # optional, an array even for one path: the unit is an app, which wires the others
root = "packages/billing"              # optional, `path` by default: where the unit's manifest is
absent = ["domain"]                    # optional: the layers or levels this unit does not have
```

`rotproof init` writes a commented example at the end of the declaration, after `files` and `declined`, so that a
project that takes it out of the comment leaves them above the tables. The `[[unit]]` tables come last in the file for
that reason: a key written after one belongs to it. Once the project declares units, `absent` at the top is no unit's
and must be empty: each unit says what it lacks in its own `absent`, which names the same layers and levels, and an
app, which holds only `composition` and `infrastructure`, lacks the rest. `unchecked` is the project's, and so is
every other field above the tables; `stack = "none"` has no layers and so no units. A unit `rotproof create` makes is
made as the one unit is, in the directory of its `path`, and it makes the layers alone: a Python or TypeScript unit
gets no `pyproject.toml` or `package.json`, which are the project's. A Rust unit's layers are crates, each named for its
unit (`billing_domain` for the layer `domain` of the unit `billing`, against `domain` in a project with no unit), since
Cargo refuses two crates of one name; the unit's directory holds the crates and has no manifest of its own. The root
`Cargo.toml` it writes then is a workspace with no package, naming each crate in `members` (a glob such as
`crates/*` would match the directory of a unit and fail in Cargo), and `src/main.rs` is not written. The entry file
of a unit is `<dir>/src/main.rs` in Rust, and `rotproof create` makes with it the package that holds it, `<dir>/Cargo.toml`
(named for the unit, and for the unit and its directory from its second entry on), which depends on the unit's own
`composition` by path and which `members` names; an entry file declared in another form, that is not there, fails the
run before anything is written, and one that is there is the project's, with its package. When the root `Cargo.toml` is
there already, `rotproof create` changes `members` and nothing else of it, only to add the crates and packages it made
that no member names, and to replace the member `crates/*`, the glob Rotproof wrote before units, with the crates it
matched once it would match a directory that has no `Cargo.toml`; it says each change (`changed Cargo.toml: ...`). A
project made by an older Rotproof that declares no unit keeps its glob, and `rotproof create` leaves its `Cargo.toml`
as it is.

A declaration that cannot be read as it is written fails every check, saying every reason at once, from the
declaration and the layout alone, before the tree is looked at:

- a name that is not `[a-z][a-z0-9_]*`, or two units with the same name;
- a `path` or `root` that is not written from the root with `/` as `unchecked` is (`./a`, `/a`, `a/../b`, `C:/a`, a
  `\`, an empty one), and two units with the same `path`;
- a `root` that is neither the `path` nor a directory above it;
- a unit inside a layer of another unit (a unit sits beside the layers of the unit around it, however deep, and nowhere
  in them), and a unit whose directory is named like a layer or like a path that is no layer (`tests`), which the
  unit around it would read as one;
- in Python, a unit whose name is not the name of the directory of its `path`, since Python imports a unit as that
  name (a unit at the root has none to name it); two units whose directories have one name (`two units are imported as
  billing (a/billing, b/billing)`), which Python cannot tell apart; and a unit whose directory is named like a module
  of Python's standard library (`queue`, `string`: every name of `sys.stdlib_module_names` in Python 3.10 to 3.14), for
  Python would import the unit or the module depending on how the program starts;
- an `entry` written as a string (`entry = "apps/cli/main.py"`): it is an array even for one path, `entry =
  ["apps/cli/main.py"]`, so that there is one way to write it;
- an `entry` that is not written from the root, is no code of the stack, lies inside the path of a unit other than
  the one that declares it (the innermost unit that holds an entry is its own, or none), or is declared by two units;
- a unit that is `shared` and has an `entry`: an app is imported by none, and a shared unit by all;
- a name in a unit's `absent` the layout does not have, as at the top for a project of one unit;
- a unit whose `absent` names every level of a layer but not the layer, as at the top for a project of one unit;
- an `absent` at the top, once units are declared.

What `rotproof check` checks of the tree holds in each unit as it does in a project of one, and a finding about a unit
names it (`domain of unit billing is missing: ...`), which a project that declares no unit never sees:

- The unit is there (`unit billing is missing: packages/billing/src/billing/`, which says all about it: its layers are
  not judged), and so is each entry file it declares (`the entry apps/cli/__main__.py of unit cli is missing`). The
  entry of a project of one unit stays optional, as it was.
- Every layer of the unit is present or in the `absent` of that unit, and no layer in it is there; the levels of a layer
  are judged by the layer of their own unit. Each unit holds at least one layer, so the floor is each unit's.
- No code sits outside the layers of a unit: every code file in its `path` is in a layer, or is one of its own entry
  files, or sits in a path the layout says is no layer (`tests/` in Python), or in a path `unchecked` keeps, or in the
  `path` of a unit inside it, which is that unit's to judge. A Python unit whose `path` is not the root may hold an
  `__init__.py` directly in it, read as a place that imports no layer; no other file, and none in a TypeScript or a
  Rust unit (`index.ts` at the top of a unit is code outside its layers). What the layout says of where code goes
  (a screen goes in `src/ui/pages`) is left out of a finding about a unit, whose directories are its own.
- Code in no unit's path fails too, as code outside the layers (`code outside the layers: helper.py (it is in no
  unit: ...)`), so that moving a file out of every unit does not escape the checks: the layout's scope is walked as in
  a project of one unit (Python: the whole project, TypeScript: `src/`, Rust: `crates/` and the `src/` of the root
  package), and a code file there that is in no unit's `path`, is no unit's entry file, is not in a path that is no
  layer (`tests/` in the scope, and in the root of each unit, where a package keeps its own) and is not kept by
  `unchecked` fails. A unit at the root of the project holds every path. A unit whose `path` is outside the scope (a
  monorepo's `packages/billing/src/billing` in TypeScript) is walked all the same.
- A path in `unchecked` neither holds nor sits in a layer, and does not hold a unit: `unchecked = ["packages/billing"]`
  would switch the unit off. A path inside a unit's `path` and outside its layers is what `unchecked` is for.
- A manifest (`pyproject.toml` in Python, `package.json` in TypeScript, `Cargo.toml` in Rust, in any case: the one
  kind of the stack) belongs to the unit whose `root` holds its directory, and a manifest elsewhere is a package no
  unit says it is: `apps/x/package.json is a manifest outside every unit's root: declare the unit whose root is apps/x
  (root = "apps/x"), or list apps/x in unchecked`. The manifest at the root of the project (a workspace's), one in a
  layer (a Rust layer is a crate of its own), and one in a path `unchecked` keeps are not counted. A project that
  declares no unit is not asked, so a project that is one package with others of tooling in it is read as before.

A knowledge document that records why the code is divided so follows the table `.config/rotproof.toml::unit` (see
The records and "The document of the units"), and is read again when a unit changes, not when `areas` or `files` does.

### What a unit may import

A unit imports the layers of its own unit as the table says. Only direct imports are judged, as everywhere. Between
units, the rules are these, applied to an import from one unit's layer into another unit's, from the top:

1. An import that lands in the directory of another unit outside its layers fails: what is read from it afterwards
   cannot be seen. A layer is named, never the unit as a whole: the finding says how the stack imports one (a Python
   module, `billing.protocol.inbound`; a TypeScript subpath of the unit's package, `@acme/billing/protocol/inbound`, or
   its path from the importing file when the unit has no package; a Rust dependency on the crate of the layer,
   `billing_protocol`), with a layer the unit holds as the example: `billing.domain` for a unit that lacks `protocol`.
2. A unit that is not shared is imported by no other, except that the `composition` of an app wires the units: it
   imports the `composition` and the `protocol` (both levels) of the units it wires, and no other layer of them, and
   never another app's. Each unit builds its own insides in its own `composition`, and the app wires the units to each
   other. The `composition` of a unit that is no app imports no unit that is not shared. Of a shared unit it reads
   what rule 4 says.
3. A shared unit imports shared units only.
4. Into a shared unit, a layer imports what the table lets it import, the layers of the other unit standing where those
   of its own do (`infrastructure.inbound` imports `protocol.inbound` of a shared unit), except that `application`
   never imports another unit's `application`, which `composition` passes in as use cases, and that the `composition`
   of a unit is imported by the `composition` of an app alone, and with two more. The `protocol.inbound` of any unit
   also imports the `domain` of a shared unit, where the value types used across units live: an input port that could
   not name them would copy their shape into its own data, and a copy drops what the original has (the currency of a
   sum of money). It imports nothing of its own unit's `domain` and nothing of a unit that is not shared, as the table
   says. And a `composition` imports of a shared unit its `protocol` and its `domain` (a kernel of value types such as
   `Money` and `OrderId` is what the ports take and the app builds), an app's its `composition` too, and no other layer
   of it: each unit, a shared one included, builds its own insides in its own `composition`, and the app wires them (a
   shared database client offers a factory there), as rule 2 says of a unit that is not shared.
5. An entry file imports the `composition` of its own unit alone.
6. Shared units do not import each other in a cycle. This finding names the cycle and the imports that close it, and
   cannot be approved.

An import across units that the rules forbid is approved as any other forbidden import is
(`rotproof approve <file> <import>`), except for the cycle. Only direct imports are judged across units too, and a chain
of allowed imports across units can reach a layer that one import could not, in three places:

- The entry file of an app, as in a project of one unit, imports its `composition`, which imports what the entry file
  may not.
- A `composition` builds its own unit, and the places it imports there import places of other units that it may not
  import itself; it imports the `protocol` of another unit, and an app's the `composition` too, which import what it
  may not read there (the ports of a `protocol` carry that unit's own `domain`, as they do in any unit).
- A place that imports a `protocol.inbound` (the `ui`, `presentation.models`, `infrastructure.inbound`, `application`
  and `composition`, in its own unit and, by rule 4, in another) meets the types of a shared unit's `domain` in the
  data of its ports, though it may not import that `domain`. This holds in what the data carries and in no import:
  nothing but a `protocol.inbound` imports that `domain`. A `protocol.inbound` of a shared unit that imports the
  `domain` of another shared unit, which imports the first one's `domain`, closes a cycle of shared units, which rule 6
  fails.

### Who reads the shared units

A shared unit is imported by any unit, and nothing stops it from collecting what no one needs. A project that declares
a shared unit keeps `.rotproof/readers.md`, a list of what the other units read of each shared unit, which
`rotproof index` and `rotproof create` write from the imports of the code and `rotproof check` compares with what they
would write now. No rule fails for what the list shows: a library that one unit uses may be shared on purpose, and the
list is for the person who reads it, and for the diff that shows a name appearing or its readers changing.

```markdown
<!-- Generated by `rotproof index` from the imports of the code: do not edit. -->
# Who reads the shared units

...

## core

`packages/core/src/core`

| Name | Read by |
|---|---|
| `core.domain.score.count_star6` | billing, steps |

### Read by one unit

| Name | Read by |
|---|---|
| `core.domain.rank.top` | steps |
```

The shared units are in the order the declaration gives them, each with its `path`; the names under each are in byte
order, and each is followed by the names of the units that read it, in byte order too, and no file (a change of a line
moves nothing in the list). A name that exactly one unit reads is in the section `Read by one unit`, where what a
shared unit holds for a single reader stands out. A shared unit that no unit reads says `Read by no unit.`. A name is
what an import of another unit names in the shared unit, whether or not the rules between units allow it, as far as
the reader sees it:

- in Python, the dotted module as the import resolves it (`from m import n` is `m.n`, `import m.n` is `m.n`);
- in TypeScript, the path the import lands in, from the root and without its extension, and `: name` for each name it
  takes (`default`, `*` for `import * as ns` and `export * from`, the name as the module exports it for `{ a as b }`);
  an import that takes none (`import "x"`, `require("x")`, `import("x")`, `import.meta.resolve("x")`) is the path alone;
- in Rust, the path the crate writes through its dependency on a crate of the unit, from the crate
  (`core_domain::score::count_star6`, also through a name bound with `as`); a module imported whole (`use
  core_domain::score;`) is the module, and a crate bound to a name is the crate. A crate that declares the dependency
  and names nothing from it reads nothing.

An import of a unit's code outside its layers is no read of a name, nor is an import within a unit. The list is there
when at least one unit is shared, and is not otherwise: `rotproof index` takes away a list that is left when no unit is
shared any more, and `rotproof check` fails while it is there.

When the list is there and is not what the code says, the finding names what changed in the names, a line each under
it, so that a person sees which import moved the list and does not run `rotproof index` unread: the shared unit and
the name, as `core: core.domain.rank is read now, by billing` for a name the list lacks, `core: core.domain.old is
read no more (was read by steps)` for a name that no unit reads now, and `core: core.domain.score is read by billing,
steps (was billing)` for a name whose readers are not the ones listed; a unit that is shared no more but is still in the
list is `<unit> is listed, and is shared no more`. The lines are in the order of the unit's name and then the name's,
the first ten are said and the rest counted (`and 26 more`). They are read from the rows of the list's tables, so
nothing else that was edited in it is named, and a list that is missing says none.

### The document of the units

The rules above say what a unit may import. Why the code is split into these units, and why each edge between them
exists, are for a person to say, in a knowledge document that follows the declaration: `follows` takes the key
`.config/rotproof.toml::unit`, every `[[unit]]` table of the file and not its other fields, so `rotproof check` fails
when a unit is added, removed or changed until the document is read against it again and pinned
(`rotproof follows knowledge/units.md .config/rotproof.toml::unit`).

`rotproof init` drafts that document when it runs on a project that has a declaration with `[[unit]]` tables, and
neither a document of `docs/knowledge/` follows that key (a deprecated one counts, as it was deprecated on purpose) nor
a file named `units.md` is there, in any case. It writes `docs/knowledge/units.md` once, then the index of
`docs/knowledge/`, and says `drafted docs/knowledge/units.md`; a second run, with the document rewritten or not,
writes nothing, and it never writes over a file. The draft is a knowledge document that passes `rotproof check` as it
is written:

- the frontmatter has the type `Knowledge`, the title `Units`, the first of the `areas` as its tag, the status `stable`,
  and `follows` with the key above at the hash of the tables as they are now;
- a `flowchart LR` of Mermaid has each unit as a node (a shared unit marked `(shared)`, an app `(app)`) and an arrow
  from each unit to each unit it imports, once, from the imports the direction check reads in all three stacks,
  whether or not the rules allow them: an import by the `composition` of an app of the units it wires, and the reads
  of the shared units, as "Who reads the shared units" lists them;
- each unit has a section, `## <unit>`, with a line that says its responsibility is not written yet: what the unit is
  responsible for, why the code is split this way, why each of its edges exists, and which edges are left out on
  purpose.

The edges are what the code has, as a start to write over, and the draft says so: one that should not be there is a
defect to fix. With no `areas` there is no tag to give it, so `rotproof init` writes nothing and says why, on every run,
until there is an area; the same when the units are written in a form the key cannot follow (`unit = [{ ... }]`).

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
`src/api.ts::Router.add`, `crates/x/src/cli.rs::Command.Before`); in a `.toml` file also a table, written
`<file>::<name>` (`.config/rotproof.toml::unit`, `Cargo.toml::workspace.dependencies`); or every file Rotproof reads as
code under a directory (`src/api/`: `.py`, `.ts`, `.tsx`, `.mts`, `.cts`, `.js`, `.jsx`, `.mjs`, `.cjs` and `.rs`, in any case,
whatever the stack of the project). A definition is hashed on its own source, so a change elsewhere in its file does
not fail it. A name that is more than one piece in its file (overloads, a getter and its setter, a Python property and
its `@x.setter`, an interface merged with a class, the `impl` blocks of a Rust type) is hashed on all of them, in the
order they appear, joined with a line break. Every definition counts, exported or not. In Python, a function or class
and its methods, from the first decorator; in TypeScript and JavaScript, a function, class, interface, type, enum or
namespace, a variable (`export const f = () => ...`, the whole statement), the members of a class, an interface and an
enum, and `default` for an `export default` that has no name; in Rust, a `fn`, `struct`, `enum` and its variants,
`union`, `trait` and its items, `type`, `const`, `static`, `macro_rules!` and an inline `mod` with its items
(`tests.helper`). A Rust type's key covers its definition and every `impl` block for it in the file, `Type.method` is
the method of that name in any of them, and a Rust definition starts at its first attribute or `///` doc comment, and one
named by a raw identifier (`pub fn r#match`) is written without the `r#` (`src/lib.rs::match`). What
a macro defines is not seen. A table is a heading, `[unit]` or `[[unit]]`, and a piece of it is that table as it is read, up to
the next heading in the file. A name selects every table whose heading begins with it, as a class holds its methods: `unit` is
each `[[unit]]` and each `[unit.x]` below one, `workspace` the `[workspace]` table and `[workspace.dependencies]`,
and `workspace.dependencies` that table alone; the pieces are joined in the order they appear, as an overload's are.
The keys above the first heading are in no table, so a document that follows `.config/rotproof.toml::unit` fails when
a unit changes and not when `areas` or `files` does (the file as a key fails for both). A table is hashed as it is read,
not as it is written: its heading and then each field on a line, `key = value`, in the byte order of the keys (a dotted
key written out in full), strings in double quotes with `\`, `"` and the control characters escaped, arrays and inline
tables on one line (an inline table's keys in order), so a comment, a blank line, the quotes of a string, how an array
is wrapped, and the order of the keys leave the hash as it was, while a value written as another one (`a = "x"` for
`a = ["x"]`) is another value, which is why a unit's `entry` has one way to be written (see "Units"). **Known limit:** what a
person reads as the same table may be written in another order, and the hash does not know it. An array keeps the order
it is written in, so a document that follows `.config/rotproof.toml::unit` fails when the strings of a unit's `absent`
change places, and the pieces of a name are joined in the order they appear, so it fails too when the `[[unit]]`
tables of the declaration change places, though what the project declares is the same. Read the table, and pin it
again with `rotproof follows`. To avoid it, write `absent` in one order, and add a
unit at the end of the declaration. A name is bare keys joined by dots (letters, digits, `-` and `_`). A table with no heading of its own (a value, `unit = [{ ... }]`, or one made by a
dotted key) is not named, so it is "not there"; a `.toml` file that is not TOML cannot be read, which the check says as
it says it of a Python file that does not parse. A `.toml` file is no code of a directory: `src/` is hashed on the same
files as before. Once the document is reviewed against the code, `rotproof follows knowledge/api.md
<key>...` (or `docs/knowledge/api.md`) pins each key at its hash as the code is now, adding a key `follows` lacks with
its hash quoted, and writes all the keys or none: pinning changes the document, so its commit records the review.
Nothing else in the document changes: it keeps a byte order mark and its line endings (CRLF on every line when one
line had it). Run it when a document starts to follow something, and when `rotproof check` fails because what it
follows changed since: the check names the command to run. It pins only the keys named, needs no terminal, and leaves
a key gone from the code to edit by hand. There is no command that re-pins every changed hash in one act: a hash says
only that the code changed, so each document is read against its code and pinned with the keys named.

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

- **The tree matches `.config/rotproof.toml`, either way:** the declaration names a known stack and a UI it offers,
  and only layers that stack and UI have in `absent` (a misspelled field fails; a layer the UI leaves out, as `ui`
  with `ui = "none"`, fails until it is taken out). Without a declaration nothing is checked, and `rotproof check`
  exits 2, not 1: CI tells a project that has not adopted Rotproof from one that breaks a rule. Every layer of the stack is present or declared
  absent, and no layer declared absent is present. No code sits outside the layers, the stack's own paths (`tests/`),
  the entry file and `unchecked`. In Rust, the root package's `src/` holds the entry file and nothing else: a `.rs`
  file there (`src/pricing.rs`, `src/lib.rs`, `src/bin/`, a module of `main.rs`) fails as code outside the layers,
  and the finding says where code goes, while what Cargo keeps outside `src/` in the root package (`build.rs`,
  `tests/`, `benches/`, `examples/`) and `xtask/` are not looked at. A path in `unchecked` exists and neither holds nor sits in a layer, so a layer cannot be switched
  off by listing it. At least one layer is present: with every layer declared absent, nothing would be checked.
  A project that declares units (see "Units") is checked unit by unit: each unit's `path` and entry files are there, and
  each of its layers is present or in its own `absent`, with at least one present; its code outside the layers is
  looked for in its `path` up to the units inside it, and in the scope of the stack outside every unit's `path`; and
  every manifest of the stack outside the root of every unit fails.
  A layer with levels holds only its levels: code beside them fails, apart from the layer's own file
  (`ui/__init__.py`). What the whole UI shares goes in a level: a part that knows no project concept, visible or not
  (a design value, one behaviour, a provider of a theme), is an atom, and what a screen holds and decides goes in
  `presentation.models`. A directory of a layer of the table before 0.4.0 (`handler`, `utils`) is named with where
  its code goes now. In TypeScript, a `.tsx` or `.jsx` file in `presentation` fails, by its extension alone (case
  does not count): a view written with `createElement` in a `.ts` file passes, and so does a test outside `src/`,
  which Rotproof does not read; a test in `presentation` that needs a JSX `wrapper` writes it with `createElement`,
  or sits outside `src/`.
- **The layers import only what the table allows** (Python): every `import` and `from ... import` in the layers,
  relative ones and those inside functions or under `if TYPE_CHECKING:` included, and those of the entry file. A
  layer may import itself and what its `imports` name (a layer with its levels, or one level); a level what its own
  `imports` name, and in `ui` and `presentation` the levels below it. Only direct imports
  are judged: what the table allows is closed under chaining, so a chain of allowed imports inside a unit never reaches
  a forbidden layer (between units it can: see "Units"). A module is placed where its parts, as a path, land as the operating system that runs the check resolves
  them: on Windows, `Infrastructure.db` lands in `infrastructure`, as Python imports it with `PYTHONCASEOK` set.
  Imports of modules in no layer (the standard library, packages) are not judged. A call of
  `importlib.import_module` or `__import__` (also as `builtins.__import__`, or by a name imported with `from
  importlib import import_module`) whose module is a string literal is an import of that module, read as one; a module
  that is not a literal (a variable, an f-string, a concatenation with `+`) or is relative (`".x"`) is built at run
  time and is not seen. A file that is not UTF-8 or has a syntax error fails, since its imports cannot all be read.
- **The layers import only what the table allows** (TypeScript): every `import` (`import type` too), `export ...
  from`, `import x = require(...)`, and `import(...)`, `import.meta.resolve(...)` and `require(...)` with a literal
  string, in the `.ts`, `.tsx`, `.js` and `.jsx` files (and `.mts`, `.cts`, `.mjs`, `.cjs`) of the layers, read with
  [oxc](https://oxc.rs/). The place of an import is the path it lands on, whether or not a file is there: a relative
  specifier from the file, one starting with `/` from the root (as Vite reads it), and any other through
  `compilerOptions.paths` of the `tsconfig*.json` files at the root (and the local files they extend), then through
  their `baseUrl` when a module is there, then through the package of a unit when it starts with the `name` of the
  `package.json` at a unit's `root` (see below); otherwise it names a package, which is not judged. The path is placed where
  it lands as the operating system that runs the check resolves it: on Windows, `../Infrastructure/db`,
  `../infrastructure./db` and a short name (`../INFRAS~1/db`) land in `infrastructure`, as Vite builds them, and
  links are followed everywhere. A `tsconfig*.json` that
  cannot be read, or a `paths` entry with more than one `*`, fails rather than leaving its aliases unjudged. Two
  limits: only the first target of a `paths` entry is used, and a config extended from a package is not read, so an
  alias defined only there is taken for a package. A project that declares units reads the `package.json` at the `root`
  of each unit (the root of the project too, when a unit's root is there), and an import through a package's `name`
  and a subpath (`@acme/db/client`) is placed where that package says it lands, then judged between units as any
  import is: the package without `exports` takes the subpath as a path in the package (`@acme/db/src/domain`), and
  the package itself is its `types`, or its `main`, or its root; with `exports` (a string, a map of subpaths, a pattern
  with one `*`, a map of conditions, a list), only the subpaths it lists are the package's, each an exact key or the
  pattern with the longest prefix, and among conditions the first of `types`, `import` and `default` that leads to a
  path is taken. An import of a subpath a package with `exports` does not list (or lists as `null`) fails as `imports
  @acme/db/domain, which packages/db/package.json does not export, so Node and tsc fail it and its layer cannot be
  judged: export the subpath in packages/db/package.json, or import one that is exported`, which no approval covers:
  it is a failure, not an import skipped, and nothing is judged of it until it names a subpath the package exports.
  An import of a subpath it lists whose target holds no file fails too, such as `@acme/db/domain/invoice.ts` through
  `"./*": "./src/*/index.ts"`, which leads to `src/domain/invoice.ts/index.ts`: it fails as `imports
  @acme/db/domain/invoice.ts, which packages/db/package.json exports as packages/db/src/domain/invoice.ts/index.ts,
  where no file is`, and is judged by where it leads all the same, so that a rule it breaks is not hidden by the path
  being wrong. A file is there when the path is one, or has an extension of source (`.ts`, `.tsx`, `.mts`, `.cts`,
  `.d.ts`, `.js`, ...) added, or holds an `index` of those, or is written `.js`, `.jsx`, `.mjs` or `.cjs` beside the
  source it is built from; a target that lands outside every unit (build output that is not there yet) says nothing.
  A package with no `exports`, which takes the subpath as a path, is held to the same. A `package.json` that cannot be
  read as JSON, or whose `exports` is not a string, a list or a map, fails too, and a package with no `name` is no
  package an import names. Limits: the `tsconfig` of a
  unit is not read (its `paths` are not applied; only those at the project's root are), `imports` (`#name`) and a
  condition other than the three are not read, and an import that lands outside every unit's path (a package
  whose `exports` point to build output such as `./dist/client.js`) is not judged.
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
  TOML fails, and so does a dependency taken from a workspace that does not declare it. An entry file is read through
  the manifest of its package: the first `Cargo.toml` with a `[package]` from the entry's directory up to the root
  (the root package's for `src/main.rs`, and for `src/bin/w.rs` of a unit's package the unit's), so the entry files of
  every unit are read; an entry file that is there and has none fails, since what it imports cannot be judged (in a
  project that declares no unit too), and one that is not there says nothing. A dependency on another unit's crate is
  judged by the rules between units (below): in a project that declares units, `shipping_infrastructure` depending on
  `billing_protocol`, both not shared, fails, a shared unit's `core_protocol` may be depended on as `protocol` is, and
  `application` never depends on another unit's `application`; the levels of that crate are judged with the same rules
  where its code names them (the `inbound` module of a `protocol` crate may name the `domain` crate of a shared unit,
  and no other). Not seen: `[patch]` and `[replace]`, and source files taken from another crate's
  directory (`include!`, `[lib] path`).
- **The levels of a crate import only what the table allows** (Rust): a level is a module of its layer's crate,
  which the manifest cannot tell apart, so the code of `src/` is read. Every path from `crate`, `super`, `self` or a
  dependency on another layer, in a `use`, an expression or the tokens of a macro, is judged by the level it lands in
  (`crate::outbound::Db` from `src/inbound/` fails, and so does `domain::Song` there). The crate's `src/lib.rs` only
  declares its levels' modules, so no re-export lets one level reach another; every other file of `src/` sits in a
  level, and `#[path]` fails. A path that starts with `::` (`::protocol::outbound::Port`) is the path without them.
  A name bound with `as` is the path it renames (`use protocol as p;`,
  `extern crate protocol as p;` or `use crate as c;`, then `p::outbound::Port` from `src/inbound/` fails as
  `protocol::outbound::Port` does), and so is a name bound to such a name; the file is read as one scope, so a name
  bound twice stands for both. Not seen: a path a macro builds from pieces, and a name bound in one file and used
  through another.
- **Imports between units follow the units' rules** (every language): the units are the parts of the code that each
  hold the layers (see Units); a project that declares none is one unit, and none of this applies. An import inside a
  unit is judged as above. An import into another unit is judged by what each is, first:
  1. the code of another unit outside its layers fails (`imports billing, in the unit billing outside its layers`):
     import the module of a layer, such as `billing.protocol.inbound` (in TypeScript, a layer through the package of
     the unit, such as `@acme/billing/protocol/inbound`, or by its path from the importing file when the unit has no
     package; in Rust, depend on the crate of a layer, such as `billing_protocol`, since a dependency of Cargo is no
     import of a module). The example is a layer the unit holds: `protocol.inbound` when it has it, else the first it
     has of `protocol.outbound`, `domain`, `application`, `composition`, `infrastructure.inbound`,
     `infrastructure.outbound`, `presentation.models` and `ui.pages` (`billing.domain` for a unit that lacks
     `protocol`);
  2. a shared unit imports only shared units (`the shared unit core imports only shared units, and billing is not
     shared`);
  3. a unit that is not shared is imported by the `composition` of an app, which is a unit with an entry, and by no
     other place, nor by an app's `composition` into another app, and the `composition` of an app reads of it its
     `composition` and its `protocol` (both levels) and no other layer (`the composition of an app imports only the
     composition and protocol of a unit that is not shared: billing builds its own insides in its own composition`):
     each unit builds its own insides, and `composition` wires the units and nothing else does; the `composition` of a
     unit that is no app reads none of them;
  4. a shared unit is read as if its place sat in the importer's own unit, so the table decides, with these more
     rules: `application` never imports another unit's `application` (`composition` passes a shared unit's use cases
     in), the `composition` of a shared unit is imported by the `composition` of an app alone, the `domain` of a shared
     unit is imported by the `protocol.inbound` of any unit, which the table lets import nothing (the value types used
     across units live in a shared unit, and an input port that cannot name them copies their shape into its own
     data; a `protocol.inbound` imports no `domain` of its own unit and none of a unit that is not shared), and a
     `composition` reads of it its `protocol` and its `domain` (the value types the ports take) and no other layer,
     an app's its `composition` too (`the composition of a unit imports only the protocol and domain of a shared
     unit: core builds its own insides in its own composition`; of an app, `...imports only the composition, protocol
     and domain of a shared unit: ...`): a shared unit builds its own insides as any unit does.

  The entry file reads the `composition` of its own unit and nothing of another unit. Only direct imports are
  judged, and what these rules allow is closed under chaining except in three places. An entry file imports a
  `composition` that wires what the entry file may not import. A `composition` builds its own unit, whose places
  import places of other units that the `composition` may not import itself, and imports the `protocol` of another
  unit (an app's `composition` its `composition` too, and a shared unit's `domain`), which import what it may not
  read there. And every place that imports a
  `protocol.inbound` (the `ui`, `presentation.models`, `infrastructure.inbound`, `application` and `composition`)
  meets in the data of its ports the types of the shared `domain` that `protocol.inbound` imports, which it may not
  import itself: this holds in what the data carries, and in no import, since nothing but a `protocol.inbound`
  imports that `domain`. A forbidden import between units is approved as any
  other. The shared units that import each other, one way round or more, fail once each, as one finding that names
  the units in a cycle and the first import of each step (`the shared units import each other in a cycle: core →
  steps → core (core/application/x.py:3 imports steps.domain.a; steps/domain/b.py:1 imports core.domain.c)`); it is
  no import of a file, so no approval covers it. In Python, a name that is a layer's alone (`domain`) that fails in
  another unit also says where the unit's own is: Python finds `domain` from where the program starts, and the
  unit's own layer is `billing.domain`.

  In Python a unit is imported as the name of its directory, so an import whose first part is that name is looked up
  in the directory that holds the unit (`packages/billing/src` for `packages/billing/src/billing`) and lands in a layer
  of the unit, or in the unit outside its layers: `import billing`, `from billing import models` and the like fail by
  the first rule (a module reached through an attribute, `billing.protocol` after `import billing.domain`, is not
  seen). A bare layer name is looked up from the root of the project, as Python does when the program starts there: it lands in the layer of a
  unit at the root (`path = "."`) and, where the root has none, nowhere, so what it imports is not judged. The
  `__init__.py` directly in a unit's directory is read as a place that imports no layer: it may import none, and the
  code of another unit outside its layers fails there as everywhere. A relative import is a path, and lands where the
  path does; it is named by its full dotted name in a finding and in an approval.
- **Only `infrastructure` reads or writes the environment variables:** the environment is input from outside, as
  unchecked as a request, so `infrastructure` reads it into a settings type it validates, and `composition` passes
  that on. The code of every other layer and the entry file fails on `os.environ`, `os.environb`, `os.getenv`,
  `os.getenvb`, `os.putenv` and `os.unsetenv` in Python, `process.env` and `import.meta.env` in TypeScript, and
  `std::env::var`, `var_os`, `vars`, `vars_os`, `set_var` and `remove_var` in Rust, each through an alias the file
  binds too (`import os as o`, `from os import getenv as g`, `import { env } from "node:process"`,
  `const { env } = process`, `use std::env;` then `env::var`, `::std::env::var`). Not judged: the command line (`sys.argv`,
  `std::env::args`), `env!`, `option_env!` and a crate's `build.rs` (read when Rust compiles), the values of
  `import.meta.env` Vite sets itself (`DEV`, `PROD`, `MODE`, `BASE_URL`, `SSR`), what is built at run time
  (`getattr(os, "environ")`, `from os import *`), and the code outside the layers (`tests/` in Python).
- **Every approval matches a forbidden import** (see "Approving a forbidden import"): a forbidden import with an entry
  in `.config/rotproof-approved.toml` for its file and import passes, and is printed on every run; an entry that
  matches none fails, and so does a file that is not TOML, an entry with a field missing, empty or unknown, an
  `approved.at` that is not a datetime with a time zone, and two entries for one import. A file named otherwise
  (`Rotproof-Approved.toml`) approves nothing, and fails. With `approved_max` in the declaration, **the approvals stay
  within it**: more entries than it lets fails, as one finding that names both files.
- **The project's files are up to this version of Rotproof** (see "Upgrading Rotproof"): `files` in the declaration
  (0.1.0 when it is missing) is a version as `major.minor.patch`, neither older nor newer than the running Rotproof,
  and every name in `declined` is an update.
- **`.rotproof/AGENTS.md` is up to date:** it equals what `rotproof create` writes for the stack with this version of
  Rotproof (see "Upgrading Rotproof"), so it fails after an upgrade until `rotproof init` has run, and when edited by
  hand. The finding names the command that applies: `rotproof create` when `files` names the running version (the
  guide changed under it, as in a development build, or by hand), `rotproof init` otherwise.
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
  strings. Comments in other files (a stylesheet, a `Cargo.toml`) are not read. A project that declares `todo_link`
  may keep a `TODO` that names the issue tracking its work: see "Work put off that an issue tracks".
- **No comment switches the type checker off**, in the comments the marker check reads: in Python, `# type: ignore`
  and `# mypy: ignore`, `# pyright: ignore`, `# pyrefly: ignore` or `# ty: ignore`, with or without codes, and the
  file-level `ignore-errors` of mypy and Pyrefly, also after another comment (`# noqa  # type: ignore`); in
  TypeScript, `@ts-ignore` and `@ts-expect-error` where the compiler reads them, at the start of a `//` comment or of
  the last line of a `/* */` comment, and `@ts-nocheck` at the start of a `//` comment; as the compiler does, the start
  of the name counts (`@ts-ignored` silences it too). Data from outside becomes a validated type where it comes in, and a comment that hides the
  checker's error hides where that is missing. Lint suppressions (`eslint-disable`, `# noqa`) are not read. Any other
  comment of those four checkers is a warning, which fails nothing, unless every option in it only makes the checker
  stricter: `strict` and `report<Rule>=error` for Pyright, and an option that starts with `disallow-`, `warn-`,
  `strict-` or `enable-error-code` for mypy (`# pyright: basic` and `# mypy: allow-untyped-defs` warn). Rust has no
  such comment.

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
  `::`, as `f`, `Class` or `Class.method`; a table of a `.toml` file, as `unit` or `workspace.dependencies`) with a
  hash of 8 lower-case hex digits, and that hash is what it names now:
  the file's text, every line ending as `\n`; the definition's source from its first decorator or attribute to its
  end, all the pieces of a name defined more than once in the order they appear, joined with `\n`; a table's
  headings, each to the next heading in the file, joined in the same way; or every file read
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
  day written in Tokyo before 09:00. Time alone only makes one pass, so this never turns CI red with no change.
- **Every markdown file under `docs/knowledge/` is a document Rotproof reads:** it sits in `docs/knowledge/` itself (a
  markdown file in a directory under it fails, as Rotproof would not read it; a picture there passes), is named `.md`
  in lowercase (GitHub shows a `.MD` file, but Rotproof would not read it), is not `log.md`, a name OKF reserves for
  the log in `docs/`, and is a knowledge document or a guide. The fields OKF defines for a document pass as OKF writes
  them (`generated` needs only `by`; every entry of `sources` needs a `resource`; `usage_window` is a `{from, to}`
  range).
- **Every file Rotproof generates equals what `rotproof index` writes:** `docs/index.md` and `docs/knowledge/index.md`,
  so nobody maintains a list by hand, and `docs/knowledge/rules.md`, so the rules a project reads are the rules its
  Rotproof checks. A project's own rules go in another guide beside it.
- **The list of who reads the shared units equals what `rotproof index` writes:** `.rotproof/readers.md` is there, as
  written from the imports of the code now, when a unit is shared, and is not there when none is (see "Who reads the
  shared units"). It fails as ``out of date, run `rotproof index`: .rotproof/readers.md``, for a list that is missing,
  edited, or no longer what the code says, with the names that changed under it (see "Who reads the shared units").

What a check finds file by file (the layers' imports, the markers, `NOTE`, the environment variables and the
type-checker suppressions) is listed in the byte order of the whole path, written with `/` from the root and compared
as bytes compare: `Beta.py`, `_under.py`, `alpha.py`, and `mid2.py` before `mid2/x.py`, since `.` comes before `/`. It
is the same on every operating system, whatever order its file system lists files in, so two machines that run the
check on the same tree print the same lines in the same order. The list of linked `TODO` comments keeps the order it
had (path, then line), which this order agrees with.

One thing is a warning and fails nothing, besides the comments that may make a type checker check less and the map in
`AGENTS.md` (below): **a knowledge document whose `stale_after` has come** (the day in the time
zone of the machine that runs the check) is printed on every run from that day, with what to do: measure again what it
says of outside the code, which `follows` cannot watch, then write the day in `verified` and the next day in
`stale_after`. Failing would turn CI red with no change, from the clock alone. A deprecated document is not read for
it.

### The map in `AGENTS.md`

`AGENTS.md` is the project's own file, written once by `rotproof create` with a map: a table under `## Map` with a row
for each layer present, whose first cell is the layer's path. The map goes behind when the declaration changes later: a
layer declared absent and made afterwards, a unit added, a layer taken out. `rotproof check` warns of it, on every run
and failing nothing, under `the map in AGENTS.md lists the layers .config/rotproof.toml declares`: a line for each layer
present that has no row (`AGENTS.md has no row in its map for packages/cli/src/cli/protocol/, the layer protocol of the
unit cli: add it, or declare the layer absent in .config/rotproof.toml`), and for each row of a layer declared absent;
after six lines it counts the rest. `rotproof init` says the same, each line after `note: `. The words of a row are the
project's and are never judged, and a file with no `## Map` and table, or none at all, is not read: a project may keep
its map as it likes. The map `rotproof create` writes names the unit in each row when the project declares units (`cli:
The composition root: ...`), as the same layer in two units would otherwise read alike, and gives the whole first
sentence of the layer's role, not its first line.

The frontmatter is read as YAML 1.2: quoting a date or a datetime never changes whether it passes. It holds no comment: a `#`
after a space or at the start of a line starts one, which YAML reads as nothing, so an unquoted `title: PR #1` would
be the title `PR`; quote a value that holds one. Its closing `---` may end the file. The body is read as GitHub
renders it: an HTML comment or a fenced code block (``` or ~~~, indented by up to 3 spaces, running to the end when
it is not closed) is not text, so a heading, a link or the only text of a section inside one counts for nothing. A
heading may be indented by up to 3 spaces, and a closing run of `#` (`## Notes ##`) is not part of it.

### Work put off that an issue tracks

The marker check exists so that put-off work does not rot where nothing asks when it is done. A project may let a
`TODO` stay when an issue already tracks the work, by declaring `todo_link` (see "The layers"). Then a `TODO` passes
into a list, printed on every run, when it has exactly this form, on one line:

| `todo_link` | The form | Reference listed |
|---|---|---|
| `"number"` | `TODO(#123): what is left` | `#123` |
| `"key"` | `TODO(ABC-123): what is left` | `ABC-123` |

The number has no leading zero. The key is an upper-case letter, one or more upper-case letters or digits, `-` and
digits. The reference is followed by `): ` and a description with a character in it. One issue goes with one `TODO`:
`TODO(#1, #2): x` fails. Everything else still fails, a `key` where `number` is declared and the reverse included
(`TODO later`, `TODO #123`, `TODO(#123)`, `TODO(#123) x`, `TODO(me): x`), and so does `FIXME`, `XXX`, `HACK` or `NOTE`
in the same form. The reference between the parentheses is not read for markers (`TODO(XXX-12): x` is no `XXX`); the
description is read as any comment. A line that holds a linked `TODO` and another marker, or a `TODO` not in the form,
fails as it does without the key, and is not listed. A line that also holds `NOTE` is listed, and its `NOTE` is a
finding of its own kind, as when it stands alone. Without `todo_link`, none of this applies and the output has nothing
new. When a `TODO` fails and `todo_link` is declared, the heading names the form, after the three ways out.

```text
linked TODO (2)
  src/app/handler/upload.py:42  #123
  src/app/domain/price.py:10    #130
```

The list is no finding and no warning: it never changes the exit code, and it is printed apart from both (after the
approvals, before the warnings), nothing at all when it is empty. The form of its lines is kept, so that a CI step can
read it: a line is two spaces, `path:line`, padding, and the reference, sorted by path and then by line; the
first field is the place, the last the reference (a path that holds a space is the one case where the place is every
field but the last).

`rotproof check` does not look at whether the issue exists or is open: it reads files alone and runs offline, so it
gives the same answer every time, and an invented number passes. Without a step that asks the tracker, nothing but the
guide in `.rotproof/AGENTS.md` stops one. In GitHub Actions, with `todo_link = "number"`, a step that fails on a linked
`TODO` whose number is closed, missing, or a pull request's (the job needs `issues: read`, and a `key` needs the
tracker's own lookup in place of `gh`):

```yaml
      - name: Every linked TODO names an open issue
        env:
          GH_TOKEN: ${{ github.token }}
        run: |
          list=$(rotproof check || true)
          failed=0
          while read -r place ref; do
            state=$(gh api "repos/{owner}/{repo}/issues/${ref#\#}" \
              --jq 'if .pull_request then "a pull request" else .state end' 2>/dev/null) || state=missing
            if [ "$state" != open ]; then
              echo "::error::$place: issue $ref is $state"
              failed=1
            fi
          done < <(printf '%s\n' "$list" | awk '/^linked TODO/ {on=1; next} on && /^  / {print $1, $NF; next} {on=0}')
          exit $failed
```
