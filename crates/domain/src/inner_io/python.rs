//! Python's names that do I/O, for what is I/O in part: the allowlists of the modules that read or write outside the
//! program through some of their names, the groups of one act (the clock, randomness, the terminal, the command-line
//! arguments), `pathlib.Path` by name, and the net of method names.
//!
//! - **An allowlist names what is pure; everything else of the module is I/O.** It starts narrow, since a name added
//!   later is a patch and a name taken away is a major change. The names the environment check owns
//!   (`os.getenv`, `os.path.expanduser`, `pathlib.Path.home`) are left to it, and never reported twice. A name that
//!   no list below judges (a module of the standard library not named here, a package) is not I/O.
//! - **A name is judged by what it resolves to** ([`crate::code::Use`]): `import os as o` then `o.getcwd()` is
//!   `os.getcwd`, and a parameter called `os` is nothing. A reference counts as a call: `default_factory=datetime.now`
//!   is the clock as `datetime.now()` is. A use in a type position is never judged, and a module of the table of what
//!   does I/O wholly (`shutil`) is judged by the import, as the commit before this did.
//! - **A module object used as a value** (`sh = os`, `getattr(os, "system")`, `vars(os)`, `check(os)`) fails: what is
//!   done through it cannot be read.
//! - **A method is judged by its name on any receiver** when the name is one of a path's (`read_text`, `mkdir`,
//!   `unlink`), because the type of what it is called on cannot be read: the net. A name the project itself defines,
//!   with the same number of parameters, is left out of it ([`excluded`]).

use crate::code::{Defined, First, How, Method, Use};
use crate::environment::{owns_python, owns_python_star};

use super::groups::{Excluded, Group};
use super::{Cause, Found, python_entry};

/// The names of `os` that are pure: they turn a path into text or bytes and name the values that never change.
pub const OS_PURE: [&str; 9] = [
    "fspath", "fsencode", "fsdecode", "PathLike", "error", "strerror", "curdir", "pardir", "extsep",
];

/// What `os` takes from the system's source of randomness: it is randomness, so `random`'s group.
pub const OS_RANDOM: [&str; 2] = ["urandom", "getrandom"];

/// The functions of those modules that only work on the text of a path.
pub const PATH_PURE: [&str; 12] = [
    "join",
    "split",
    "splitext",
    "basename",
    "dirname",
    "normpath",
    "normcase",
    "isabs",
    "commonpath",
    "commonprefix",
    "splitdrive",
    "splitroot",
];

/// The facts of the interpreter that `sys` gives and that no run changes.
pub const SYS_PURE: [&str; 14] = [
    "version_info",
    "version",
    "hexversion",
    "maxsize",
    "maxunicode",
    "byteorder",
    "float_info",
    "int_info",
    "hash_info",
    "implementation",
    "getsizeof",
    "intern",
    "exc_info",
    "exception",
];

/// The streams of `sys`: the terminal.
pub const SYS_TERMINAL: [&str; 6] = [
    "stdin",
    "stdout",
    "stderr",
    "__stdin__",
    "__stdout__",
    "__stderr__",
];

/// The command-line arguments of `sys`.
pub const SYS_ARGV: [&str; 2] = ["argv", "orig_argv"];

/// What `time` holds that is pure. `sleep` is on this side on purpose: waiting reads nothing from outside, and the
/// program that waits is the program the clock is given to.
pub const TIME_PURE: [&str; 3] = ["struct_time", "strptime", "sleep"];

/// What `time` reads the clock with.
pub const TIME_CLOCK: [&str; 12] = [
    "time",
    "time_ns",
    "monotonic",
    "monotonic_ns",
    "perf_counter",
    "perf_counter_ns",
    "process_time",
    "process_time_ns",
    "thread_time",
    "thread_time_ns",
    "clock_gettime",
    "clock_gettime_ns",
];

/// What `time` turns a moment into text or fields with, pure only with the moment given: without an argument, or with
/// `None`, it reads the clock.
pub const TIME_WITH_ARGUMENT: [&str; 4] = ["gmtime", "localtime", "ctime", "asctime"];

/// `time.strftime(format)` formats the clock; with the moment as its second argument it is pure.
pub const TIME_FORMAT: &str = "strftime";

/// What `logging` holds that is pure: the classes a logger is made of, the level names and `getLogger`. Everything else
/// of it writes: `basicConfig`, the functions of the root logger, every handler and `config`.
pub const LOGGING_PURE: [&str; 15] = [
    "getLogger",
    "Logger",
    "LoggerAdapter",
    "Filter",
    "Formatter",
    "LogRecord",
    "getLevelName",
    "NOTSET",
    "DEBUG",
    "INFO",
    "WARNING",
    "WARN",
    "ERROR",
    "CRITICAL",
    "FATAL",
];

/// What of `datetime` reads the clock, as the class and the method: `datetime.datetime.now`, `datetime.date.today`.
pub const DATETIME_CLOCK: [(&str, &str); 4] = [
    ("datetime", "now"),
    ("datetime", "utcnow"),
    ("datetime", "today"),
    ("date", "today"),
];

/// The functions of the module `random`, which draw on a generator the module holds, and its source of the system.
pub const RANDOM_FUNCTIONS: [&str; 25] = [
    "random",
    "randint",
    "randrange",
    "choice",
    "choices",
    "shuffle",
    "sample",
    "uniform",
    "triangular",
    "gauss",
    "normalvariate",
    "lognormvariate",
    "expovariate",
    "vonmisesvariate",
    "gammavariate",
    "betavariate",
    "paretovariate",
    "weibullvariate",
    "binomialvariate",
    "getrandbits",
    "randbytes",
    "seed",
    "getstate",
    "setstate",
    "SystemRandom",
];

/// `random.Random`, pure when it is given a seed.
pub const RANDOM_CLASS: &str = "Random";

/// The functions of `uuid` that draw on randomness or on the clock and the machine; `uuid3`, `uuid5` and `UUID` are
/// pure.
pub const UUID_RANDOM: [&str; 2] = ["uuid1", "uuid4"];

/// The functions of `io` that open a file.
pub const IO_OPEN: [&str; 3] = ["open", "FileIO", "open_code"];

/// The builtins that touch the terminal, a file or the interpreter's end.
pub const BUILTINS_TERMINAL: [&str; 7] = [
    "print",
    "input",
    "open",
    "breakpoint",
    "exit",
    "quit",
    "help",
];

/// The classes of `pathlib` that touch the file system by being a concrete path; `PurePath` and its two kinds do not.
pub const PATHLIB_CLASSES: [&str; 3] = ["Path", "PosixPath", "WindowsPath"];

/// The modules whose star import fails, besides the ones the environment check fails: each has a name that is I/O
/// among those it brings in. `secrets` is not here: it is I/O whole, and its import is the finding.
pub const STAR_MODULES: [&str; 11] = [
    "sys", "time", "logging", "datetime", "random", "uuid", "io", "codecs", "builtins", "posix",
    "nt",
];

/// The names of a path that are judged on any receiver.
pub const NET_ALWAYS: [&str; 23] = [
    "read_text",
    "write_text",
    "read_bytes",
    "write_bytes",
    "iterdir",
    "rglob",
    "mkdir",
    "unlink",
    "rmdir",
    "symlink_to",
    "hardlink_to",
    "readlink",
    "lstat",
    "touch",
    "chmod",
    "lchmod",
    "samefile",
    "is_symlink",
    "is_mount",
    "is_junction",
    "copy_into",
    "move_into",
    "absolute",
];

/// The names of a path that are judged when called with no argument: with one, they are the name of another thing.
pub const NET_NO_ARGUMENT: [&str; 6] = ["exists", "is_file", "is_dir", "stat", "owner", "resolve"];

/// `open` is judged when it is called with a string literal for its mode, or with one of these keywords, as a path's
/// `open` is.
pub const NET_OPEN_KEYWORDS: [&str; 4] = ["mode", "encoding", "newline", "buffering"];

/// The module family of a name, and the module as written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Module {
    Os,
    Path,
    Sys,
    Time,
    Logging,
    Datetime,
    Random,
    Secrets,
    Uuid,
    Io,
    Codecs,
    Builtins,
    Pathlib,
}

fn module_of(name: &str) -> Option<Module> {
    Some(match name {
        "os" | "posix" | "nt" => Module::Os,
        "os.path" | "posixpath" | "ntpath" | "genericpath" => Module::Path,
        "sys" => Module::Sys,
        "time" => Module::Time,
        "logging" => Module::Logging,
        "datetime" => Module::Datetime,
        "random" => Module::Random,
        "secrets" => Module::Secrets,
        "uuid" => Module::Uuid,
        "io" => Module::Io,
        "codecs" => Module::Codecs,
        "builtins" => Module::Builtins,
        "pathlib" => Module::Pathlib,
        _ => return None,
    })
}

/// The module a dotted name starts with (the longest: `os.path`, not `os`), as written, and the names after it.
fn split(name: &str) -> Option<(Module, String, Vec<&str>)> {
    let parts: Vec<&str> = name.split('.').collect();
    (1..=parts.len()).rev().find_map(|at| {
        let written = parts[..at].join(".");
        module_of(&written).map(|module| (module, written, parts[at..].to_vec()))
    })
}

/// What a name comes to.
enum Verdict {
    /// An entry of its own, with the name used under it
    Entry(String, Option<String>),
    /// A group's act, by this name
    Group(Group, String),
    /// A module used as a value
    Module(String),
}

/// What `used` is, by the allowlists and the groups.
fn verdict(used: &Use) -> Option<Verdict> {
    let (module, written, rest) = split(&used.name)?;
    let Some((first, rest)) = rest.split_first() else {
        // The module itself: `from os import path` takes the module `os.path`, which is no use of it as a value
        return match used.how {
            How::Imported => None,
            _ => Some(Verdict::Module(written)),
        };
    };
    let key = format!("{written}.{first}");
    let (given, unpacked, lead) = match &used.how {
        How::Called {
            positional,
            keywords,
            unpacked,
            first,
        } => (Some(positional + keywords.len()), *unpacked, *first),
        _ => (None, false, First::Other),
    };
    let imported = used.how == How::Imported;
    let in_group = |group: Group| Some(Verdict::Group(group, key.clone()));
    let own = || Some(Verdict::Entry(key.clone(), None));
    match module {
        Module::Os if OS_RANDOM.contains(first) => in_group(Group::Random),
        Module::Os if OS_PURE.contains(first) => None,
        Module::Os => own(),
        Module::Path if PATH_PURE.contains(first) => None,
        Module::Path => own(),
        Module::Sys if SYS_PURE.contains(first) => None,
        Module::Sys if SYS_TERMINAL.contains(first) => in_group(Group::Terminal),
        Module::Sys if SYS_ARGV.contains(first) => in_group(Group::Argv),
        Module::Sys => own(),
        Module::Time if TIME_PURE.contains(first) => None,
        Module::Time if TIME_CLOCK.contains(first) => in_group(Group::Clock),
        // Pure with the moment given; a reference cannot say, and fails; an import of the name is judged where it is
        // called
        Module::Time if TIME_WITH_ARGUMENT.contains(first) || *first == TIME_FORMAT => {
            let pure = match given {
                _ if imported => true,
                Some(given) if *first == TIME_FORMAT => given >= 2 && !unpacked,
                Some(given) => given >= 1 && !unpacked && lead != First::Null,
                None => false,
            };
            if pure { None } else { in_group(Group::Clock) }
        }
        Module::Time => own(),
        Module::Logging if LOGGING_PURE.contains(first) => None,
        Module::Logging => own(),
        Module::Datetime => {
            let method = rest.first()?;
            DATETIME_CLOCK
                .iter()
                .any(|(class, name)| class == first && name == method)
                .then(|| Verdict::Group(Group::Clock, format!("{key}.{method}")))
        }
        Module::Random if *first == RANDOM_CLASS => {
            // `random.Random(seed)` is pure; unseeded, `random.Random()` draws on the system
            let seeded = match given {
                _ if imported => true,
                Some(given) => given >= 1 && !unpacked && lead != First::Null,
                None => false,
            };
            if seeded {
                None
            } else {
                in_group(Group::Random)
            }
        }
        Module::Random => RANDOM_FUNCTIONS
            .contains(first)
            .then(|| Verdict::Group(Group::Random, key.clone())),
        Module::Secrets => in_group(Group::Random),
        Module::Uuid => UUID_RANDOM
            .contains(first)
            .then(|| Verdict::Group(Group::Random, key.clone())),
        Module::Io => IO_OPEN
            .contains(first)
            .then(|| Verdict::Group(Group::Terminal, key.clone())),
        Module::Codecs => (*first == "open").then(|| Verdict::Group(Group::Terminal, key.clone())),
        Module::Builtins => BUILTINS_TERMINAL
            .contains(first)
            .then(|| Verdict::Group(Group::Terminal, (*first).to_string())),
        Module::Pathlib => PATHLIB_CLASSES
            .contains(first)
            .then(|| Verdict::Entry(key.clone(), rest.first().map(|name| name.to_string()))),
    }
}

/// What is I/O in the use of a name of the modules above, as a finding: `None` when it passes, and when the name is a
/// use in a type position, which is never judged, or one the environment check judges.
pub fn judge(used: &Use) -> Option<Found> {
    if used.in_type || used.how == How::Star || owns_python(&used.name) {
        return None;
    }
    let line = used.line;
    Some(match verdict(used)? {
        Verdict::Entry(entry, name) => Found {
            line,
            entry,
            name,
            cause: Cause::Used,
        },
        Verdict::Group(group, name) => Found {
            line,
            entry: group.key().to_string(),
            name: Some(name),
            cause: Cause::Group(group),
        },
        Verdict::Module(module) => Found {
            line,
            entry: module,
            name: None,
            cause: Cause::Module,
        },
    })
}

/// The import of the module `module` (its dotted parts): `secrets` is randomness whole, so it is found where it is
/// imported, as a module of the table is.
pub fn judge_import(line: usize, parts: &[String]) -> Option<Found> {
    (parts.first().map(String::as_str) == Some("secrets")).then(|| Found {
        line,
        entry: Group::Random.key().to_string(),
        name: Some(parts.join(".")),
        cause: Cause::Group(Group::Random),
    })
}

/// A star import of a module that has a name that is I/O among those it brings in: the names cannot be read, so it
/// fails. The modules whose star import the environment check fails are left to it.
pub fn judge_star(used: &Use) -> Option<Found> {
    (used.how == How::Star
        && STAR_MODULES.contains(&used.name.as_str())
        && !owns_python_star(&used.name))
    .then(|| Found {
        line: used.line,
        entry: used.name.clone(),
        name: None,
        cause: Cause::Star(used.name.clone()),
    })
}

/// Whether a callee that resolves to `name` is judged by its module already: a module of the table, or one of the lists
/// above. Its method is not judged by its name as well.
fn covered(name: &str) -> bool {
    python_entry(name).is_some() || split(name).is_some()
}

/// The net: a call of an attribute whose name is a path's, on any receiver, which is I/O unless the project defines a
/// method of that name and the same number of parameters (`excluded`).
pub fn net(method: &Method, excluded: &Excluded) -> Option<Found> {
    if method.resolved.iter().any(|name| covered(name)) {
        return None;
    }
    let name = method.name.as_str();
    let given = method.positional + method.keywords.len();
    let judged = NET_ALWAYS.contains(&name)
        || (NET_NO_ARGUMENT.contains(&name) && given == 0 && !method.unpacked)
        || (name == "open"
            && (method.first == First::Text
                || method
                    .keywords
                    .iter()
                    .any(|keyword| NET_OPEN_KEYWORDS.contains(&keyword.as_str()))));
    if !judged || (!method.unpacked && excluded.contains(name, given)) {
        return None;
    }
    Some(Found {
        line: method.line,
        entry: name.to_string(),
        name: None,
        cause: Cause::Method,
    })
}

/// The methods of the project that the net leaves out, from the methods its classes define: the ones the net would
/// judge a call of, by that many parameters.
pub fn excluded(defined: &[Defined]) -> Excluded {
    Excluded::new(
        defined
            .iter()
            .filter(|method| {
                let (name, parameters) = (method.name.as_str(), method.parameters);
                NET_ALWAYS.contains(&name)
                    || (NET_NO_ARGUMENT.contains(&name) && parameters == 0)
                    || (name == "open" && parameters >= 1)
            })
            .map(|method| (method.name.clone(), method.parameters))
            .collect(),
    )
}

/// What the guide says of the names of Python: the allowlists in full, the groups, `Path` and the net.
pub fn listing() -> String {
    let join = |names: &[&str]| {
        names
            .iter()
            .map(|name| format!("`{name}`"))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let pairs = |pairs: &[(&str, &str)]| {
        pairs
            .iter()
            .map(|(class, name)| format!("`datetime.{class}.{name}`"))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let mut text = String::from(
        "  What a module does only in part, and a few builtins, are judged by the name used: the lists below are \
         Rotproof's, a project cannot change them, and a name in none of them is not I/O.\n",
    );
    text.push_str(&format!(
        "  - **`os`** (and `posix` and `nt`, which it takes its names from): pure are {} and nothing else; `os.sep`, \
         `os.name`, `os.getcwd`, `os.listdir` and every other name of it is I/O (`os.urandom` and `os.getrandom` are \
         `random`).\n",
        join(&OS_PURE)
    ));
    text.push_str(&format!(
        "  - **`os.path`** (and `posixpath`, `ntpath` and `genericpath`): pure are {} and nothing else; `exists`, \
         `isfile`, `getsize`, `realpath`, `abspath`, `relpath` and every other name of it is I/O.\n",
        join(&PATH_PURE)
    ));
    text.push_str(&format!(
        "  - **`sys`**: pure are {} and nothing else. `sys.stdin`, `sys.stdout`, `sys.stderr` and the `sys.__std*__` are \
         `terminal`, `sys.argv` and `sys.orig_argv` are `argv`, and every other name of it (`sys.exit`, `sys.path`, \
         `sys.modules`, `sys.platform`, `sys.getrecursionlimit`) is I/O.\n",
        join(&SYS_PURE)
    ));
    text.push_str(&format!(
        "  - **`time`**: pure are {} (`sleep` waits and reads nothing), `gmtime`, `localtime`, `ctime` and `asctime` \
         with an argument that is not `None`, and `strftime` with two arguments. The clock is `time.time`, `time.time_ns` \
         and the rest of {}, and those four without an argument or with `None`, and `strftime` with one, and a reference \
         to one of them; every other name (`mktime`, `tzset`, `timezone`, `tzname`, `clock_settime`) is I/O.\n",
        join(&TIME_PURE),
        join(&TIME_CLOCK)
    ));
    text.push_str(&format!(
        "  - **`logging`**: pure are {} and nothing else; `basicConfig`, the functions of the root logger (`debug`, `info`, \
         `warning`, `warn`, `error`, `exception`, `critical`, `fatal`, `log`), every handler class, `disable`, \
         `shutdown`, `captureWarnings`, `config` and `handlers` are I/O. A method of a logger (`logger.info`) is not \
         seen, and is free.\n",
        join(&LOGGING_PURE)
    ));
    text.push_str(&format!(
        "  - **The clock** (`clock`): {} and the names of `time` above, called or taken as a value \
         (`default_factory=datetime.now`, `Annotated[datetime, Field(default_factory=datetime.now)]`), also through \
         `from datetime import datetime`.\n",
        pairs(&DATETIME_CLOCK)
    ));
    text.push_str(&format!(
        "  - **Randomness** (`random`): the functions of `random` ({}), `random.SystemRandom`, `random.Random()` without \
         a seed (with one, `random.Random(1)` is pure), all of `secrets`, {} of `uuid` (`uuid3`, `uuid5` and `uuid.UUID(...)` \
         are pure) and `os.urandom`.\n",
        join(&RANDOM_FUNCTIONS),
        join(&UUID_RANDOM)
    ));
    text.push_str(&format!(
        "  - **The terminal and the files a builtin opens** (`terminal`): the builtins {} (a name no scope of the file \
         binds: a parameter called `input` is its own), `io.open`, `io.FileIO`, `io.open_code`, `codecs.open` and the \
         streams of `sys`. Archives (`zipfile`, `tarfile`, `gzip.open`) are not I/O here.\n",
        join(&BUILTINS_TERMINAL)
    ));
    text.push_str("  - **The command-line arguments** (`argv`): `sys.argv` and `sys.orig_argv`.\n");
    text.push_str(&format!(
        "  - **`pathlib.Path`, `pathlib.PosixPath` and `pathlib.WindowsPath`** are I/O by name: an import or a use of one \
         fails ({}, one entry for each class). `PurePath`, `PurePosixPath` and `PureWindowsPath` are the pure way.\n",
        join(&PATHLIB_CLASSES.map(|name| format!("pathlib.{name}")).iter().map(String::as_str).collect::<Vec<_>>())
    ));
    text.push_str(&format!(
        "  - **A module used as a value**, and so what is done through it unread (`sh = os`, `getattr(os, \"system\")`, \
         `vars(os)`, `check(os)`), fails for every module above and `datetime`, `random`, `secrets`, `uuid`, `io`, \
         `codecs`, `builtins` and `pathlib`; so does a star import of {} (the star import of `os`, `os.path` and \
         `pathlib` is the environment check's), and `eval`, `exec` and `importlib.import_module` or `__import__` with a \
         name that is no string literal.\n",
        join(&STAR_MODULES)
    ));
    text.push_str(&format!(
        "  - **A method of a path, on any receiver** (the type of what a method is called on cannot be read, so its name is \
         judged): {} always, {} when called with no argument, and `open` when called with a string literal or a keyword of \
         {}. A name is left out of this net when the project defines a method (a `def` in a class) of the same name and the \
         same number of parameters, not counting `self`, anywhere in it; `rotproof check` prints each name left out on every \
         run.\n",
        join(&NET_ALWAYS),
        join(&NET_NO_ARGUMENT),
        join(&NET_OPEN_KEYWORDS)
    ));
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn used(name: &str, how: How) -> Use {
        Use {
            line: 1,
            name: name.into(),
            how,
            in_type: false,
        }
    }

    fn called(name: &str, positional: usize, lead: First) -> Use {
        used(
            name,
            How::Called {
                positional,
                keywords: Vec::new(),
                unpacked: false,
                first: lead,
            },
        )
    }

    /// The entry a name is, and the name used under it: `None` for what passes.
    fn said(used: &Use) -> Option<(String, Option<String>)> {
        judge(used).map(|found| (found.entry, found.name))
    }

    fn entry(entry: &str) -> Option<(String, Option<String>)> {
        Some((entry.to_string(), None))
    }

    fn group(group: Group, name: &str) -> Option<(String, Option<String>)> {
        Some((group.key().to_string(), Some(name.to_string())))
    }

    #[test]
    fn the_pure_names_of_each_allowlist_pass_and_every_other_name_of_the_module_is_io() {
        for (name, pure) in [
            ("os", OS_PURE.to_vec()),
            ("posix", OS_PURE.to_vec()),
            ("nt", OS_PURE.to_vec()),
            ("os.path", PATH_PURE.to_vec()),
            ("posixpath", PATH_PURE.to_vec()),
            ("ntpath", PATH_PURE.to_vec()),
            ("genericpath", PATH_PURE.to_vec()),
            ("sys", SYS_PURE.to_vec()),
            ("logging", LOGGING_PURE.to_vec()),
        ] {
            for each in pure {
                let used = used(&format!("{name}.{each}"), How::Value);
                assert_eq!(said(&used), None, "{name}.{each}");
            }
        }
        for name in [
            "os.getcwd",
            "os.sep",
            "os.name",
            "os.devnull",
            "os.listdir",
            "os.system",
            "posix.getcwd",
            "nt.startfile",
            "os.path.exists",
            "os.path.getsize",
            "os.path.realpath",
            "os.path.abspath",
            "os.path.relpath",
            "posixpath.isfile",
            "ntpath.sep",
            "genericpath.exists",
            "sys.exit",
            "sys.path",
            "sys.modules",
            "sys.platform",
            "sys.getrecursionlimit",
            "logging.basicConfig",
            "logging.info",
            "logging.StreamHandler",
            "logging.disable",
            "logging.shutdown",
            "logging.captureWarnings",
            "time.mktime",
            "time.tzset",
            "time.timezone",
            "time.tzname",
            "time.clock_settime",
        ] {
            assert_eq!(said(&used(name, How::Value)), entry(name), "{name}");
        }
    }

    #[test]
    fn a_name_the_environment_check_owns_is_left_to_it() {
        for name in [
            "os.environ",
            "os.environ.get",
            "os.getenv",
            "os.putenv",
            "os.unsetenv",
            "os.environb",
            "os.getenvb",
            "os.path.expandvars",
            "os.path.expanduser",
            "posixpath.expanduser",
            "pathlib.Path.home",
            "pathlib.PosixPath.home",
            "pathlib.WindowsPath.home",
        ] {
            assert_eq!(said(&used(name, How::Value)), None, "{name}");
        }
        // `posix` is not `os` for the environment check, which does not see `posix.environ`
        assert_eq!(
            said(&used("posix.environ", How::Value)),
            entry("posix.environ")
        );
        assert_eq!(
            said(&used("pathlib.Path", How::Imported)),
            entry("pathlib.Path")
        );
    }

    #[test]
    fn the_clock_is_one_group_called_or_taken_as_a_value() {
        for name in [
            "datetime.datetime.now",
            "datetime.datetime.utcnow",
            "datetime.datetime.today",
            "datetime.date.today",
        ] {
            assert_eq!(
                said(&called(name, 0, First::Other)),
                group(Group::Clock, name)
            );
            assert_eq!(said(&used(name, How::Value)), group(Group::Clock, name));
        }
        for name in [
            "time.time",
            "time.time_ns",
            "time.monotonic",
            "time.perf_counter",
            "time.process_time_ns",
            "time.thread_time",
            "time.clock_gettime",
        ] {
            assert_eq!(said(&used(name, How::Value)), group(Group::Clock, name));
        }
        // What of `datetime` is a value is the clock of nobody
        for name in [
            "datetime.datetime.fromtimestamp",
            "datetime.datetime",
            "datetime.date.fromisoformat",
            "datetime.timedelta",
            "datetime.timezone.utc",
        ] {
            assert_eq!(said(&called(name, 1, First::Other)), None, "{name}");
        }
        // A name the file binds is not the module
        assert_eq!(said(&used("time.sleep", How::Value)), None);
        assert_eq!(said(&called("time.strptime", 2, First::Text)), None);
        assert_eq!(said(&used("time.struct_time", How::Value)), None);
    }

    #[test]
    fn a_time_function_is_pure_only_with_the_moment_given() {
        for name in [
            "time.gmtime",
            "time.localtime",
            "time.ctime",
            "time.asctime",
        ] {
            assert_eq!(
                said(&called(name, 0, First::Other)),
                group(Group::Clock, name)
            );
            assert_eq!(
                said(&called(name, 1, First::Null)),
                group(Group::Clock, name)
            );
            assert_eq!(said(&called(name, 1, First::Other)), None, "{name}");
            assert_eq!(said(&used(name, How::Value)), group(Group::Clock, name));
            // Taken by name at an import, it is judged where it is called
            assert_eq!(said(&used(name, How::Imported)), None);
        }
        let unpacked = used(
            "time.gmtime",
            How::Called {
                positional: 0,
                keywords: Vec::new(),
                unpacked: true,
                first: First::Other,
            },
        );
        assert_eq!(said(&unpacked), group(Group::Clock, "time.gmtime"));
        assert_eq!(
            said(&called("time.strftime", 1, First::Text)),
            group(Group::Clock, "time.strftime")
        );
        assert_eq!(said(&called("time.strftime", 2, First::Text)), None);
    }

    #[test]
    fn randomness_is_one_group_and_a_seeded_generator_is_pure() {
        for name in [
            "random.random",
            "random.randint",
            "random.choice",
            "random.shuffle",
            "random.sample",
            "random.uniform",
            "random.gauss",
            "random.getrandbits",
            "random.seed",
            "random.SystemRandom",
            "uuid.uuid1",
            "uuid.uuid4",
            "secrets.token_hex",
            "secrets.choice",
            "os.urandom",
        ] {
            assert_eq!(
                said(&used(name, How::Value)),
                group(Group::Random, name),
                "{name}"
            );
        }
        assert_eq!(
            said(&called("random.Random", 0, First::Other)),
            group(Group::Random, "random.Random")
        );
        assert_eq!(
            said(&called("random.Random", 1, First::Null)),
            group(Group::Random, "random.Random")
        );
        assert_eq!(said(&called("random.Random", 1, First::Other)), None);
        assert_eq!(said(&called("random.Random", 1, First::Text)), None);
        for name in [
            "uuid.uuid3",
            "uuid.uuid5",
            "uuid.UUID",
            "uuid.NAMESPACE_DNS",
        ] {
            assert_eq!(said(&called(name, 2, First::Other)), None, "{name}");
        }
        assert_eq!(
            judge_import(3, &["secrets".to_string()]).map(|f| (f.line, f.entry)),
            Some((3, "random".to_string()))
        );
        assert!(judge_import(3, &["random".to_string()]).is_none());
    }

    #[test]
    fn the_terminal_is_one_group_by_builtin_stream_and_opener() {
        for name in [
            "builtins.print",
            "builtins.input",
            "builtins.open",
            "builtins.breakpoint",
            "builtins.exit",
            "builtins.quit",
            "builtins.help",
        ] {
            let name = name.trim_start_matches("builtins.");
            assert_eq!(
                said(&called(&format!("builtins.{name}"), 1, First::Other)),
                group(Group::Terminal, name)
            );
        }
        for name in [
            "sys.stdin",
            "sys.stdout",
            "sys.stderr",
            "sys.__stdout__",
            "sys.stdout.write",
            "io.open",
            "io.FileIO",
            "io.open_code",
            "codecs.open",
        ] {
            let key = if name.starts_with("sys.stdout.") {
                "sys.stdout"
            } else {
                name
            };
            assert_eq!(
                said(&used(name, How::Value)),
                group(Group::Terminal, key),
                "{name}"
            );
        }
        assert_eq!(said(&used("builtins.len", How::Value)), None);
        assert_eq!(said(&used("io.StringIO", How::Value)), None);
        assert_eq!(said(&used("io.BytesIO", How::Value)), None);
        assert_eq!(said(&used("codecs.encode", How::Value)), None);
    }

    #[test]
    fn the_arguments_of_the_command_line_are_a_group() {
        assert_eq!(
            said(&used("sys.argv", How::Value)),
            group(Group::Argv, "sys.argv")
        );
        assert_eq!(
            said(&used("sys.orig_argv", How::Value)),
            group(Group::Argv, "sys.orig_argv")
        );
    }

    #[test]
    fn a_path_is_io_by_its_class_and_a_pure_path_is_not() {
        assert_eq!(
            said(&called("pathlib.Path", 1, First::Text)),
            entry("pathlib.Path")
        );
        assert_eq!(
            said(&called("pathlib.Path.cwd", 0, First::Other)),
            Some(("pathlib.Path".to_string(), Some("cwd".to_string())))
        );
        assert_eq!(
            said(&used("pathlib.PosixPath", How::Value)),
            entry("pathlib.PosixPath")
        );
        assert_eq!(
            said(&used("pathlib.WindowsPath", How::Value)),
            entry("pathlib.WindowsPath")
        );
        for name in [
            "pathlib.PurePath",
            "pathlib.PurePosixPath",
            "pathlib.PureWindowsPath",
        ] {
            assert_eq!(said(&called(name, 1, First::Text)), None, "{name}");
        }
    }

    #[test]
    fn a_module_used_as_a_value_cannot_be_read_and_fails() {
        for module in [
            "os",
            "posix",
            "nt",
            "os.path",
            "posixpath",
            "sys",
            "time",
            "logging",
            "datetime",
            "random",
            "secrets",
            "uuid",
            "io",
            "codecs",
            "builtins",
            "pathlib",
        ] {
            let found = judge(&used(module, How::Value)).unwrap();
            assert_eq!(
                (found.entry.as_str(), &found.cause),
                (module, &Cause::Module),
                "{module}"
            );
        }
        // Taken as a module by `from os import path`, it is no value
        assert_eq!(said(&used("os.path", How::Imported)), None);
        assert_eq!(said(&used("typing", How::Value)), None);
    }

    #[test]
    fn a_use_in_a_type_position_and_a_star_are_not_judged_as_a_name() {
        let mut typed = used("os.getcwd", How::Value);
        typed.in_type = true;
        assert_eq!(said(&typed), None);
        assert_eq!(said(&used("time", How::Star)), None);
    }

    #[test]
    fn a_star_import_of_a_module_with_an_entry_fails_but_not_the_environment_checks() {
        for module in STAR_MODULES {
            let found = judge_star(&used(module, How::Star)).unwrap();
            assert_eq!(found.entry, module);
            assert_eq!(found.cause, Cause::Star(module.to_string()));
        }
        for module in ["os", "os.path", "pathlib", "typing", "secrets", "shutil"] {
            assert!(judge_star(&used(module, How::Star)).is_none(), "{module}");
        }
        assert!(judge_star(&used("time", How::Value)).is_none());
    }

    fn method(name: &str, positional: usize, keywords: &[&str], first: First) -> Method {
        Method {
            line: 4,
            name: name.into(),
            resolved: Vec::new(),
            positional,
            keywords: keywords.iter().map(|k| k.to_string()).collect(),
            unpacked: false,
            first,
        }
    }

    #[test]
    fn the_net_judges_a_name_of_a_path_on_any_receiver() {
        for name in NET_ALWAYS {
            for positional in [0, 1, 3] {
                let found = net(
                    &method(name, positional, &[], First::Other),
                    &Excluded::default(),
                );
                assert_eq!(found.map(|f| f.entry), Some(name.to_string()), "{name}");
            }
        }
        for name in NET_NO_ARGUMENT {
            assert!(net(&method(name, 0, &[], First::Other), &Excluded::default()).is_some());
            assert!(net(&method(name, 1, &[], First::Other), &Excluded::default()).is_none());
            assert!(
                net(
                    &method(name, 0, &["follow_symlinks"], First::Other),
                    &Excluded::default()
                )
                .is_none()
            );
        }
        for name in [
            "get", "read", "write", "close", "glob", "rename", "join", "items", "group",
        ] {
            assert!(net(&method(name, 0, &[], First::Other), &Excluded::default()).is_none());
        }
    }

    #[test]
    fn open_is_judged_with_a_literal_mode_or_a_keyword_of_a_files() {
        let none = Excluded::default();
        assert!(net(&method("open", 1, &[], First::Text), &none).is_some());
        assert!(net(&method("open", 0, &["encoding"], First::Other), &none).is_some());
        assert!(net(&method("open", 0, &["newline"], First::Other), &none).is_some());
        assert!(net(&method("open", 0, &["buffering"], First::Other), &none).is_some());
        assert!(net(&method("open", 0, &["mode"], First::Other), &none).is_some());
        assert!(net(&method("open", 0, &[], First::Other), &none).is_none());
        assert!(net(&method("open", 1, &[], First::Other), &none).is_none());
        assert!(net(&method("open", 1, &["url"], First::Other), &none).is_none());
    }

    #[test]
    fn a_method_a_module_judges_is_not_judged_again_by_its_name() {
        let mut call = method("mkdir", 1, &[], First::Text);
        call.resolved = vec!["os.mkdir".to_string()];
        assert!(net(&call, &Excluded::default()).is_none());
        call.resolved = vec!["shutil.rmtree".to_string()];
        assert!(net(&call, &Excluded::default()).is_none());
        call.resolved = vec!["app.repo.mkdir".to_string()];
        assert!(net(&call, &Excluded::default()).is_some());
    }

    #[test]
    fn the_project_that_defines_a_method_leaves_its_name_out_for_that_many_parameters() {
        let defined = [
            Defined {
                name: "exists".into(),
                parameters: 0,
            },
            Defined {
                name: "read_text".into(),
                parameters: 1,
            },
            // The net would not judge a call of `exists` with a parameter, so it leaves nothing out
            Defined {
                name: "stat".into(),
                parameters: 2,
            },
            Defined {
                name: "close".into(),
                parameters: 0,
            },
        ];
        let excluded = excluded(&defined);
        assert_eq!(
            excluded.line().as_deref(),
            Some(
                "I/O net: left out, as the project defines a method of the same name and number of parameters: \
                 exists(0), read_text(1)"
            )
        );
        assert!(net(&method("exists", 0, &[], First::Other), &excluded).is_none());
        assert!(net(&method("read_text", 1, &[], First::Other), &excluded).is_none());
        // A different number of arguments is not the project's method
        assert!(net(&method("read_text", 0, &[], First::Other), &excluded).is_some());
        assert!(net(&method("read_text", 2, &[], First::Other), &excluded).is_some());
        // A call that hides how many arguments it has is judged
        let mut hidden = method("read_text", 1, &[], First::Other);
        hidden.unpacked = true;
        assert!(net(&hidden, &excluded).is_some());
    }

    #[test]
    fn every_name_of_every_list_is_in_the_guides_listing() {
        let listing = listing();
        for name in OS_PURE
            .iter()
            .chain(&PATH_PURE)
            .chain(&SYS_PURE)
            .chain(&TIME_PURE)
            .chain(&TIME_CLOCK)
            .chain(&TIME_WITH_ARGUMENT)
            .chain(&LOGGING_PURE)
            .chain(&BUILTINS_TERMINAL)
            .chain(&NET_ALWAYS)
            .chain(&NET_NO_ARGUMENT)
            .chain(&NET_OPEN_KEYWORDS)
            .chain(&UUID_RANDOM)
            .chain(&STAR_MODULES)
        {
            assert!(listing.contains(&format!("`{name}`")), "{name}");
        }
        for name in RANDOM_FUNCTIONS {
            assert!(listing.contains(&format!("`{name}`")), "{name}");
        }
        assert!(
            listing.contains("`datetime.datetime.now`")
                && listing.contains("`datetime.date.today`")
        );
    }
}
