# Rotproof specification

What Rotproof checks, keeps and writes, in full: the commands, the project's files, the layers, the records and every
rule of `rotproof check`. The [README](https://github.com/secta113/rotproof#readme) says what Rotproof gives, how to
install it and how to try it in five minutes, and what a version promises. A change to what Rotproof checks, keeps or
writes changes this file in the same commit.

## Overview

Rotproof makes the layer directories, checks that they are where the project declares them, and checks their
direction: from the imports in Python and TypeScript, and in Rust from the dependencies each crate declares and, for
the levels inside a crate, from the paths its code names. From the same reading of the code it checks that only
`infrastructure` reads the environment variables, that the inner layers do no I/O and import only the packages they are allowed, that no comment switches the type checker off, and that the manifest
of each package lists the dependencies its code imports, no more and no fewer (see "What `rotproof check` checks"). A project made of several parts that each hold the layers declares them as units, and all
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
rotproof --root <repository> init --stack python --ui-design atomic  # write .config/rotproof.toml, when there is none
rotproof --root <repository> init    # after upgrading Rotproof: update the project's files (see "Upgrading Rotproof")
rotproof --root <repository> create  # list the layers it would make, writing nothing (makes docs/ when it makes no layer)
rotproof --root <repository> create --yes  # make the layers .config/rotproof.toml declares, the records and the guide
rotproof --root <repository> check   # check the layers and the records; exits 1 when a rule is broken
rotproof --root <repository> index   # write the generated files that are out of date (docs/'s index files and the knowledge rules, and who reads the shared units)
rotproof --root <repository> approve <file> <import>... --reason "<why>"  # keep a forbidden import (see below)
rotproof --root <repository> approve --prune  # remove the approvals that match no forbidden import
rotproof --root <repository> baseline  # freeze the violations there are, once, with no code moving (see "Freezing the violations there are")
rotproof --root <repository> baseline --prune  # remove the baseline's entries that match nothing, and lower its count
rotproof --root <repository> follows <document> <key>...  # pin keys in a knowledge document's follows, as the code is now
rotproof --root <repository> new <slug> [--area <area>] [--alarm [--match <string>]...]
                                     # start a knowledge document: its frontmatter, the description left empty
rotproof --root <repository> log --title <task> [--issue <owner/repo#n>] --problem <text>... --lesson <text>...
                                     # add an entry to the log, under today's heading
rotproof guide                       # print the guide `rotproof create` writes for the stack and the UI the declaration has
rotproof guide --stack python        # print the rules Rotproof keeps for a stack, with or without a project
rotproof guide --stack typescript --ui-design none  # the same for the layers of that UI (the values of init's --ui-design), with no project
```

`--root` defaults to the current directory. The declaration is read from `<repository>/.config/rotproof.toml`, and the
records from `<repository>/docs`.

The binary leads the way without this file, for an agent that has only Rotproof: `rotproof --help` says what it is,
the order to start in and the exit codes, `rotproof <command> --help` what a command reads, writes and never does, and
each command's output names the next step. A test follows that path from an empty directory to a passing check.

Start a project with `rotproof init --stack <stack>` (`python`, `typescript`, `rust`, or `none` for a repository
that keeps records only), and `--ui-design` for the stacks that offer a UI: `--ui-design atomic` for a project with screens (in Python, whose views
may be templates the server renders too), `--ui-design none` for one without screens (a command-line tool, a library,
a server); `--ui`, the name of the field before 2.0.0-rc.2, is the same flag. Python
and TypeScript need it named, as a project with screens and one
without start from the same command; Rust offers `none` alone. It writes only the declaration,
so you declare in `absent` the layers you do not want before anything is made. Then run `rotproof create`, which
lists the layers it would make and writes nothing, and `rotproof create --yes` once `absent` lists every layer the
project lacks. Run on a project that has a declaration, `rotproof init` upgrades the project's files instead (see
"Upgrading Rotproof"), and never changes the declaration's values but `files`.

`rotproof create` makes layers only with `--yes`. Without it, a run that would make one lists the layers, writes
nothing and exits 2: run straight after `rotproof init`, it would otherwise make every layer the declaration names
before anyone read the list, and it never deletes them again. A run that makes no layer, as after an upgrade, needs no
`--yes`. A run that writes ends by asking whether the project's agent reads the guide, and exits 2, while the project
has not answered it (see the guide under "Upgrading Rotproof").

A TypeScript project with React starts from Vite: run `npm create vite@latest <name> -- --template react-ts` first,
pin Rotproof in it (`npm install -D -E rotproof@2.0.0-rc.1`), then run `rotproof init --stack typescript --ui-design atomic`
and `rotproof create --yes` there (`npx --no -- rotproof ...`), which keep what Vite wrote. Vite's entry point,
`src/main.tsx`, is the entry file where it is (`index.html` loads it), and may import `composition` alone: what Vite
has it import (`./App.tsx`, `./index.css`) moves into `src/composition/`, which `main.tsx` calls. `rotproof check`
names the rest of the starter (`App.tsx`, the styles, `assets/`) as code outside the layers, with where each goes.

Run `rotproof create` when a project starts, and again after you change `.config/rotproof.toml` on purpose. It makes
only what is missing: a layer that is neither present nor declared absent, the project's files when they do not exist
(below), `docs/knowledge/`, and the files Rotproof generates (`.rotproof/AGENTS.md`, `.rotproof/running.md`,
`.rotproof/readers.md` when a unit
is shared, and the index files and the knowledge rules in `docs/`), so a new project passes its first check with
nothing for a person to write but one answer: whether its agent reads the guide (below). It never
overwrites a file it does not generate, apart from adding the fields the declaration lacks (below) and, in a Rust
project, adding the crates it makes to the workspace's `members` when none names them (see "Units"), and never moves
or deletes one. Nothing runs it on its own, so a layer removed by mistake fails `rotproof check` instead of coming back.
A project that declares units (see "Units") also gets each entry file its units declare that is missing, made from
one text for the stack (the entry file's docstring or doc comment, and in Rust `fn main() {}`), whatever the file is
named, and in Rust the package that holds it (`Cargo.toml` beside its `src/`), and, in Python and TypeScript, the
manifest at each unit's `root` that is missing (see "Units"); the entry file of a project of one unit
stays the project's to write, but in Rust, where it is the root package's `src/main.rs`.

The project's files are written once, as a starting point, and are the project's from then on:

| File | Content |
|---|---|
| `README.md` | The project's name (its root directory's) and how to run Rotproof |
| `.gitignore`, `.gitattributes` | For the stack (`python` ignores `.venv/`); line endings as LF |
| `docs/log.md` | The log, with its title |
| `pyproject.toml` | `python`: the project's name (its directory's, as a distribution is named), version `0.1.0`, `requires-python = ">=3.10"`, no dependency, and Rotproof pinned with `==` in `dev` of `[dependency-groups]` |
| `.github/workflows/ci.yml` | `python`: installs what the lock file at the root holds with the tool that made it (below) and runs `rotproof check`, with a time limit. `typescript`: installs what `package.json` and its lockfile pin (`npm ci`) and runs `npx --no -- rotproof check`, which never downloads another Rotproof, with a time limit. `rust` and `none`: pins Rotproof's version in `ROTPROOF_VERSION`, takes the Linux archive of that GitHub Release, checks it against the release's `SHA256SUMS`, and runs `rotproof check`, with a time limit |
| `Cargo.toml` | `rust`: the workspace, whose members are the crates of the layers present, each named on a line of its own (no glob: see "Units"), so `cargo build` builds every layer present, and its root package, which holds the entry file and lists no dependency until the entry file names one (`composition`, which it calls). A project that declares units has the workspace alone |
| `src/main.rs` | `rust`: the entry file, which calls `composition` and starts the program. A project that declares units has none: an entry file is a unit's own |

A `python` project pins Rotproof in the `dev` group of its `pyproject.toml` (PEP 735), which its lock file locks, so
that Rotproof is installed into the virtual environment of the project and nowhere else. `rotproof create` writes the
`pyproject.toml` when there is none, and, as `package.json` below, never edits one that is there: it says that the pin
is the project's to add. It writes no lock file, which only a tool can make: it says so, and `rotproof check` fails
until one is committed (see "What `rotproof check` checks"). The workflow is written for the tool of the lock file the
root has (`uv.lock`, `poetry.lock`, `pdm.lock`, a `pylock.toml` or a `pylock.<name>.toml`; the first of these when
there are several), and for uv when it has none. Each installs from the lock, failing when the lock is not what
`pyproject.toml` says where the tool can tell, then runs `rotproof check` in the environment: uv `uv sync --locked`
and `uv run --no-sync rotproof check`; Poetry `poetry install --no-root` and `poetry run rotproof check`; PDM
`pdm install --check --no-self` and `pdm run rotproof check`; pip, which installs a `pylock.toml` (PEP 751, which pip
installs experimentally), `pip install -r <the lock file>` into a `.venv` it makes. uv, Poetry and PDM come from PyPI
in the workflow at the version they have on the day it runs, which the project may freeze there. Each README holds the
same commands for a person.

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
  `.rotproof/AGENTS.md`, `.rotproof/running.md`, `.rotproof/readers.md` when a unit is shared, the
  generated files in `docs/`, and
  `docs/knowledge/`, the log and the project's files where they are missing), and sets `files` to the running version, adding `declined = []` when the declaration lacks it; it
  names every field it adds to the declaration, as `rotproof create` does. Crossing 0.4.0, which builds the layer table
  on the onion architecture, it says where the code of each layer that is gone goes (`handler`, `utils`) and that the
  ports move from `domain` to `protocol.outbound`, as "move by hand" lines, and, when a directory of a layer that is
  gone is there, one line more that names the directories it found and says to search the files Rotproof does not read
  (documents, scripts, configuration, comments) for those paths once the code has moved, since no check reads them;
  it moves no code, and keeps nothing behind: `rotproof check` names what is not moved yet. Crossing 1.0.0-rc.1, the first version that reads no record of work, it says, for a
  project whose `docs/work/` is there, as one more "move by hand" line, that Rotproof reads none of that directory and
  writes no `index.md`, `closed.md` or `rules.md` in it, so those stay as they were, that `docs/index.md` no longer
  links to it, and that its open items need a home the person chooses (the project's issues, or kept as frozen
  history); it moves and deletes nothing there, and says nothing for a project without the directory. Crossing
  1.0.0-rc.3, which splits the UI into the view (`ui`) and the
  state of the screens (`presentation`) and declares the UI, the refresh adds the UI to the declaration (as `ui_design` now): `"atomic"` when
  `ui/` is there, `"none"` otherwise. With `"atomic"`, it says, as the same lines, that `presentation` is to be made
  (`rotproof create --yes`), and that the state of a page and its calls of the use cases move to
  `presentation.models` and the domain values the parts show to `presentation.display`; with `"none"`, that `ui` is
  to be taken out of `absent`, where `rotproof check` fails on it. Crossing 2.0.0-rc.2, which makes a Python project
  locked (see "What `rotproof check` checks"), it says, for a project of the `python` stack, to list its dependencies in
  `pyproject.toml` and commit a lock file, to move the pin of Rotproof from `requirements-dev.txt` to `dev` of
  `[dependency-groups]` when that file is there, to install the lock in the CI workflow it wrote before, and to keep
  one `.venv`, as "move by hand" lines; it edits none of those files. A declaration that names the field `ui`, as
  one written before 2.0.0-rc.2 does, has it renamed `ui_design` first, on its line, with its value and comments kept
  (`renamed in .config/rotproof.toml: ui to ui_design`); until then `rotproof check` fails on the field it does not
  know, and says that `rotproof init` renames it. For a project that declares units and has no
  document that follows them, it also drafts one, once ("The document of the units"). It never changes the
  declaration's other values, and once it has set `files`, running it again changes nothing. It never writes the
  project's `AGENTS.md`, but says on every run, as `note: ...` lines, what its map has gone behind on (see "The map in
  `AGENTS.md`").
- **An update only adds, or takes out what Rotproof itself wrote and no longer keeps, matched exactly,** and does
  nothing when its file is as the update leaves it already. It writes the file back with the line endings and the
  byte order mark it had. What it cannot do without a person (its file is not in a form Rotproof can read),
  `rotproof init` says, exits 2 and leaves `files` as it was: do it by hand, or decline the update by its name in
  `declined`, and run `rotproof init` again. When its file is missing, the update has nothing to take out, and
  asks for nothing.

| Update | Version | File | What it does |
|---|---|---|---|
| `claude-remove-stop-hook` | 1.0.0-rc.1 | `.claude/settings.json` | Takes out each `Stop` hook whose command is exactly `rotproof stop-hook`, or the hook beside it that 0.4.0 added (it said when `rotproof` was not on the `PATH`), a group left with no hook, `Stop` when no group is left, and `hooks` when no event is left (see "The hooks Rotproof no longer writes"). Written again as above |
| `claude-remove-time-hook` | 1.0.0-rc.4 | `.claude/settings.json` | Takes out each hook on `SessionStart` and `UserPromptSubmit` whose command is exactly `rotproof time-hook`, a group left with no hook, an event left with no group, and `hooks` when no event is left (see "The hooks Rotproof no longer writes"). A hook with another command that runs it, such as a path to the binary, stays. Written again as above |
| `claude-remove-deny-approvals` | 2.0.0-rc.2 | `.claude/settings.json` | Takes out each `"Edit(/.config/rotproof-approved.toml)"` of `permissions.deny`, which `rotproof create` wrote until 2.0.0-rc.1 so that an agent could not write the approvals file, `deny` when no rule is left, and `permissions` when no key is left (see "Approving a forbidden import"). A rule with another text stays. Written again as above: a file with nothing else in it ends as `{}` and is not deleted |

The two updates 0.4.0 brought, `claude-stop-hook-on-path` and `claude-time-hook`, which added the hooks the first two
updates above take out again, are gone, as nothing adds those hooks now, and so is `claude-deny-approvals`, which added
the rule the last one takes out. A declaration that still names one of them in `declined` is reported by `rotproof
check` as naming no update: take the name out. `rotproof create` writes no `.claude/settings.json` any more.

Among the files Rotproof generates is `.rotproof/AGENTS.md`: the rules Rotproof keeps, written for the project's
stack. It lists the layers with where each lives and what it may import (from `layers/table.toml`), gives the rules of
units, and gives the rules of the records, starting with reading the knowledge index before work, and where open work
is kept (an issue); the rules every session needs are in it. For how to run Rotproof it says one thing, to read
`.rotproof/running.md`. It names
the Rotproof version that wrote it, so after an upgrade `rotproof check` fails until `rotproof init` (or
`rotproof create`) has rewritten it. Edited by hand, it fails too: a project's own rules go in its own `AGENTS.md`.

`.rotproof/running.md` is generated too, the same text whatever the stack. It holds what an agent needs only when it
upgrades Rotproof or runs a command: when to run each command, and how to upgrade (in a commit of its own,
`rotproof init`, and how to read each line it prints, with `declined`). It says how to judge, and never which command
comes next: `rotproof check` and `rotproof init` say that, so a second place that did could drift from them. It is
written beside the guide and nowhere an agent loads by itself, such as a directory of skills: an agent reads it because
the guide names it, and reads the guide only when the project points it there. It carries no version, so an upgrade
rewrites it only when its text changed; when `files` is behind, `rotproof check` names `rotproof init` for it, as for
the guide.

The guide only helps if your agent reads it. Rotproof writes no `AGENTS.md` or `CLAUDE.md`, nor anything else an agent
reads by itself: what an agent is told belongs to the person, and a line Rotproof wrote there would change what the
agent obeys whenever Rotproof is upgraded. So it asks instead. When neither the root's `AGENTS.md` nor its `CLAUDE.md`
names `.rotproof/AGENTS.md`, in any words, and `declined` does not name `guide-pointer`, `rotproof create` and
`rotproof init` end by asking whether the project's agent should read the guide, and exit 2 (after doing all they do
otherwise: `rotproof init` sets `files`):

```text
ask the person: should this project's agent read the rules Rotproof keeps, in .rotproof/AGENTS.md? The person decides; an agent asks before it adds the line or declines
  yes: add this line to the file the agent reads first (here: AGENTS.md):
    The rules Rotproof keeps: @.rotproof/AGENTS.md
  no: add "guide-pointer" to declined in .config/rotproof.toml
```

The question names the files of the two that are there, or, with neither, says to ask the person which file to make.
The line is a sentence with the path bare, not in backticks: an agent that brings files in by `@path` (Claude Code,
from `CLAUDE.md`) brings the guide in, and one that does not reads the path as where to look. Which file the agent reads
first, and whether one file brings in another, is not looked at: that is the agent's own, and changes with its version.
`rotproof check` fails on the same judgement until the question is answered (see "What `rotproof check` checks"), so it
cannot be skipped; answered either way, nothing asks it again. A project that declines it is checked as any other, but
its agent knows only the rules `rotproof check` enforces, not those it cannot (reading the knowledge index before work,
asking a person before writing knowledge, keeping open work in an issue). A project that kept the `CLAUDE.md`
`rotproof create` wrote before 2.0 imports the guide already, and is not asked.

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
`from m import n` is its own import, `m.n`, whether `n` is a module or not: approving `m` approves none of them. The
I/O of the inner layers (see "The inner layers do no I/O") is approved the same way, and the import is the entry of
Rotproof's table that the finding names (`rotproof approve domain/report.py shutil --reason "..."`), which covers every
use of that entry in the file, whatever names it uses under it; a name judged by an allowlist (`os.getcwd`), a group
(`clock`) and a method of the net (`read_text`) are entries of their own. A package no list allows in an inner layer
is approved by the package as the finding names it (a Python distribution as PEP 503 writes it: lowercase, each run of
`-`, `_` and `.` one `-`): `rotproof approve domain/report.py requests --reason "..."`. Several imports of one file that are kept for one reason
are named together (`rotproof approve domain/report.py shutil clock --reason "..."`): each gets an entry of its own,
the same as when it is approved alone, and when any of them is not forbidden, is approved already or is named twice,
none is written, and every reason is said.
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
- **Claude Code is not denied editing the file.** Until 2.0.0-rc.1 the `.claude/settings.json` that `rotproof create`
  wrote held `"permissions": { "deny": ["Edit(/.config/rotproof-approved.toml)"] }`, which refused Claude Code's edit
  tools and its shell's file commands and redirects on the file. It is not written now, and `rotproof init` takes it
  out of a project's settings (see "Upgrading Rotproof"): an approval is kept in view by the check, which prints it on
  every run and counts it, and not by keeping the agent from the file.
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

## Freezing the violations there are

A project that adopts Rotproof in code that already breaks the layer table has more violations than anyone fixes on
the first day. `moving` (see "The code that is moving") carries it while the code is moved into the layers. Once every
file is in a layer, what still fails can be frozen in one act, from 2.0.0-rc.2, instead of being approved one import at a
time with a reason each (see "Approving a forbidden import"):

```sh
rotproof baseline
```

It writes `.config/rotproof-baseline.toml` with every violation `rotproof approve` could keep: a forbidden import of
the layer table, and a use of the I/O the inner layers do none of (see "The inner layers do no I/O"), each keyed as an
approval is, by the file that imports and what it imports as `rotproof check` names it (the entry of Rotproof's table,
for the I/O), never by the line, and one entry for each such pair whatever lines it is on. An entry has no reason and
no name, as nobody decided to keep it: it was there. What an approval covers is left out, so each violation has one
home. The file records how many entries it was written with:

```toml
# (a comment at the top, which says what the file is)
count = 2

[[entry]]
from = "domain/model.py"
import = "infrastructure.db"

[[entry]]
from = "domain/report.py"
import = "shutil"
```

Environment reads, comments that switch the type checker off, markers and missing dependencies are not frozen: they
stay failures to fix, and `rotproof baseline` says so.

- **It is written once.** `rotproof baseline` exits 2, writing nothing, when the file is there already, when `moving`
  in `.config/rotproof.toml` is not empty (move the code into the layers first: the baseline freezes what is left once
  everything is in one, and a path still moving is not judged by any of these checks), when the approvals file cannot
  be read, when the project is a repository of records only, which has no layers, and when there is nothing to freeze.
  It needs no terminal and no `user.name`, as it signs nothing.
- **A violation that is not in the file fails,** as it did before the file: a new import, another file importing the
  same thing, and another entry of the table in a file that has one frozen are each a finding, printed as any is. A
  violation in the file passes and is not printed.
- **The baseline follows the code and only shrinks.** An entry that matches no violation fails: the import was fixed,
  moved, became allowed, or an approval covers it now. `rotproof baseline --prune` removes those entries and lowers
  `count` to the number of entries left, so that the room a fixed violation left cannot be taken by another. It never
  lets anything pass that failed before. An approval and a baseline entry for one import are two homes, and the
  entry is the one that matches nothing. `rotproof approve --prune` prunes the approvals only.
- **The count detects growth; it is not a barrier.** `rotproof check` fails when the file holds more entries than its
  `count`, as one finding that names the file. Nothing stops a person or an agent from writing the file again, or from
  raising the count with a new entry: what stays is that the file is written once and `check` can read only the
  working tree, so a baseline written again larger shows in the diff of the file, which a reviewer sees. The guide in
  `.rotproof/AGENTS.md` says to run `rotproof baseline` only when the user asked, never to write it again and never to
  raise `count` or add an entry to pass the check, as it says of `approved_max`. That is the same spirit as
  `approved_max`: the file is kept in view, not made unwritable.
- **One line is printed on every run,** apart from the findings and the warnings, once the file is there, and no line
  for each entry: `baseline: 12 entries remain in .config/rotproof-baseline.toml` (`1 entry remains`, and
  `baseline: no entry remains in .config/rotproof-baseline.toml: remove the file` once every violation is fixed). It
  counts the entries of the file, an entry that matches nothing included, which fails on its own. A file that cannot
  be read fails, with a missing `count`, an unknown field, an entry with a field empty and two entries for one import
  among its reasons, and a file named otherwise (`Rotproof-Baseline.toml`) freezes nothing, and fails.
- **The file keeps its line endings and its byte order mark** when `--prune` writes it, as the approvals file does.
  `rotproof init` and `rotproof create` write and mention nothing of it: it is a decision to freeze, and the user's.

## The layers

A project declares its structure in `.config/rotproof.toml`, the directory tools share for their configuration.
`rotproof init` writes it, and from then on it is the project's file: Rotproof only adds a field it lacks and sets
`files` on an upgrade. pip installs only the Rotproof binary: the layer
definitions are built into it, and the declaration is never shipped with it.

```toml
stack = "python"         # python | typescript | rust | none
areas = ["billing", "records"]  # the areas knowledge is grouped by, in this order (see The records)
ui_design = "atomic"     # the screens: atomic | none (rust: none; a stack of none has no ui_design)
absent = ["ui.templates"]  # layers or levels this project does not have
unchecked = ["scripts"]  # paths outside the layers that Rotproof does not look into
moving = ["legacy"]      # optional: paths of the project's own code that is not in a layer yet (see below)
todo_link = "number"     # optional, number | key: a TODO may stay when it names its issue (see below)
approved_max = 3         # optional, a count: the most approvals the project holds (see "Approving a forbidden import")

[dependencies]           # optional: what the manifests and the tree cannot say (see "What `rotproof check` checks")
imports = { dotenv = "python-dotenv" }  # Python only: the distribution an import is of
without_import = ["openpyxl"]           # dependencies the code uses without importing them
allowed = { domain = ["pydantic"], "presentation.models" = ["zustand"] }  # Python and TypeScript: the packages an inner layer may import
```

`todo_link`, `approved_max` and `allowed` (under `[dependencies]`, below) are the keys of the declaration that
`rotproof init` and `rotproof create` never write: a project that wants one adds it by hand, and everything else is as
without it. Without `todo_link` the marker
check below fails on every `TODO`; without `approved_max` the number of approvals has no limit. `todo_link` is
`"number"` for `TODO(#123): what is left`, or `"key"` for `TODO(ABC-123): what is left`; any other value fails, naming
the two, and so does the key with `stack = "none"`, which has no code to read. The declaration is read strictly, with
a field Rotproof does not know failing it, so a Rotproof older than the one that brought `todo_link` cannot read a
declaration that holds it: a project pins an exact version of Rotproof, and moves it before it adds the key. Whether a
project declares any of them is a person's decision, which an agent never makes to pass the check, and the guide says so.

`moving` lists the paths of the project's own code that is not in a layer yet, in a project that adopts Rotproof in the
middle of a refactoring. It is no `unchecked`: `unchecked` is for code that is never moved (generated or vendored
files), and `moving` for code that is to go into the layers. It is written as `unchecked` is, from the root with `/`,
and judged as it is: a path that holds or sits in a layer, that holds a unit, or that does not exist fails, and the code
in it is judged by no check (not the direction, not the markers, not the structure). What `rotproof check` does with
it is to count it and say which files to move first (see "The code that is moving"), and, as for `unchecked`, to list
every import of it by the code of an inner layer (see "The inner layers do no I/O"). `rotproof init` writes
`moving = []` into a declaration it makes, `rotproof create` adds it to none, and a Rotproof older than the one that
brought it cannot read a declaration that holds it, as for `todo_link`. A repository with `stack = "none"` has no code
to move, so `moving` must be empty there.

`[dependencies]` is the table that tells the check of the dependencies (see "What `rotproof check` checks") what the
manifests and the tree cannot: `imports` maps the name of an import to the distribution that provides it, as the code
writes the name (`dotenv`), and a dotted name covers the modules below it (`google.cloud.storage`), the longest entry
winning; and `without_import` lists the dependencies the code uses without importing them, as the manifest lists them
(`openpyxl`, which `pandas` loads). `imports` is Python's: an import and a dependency are one name in the other stacks,
so it fails there, and so does any key of the table with `stack = "none"`, which has no code. A name that
`without_import` lists and no manifest of the project lists as a runtime dependency fails, so the table does not keep
what is gone. `rotproof init` and `rotproof create` write none of it, a project adds the table by hand, and a Rotproof
older than the one that brought it cannot read a declaration that holds it, as for `todo_link`. The verdict is decided
from this table and the tree alone: nothing is installed, so it is the same on every machine and in CI.

`allowed` lists the packages the inner layers may import, in Python and TypeScript (see "The inner layers do no I/O",
"Packages"): a table whose keys are a layer (`domain`, `protocol`, `application`, `presentation`) or a level of one
(`protocol.inbound`, `presentation.models`) and whose values are lists of package names, as the manifest lists them (a
Python distribution, compared as PEP 503 compares names; an npm package as written, `@scope/name` included). A layer's
key covers its levels, and a level's key adds to that level. A `[[unit]]` table has an `allowed` of its own, **added**
to the project's for the code of that unit. A key written with a dot is written in quotes
(`"presentation.models" = ["zustand"]`): without them TOML reads `presentation.models = [...]` as the table `models`
inside the table `presentation`, which the declaration refuses, naming the key to quote. The declaration fails on: a key
that is no layer or level of the table; a key for `infrastructure`, `composition` or `ui` (and their levels), which
import any package; a key for a layer or level the project lacks, which is a project-level key that no unit has, or a
unit's own key for a place that unit lacks (declared absent, or left out by the UI, as `presentation` is with `ui =
"none"`); a value that is no list of names, or a name that is empty; a name allowed twice for the same code (in one
list, in a layer's key and the key of its level, in the project's list and a unit's, or one the layout allows already);
`allowed` in a Rust project, in the project's table or a unit's, and with `stack = "none"`, as for `imports`. A name that
no manifest of the project lists as a dependency of any kind fails, and so does one that no file of the layer or level
(of the unit, for a unit's own list) it is allowed for imports, so that the list does not keep what is gone; these two
are said under the heading of the check of the inner layers, and the first is not judged while no manifest of the
project was read. What the layout of the stack allows (TypeScript: `react` in `presentation.models`, and not
`react-dom`; Python: nothing) is in `layers/<stack>.toml`, is no name of the declaration, and is never judged gone.
Allowing a package is a person's decision: no command writes `allowed`, nothing drafts it from the code, and the guide
tells an agent never to add, change or take out a name to pass the check.

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
draws nothing. A screen that serves several flows (a sign-in that returns to the screen that opened it, a picker)
does not decide where to go next: whatever opens it passes the destination in (guidance in the role of
`presentation.models` and in the guide; no check reads it). `ui_design` says whether the project has screens, and `absent` which of the layers and levels it has it leaves out:
with `ui_design = "atomic"` the two come, and a project with `ui_design = "none"` has neither. A project with screens
may declare `presentation` absent (a screen that holds no state of its own, as a template the server renders), or a
level of `ui`, but not both `ui` and `presentation`: that says `ui_design = "none"` a second way, and the declaration
fails, naming it. A unit has no `ui_design` of its own, so a unit with no screens in a project that has them lists
both in its own `absent`. In TypeScript, `presentation` also holds
no view: a `.tsx` or `.jsx` file there fails `rotproof check`, so the state and the hooks of a screen are `.ts` files
and a component goes in `ui` or `composition` (a provider keeps its context and state hook in `presentation` and its
thin component in `ui.pages`, or in `composition` for the whole program). The entry file is no layer, and may
import `composition` alone. No layer holds general-purpose parts: a part goes in the layer whose concept it speaks.

Where they live is written once per stack:

| Stack | A layer is | A level is | The entry file | Code Rotproof looks at |
|---|---|---|---|---|
| `python` | `<layer>/__init__.py`, the role as its docstring | `<layer>/<level>/__init__.py` | `__main__.py` | `.py` files anywhere (`.PY` too), except `tests/`; with `ui_design = "atomic"`, template files too (below) |
| `typescript` | `src/<layer>/index.ts`, the role as a doc comment | `src/<layer>/<level>/index.ts` | `src/main.tsx` (Vite's entry point) | every file in `src/`, except the tests (below) |
| `rust` | a crate, `crates/<layer>/`, the role as `//!` | a module, `crates/<layer>/src/<level>/mod.rs` (no UI yet: `ui_design = "none"`) | `src/main.rs` of the root package | every crate in `crates/`, and every `.rs` file in `src/` of the root package but the entry file |

`stack = "none"` declares a repository that keeps records only: `rotproof create` makes no layer, only `docs/`, the
guide and the project's files, and `rotproof check` checks only the records and prints that it did not check the
layers. It is a line in the declaration, not a flag, so the structure check is never switched off where the
declaration still declares layers.

What is declared absent is imported by no layer of the project (of the unit, once units are declared), though the
table allows it: the row of a layer is read without it, in the direction check, in the finding, which says that the
place is absent, and in the documentation `rotproof create` writes into each layer. Between units the row holds as
it is, since an app reads the layers of a shared unit that it lacks itself (see "What a unit may import").

A level is declared absent by its dotted name (`absent = ["ui.templates"]`). A layer holds only its levels, so one
whose every level is in `absent` is not there either, and is declared absent as a layer: `absent = ["infrastructure"]`,
not both of its levels, which fails the declaration (it would otherwise make a directory with nothing in it, and
there would be two ways to write one thing). Files `.gitignore` excludes and
hidden files are not looked at, so a virtual environment or a build directory is not code (one thing looks at them, for
a Python project: the search for virtual environments, below). A code file is matched in any
case (`stray.PY`, `cargo.toml`): Windows runs or reads it all the same.

A project made of several parts that each hold the layers declares them as units, with `[[unit]]` tables (see
"Units"); without them the whole project is one unit, which is what the rest of this file describes.

### Tests

A test is no code of a layer, and every rule reads it as one thing, whatever the stack: where a stack's tests sit decides
only how Rotproof tells them.

- **Python:** a file in `tests/` (at the root of the project, and in the directory and the root of each unit): a place
  that is no layer (`not_layers` of the layout).
- **TypeScript:** a file named as Vitest and Jest take tests by default, since a TypeScript project has no place for
  tests outside `src/` and keeps them beside the code they test: a source file (`.ts`, `.tsx`, `.mts`, `.cts`, `.js`,
  `.jsx`, `.mjs` or `.cjs`) named `*.test.*` or `*.spec.*`, in any case, and any file, source or not, in a directory
  named `__tests__`, at any depth. It is a test wherever it sits in `src/`: in a layer, beside the levels of one, or in
  no layer. The names are the `tests` of `layers/typescript.toml`, which is data and not code; a project cannot
  change them. A test whose name says nothing (`src/domain/songs.ts` that only a runner reads) is code of its layer.
- **Rust:** a test is `#[cfg(test)]` code in a crate, or a file in the `tests/` of a crate or of the root package,
  which Cargo keeps apart from `src/`: no name tells one, and what this section says of the two stacks above does not
  apply.

What a test is in each rule that follows, the same in Python and TypeScript:

| Rule | A test |
|---|---|
| No code outside the layers, none beside the levels of a layer, and no view in `presentation` | is none of them, wherever it sits |
| The layers import only what the table allows | its imports are not read, so it imports any layer and another test |
| No code that is not a test imports a test | an import of a test fails, from a layer, the entry file and a unit's `__init__.py` |
| The manifest lists what is imported | its imports are read, as a test's: it uses development dependencies too |
| The code of a layer imports no development dependency | only a test may |
| A runtime dependency only tests import fails | the imports of tests are the ones that count |
| Only `infrastructure` reads the environment | it is not judged |
| The inner layers do no I/O | it is not judged (see "The inner layers do no I/O") |
| No marker (`TODO` and the like) in a comment, and no comment that switches the type checker off | its comments are read, as the code's |

In Rust no name tells a test: `[dev-dependencies]` are never judged, a crate's `tests/` are read by the dependencies
check with its `src/`, and the marker check reads them. **The environment check and the check of the I/O of the inner
layers read a Rust test the same way,** by one judgement the two share: the items under `#[cfg(test)]` (a module with a
body, a function, a `use`; a file that starts with `#![cfg(test)]` as a whole), the file of a `#[cfg(test)] mod x;`
(`x.rs` or `x/mod.rs`, with the files under `x/`) and everything outside a crate's `src/` (`tests/`, `benches/`,
`examples/`, `build.rs`) are free. `cfg(all(test, ...))` and `cfg(any(test, ...))` are not read as tests, and a
`#[cfg(test)] mod x;` inside a module with a body is not followed to its file.

A path in `unchecked` or `moving` is not read by any check, tests included. `rotproof check` prints, on every run and
only for the TypeScript stack, how many source files it read as tests and by which names, apart from the findings and
failing nothing:

```text
tests: 12 files read as tests (*.test.*, *.spec.*, __tests__/)
```

It is printed with a count of 0 too (`tests: 0 files read as tests (...)`), so that a project whose tests are named
otherwise (`song.tests.ts`, a directory `test/`), and which Rotproof therefore reads as code, sees it on every run. The
count is of source files outside `unchecked` and `moving`, each once (a non-source file in `__tests__/`, a snapshot or a
fixture, is a test and is not counted).

### The inner layers do no I/O

The code of `domain`, `protocol`, `application` and `presentation`, and of their levels (`protocol.outbound`,
`presentation.models`), reads and writes nothing outside the program: the files, the network, the processes and the
terminal are `infrastructure`'s, behind a port in `protocol.outbound` that a use case is given. `rotproof check` fails an
**import of an entry of a table of what does I/O wholly**, which Rotproof keeps for each stack and a project cannot
change. The rule reads imports and, in Python and TypeScript, the names a file uses of a module it has imported; what a
module does only in part (`os`, `pathlib`, `time`, `datetime`, `logging` itself) is not in the table, and is judged by
the name it is used through: in Python by the lists of "Python: what is judged by the name", below; in TypeScript by
those of "TypeScript: what is judged by the name", after them (the globals, which are no import); in Rust by "Rust: what
is judged by the name", after TypeScript's (`std::env`, the clocks, the methods of a path).

- **Where:** the code of the four layers and their levels. Not `infrastructure` (it does the I/O), not `composition` and
  `ui` (they wire and draw), not the entry file, and not a test: in Python a file in `tests/`; in TypeScript a file named
  as a test (see "Tests"); in Rust the items under `#[cfg(test)]` (and a file that starts with `#![cfg(test)]`), the file
  of a module declared `#[cfg(test)] mod x;` (`x.rs` or `x/mod.rs` beside the file that declares it, in the directory of
  the file for `lib.rs`, `main.rs` and `mod.rs` and in the directory named as the file otherwise, and what is under it),
  and everything outside a crate's `src/` (`tests/`, `benches/`, `examples/`, `build.rs`). A `cfg` other than the plain
  `#[cfg(test)]` (`cfg(any(test, ...))`) is not read as a test, and a `#[cfg(test)] mod x;` inside a module with a body
  is not followed to its file.
- **The finding is a forbidden import,** kept as the layer table's are (see "Approving a forbidden import"): `rotproof
  approve <file> <entry> --reason "<why>"` approves it, the file and the **entry of the table** are what the approval is
  made for, and **one approval covers every use of that entry in that file**, whatever names it uses under it. The
  entry is the module (`shutil`, `urllib.request`, `asyncio.open_connection`), Node's module by its bare name (`fs` for
  `fs`, `node:fs` and `fs/promises`), a scheme by the scheme (`bun:`), and in Rust a row of the table: `std::fs`,
  `std::net`, `std::process`, `std::path` (for `std::path::absolute`), `std::io` (the terminal items and the print
  macros together; a name of `std::env` that is I/O is an entry `std::env`, see "Rust: what is judged by the name"),
  `std::os::fs`, `std::os::net` and `std::os::process` (the module of every platform of `std::os`: `std::os::unix::fs`
  and `std::os::windows::fs` are one entry), and a glob above them as `std::*` or `std::os::unix::*`. Each approval is
  printed on every run, as any is, **with the names the file now uses under the entry** (`; now used: copyfile,
  rmtree`), so that a reason that no longer fits the code is seen; an approval that matches nothing fails.
- **A finding starts with what to do,** by layer, then says what it found: "move the I/O into
  `infrastructure.outbound`, behind a port in `protocol.outbound` that the use case is given" for `domain`, `protocol` and
  `application`, and "a screen's model does no I/O: it gets what it needs from its page, or through a use case of
  `protocol.inbound` it is given" for `presentation` (whose models import only `protocol.inbound`; in TypeScript "from
  its page, through a hook in `ui.atoms`, or through a use case"). It names the line,
  the entry and the names used under it. Approving, renaming the import, or moving the code to a path in `unchecked` or
  `moving` is no fix, and the guide says so.
- **A file is one finding for each entry, with every line that uses it.** The key of an approval is the file and the
  entry, so the check says one finding for that key and not one for each line: a file that prints on 13 lines is one
  finding of the entry `terminal`. The finding starts with the first line (`domain/a.py:2: ...`), says what it found
  with the names used under the entry on every line (the names an approval prints as `now used`), ends that sentence
  with `, on lines 2, 3, 4, 5, 7, 8` (every line in ascending order, the first included; nothing when the entry is on
  one line), and shows the source text of the first line. The findings of a file are in the order of the first line of
  each. What is said differently is not one finding: a glob of an entry (`use std::fs::*;`), a scheme, a star import
  or a module used as a value says what it is, and a use of the same entry by name on other lines is a finding apart
  with the same key. This changes how the findings are printed and never which files fail or what an approval
  matches: the entries, the keys and the names are those of the check before, and so is every approval. The findings
  of the other checks (the direction of the layers, the environment, the markers) are one for each line, as they were.
- **What an inner layer reads of the code outside the layers is listed, and judged by no one.** The direction check
  lets a layer import the code of its own unit that is in no layer (the paths of `unchecked` and `moving` are there),
  and this check judges the file that imports it and not the file it imports, so the I/O of a helper moved to such a
  path would be seen by none. `rotproof check` prints, on every run and apart from the findings, as it does the
  approvals, every import by the code of `domain`, `protocol`, `application` or `presentation` (and their levels) of a
  file in a path of `unchecked` or `moving`, in the order of the file and the line, and fails nothing for it:

  ```text
  inner layers read code outside the layers (not judged) (2):
    domain/x.py:3 imports legacy.io_helper, in a path of moving
    domain/x.py:4 imports scripts.tool, in a path of unchecked
  ```

  The import is named as the direction check names it (a dotted name in Python, a specifier in TypeScript, the
  dependency in Rust, where the line is the line of the `Cargo.toml` that declares it), and the path is the one it lands
  in as an import is resolved there. The list asks for nothing and approves nothing: it says where to look. Nothing is
  printed when there is none, or when the declaration lists no path. An import of a path that holds or sits in a layer
  (which switches nothing off), of a file that is in no listed path, and by `infrastructure`, `composition`, `ui`, the
  entry file or a test, is not listed.
- **Seen through every spelling the file binds an entry by.** In Python, an import is matched by its dotted name as a
  prefix of the entry: `urllib.request` is matched by `import urllib.request`, `import urllib.request as r`,
  `from urllib import request` and `from urllib.request import urlopen`, and not by `from urllib import parse`;
  `from m import *` is the import of `m`; an import under `if TYPE_CHECKING:` and a call of `importlib.import_module` with
  a literal are imports; and the names the file uses are read as Python's scopes read them, so `import urllib` then
  `urllib.request.urlopen(...)`, and `import asyncio` then `asyncio.open_connection(...)`, are uses of the entries
  `urllib.request` and `asyncio.open_connection`. A use in a type position (`def f(a: argparse.Namespace)`,
  `Callable[[argparse.ArgumentParser], None]`) is never judged, by the table either: the import it names fails at its
  own line, an `import argparse` under `if TYPE_CHECKING:` too. In TypeScript, a specifier is matched with `node:` or without it, a
  specifier of `import`, `export ... from`, `import()` and `require` with a literal included, `import type` too; a name
  taken from it is said (`"node:fs".readFile`). In Rust, an item is found as the scanner finds the items of a module
  (see the environment check, in "What `rotproof check` checks"): by its whole path, through a `use` of the module or of one above it, through a rename of
  either (`use std::fs as f;`, `use std as s;`), from `::std`, and a module that does I/O whatever is used of it is also
  found where it is imported (`use std::fs;`, `pub use std::os::unix::{fs, net};`). A glob of an entry, or of a module
  above one (`use std::fs::*;`, `use std::*;`), fails, as its names cannot be read.
- **The table is matched before the manifest.** A bare `fs` is Node's `fs` even when `package.json` lists a package of
  that name: the table decides, and the dependencies check is a rule of its own. **A scheme other than `node:`**
  (`bun:`, `npm:`, `jsr:`, `https:`, `data:`, `virtual:`, any URL) **fails in an inner layer**, as what it names cannot
  be judged; it is approved by the scheme (`bun:`).

**The table, by stack.** An entry is the module or the item; a module is matched with what is under it.

- **Python:** the network (`socket`, `socketserver`, `ssl`, `select`, `selectors`, `http.client`, `http.server`,
  `urllib.request`, `ftplib`, `smtplib`, `imaplib`, `poplib`, `xmlrpc.client`, `xmlrpc.server`, `wsgiref.simple_server`,
  `nntplib`, `telnetlib`, `asyncore`, `asynchat`, `smtpd`); storage and the file system (`sqlite3`, `dbm`, `shelve`,
  `mailbox`, `shutil`, `tempfile`, `filecmp`, `fileinput`, `glob`, `netrc`, `zipapp`, `py_compile`, `compileall`,
  `linecache`, `importlib.resources`, `importlib.metadata`, `logging.handlers`, `logging.config`); processes and what
  the processes of a program share (`subprocess`, `multiprocessing.connection`, `multiprocessing.shared_memory`,
  `asyncio.streams`, `asyncio.subprocess`, and the functions of `asyncio` that start a process or a connection when
  taken by name: `asyncio.create_subprocess_exec`, `asyncio.create_subprocess_shell`, `asyncio.open_connection`,
  `asyncio.start_server`, `asyncio.open_unix_connection`, `asyncio.start_unix_server`); the terminal, the console and the
  system around the program (`pty`, `tty`, `termios`, `fcntl`, `curses`, `readline`, `msvcrt`, `winreg`, `winsound`,
  `syslog`, `getpass`, `webbrowser`, `tkinter`, `turtle`, `pwd`, `grp`, `resource`, `pdb`, `code`, `pydoc`, `mailcap`,
  `nis`, `spwd`, `ossaudiodev`, `msilib`, `antigravity`, `argparse`, `optparse`, `platform`, `locale`); and the private
  twins the standard library holds, which are the C modules the public ones are made of (`_socket`, `_ssl`, `_sqlite3`,
  `_posixsubprocess`, `_winapi`, `_multiprocessing`, `_posixshmem`, `_curses`, `_curses_panel`, `_tkinter`, `_dbm`,
  `_gdbm`, `_locale`, `_msi`) and five private modules of the same kind that have no public name to be a twin of
  (`_overlapped`, the C half of asyncio's Windows event loop; `_bootsubprocess`; `_wmi`; `_scproxy`; `_pyrepl`). The
  names are those of `sys.stdlib_module_names` of Python 3.10 to 3.14, and a test keeps each entry among them.
- **TypeScript:** Node's `fs` (and `fs/promises`), `net`, `dgram`, `dns` (and `dns/promises`), `tls`, `http`, `https`,
  `http2`, `child_process`, `cluster`, `readline` (and `readline/promises`), `repl`, `tty`, `wasi`, `process`, the
  internal aliases `_http_agent`, `_http_client`, `_http_common`, `_http_incoming`, `_http_outgoing`, `_http_server`,
  `_tls_common` and `_tls_wrap`, each with or without `node:`; `sqlite` with `node:` alone (a bare `sqlite` is a package
  of npm); and any other scheme.
- **Rust:** `std::fs` whole; `std::os::<platform>::fs`, `std::os::<platform>::net` and `std::os::<platform>::process`
  whole, for every platform `std::os` has; of `std::net`, `TcpListener`, `TcpStream`, `UdpSocket`, `ToSocketAddrs` and
  `Incoming`; of `std::process`, `Command`, `Child`, `ChildStdin`, `ChildStdout`, `ChildStderr` and `Stdio`; of `std::io`,
  `stdin`, `stdout`, `stderr`, `Stdin`, `Stdout`, `Stderr`, `StdinLock`, `StdoutLock`, `StderrLock` and `IsTerminal`,
  and the macros `print!`, `println!`, `eprint!`, `eprintln!` and `dbg!` (also written `std::println!`), which count
  under `std::io`; of `std::path`, `absolute`. The macros are found as words followed by `!`, not as a path of another
  crate (`log::println!`); a macro renamed with `use std::println as p;` is found at the `use` and not at `p!`.
  `std::env` has no row: it is judged by an allowlist of the names that are pure, in "Rust: what is judged by the name".

**What is out of the table, on purpose.** What is I/O only through some of its names or calls, which a later rule
judges: Python's `io`, `logging` itself, `asyncio` as a whole (its tasks, queues and locks are pure), `os`, `os.path`,
`pathlib`, `sys`, `time` and `datetime`; TypeScript's `os`, `crypto`, `stream`, `path`, `url`, `util`, `events`,
`buffer`, `zlib`, `worker_threads`, `timers`, `perf_hooks`, `console` and `test`; Rust's `std::process::exit`,
`ExitCode`, `std::net::IpAddr` and the other plain values of `std::net`, `std::io::{Read, Write, BufRead, Cursor,
Result, Error}` and `std::path` but for `absolute`. What is pure when it holds bytes in memory: `zipfile`, `tarfile`,
`gzip`, `bz2` and `lzma`, which read a file only when handed one, `mmap` and `ctypes`, which need a file or a library
to be given, and `json` and `csv`. A TypeScript global (`fetch`, `console.log`) is no import: it is judged by its name,
in the section after Python's.

**Not seen:** in every stack, a name bound in one
file and used through another (in Rust, `pub use std::fs;` in a module and `reexport::fs::read` in the next: the `use`
itself is found), a path a macro builds from pieces, a module of the project that is named as one of the table (a bare
`import code` is the standard library's), and a type-only import, which counts as any (but for the classes of
`pathlib.Path`, whose import is free when the file uses the name in a type position alone). What Python, TypeScript and Rust
do not see is in their sections that follow.

#### Python: what is judged by the name

What a Python module does only in part is judged by the name that is used, after the table, on the names the file's
scopes resolve (`import os as o` then `o.getcwd()` is `os.getcwd`; a parameter called `os` is nothing). **A reference
counts as a call** (`default_factory=datetime.now`, `f = uuid.uuid4`), and a use in a type position is never judged
(`x: sys.stdout`, `def f(a: os.PathLike)`), but a call inside an annotation is
(`Annotated[str, Field(default_factory=uuid.uuid4)]`). The finding is a forbidden import as the table's are, approved by
the file and the **entry**; a name of an allowlist-judged module is an entry of its own (`os.getcwd`, `sys.platform`,
`logging.basicConfig`, `pathlib.Path`, and `read_text` for a method of the net), and **an act that more than one name
does is one entry, a group**: `clock`, `random`, `terminal` and `argv`. The approval prints the names the file uses
under the entry now (`; now used: datetime.datetime.now, time.time`). The environment check's names (`os.environ`,
`os.getenv`, `os.putenv`, `os.unsetenv`, `os.environb`, `os.getenvb`, `os.path.expandvars`, `os.path.expanduser`,
`pathlib.Path.home`, and the star imports of `os`, `os.path` and `pathlib`) are left to it, and are never reported twice;
the environment check is not approved.

- **Allowlists: what is pure is listed, everything else of the module is I/O.** They start narrow: a name added later
  is a patch, a name taken away is a major change. The guide of a Python project prints each in full.
  - `os` (and `posix` and `nt`): `fspath`, `fsencode`, `fsdecode`, `PathLike`, `error`, `strerror`, `curdir`, `pardir`,
    `extsep`. The constants `sep`, `altsep`, `pathsep`, `name`, `linesep` and `devnull` are I/O for now. `os.urandom` and
    `os.getrandom` are randomness.
  - `os.path` (and `posixpath`, `ntpath` and `genericpath`): `join`, `split`, `splitext`, `basename`, `dirname`,
    `normpath`, `normcase`, `isabs`, `commonpath`, `commonprefix`, `splitdrive`, `splitroot`. `exists`, `isfile`,
    `getsize`, `realpath`, `abspath`, `relpath` and the rest are I/O.
  - `sys`: `version_info`, `version`, `hexversion`, `maxsize`, `maxunicode`, `byteorder`, `float_info`, `int_info`,
    `hash_info`, `implementation`, `getsizeof`, `intern`, `exc_info`, `exception`. `stdin`, `stdout`, `stderr` and the
    `__std*__` are the terminal, `argv` and `orig_argv` the arguments, and `exit`, `path`, `modules`, `platform`,
    `getrecursionlimit` and the rest are I/O.
  - `time`: `struct_time`, `strptime`, and `sleep`, which is on this side on purpose (waiting reads nothing from outside;
    the program that waits is the program that is given the clock); `gmtime`, `localtime`, `ctime` and `asctime` are pure
    only with an argument that is not `None` (no argument, `None`, a `*args` or a reference is the clock), and `strftime`
    only with two arguments. The clock is `time`, `time_ns`, `monotonic`, `perf_counter`, `process_time`, `thread_time`
    and `clock_gettime`, each with `_ns` where it has one; `mktime` (the time zone), `tzset`, `timezone`, `tzname`,
    `clock_settime` and the rest are I/O.
  - `logging`: `getLogger`, `Logger`, `LoggerAdapter`, `Filter`, `Formatter`, `LogRecord`, `getLevelName`, `NOTSET`,
    `DEBUG`, `INFO`, `WARNING`, `WARN`, `ERROR`, `CRITICAL`, `FATAL`. `basicConfig`, the functions of the root logger,
    every handler class, `disable`, `shutdown`, `captureWarnings`, `config` and `handlers` are I/O. A method of a logger
    (`logger.info`) is not seen, and is free.
- **The clock** (`clock`): `datetime.datetime.now`, `utcnow` and `today`, `datetime.date.today` (also through `from
  datetime import datetime`), and the clock of `time` above. Clocks of packages (`pendulum.now()`,
  `django.utils.timezone.now`) are in no table, and are not seen.
- **Randomness** (`random`): the functions of `random` (`random`, `randint`, `randrange`, `choice`, `choices`, `shuffle`,
  `sample`, `uniform`, `gauss` and the other distributions, `getrandbits`, `randbytes`, `seed`, `getstate`, `setstate`),
  `random.SystemRandom`, `random.Random()` with no seed or `None` (`random.Random(1)` is pure), all of `secrets` (found
  at its import), `uuid.uuid1` and `uuid.uuid4`, `os.urandom`. `uuid.uuid3`, `uuid.uuid5` and `uuid.UUID(...)` are pure.
- **The terminal and the files a builtin opens** (`terminal`): the builtins `print`, `input`, `open`, `breakpoint`,
  `exit`, `quit` and `help` (a bare name no scope of the file binds, read as `builtins.<name>`: a parameter called
  `input` is its own), `io.open`, `io.FileIO`, `io.open_code`, `codecs.open`, and the streams of `sys`. Archives
  (`zipfile`, `tarfile`, `gzip.open`) are not I/O here.
- **The command-line arguments** (`argv`): `sys.argv` and `sys.orig_argv`; the outer layers and the entry file read them.
- **`pathlib.Path`, `pathlib.PosixPath` and `pathlib.WindowsPath` are I/O by name:** a use of one fails (one entry for
  each): constructing it, `Path.cwd()`, `Path.home()` (the environment check's, so it is not reported for the call, but
  it counts as a use that runs), `isinstance(x, Path)`, a reference (`f = Path`), and any use outside a type position.
  Its import line fails with them, **but not when every other use of the imported name in the file is in a type
  position, or there is none:** `from pathlib import Path` with `def load_config(path: str | Path = "config.yaml")` is
  free, also under `if TYPE_CHECKING:` and through an alias (`from pathlib import Path as P`: the uses of `P` count; the
  uses are read as the name they resolve to, so two bindings of one class are not told apart, which changes no verdict
  since the file fails at the use that runs). A module of the table (`import argparse`) or a star import is not freed:
  only these three by-name entries are. The pure way is `PurePath`, `PurePosixPath` and `PureWindowsPath` (a path as a
  value), and `isinstance(x, PurePath)` narrows a `Path` too; the finding says so. A type checker may ask for `Path`
  where a `PurePath` is returned (a `Path` method called on it, a port that takes `Path`): writing `Path` in a type
  position alone is the way, and it is not judged. A switch per file that is stricter was rejected, since a `Path`
  value reaches files that never name `Path` through attributes and return values.
- **A method is judged by its name on any receiver, the net,** because the type of what it is called on cannot be read:
  `read_text`, `write_text`, `read_bytes`, `write_bytes`, `iterdir`, `rglob`, `mkdir`, `unlink`, `rmdir`, `symlink_to`,
  `hardlink_to`, `readlink`, `lstat`, `touch`, `chmod`, `lchmod`, `samefile`, `is_symlink`, `is_mount`, `is_junction`,
  `copy_into`, `move_into` and `absolute` always; `exists`, `is_file`, `is_dir`, `stat`, `owner` and `resolve`
  only called with no argument; `open` only called with no argument at all (`x.open()`: no positional argument, no
  keyword and nothing unpacked, which is a file read in practice), with a string literal first (the mode) or a keyword
  `mode`, `encoding`, `newline` or `buffering` (`x.open(name)` is not judged). `group` (`Path.group()`) is left out of the net on purpose, because
  `re.Match.group()` shares the name, is pure and is common in domain code. A call whose callee is a name of a module
  that is judged already
  (`os.mkdir(p)`, `shutil.rmtree(p)`) is judged as that name and not again by the net. **A name is left out of the net
  when the project defines a method of the same name and the same number of parameters** (a `def` in a class body,
  anywhere in the project, a test and a file in `unchecked` included; not counting `self`, `cls` of a `@classmethod`,
  and counting all of a `@staticmethod`; `*args` and `**kwargs` one each): a call of `mkdir` with one argument is left
  out when the project has a `mkdir(self, name)`, and one with none is not. `rotproof check` prints every name left out
  on every run, apart from the findings: `I/O net: left out, as the project defines a method of the same name and number
  of parameters: exists(0), mkdir(1)`; nothing is printed when the project defines none.
- **What cannot be read fails.** A module used as a value, other than through an attribute a rule judges: `sh = os`,
  `getattr(os, "system")`, `vars(os)`, `check(os)` (the modules of every list above, and `datetime`, `random`, `secrets`,
  `uuid`, `io`, `codecs`, `builtins` and `pathlib`; `from os import path` takes a module and is no value). `eval` and
  `exec`, called or referenced, and `importlib.import_module` and `__import__` with a name that is no string literal
  (`Source::unread`); their entries are `eval`, `exec`, `importlib.import_module` and `__import__`. A star import of a
  module with a name that is I/O (`from time import *`, `datetime`, `random`, `sys`, `logging`, `uuid`, `io`, `codecs`,
  `builtins`, `posix`, `nt`) fails, as the names it brings in cannot be read; the entry is the module.
- **Each finding starts with what to do,** by layer: for the clock, randomness and the arguments, pass the time, the id
  or the value in as an argument, or take it from a port in `protocol.outbound` that the use case is given (a port may be
  a callable alias: `Clock = Callable[[], datetime]`); a screen's model, which imports only `protocol.inbound`, gets it
  from its page or through a use case. The guide lists what is not a fix: approving it, renaming or respelling it,
  moving the code to a layer that is not judged or to a path in `unchecked` or `moving`, and listing it as allowed.

**What Python does not see:** a method on an object whose type the reader cannot know and whose name is not in the net
(`logger.info`, `connection.execute`), a name an object holds (`self.clock.now()`), a name reached through a module held
in a variable (`o = os` then `o.system`: the `os` of `o = os` is a module used as a value, and fails there), a string
annotation, a name a star import of a module without an entry brings in, `compile`, the clocks and ids of packages, and a
method of the net on a name an import brings in whose module is not one of the lists (`from app import repo`, then
`repo.mkdir()`: judged by the net, and left out only by a definition in the project).

#### TypeScript: what is judged by the name

A TypeScript or JavaScript global is judged by its name after the table, on the names the file's scopes resolve (a
parameter or a variable called `fetch` is its own, and `function f(process) { process.cwd() }` is no use of the global).
**A global is a name no scope binds, and what the global object holds is the global:** `window.fetch`,
`globalThis.fetch`, `self.fetch` and `global.fetch` are `fetch`. **A reference counts as a call** (`useState(Date.now)`,
`const f = fetch`), and a name in a type position (`let x: Date`, `type T = typeof window`) and the operand of `typeof`
(`typeof window !== "undefined"`) are never judged, a name taken from a module included (`typeof fs.readFile`: the import fails at its own line). A name
taken from a module (`"node:fs".readFile`) is the table's.
The finding is a forbidden import as the table's are, approved by the file and the **entry**: a global is an entry of
its own by its name (`fetch`, `localStorage` for `localStorage.getItem`, `document`; the finding names the member), and
`process` by its member as a module is (`process.cwd`); **an act that more than one name does is one entry, a group:**
`clock`, `random`, `terminal` and `argv`, which the approval prints with the names the file uses under it now. The
environment check's `process.env` (and `import.meta.env`) is left to it, never reported twice, and not approved.

- **An allowlist of the globals that are pure; every other global is I/O.** It starts narrow: a name added later is a
  patch, a name taken away is a major change. The guide of a TypeScript project prints it in full. Pure are `Object`,
  `Array`, `Map`, `Set`, `WeakMap`, `WeakSet`, `WeakRef`, `FinalizationRegistry`, `Promise`, `JSON`, `Math` (but
  `Math.random` is randomness), `Number`, `String`, `Boolean`, `Symbol`, `BigInt`, `Error` and its subclasses (`EvalError`,
  `RangeError`, `ReferenceError`, `SyntaxError`, `TypeError`, `URIError`), `AggregateError`, `RegExp`, `Date` (but see the
  clock), `Intl`, `Reflect`, `Proxy`, `ArrayBuffer`, `SharedArrayBuffer`, `DataView`, the typed arrays (`Int8Array`,
  `Uint8Array`, `Uint8ClampedArray`, `Int16Array`, `Uint16Array`, `Int32Array`, `Uint32Array`, `Float16Array`,
  `Float32Array`, `Float64Array`, `BigInt64Array`, `BigUint64Array`), `Atomics`, `Iterator`, `parseInt`, `parseFloat`,
  `isNaN`, `isFinite`, `encodeURI`, `encodeURIComponent`, `decodeURI`, `decodeURIComponent`, `undefined`, `NaN`,
  `Infinity`, `globalThis` alone, `structuredClone`, `queueMicrotask`, the timers `setTimeout`, `clearTimeout`,
  `setInterval` and `clearInterval` (waiting reads nothing from outside), `AbortController`, `AbortSignal`, `URL`,
  `URLSearchParams`, `TextEncoder`, `TextDecoder`, `Blob`, `atob`, `btoa`, `DOMException`, `EventTarget`, `Event`,
  `CustomEvent`, `Headers`, `Request`, `Response` (constructing them is pure; `fetch` is not) and `FormData`. `Function`
  is not on it: it is unreadable code. Everything else global is I/O, an entry of its own: `fetch`, `XMLHttpRequest`,
  `WebSocket`, `EventSource`, `document`, `window` and its members, `localStorage`, `sessionStorage`, `indexedDB`,
  `navigator`, `location`, `history`, `screen`, `alert`, `confirm`, `prompt`, `matchMedia`, `requestAnimationFrame`,
  `crypto` (but see randomness), `Worker`, `BroadcastChannel`, `Notification`, the observers, `Buffer`, `require`,
  `module`, `Temporal` (but `Temporal.Now` is the clock) and the rest.
- **`process`** is judged as a module is: `process.env` is the environment check's; `process.argv` is `argv`,
  `process.hrtime` is the clock, `process.stdin`, `process.stdout` and `process.stderr` are the terminal, and every
  other name (`process.cwd`, `process.platform`, `process.exit`, `process.uptime`) is I/O, an entry of its own
  (`process.cwd`). `process` used as a value (`const p = process`, `check(process)`) fails, as what is done through it
  cannot be read; `typeof process` does not.
- **The clock** (`clock`): `Date.now`, `new Date()` with no argument (and `new Date(...args)`, which may be none),
  `Date()` called (it is the time now as text whatever it is given), all of `performance`, `process.hrtime` and
  `Temporal.Now.*`. `new Date(x)`, `Date.parse` and `Date.UTC` are pure. Clocks of packages (`dayjs()`) are in no table,
  and are not seen.
- **Randomness** (`random`): `Math.random`, `crypto.randomUUID` and `crypto.getRandomValues`. The rest of `crypto`
  (`crypto.subtle`) is I/O under the entry `crypto`.
- **The terminal** (`terminal`): all of `console`, and the streams of `process`. **The command-line arguments** (`argv`):
  `process.argv`; the outer layers and the entry file read them.
- **What cannot be read fails.** `eval` and `Function`, called, constructed (`new Function(...)`) or taken as a value, and
  `import()` and `require` of what is no string literal (`Source::unread`); their entries are `eval`, `Function`,
  `import()` and `require`. A reference to `require` (`const r = require`, `require.resolve`) is I/O under the entry
  `require`; `require("m")` with a literal is the table's, by the module.
- **Each finding starts with what to do,** by layer, as Python's does: pass the time, the id or the value in as an
  argument, or take it from a port in `protocol.outbound` that the use case is given (a port may be a callable alias:
  `() => Date`). A screen's model, which imports only `protocol.inbound`, **has no exception:** the state of the browser
  it needs (`matchMedia`, `localStorage`, `document.title`) comes from a hook in `ui.atoms` that its page uses, or through
  a use case. The guide lists what is not a fix: approving it, renaming or respelling it, moving the code to a layer that
  is not judged or to a path in `unchecked` or `moving`, and listing it as allowed.

**What TypeScript does not see:** a method on an object whose type the reader cannot know (`client.fetch()`,
`store.getItem()`, `this.clock.now()`), a global reached through a name assigned at run time (`const g = globalThis;
g.fetch(x)`, `window[name]`), the clocks and ids of packages (`dayjs()`, `uuid()`, `nanoid()`), and a global another file
brings in (`declare global`), which is a global and, not being on the list, fails. A name taken from a module is not
judged by the allowlist (`import { fetch } from "./net"` is the project's own).

#### Rust: what is judged by the name

What the Rust standard library does only in part is judged by the name, after the table, on the names the scanner finds as
it finds the items of a module (see the environment check): by the whole path, through a `use` of the module or of one
above it, through a rename (`use std::time::Instant as I;`, `use std as s;`), and from `::std`. **A reference counts as a
call** (`let now = Instant::now;`). Only the code outside `#[cfg(test)]` is judged, as the table is: the items under it,
the file of a `#[cfg(test)] mod x;`, and what is outside a crate's `src/` (`tests/`, `benches/`, `examples/`,
`build.rs`) are free. The finding is a forbidden import as the table's are, approved by the file and the **entry**: a
name of `std::env` or of `std::path` that is I/O is an entry of its own under the module (`std::env`, which covers every
such name of it in that file, `std::path`), a method of the net by its name (`canonicalize`), `extern` for an `extern`
block, and **an act that more than one name does is one entry, a group:** `clock` and `argv`, which the approval prints
with the names the file uses under it now. The environment check's names are left to it, never reported twice, and the
environment check is not approved.

- **`std::env`: an allowlist of the names that are pure; everything else of it is I/O.** It starts narrow: a name added
  later is a patch, a name taken away is a major change. The guide of a Rust project prints it in full. Pure are
  `consts`, `split_paths`, `join_paths`, `VarError`, `JoinPathsError` and `SplitPaths`, and nothing else. `args` and
  `args_os` are the command-line arguments (`argv`: the outer layers and the entry file read them). The functions of the
  environment (`var`, `var_os`, `vars`, `vars_os`, `set_var`, `remove_var`, `home_dir`, `temp_dir`) are the environment
  check's. Every other name (`current_dir`, `set_current_dir`, `current_exe`, the type `Args`, ...) is I/O, an entry of
  its own by its name under `std::env`; the finding names it. A glob of `std::env` fails, as the names it brings in cannot
  be read. `std::env` is one place: it has no row in the table.
- **The clock** (`clock`): `std::time::SystemTime::now`, `std::time::Instant::now`, and `UNIX_EPOCH.elapsed()`, which is
  the time now as `now` is, written `std::time::UNIX_EPOCH.elapsed()`, `SystemTime::UNIX_EPOCH.elapsed()` or, after a
  `use` or a rename, `UNIX_EPOCH.elapsed()` and `E.elapsed()`. A glob of `std::time` (`use std::time::*;`) fails, as the
  names it brings in (`Instant`, `SystemTime`) cannot be read; it is an entry of its own, `std::time`. Randomness has no
  name in `std`. The clocks of crates (`chrono::Utc::now`, `time::OffsetDateTime::now_utc`) are in no table, and are not
  seen.
- **The method net, with no gate on the file:** a method is judged by its name on any receiver, because the scanner
  cannot know the type of what it is called on. **Always judged:** `canonicalize`, `read_dir`, `read_link`,
  `try_exists`, `symlink_metadata` and `is_symlink`. **Judged only with no argument:** `exists`, `is_file`, `is_dir` and
  `metadata` (with an argument, `.exists(x)` is another method). `std::path::absolute` is I/O, a row of the table under
  `std::path`; so is a method of `std::path::Path` named by its path (`Path::exists`, `.filter(Path::is_file)`), which is
  the standard library's whatever the project defines, an entry by the method's name (`exists`).
- **The exclusion:** a method name is left out of the net when the project itself defines a method (a `fn` in an `impl`
  block or a `trait`) of the **same name and the same number of parameters, not counting `self`**, anywhere in the
  project: any code file of it, a test, another layer, a file in `unchecked`. The names left out this way are printed on
  every run of `rotproof check`, in the one line that Python's are, so that the exclusion is not silent (`I/O net: left
  out, as the project defines a method of the same name and number of parameters: exists(0), read_dir(1)`). What the
  net leaves out only for the calls of that many arguments: `read_dir()` is still judged where the project defines
  `read_dir(path)`.
- **What cannot be read fails:** an `extern` block (`extern "C" { ... }`, `unsafe extern "C" { ... }`), which declares
  what another language defines; its entry is `extern`. `extern crate` and an `extern "C" fn` with a body are no block.
- **Each finding starts with what to do,** by layer, as Python's and TypeScript's do: pass the time now in as an
  argument, or take it from a port in `protocol.outbound` that the use case is given; **a port may be a callable alias**
  (`Box<dyn Fn() -> SystemTime>`, or a generic `F: Fn() -> SystemTime`). The arguments are read where the program
  starts. A screen's model, which imports only `protocol.inbound`, gets the value from its page or through a use case.
  The guide lists what is not a fix: approving it, renaming or respelling it, moving the code to a layer that is not
  judged or to a path in `unchecked` or `moving`, and listing it as allowed.

**What Rust does not see:** a method on a value whose type the scanner cannot know, where no definition in the project
leaves it out (`store.exists()` is judged, and left out only by a definition), `instant.elapsed()` on an `Instant` held
in a variable (the `Instant::now` that made it is seen), a name bound in one file and used through another
(`pub use std::time::Instant;` in a module, `reexport::Instant::now()` in the next), a path a macro builds from pieces,
a name a glob brings in (a glob of `std::time` fails, but the names it brought in are not followed after it is
approved), a function of the standard library held in a variable and called through it (`let f = std::path::absolute;
f(x)` is seen at `absolute`, but `let g = f; g(x)` only where `f` is), and the clocks of crates. A name a `use` renames
away (`UNIX_EPOCH` in `use std::time::UNIX_EPOCH as E;`) is read as still bound, which is the side that fails.

#### Packages: what is allowed in an inner layer

What a package of PyPI or npm does is unknown to Rotproof, so the same four layers import **no package unless it is
allowed for the layer or level**, in Python and TypeScript. An import of a package is what the check of the
dependencies calls one (see "What `rotproof check` checks"): not another unit, not the standard library or a builtin
of Node, and not a module of the project, whether the unit has a manifest or not, and `import type` and an import under
`if TYPE_CHECKING:` count as any. The package is the distribution the import is of (Python, through `imports` under
`[dependencies]`) or the name before the subpath (`zod/v4` is `zod`, `@shop/kernel/utils` is `@shop/kernel`).

- **Where it is allowed** is the list of `allowed` under `[dependencies]` of the declaration and of the unit (see "The
  layers", where the keys and what makes the declaration fail are written), added to what the layout of the stack
  allows: TypeScript allows `react` in `presentation.models` (a model keeps its state with React's hooks) and not
  `react-dom`, which draws; Python's layout allows nothing. A layer's key covers its levels, and a level's key adds to
  that level, so `presentation = ["zod"]` allows it in `presentation.display` too. The code of the same four layers and
  levels is judged as for I/O: not `infrastructure`, `composition` or `ui`, not the entry file, and not a test.
- **The finding** is a forbidden import of its own, kept as the I/O of the layers is, under the same heading, **one for
  each package in a file, with every line that imports it** (`, on lines 3, 9` after the sentence when it is imported
  on more than one; the key is the file and the package, as for the other entries of the heading), listed after the I/O
  findings of the same file: `domain/model.py:3: move the code that needs the
  package to infrastructure, behind a port the use case is given, or, when the package does no I/O, ask the person
  whether to allow it for domain (allowed under [dependencies] in .config/rotproof.toml; an agent never adds it to pass
  the check): imports requests.get, of the package requests, which domain may not import`. It is approved by its file
  and the package (`rotproof approve domain/model.py requests --reason "..."`), and one approval covers every import of
  that package in the file; the names it prints on every run (`; now used: requests.get`) are the imports the file
  makes of it. A package named as one of Node's modules (`fs` listed in `package.json`) is two findings under one
  key, the table's and this one, and one approval covers both.
- **Allowing is a person's decision.** The guide says, as for `todo_link` and `approved_max`, that an agent never adds,
  changes or takes out a name to pass the check; `rotproof init` and `rotproof create` never write `allowed`, and
  nothing drafts it from the code. A finding is fixed by moving the code that needs the package to `infrastructure`,
  behind a port, or, when the package does no I/O, by asking the person whether to allow it for the layer.
- **The list keeps nothing that is gone:** a name that no manifest of the project lists, or that no file of the layer or
  level it is allowed for imports, fails, and so do the other forms of the declaration that cannot be kept (see "The
  layers"). A `@types/*` package is no exception: a package the code names is a package.
- **Rust has no list.** Cargo stops a crate that names a crate its `Cargo.toml` does not list, and the dependencies
  check fails a dependency that nothing names, so the `[dependencies]` of the crate of an inner layer is that layer's
  list, and a second one would hold one fact twice. `allowed` in a Rust project fails, in the project's table and in a
  unit's, and says so; the guide says that adding a dependency to the crate of an inner layer is a person's decision.

**What the rule does not see:** a package that is loaded by a name built at run time (the reader of Python and of
TypeScript fails it already, as what cannot be read), one a file takes through another file of the same layer that
re-exports it (the file that imports it is judged, in its own layer), and the packages a Python file reaches through the
module of the project it imports, as that module is judged for itself.

### Views that are templates

In a Python project with screens (`ui_design = "atomic"`), a view may be a template the server renders (Jinja, Django
templates), beside views written in Python or in place of them. No value of `ui_design` says so: whether a view is a
template is how it is written, not a shape of the screens, and which templates are read, and how, is the Python
layout's (`layers/python.toml`). The table above is the same for it.

- **A view is a template file in a level of `ui`**, which follows atomic design as for `ui_design = "atomic"` (a level refers
  to the levels below it, and to its own). A file of the extensions `.html`, `.htm`, `.jinja`, `.jinja2` and `.j2` (in
  any case) is code for the structure check, as a `.py` file is: one outside the levels of `ui`, or beside them in
  `ui/`, fails as code outside the layers, so a file that is no template (a generated page, a report) is listed in
  `unchecked`. A file of another extension (`.xml`, `.txt`) is not read, as a file that is not code is not. Templates
  hold no comment the marker check reads: it reads `.py` files alone.
- **A route function renders a template, and the table does not change.** The function, in `infrastructure.inbound`,
  names the template with a string (`render_template("pages/home.html", ...)`), which is no import, so it imports
  only `protocol.inbound`, as every entry point does, and `composition` registers it. A template draws the data the
  route passes it, and which template a route names is not judged. A template holds no state of its own, and cannot
  import `presentation`: a project whose screens are templates alone may declare `presentation` absent, and keeps it
  to hold the state of a screen in a model that a `.py` file of `ui.pages` uses.
- **A reference between templates is judged as an import is.** The tags `{% extends %}`, `{% include %}`,
  `{% import %}` and `{% from %}` name another template with a string, and the string is a path from the directory
  `ui/` of the unit, which is the directory the project's template loader is set to (Flask's `template_folder`, the
  `directory` of `Jinja2Templates`, Django's `DIRS`): `{% extends "templates/base.html" %}` names
  `ui/templates/base.html`. The reference lands in the level that holds that file, and a level may refer to what it
  may import: itself and the levels below it, so `atoms` refer to `atoms` alone, and no template refers to a level
  above its own. The finding names the file and the line as an import's does (`ui/atoms/a.html:2: includes
  pages/x.html, in ui.pages; ...`), and an approval keys it by the file and the name as written
  (`rotproof approve ui/atoms/a.html pages/x.html --reason ...`). A reference whose name is not a literal (a variable,
  a concatenation, a list, a condition), that climbs out of the directory (`..`) or starts with `/`, or that names no
  file under `ui/` (a template of an installed package, as `admin/base.html`, or a template of a directory the
  loader also looks in) is not seen: Rotproof does not know the loader, so a name no file answers to is no finding.
  What is inside `{# #}`, `{% raw %}`, `{% verbatim %}` and `{% comment %}` is no tag. Not read: a template whose
  delimiters were changed from `{% %}`, Jinja's line statements, a reference by a template's other name, the
  `{% load %}` of Django and what a template imports in Python.
- **The syntax is the languages' own, so `rotproof check` says which it assumed, on every run that reads a
  template.** The layout names the template languages and the major versions the references are read as (now Jinja2
  3, which has all four tags, and Django 5 and 6, which have `extends` and `include`), and `rotproof check` prints, on
  every run for a project whose `ui` holds a template, after the approvals and the comments tied to an issue and before the warnings,
  `templates: references are read as the template languages Jinja2 3 and Django 5/6`, which fails nothing. A template
  does not say which language it is written in, so the tags are the union of those of the languages read. **When a unit's `pyproject.toml` asks for another version, the references are not read
  as that language**, and the check prints a line for the unit saying so and why: `templates: references are not
  read as Jinja2 3: pyproject.toml asks for jinja2>=4, which is not Jinja2 3` (the `import` and `from` tags are then
  no tag), and `templates: references are not judged: ...` when it asks for another version of every language. The
  unit's `pyproject.toml` is the manifest of the unit (in its `root`, or at the root of the project): the requirements
  of `project.dependencies`, `project.optional-dependencies` and `dependency-groups`, as PEP 508 writes them (a name,
  extras, a range of versions, a marker). A package it does not name is read, since a framework brings Jinja2 or Django
  in; a name with no upper bound (`jinja2`, `jinja2>=3`) is read, and so is one whose range holds a version that begins
  with a major version the layout names, in any requirement of the manifest (`jinja2>=3` anywhere is enough). A range
  that holds none (`jinja2>=4`, `jinja2<3`) leaves the language out. A requirement that cannot be read as a version
  (`jinja2===3.1`, `django @ https://...`), and a manifest that is no TOML, leave the language out and say so. The
  lock file and the settings of the loader are not read: no tool's own format is. Changing a requirement so that the
  check stops judging is no fix, and the guide says so.
- **Each unit is read as itself:** a project that declares units has a `ui` in each unit that holds one, its loader's
  directory is that unit's `ui/`, and the manifest is that unit's. A reference is looked up under its own unit's `ui/`
  alone, as the loader of one unit does not look in another's, so a template does not refer to another unit's.

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
made as the one unit is, in the directory of its `path`. A Python or TypeScript unit also gets its manifest at its
`root` when there is none there (from 2.0.0-rc.2): a `pyproject.toml` (`[project]` with the unit's name, `0.1.0`,
`requires-python = ">=3.10"` and no dependencies) or a `package.json` (the unit's name, `0.1.0`, `"private": true`),
written once for a root two units share, named for the first. The check of a locked project fails a Python unit
without one, and the dependencies check does not compare a TypeScript unit's imports with any manifest until it has
one, so a new unit would otherwise fail its first check, or have its imports go unchecked; a manifest that is there is
the project's and never written over, and an upgrade, which makes no layer, writes none. A Rust unit's layers are crates, each named for its
unit (`billing_domain` for the layer `domain` of the unit `billing`, against `domain` in a project with no unit), since
Cargo refuses two crates of one name; the unit's directory holds the crates and has no manifest of its own. The root
`Cargo.toml` it writes then is a workspace with no package, naming each crate in `members` (a glob such as
`crates/*` would match the directory of a unit and fail in Cargo), and `src/main.rs` is not written. The entry file
of a unit is `<dir>/src/main.rs` in Rust, and `rotproof create` makes with it the package that holds it, `<dir>/Cargo.toml`
(named for the unit, and for the unit and its directory from its second entry on), which lists no dependency, since the
dependencies check fails one the code does not name, and which `members` names; an entry file declared in another form, that is not there, fails the
run before anything is written, and one that is there is the project's, with its package. When the root `Cargo.toml` is
there already, `rotproof create` changes `members` and nothing else of it, only to add the crates and packages it made
that no member names, and to replace the member `crates/*`, the glob Rotproof wrote before units, with the crates it
matched once it would match a directory that has no `Cargo.toml`; it says each change (`changed Cargo.toml: ...`). A
project made by an older Rotproof that declares no unit keeps its glob, and `rotproof create` leaves its `Cargo.toml`
as it is.

A declaration that cannot be read as it is written fails every check, saying every reason at once, from the
declaration and the layout alone, before the tree is looked at:

- a name that is not `[a-z][a-z0-9_]*`, or two units with the same name;
- a `path` or `root` that is not written from the root with `/` as `unchecked` and `moving` are (`./a`, `/a`, `a/../b`, `C:/a`, a
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
  files, or sits in a path the layout says is no layer (`tests/` in Python), or in a path `unchecked` or `moving` keeps, or in the
  `path` of a unit inside it, which is that unit's to judge. A Python unit whose `path` is not the root may hold an
  `__init__.py` directly in it, read as a place that imports no layer; no other file, and none in a TypeScript or a
  Rust unit (`index.ts` at the top of a unit is code outside its layers). What the layout says of where code goes
  (a screen goes in `src/ui/pages`) is left out of a finding about a unit, whose directories are its own.
- Code in no unit's path fails too, as code outside the layers (`code outside the layers: helper.py (it is in no
  unit: ...)`), so that moving a file out of every unit does not escape the checks: the layout's scope is walked as in
  a project of one unit (Python: the whole project, TypeScript: `src/`, Rust: `crates/` and the `src/` of the root
  package), and a code file there that is in no unit's `path`, is no unit's entry file, is not in a path that is no
  layer (`tests/` in the scope, and in the root of each unit, where a package keeps its own) and is not kept by
  `unchecked` or `moving` fails. A unit at the root of the project holds every path. A unit whose `path` is outside the scope (a
  monorepo's `packages/billing/src/billing` in TypeScript) is walked all the same.
- A path in `unchecked` or `moving` neither holds nor sits in a layer, and does not hold a unit: `unchecked = ["packages/billing"]`
  would switch the unit off. A path inside a unit's `path` and outside its layers is what `unchecked` is for.
- A manifest (`pyproject.toml` in Python, `package.json` in TypeScript, `Cargo.toml` in Rust, in any case: the one
  kind of the stack) belongs to the unit whose `root` holds its directory, and a manifest elsewhere is a package no
  unit says it is: `apps/x/package.json is a manifest outside every unit's root: declare the unit whose root is apps/x
  (root = "apps/x"), or list apps/x in unchecked`. The manifest at the root of the project (a workspace's), one in a
  layer (a Rust layer is a crate of its own), and one in a path `unchecked` or `moving` keeps are not counted. A project that
  declares no unit is not asked, so a project that is one package with others of tooling in it is read as before.

A knowledge document that records why the code is divided so follows the table `.config/rotproof.toml::unit` (see
The records and "The document of the units"), and is read again when a unit changes, not when `areas` or `files` does.

### What a unit may import

A unit imports the layers of its own unit as the table says, without those it declares absent. Only direct imports
are judged, as everywhere. Between
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

`rotproof new <slug>` starts a document: it writes `docs/knowledge/<slug>.md` with this frontmatter, the part that is
easy to write wrongly, and no body. `type`, a `title` made from the slug (its words, the first one capitalised, for a
person to change), `tags` with the area, and `status: stable` are written, and `description` is left empty, so
`rotproof check` fails (`description: empty`) until a person writes it, and a document half done cannot be committed.
`--area` names the area, one the declaration lists, and may be left out when it lists only one. It writes no
`follows`: a pin says the document was reviewed against the code, and a document with no body has nothing to review,
so `rotproof follows` pins the keys once the body says what the code does (from 2.0.0-rc.2; `--follows <key>`, which
pinned them as the document was started, is gone, and a command line that gives it exits 2). `--alarm` writes what
an alarm needs, `# When` and `# Do`, empty so that the check fails the same way until each is written, and
`--match <string>`, given any number of times with `--alarm`, the strings of `match` (trimmed, each on one line and
named once; with none, the
field is left out). Nothing else is written into the body: a heading left empty would be a record gone stale from its
first day, so the questions to ask are printed instead (why it is so, what was considered and dropped, how it should
behave, what one must know that lies outside the code), and the answers are the person's or the task's, not what the
code says. The slug is lower-case words and digits joined by single hyphens, never `index`, `log` or `rules`, and it
never changes once the file exists. A document that is there already is never written over, whatever the case of its
name or the dots and spaces at its end (as Windows reads names). The index is not written, as the document is not
read until its description is: run `rotproof index` then. It exits 2, writing nothing, when the slug, the area or a
string of `match` is not as above, a document of that name is there already, or `docs/knowledge/` is not.

The log holds one entry per task, under the day's `## YYYY-MM-DD` heading, newest first: the task, its issue, what went
wrong and what was learned, which is found nowhere else, and searchable in the repository when something breaks. What
changed is in the commits, what was measured and decided along the way in the task's issue, and what still holds in
`knowledge/`.

`rotproof log` adds the entry, so that nobody writes a day from memory or reads the whole log to place one. It reads
today from the clock of the machine (the day there, in its own zone: the check compares the days of the log with the
latest day anywhere, so a day written in any zone passes), writes the entry at the top under the `## YYYY-MM-DD`
heading of that day, and makes the heading when there is none, above the first heading of an older day, so the days
stay newest first. The entry has the form of the guide, `* **<task>**` with `**Issue**`, `**Problems**` and
`**Lessons**` as nested items, the issue left out when there is none:

```sh
rotproof log --title <task> [--issue <owner/repo#n>] --problem <text>... --lesson <text>...
```

`--title` is the task. `--issue` is `owner/repo#n`. `--problem` and `--lesson` are each needed at least once, so that
a field forgotten is told from one with nothing in it: `--problem None` says that nothing went wrong. A field given
more than once is a list under its label, and given once it stays on the label's line. A line break in a value
becomes a space, so a value never opens a line of its own, and a value that is empty or opens an HTML comment (which
would hide the rest of the log) is refused. The log keeps its line endings and its byte order mark, and the rest of
its text is as it was. It exits 2, writing nothing, when a value is not as above, `docs/log.md` is not there, or the
entry would break a rule the log kept: a heading of a day inside a comment or a code block, where no place for the
entry can be told, is one.

## What `rotproof check` checks

- **The tree matches `.config/rotproof.toml`, either way:** the declaration names a known stack and a UI it offers,
  and only layers that stack and UI have in `absent` (a misspelled field fails; a layer the UI leaves out, as `ui`
  with `ui_design = "none"`, fails until it is taken out, and so does a project with screens whose top `absent` lists
  both `ui` and `presentation`, which is `ui_design = "none"` written another way). Without a declaration nothing is checked, and `rotproof check`
  exits 2, not 1: CI tells a project that has not adopted Rotproof from one that breaks a rule. Every layer of the stack is present or declared
  absent, and no layer declared absent is present. No code sits outside the layers, the stack's own paths (`tests/`),
  the entry file, `unchecked` and `moving`. In Rust, the root package's `src/` holds the entry file and nothing else: a `.rs`
  file there (`src/pricing.rs`, `src/lib.rs`, `src/bin/`, a module of `main.rs`) fails as code outside the layers,
  and the finding says where code goes, while what Cargo keeps outside `src/` in the root package (`build.rs`,
  `tests/`, `benches/`, `examples/`) and `xtask/` are not looked at. A path in `unchecked` or `moving` exists and neither holds nor sits in a layer, so a layer cannot be switched
  off by listing it. At least one layer is present: with every layer declared absent, nothing would be checked.
  A project that declares units (see "Units") is checked unit by unit: each unit's `path` and entry files are there, and
  each of its layers is present or in its own `absent`, with at least one present; its code outside the layers is
  looked for in its `path` up to the units inside it, and in the scope of the stack outside every unit's `path`; and
  every manifest of the stack outside the root of every unit fails.
  A layer with levels holds only its levels: code beside them fails, apart from the layer's own file
  (`ui/__init__.py`). What the whole UI shares goes in a level: a part that knows no project concept, visible or not
  (a design value, one behaviour, a provider of a theme), is an atom, and what a screen holds and decides goes in
  `presentation.models`. A directory of a layer of the table before 0.4.0 (`handler`, `utils`) is named with where
  its code goes now. In Python with `ui_design = "atomic"`, a template (`.html`, `.htm`, `.jinja`, `.jinja2`, `.j2`) is code
  too, so one outside the levels of `ui` fails as code outside the layers (see "Views that are templates"). In
  TypeScript, a `.tsx` or `.jsx` file in `presentation` fails, by its extension alone (case
  does not count): a view written with `createElement` in a `.ts` file passes, and so does a test, wherever it sits
  (`useForm.test.tsx`, `__tests__/wrapper.tsx`: see "Tests"), and one outside `src/`, which Rotproof does not read.
  A test is no code outside the layers and none beside the levels of a layer, in TypeScript as the files of `tests/`
  are in Python.
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
- **No code that is not a test imports a test** (Python): a layer, the entry file and a unit's `__init__.py` import
  no module of `tests/` (at the root of the project, in the directory of a unit, and at its root), whatever the place
  they would be in: `imports tests.helper, in tests/, which holds tests; code that is not a test imports no test: move
  what it needs out of the test into a layer`. It is a forbidden import, approved as any other is by its file and its
  import (`rotproof approve domain/model.py tests.helper --reason ...`). The files of `tests/` are read by no check of
  the direction, so a test imports any layer, and another test. A project with no `tests/` directory has none to
  import: `import tests` names a package of someone else's there.
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
  whose `exports` point to build output such as `./dist/client.js`) is not judged. The imports of a test (a file named
  `*.test.*` or `*.spec.*`, or in `__tests__/`: see "Tests") are not read: it is no code of a layer.
- **No code that is not a test imports a test** (TypeScript): a file of a layer or the entry file whose import lands on
  a test fails: `imports ../domain/song.test (src/domain/song.test), a test (*.test.*, *.spec.*, __tests__/); code that
  is not a test imports no test: move what it needs out of the test into a layer`. The import lands on a test when
  the path it resolves to (through the aliases and the packages of the units, as above) is in a directory named
  `__tests__`, or is a file named as a test, or is written without its extension and a file named as a test is there
  for it (`./song.test` for `song.test.ts`, `./__tests__` for the `index` in it). It is a forbidden import,
  approved as any other is by its file and its import; a test importing a test, or code, passes. The import is judged
  by this rule alone: a rule of the table that it would also break is not said, since a test is in no layer.
- **The templates refer only to what the table allows** (Python, `ui_design = "atomic"`): every `{% extends %}`,
  `{% include %}`, `{% import %}` and `{% from %}` of the tags the template languages read have, with a literal name,
  in the templates of the levels of `ui`, judged by the level of the file it names under `ui/` as an import is. Which
  languages and versions the tags are read as is printed on every run, and a unit whose `pyproject.toml` asks for
  another version of a language is not read as it, or not judged at all, which is printed too (see "Views that are
  templates", which also says what is not seen). A forbidden reference is approved as a forbidden import is.
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
- **The manifests list the dependencies the code imports, both ways** (Python): the manifest of a package says in the
  tree what its code depends on, and is compared with it as a layer is with the table, from the tree and the declaration
  alone. The manifest is the `pyproject.toml` at the root of each unit, and at the root of the project when it declares
  none; the code is what the direction check reads (the layers, the entry file, a unit's `__init__.py`) and the tests,
  the code files in `tests/` of the unit's directory or its root, which are no layer. A path in `unchecked` or `moving` is not
  read. A unit with no `pyproject.toml` at its root is not compared with one (the next rule fails it), and a `pyproject.toml` that cannot be read as
  UTF-8 or TOML fails. What is listed is read as the tools that write the file say: `[project] dependencies` and the
  keys of `[tool.poetry.dependencies]` are the runtime dependencies (each a PEP 508 string, whose distribution is cut
  out of its extras, version and marker), each list of `[project.optional-dependencies]` is optional, and each group of
  `[dependency-groups]`, `[tool.uv] dev-dependencies`, `[tool.pdm.dev-dependencies]`, and Poetry's dev tables is for
  development. Names compare as PEP 503 says (case and runs of `-`, `_` and `.` count for nothing). Then, in each
  manifest:
  1. **an import of a package the manifest does not list fails**, once for the package, at its first import, with how
     many more there are; the extras a dependency brings are no dependency of the package, so importing what one brings
     fails as well. A package is the distribution an import is of: the entry of `imports` under `[dependencies]` of the
     declaration that covers it, or else the one named like its first part, and the message says to map it when it may
     be another. An import of another unit needs that unit's package, by the `name` its own `pyproject.toml` gives, or
     by the name of the unit when it has none; an import of a module of the unit itself, of the standard library, or of
     the project outside every layer needs nothing. The standard library is the union of Python 3.10 to 3.14; a module
     of the project outside the layers is a directory or a `.py` file of its name in a directory above the importing
     file, in the directory that holds a unit, or in the root and the path of a unit;
  2. **the code of a layer uses runtime and optional dependencies only**: an import of one a group of
     `[dependency-groups]` alone lists fails, and says to move it, or to import it in a test; a test uses all of them;
  3. **a runtime dependency that no code imports fails**, and one that only tests import fails too, and says to move it
     to a development group. An optional or development dependency is never judged unused: `ruff` and `mypy` are
     imported by nothing. A dependency the code uses without importing it is listed in `without_import`, which
     counts as an import; the package of the manifest itself is no dependency of it.

  When the project has a `.venv` (at the root of the manifest, or of the project), the message of a missing dependency
  names the distribution the environment provides the module as, from the metadata of each `*.dist-info` (the top-level
  names its `RECORD` installs), and the lines to add. This only words the message: the environment is ignored by git and
  differs from one machine to another, so the verdict never reads it.
- **A Python project is locked, and keeps one virtual environment** (Python): whether a machine runs Python in a virtual
  environment is not in the tree (`.venv` is ignored, and CI starts from nothing), so the rule asks for what the tree
  shows. A package that happens to be installed globally lets an import pass that no manifest lists, and nobody sees it
  until a clean machine runs the code; the lock file makes every machine install the same packages.
  1. **Each unit has a `pyproject.toml` at its root** (the root of the project when it declares no unit), which lists its
     dependencies: `requirements*.txt` files are no declaration, and a project that has only those fails as one that
     has nothing;
  2. **a lock file is committed at that root or in a directory above it**: `uv.lock`, `poetry.lock`, `pdm.lock`,
     `pylock.toml` or `pylock.<name>.toml` (PEP 751), any of them, compared exactly by name. The one in a directory
     above is a workspace's, which is one for all its members (a uv workspace has one at its root). It is in the tree
     the walk reads, so a lock file the project's `.gitignore` excludes is not committed, and fails as a missing
     one. A project that declares units is judged by the root of each of them. A project that uses only the standard
     library has a lock file too: Rotproof is a development dependency of every project, pinned in the `dev` group;
  3. **a virtual environment is `.venv` beside a lock file, and ignored**. A directory that holds a `pyvenv.cfg` is a
     virtual environment, whatever it is called, so a renamed one is found and a directory called `env` or a file
     `.env` is not. The directories looked at are the direct children of each directory the walk reads (the root
     included), the ignored and hidden children too, and never what is below a child the walk leaves out, so
     `node_modules/` and a `.tox/py312` of tox, which are ignored, are not entered, and an environment at any depth of
     the project's own directories is found. One named otherwise than `.venv` fails (`venv/ is a virtual
     environment ...: delete venv/ and use .venv`), the name uv, PDM and Poetry (set to keep it in the project) give by
     default, so that a tool run as it is makes no second one. A `.venv` fails when no lock file is in its own directory
     (not only above it): with one lock file at the root of a workspace, the one `.venv` is the root's, and a
     `packages/api/.venv` is a second environment. A `.venv` that the `.gitignore` files of the project do not exclude
     fails, as it can be committed; one whose own `.gitignore` holds `*`, as uv writes it, is excluded as git reads it.
     A path in `unchecked` or `moving` is not looked at.

  The search for environments is the one place a check reads what the project's `.gitignore` hides, and the one place
  its answer comes from the machine and not from the commit: CI, which has no environment of its own before it installs
  one, finds the `.venv` of the lock beside it and passes, and a machine with a `venv/` fails where the harm is done (a
  package installed in only one of two environments hides a dependency nobody listed). It is the one exception to
  "the same files, the same answer", made on purpose; the other `.gitignore` files that differ between machines (a
  global excludes file, `.git/info/exclude`) are still never read. The findings are listed in the byte order of their
  paths, and only for a project of the `python` stack.
- **The manifests list the dependencies the code imports, both ways** (TypeScript): the manifest is the `package.json`
  at the root of each unit, and at the root of the project when it declares none; the code is the source files the
  direction check reads (the layers and the entry files in `src/`) and the tests (see "Tests": the files named
  `*.test.*` or `*.spec.*`, or in `__tests__/`, wherever they sit in the `path` of the unit, in no layer too, outside
  `unchecked` and `moving`), with the imports it collects (`import`, `export ...
  from`, `import(...)` and `require(...)` with a literal string). A unit with no `package.json` at its root is not compared
  with one, and one that cannot be read as UTF-8 or JSON fails. The runtime dependencies are `dependencies`; the
  optional ones `peerDependencies` and `optionalDependencies`; the development ones `devDependencies`. An import is
  placed as the direction check places it: a relative path, one from the root, an alias of `paths` and a module under
  a `baseUrl` are files of the project and need nothing; a specifier that starts with the `name` of another unit's
  `package.json` (`@shop/kernel/domain`, wherever it lands, or if its subpath is not exported) needs that package,
  which the importing unit's manifest lists by that name (`"@shop/kernel": "workspace:*"`); `node:` names, a path, a
  subpath import (`#name`) and a scheme (`npm:`, `virtual:`) need nothing; any other specifier needs the package it
  names, without its subpath (`lodash/fp` is `lodash`, `@shop/kernel/utils` is `@shop/kernel`). A name Node reads as
  a builtin without `node:` (`fs`, `path/posix`, `buffer`) is Node's, unless the manifest lists a package of that
  name, which then is used. Then, in each manifest:
  1. **an import of a package the manifest does not list fails**, once for the package with how many more imports there
     are of it; any kind of dependency lists it. A unit below the root of the project may also use the
     `devDependencies` of the project's own `package.json`, which npm hoists to every package of the workspace;
  2. **the code of a layer uses runtime and optional dependencies only**, as in Python: an import of a package that
     only `devDependencies` lists (the unit's own, or the root's hoisted) by a file that is not a test fails, once for
     the package with how many more imports there are, and says to move it to `dependencies` (to list it there, for the
     root's), or to import it in a test alone (`imports vitest, which package.json:8 lists for development only: the
     code of a layer uses runtime dependencies only, so move vitest to dependencies, or import it in a test alone (a
     file named *.test.* or *.spec.*, or in __tests__/)`). A test uses all of them. The types of `@types/*`
     may be named anywhere, since they are erased when the code is built;
  3. **a package in `dependencies` that no source file imports fails**, unless `without_import` lists it, and one that
     only tests import fails too, and says to move it to `devDependencies`. The development, peer and optional
     dependencies are never judged unused, nor is `@types/*`, which declares types and is imported by nothing.

  `imports` of the declaration is not used: an import and a dependency are one name. Not read: a file outside `src/`
  (`vite.config.ts`), so a package only that file imports is listed in `devDependencies`, or in `without_import`.
- **The manifests list the dependencies the code imports, both ways** (Rust): Cargo stops a crate that names a crate its
  `Cargo.toml` does not list, so the other half is judged: a dependency in `[dependencies]` (`[target.<cfg>.dependencies]`
  too, from a registry, git or a path alike) that no code of the crate names fails. The crates are those the direction
  check reads, the crate of each layer and the package of the entry files of each unit, and the code is the `.rs` files in
  the crate's `src/`, `tests/`, `benches/` and `examples/`, where Cargo lets a crate use `[dependencies]` (not
  `build.rs`). A dependency is named by a path that starts with it (`serde::Serialize`, in a `use`, an expression, an
  attribute or the tokens of a macro, `serde_json::json!` too), by a name bound to it with `as`, by `extern crate`, and by
  a `use` of the crate alone, in code with the comments and the literals taken out; `-` and `_` are one name, and a
  dependency renamed with `package = ...` is named by the key it is listed under. `[dev-dependencies]` and
  `[build-dependencies]` (the latter are used by a script that is not read) are never judged, and neither are the entries
  of `[workspace.dependencies]`, which are no crate's dependency until one takes them with `workspace = true`. A
  dependency the code uses without naming it (a `-sys` crate that is only linked) is listed in `without_import`, as the
  manifest lists it (`openssl-sys` or `openssl_sys`); `imports` of the declaration is not used. This reads the `use` of
  the crates of every layer, which the direction check does only for a crate that depends on a crate of a shared unit. A
  manifest that cannot be read is the direction check's finding. Not read: a `[lib] path` or `[[bin]] path` that puts a
  crate's code outside those directories, a file taken in with `include!` or `#[path]`, and a name a macro builds from
  pieces.
- **Only `infrastructure` reads or writes the environment variables:** the environment is input from outside, as
  unchecked as a request, so `infrastructure` reads it into a settings type it validates, and `composition` passes
  that on. The code of every other layer and the entry file fails on `os.environ`, `os.environb`, `os.getenv`,
  `os.getenvb`, `os.putenv` and `os.unsetenv` in Python, and on the two functions that expand a variable or the home
  directory, `os.path.expandvars` and `os.path.expanduser` (also as `posixpath`, `ntpath` and `genericpath` name
  them), and on `pathlib.Path.home` (also `PosixPath.home` and `WindowsPath.home`); `process.env` and
  `import.meta.env` in TypeScript; and `std::env::var`, `var_os`, `vars`, `vars_os`, `set_var`, `remove_var`,
  `home_dir` and `temp_dir` in Rust. Each is found through an alias the file binds too (`import os as o`,
  `from os import getenv as g`, `import os.path as p` then `p.expanduser`, `from pathlib import Path` then
  `Path.home()`, `import { env } from "node:process"`, `const { env } = process`, `use std::env;` then `env::var`,
  `use std::env as e;` then `e::var`, `use std as s;` then `s::env::var`, `::std::env::var`), and in Python and
  TypeScript at the import (`from os import getenv`, `import { env } from "node:process"`) and at each line that then
  uses the name. A star import of a module whose names
  count fails too, as the names it brings in cannot be read, so name them: in Python `from os import *`,
  `from os.path import *` (also `posixpath`, `ntpath`, `genericpath`) and `from pathlib import *`, in Rust a glob of
  `std::env` or of `std` (`use std::env::*;`, `use std::*;`, then `use std::env::var;`). Python's names are read as
  Python's scopes read them (module, function and lambda, class body, comprehension; a name is the one of the scope it
  is bound in at all, by assignment, parameter, `def`, `class`, `import`, `for`, `with`, `except ... as`, `match`,
  `:=`, `del`, `global` and `nonlocal`): a parameter or a variable that is called `os` is not the module, and
  `os.getenv` through it passes, as an `import os` in one function does not bind `os` in another; an import under
  `if TYPE_CHECKING:` is an import as any. TypeScript's names are read as the language's scopes read them (a
  parameter, a `var` hoisted to its function, a `let` or `const` of its block, a `catch` binding, a function or class
  name, an import, and a name the file `declare`s): a `process` that a function, a block or the module binds is not the
  global, and `process.env` through it passes, while the `process.env` outside that scope is read. What the global
  object holds is the global, so `window.process.env`, `globalThis.process.env`, `self.process.env` and
  `global.process.env` read as `process.env`, also through parentheses, a cast (`(globalThis as any).process.env`),
  `!`, optional chaining and a literal in brackets (`process["env"]`, `window["process"].env`). `process` imported from
  `process` or `node:process`, by default or as a namespace, is the global, and so is `env` imported from them, which
  is also read at the import. What `require` and `import()` with a literal string bind is read as an import binds it,
  wherever the declaration is: a `const`, `let` or `var` whose initializer is exactly the call, optionally awaited (in
  parentheses or not), cast and followed by `.default`, binds the namespace (`const process = require("process")`,
  `const p = await import("node:process")`, then `p.env`), and a destructuring binds the names it takes, each read at
  its key as a named import is (`const { env } = require("process")`), only when `require` is the global one; a
  `require` or `import()` of what is no literal is not seen. `const { env } = process`, `const { env: e } = globalThis.process` and `const { env: { A }
  } = process` take it apart; `import.meta.env` is read whole (`const all = import.meta.env`), by a member
  (`import.meta.env.VITE_URL`) and by a destructuring that takes a name Vite does not set itself or a rest element. A
  type position (`type T = typeof process.env`) is not a read, and neither is `import type`. Not judged: the command
  line (`sys.argv`, `std::env::args`), `env!`, `option_env!` and a crate's `build.rs` (read when Rust compiles), the
  values of `import.meta.env` Vite sets itself (`DEV`, `PROD`, `MODE`, `BASE_URL`, `SSR`), what is built at run time
  (`getattr(os, "environ")`, `process[key]`, `globalThis["pro" + "cess"]`, `eval`), in Python and TypeScript a name
  bound through an assignment (`o = os`, then `o.environ`; `const p = process`, then `p.env`; `const { process } =
  globalThis`, then `process.env`) and, in TypeScript, a `require` or `import()` that is no declarator's whole
  initializer (`wrap(require("process"))`, `require("process").env.A`, `(await import("process")).env.A`) and a name
  assigned after it is bound (`let p = require("path"); p = process`, which stays the module), a destructuring that is
  an assignment (`({ env } = process)`) or a parameter's
  default (`function f({ env } = process)`), and `export { env } from "node:process"`; a method of an object
  (`Path("~").expanduser()`, which needs the type of what it is called on), a name a star import of another module
  brings in and a name in a string annotation, in Rust a name bound in one file and used through another (`pub use
  std::env;` in one module, `reexport::env::var` in the next) and a path a macro builds from pieces, and the code
  outside the layers (`tests/` in Python) and the tests (in TypeScript, the files named `*.test.*` or `*.spec.*`, or in
  `__tests__/`; in Rust, the items under `#[cfg(test)]`, the file of a `#[cfg(test)] mod x;` and what is outside a
  crate's `src/`, as the check of the I/O reads them: see "Tests").
- **The inner layers do no I/O** (see "The inner layers do no I/O"): the code of `domain`, `protocol`, `application` and
  `presentation`, and of their levels, imports nothing of the table Rotproof keeps of what does I/O wholly (in Python
  `shutil`, `subprocess`, `socket`, `urllib.request`, `tempfile` and the like; in TypeScript Node's `fs`, `net`,
  `child_process`, `http` and the like, with or without `node:`, and any other scheme; in Rust `std::fs`,
  `std::process::Command`, `std::net::TcpStream`, the terminal items of `std::io` and the print macros). The table is
  Rotproof's, and a project cannot change it. `infrastructure`, `composition`, `ui`, the entry file and the tests are
  not judged. It is a forbidden import as the layer table's are: it is approved by its file and the entry of the table
  (`rotproof approve domain/report.py shutil --reason "..."`), the approval is printed on every run with the names the
  file now uses under the entry, and one that matches nothing fails. Each finding says first what to do (move the I/O
  into `infrastructure.outbound`, behind a port in `protocol.outbound` the use case is given; a screen's model gets what
  it needs from its page or through a use case), and approving it, renaming it or moving the code to a path in
  `unchecked` or `moving` is no fix. In Python the rule also judges what a module does in part by the name used (the
  allowlists of `os`, `os.path`, `sys`, `time` and `logging`, the clock, randomness, the terminal, the command-line
  arguments, `pathlib.Path`, the net of method names, and what cannot be read), by entries of its own: a name
  (`os.getcwd`), a group (`clock`, `random`, `terminal`, `argv`) or a method (`read_text`); it prints on every run the
  methods of the project that the net leaves out. In TypeScript it judges the globals by the name used (an allowlist of
  the pure ones, `process`, the clock, randomness, `console`, the arguments and what cannot be read), by entries of its
  own: a global (`fetch`, `localStorage`, `process.cwd`) or a group (`clock`, `random`, `terminal`, `argv`). In Python
  and TypeScript the same four layers import **only the packages they are allowed** (`allowed` under `[dependencies]`,
  and the layout's `react` for `presentation.models` in TypeScript): a package no list allows for the layer or level is
  a finding of its own under the same heading, approved by its file and the package, and a list that keeps a name no
  manifest lists or no file of its place imports fails (see "The inner layers do no I/O", "Packages"). In Rust the
  `[dependencies]` of the crate of the layer is that list, and `allowed` fails. What these layers import of a path in
  `unchecked` or `moving`, which no check judges, is listed on every run, apart from the findings, and fails nothing.
- **Every approval matches a forbidden import** (see "Approving a forbidden import"): a forbidden import with an entry
  in `.config/rotproof-approved.toml` for its file and import passes, and is printed on every run; an entry that
  matches none fails, and so does a file that is not TOML, an entry with a field missing, empty or unknown, an
  `approved.at` that is not a datetime with a time zone, and two entries for one import. A file named otherwise
  (`Rotproof-Approved.toml`) approves nothing, and fails. With `approved_max` in the declaration, **the approvals stay
  within it**: more entries than it lets fails, as one finding that names both files.
- **A baseline, where a project froze one, keeps what it froze and nothing more** (see "Freezing the violations there
  are"): what the approvals leave of the forbidden imports and the uses of I/O of the inner layers is judged by
  `.config/rotproof-baseline.toml`, and a violation with an entry there for its file and import passes, with one line
  printed on every run (how many entries remain); one without fails. An entry that matches none fails, so does a file
  that cannot be read (a `count` missing, an unknown field, an entry with a field empty, two entries for one import,
  a file named otherwise), and more entries than its `count` fails, as one finding that names the file.
- **The project's files are up to this version of Rotproof** (see "Upgrading Rotproof"): `files` in the declaration
  (0.1.0 when it is missing) is a version as SemVer writes it, a pre-release such as `2.0.0-rc.1` included, neither
  older nor newer than the running Rotproof,
  and every name in `declined` is an update or `guide-pointer`.
- **`.rotproof/AGENTS.md` is up to date:** it equals what `rotproof create` writes for the stack with this version of
  Rotproof (see "Upgrading Rotproof"), so it fails after an upgrade until `rotproof init` has run, and when edited by
  hand. The finding names the command that applies: `rotproof create` when `files` names the running version (the
  guide changed under it, as in a development build, or by hand), `rotproof init` otherwise.
- **`.rotproof/running.md` is up to date:** it equals what `rotproof create` writes, and fails when missing or edited,
  with the command that applies as for the guide.
- **The project answered whether its agent reads the guide:** the root's `AGENTS.md` or `CLAUDE.md` names
  `.rotproof/AGENTS.md`, or `declined` names `guide-pointer` (see "Upgrading Rotproof", at the guide). It reads only
  whether the path is in those files, never what else they say.
- **No comment holds `TODO`, `FIXME`, `XXX`, `HACK` or `NOTE`**: in upper case, as whole words, in any code file
  outside `unchecked` and `moving` (a path there that holds or sits in a layer skips nothing), `tests/` included; in TypeScript, the `//` and `/* */` comments of the source files in `src/`, the tests among them,
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
  `href` in HTML, and not inside code or comments. A link to a file of code (`.py`, `.rs`, and TypeScript and
  JavaScript source, in any case) shows a name the check has looked up: its text is one code span, either the name of
  a definition in the file, written as a key of `follows` writes it after `::` (``[`render_index`](../src/bundle.py)``,
  a method with its type ``[`Router.add`](../src/api.ts)``, a Rust type covering its definition and every `impl` block
  for it ``[`Table`](../src/table.rs)``), or the file's own name (``[`bundle.py`](../src/bundle.py)``) or a path that
  ends in it (``[`src/bundle.py`](../src/bundle.py)``): its last parts, whole and spelled exactly, from a directory
  boundary, with none that is empty, `.` or `..` (`rc/bundle.py`, `Bundle.py`, `lib/bundle.py` for a file in `src/`
  and `/src/bundle.py` fail), which tells a reader which file when several share a name (`lib.rs`, `index.ts`). The
  names are those of the outline `follows` reads, so a `def` line inside a string or a docstring does not count, and
  what sits in an `if` or a `try` is not named. A text in prose (`[the router](../src/api.ts)`), a name not in a code
  span, a call (`render_index()`), a method without its type (`add`) and a fragment (`#L12`, whose number goes stale
  without a word) fail; when the text names nothing, the message says which name to write or lists the names the file
  defines. A file that cannot be parsed far enough to read its definitions can be linked by its own name or a path
  ending in it only.
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

What a check finds file by file (the layers' imports, the markers, `NOTE`, the environment variables, the I/O of the inner layers and the
type-checker suppressions; the dependencies by manifest, each in the order of the files that need it) is listed in the byte order of the whole path, written with `/` from the root and compared
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

### The code that is moving

While `moving` lists a path, `rotproof check` prints, on every run and apart from the findings and the warnings,
what is left to move and which files to move next with the layers each could go in, and fails nothing for it. No date
is set: the count falls only when a file is moved, and a path is taken out of `moving` once its code is.

```
moving: 5 files are not in a layer yet, and none is checked. Move these next (they import no other file still moving), each with the layers it could go in:
  legacy/a.py, legacy/b.py import each other, so move them together:
    legacy/a.py: any layer
    legacy/b.py: domain, protocol.outbound, application, infrastructure.outbound, composition
  legacy/money.py: any layer
  and 2 more that import no other file still moving
```

- **The count is the code files in the listed paths:** `.py` files in Python, source files in TypeScript (`.ts`,
  `.tsx`, `.mts`, `.cts`, `.js`, `.jsx`, `.mjs`, `.cjs`), `.rs` files in Rust. Outside `.gitignore` and the hidden
  directories, as everywhere. A project that lists no path hears nothing; one whose listed paths hold no code says to
  take them out.
- **The order is the import graph's, from its leaves.** An import of a file that is moving too (a module or a package
  of Python; in TypeScript, the file an import resolves to as it does for the files in a layer) is a wait: the file
  goes into a layer after what it imports. The files named are those that wait for no file still moving, in the byte
  order of their paths: the first five are named and the rest counted. An import of a file in a layer, of the standard
  library, of a package outside the project or of a file that is not listed is no wait, and an import that cannot be
  resolved is not said: no check reads this code. A file that cannot be read, or has a syntax error, is counted and
  leaves out what it could not give.
- **Files that import each other are one group,** named together: neither can go first, so they are moved together.
  A group that imports a file outside it waits for that file.
- **The layers a file could go in are the places where the direction check would pass it,** innermost first: every
  layer, and every level of a layer that has levels (such a layer holds only its levels), of the unit whose directory
  holds the file, or of every unit when none does. A place passes when each import the file makes of a layer, as an
  import from that place, and each import of the file by a file in a layer or by an entry file, as an import of that
  place, is allowed by the table and by the rules between units. `any layer` says that every place passes, as for a
  file that imports nothing of a layer and that no layer imports; none passing is said as it is. A name says its unit
  when the project declares units (`domain of unit billing`). Only the places that are there are offered, and a file's
  imports of the other files that are moving are not counted: where those go is not known yet. Which of the layers
  that pass is the one is decided by what the code is for, and Rotproof never says it: that a file belongs to `domain`
  or to `application` is for the person who moves it.
- **In Python and TypeScript, a place the check of the I/O of the inner layers judges is left out for a file that
  would fail it there** (see "The inner layers do no I/O"): `domain`, `protocol`, `application` and `presentation`, and
  their levels. The file is read by the same functions that read a file in those layers, as if it sat in the place:
  an import of an entry of the table (`shutil`, `fs`), a name judged by its name (the allowlists of `os`, `sys`, `time`
  and `logging`, the clock, randomness, the terminal, the arguments, `pathlib.Path`, a TypeScript global, a method of a
  path unless the project defines a method of that name and number of parameters anywhere, a test and the code outside
  the layers included), a star import and what cannot be read (`eval`, an import by a name built at run time) fail the
  file in each of those places; a package that the `allowed` of the project and of the unit holding the file, and the
  layout's, do not list for a place fails it in that place alone. The places that judge nothing (`infrastructure`,
  `composition` and `ui`) are offered as before. A file left with no place, because a layer that may not import what
  is left imports it, is told as none passing. Rotproof still never says that a file belongs in `infrastructure`, or in
  `domain` or in `application`: it says where the check would pass it and, by what it sees of its I/O, where it would
  not. What it does not see (a method on an object whose type cannot be known and whose name is no path's, a name an
  object holds) narrows nothing, as in a layer.
- **In Rust the files are counted and no order is given:** a layer is a crate, and what a crate imports is the
  dependencies of its manifest, which are no files' to order.

### The map in `AGENTS.md`

`AGENTS.md` is the project's own file, which Rotproof never writes. A project may keep a map of its layers in it: a
table under `## Map` with a row for each layer present, whose first cell is the layer's path. The map goes behind when the declaration changes later: a
layer declared absent and made afterwards, a unit added, a layer taken out. `rotproof check` warns of it, on every run
and failing nothing, under `the map in AGENTS.md lists the layers .config/rotproof.toml declares`: a line for each layer
present that has no row (`AGENTS.md has no row in its map for packages/cli/src/cli/protocol/, the layer protocol of the
unit cli: add it, or declare the layer absent in .config/rotproof.toml`), and for each row of a layer declared absent;
after six lines it counts the rest. `rotproof init` says the same, each line after `note: `. The words of a row are the
project's and are never judged, and a file with no `## Map` and table, or none at all, is not read: a project may keep
its map as it likes.

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
