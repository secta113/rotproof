//! The table of what does I/O wholly, by stack: Rotproof's, which a project cannot change.
//!
//! An entry is a module (or, where a module does some I/O and some not, the items of it that do) whose import puts I/O
//! in the code that imports it. Only what does I/O whatever it is used for is in: a module that is partly pure is
//! judged by the attribute or the call, which a later rule reads. Each stack's table is written for the guide
//! ([`listing`]), so the guide and the check cannot disagree.

use crate::layers::Language;

/// Python's modules, matched by their dotted name as a prefix of what an import names (`urllib.request` is matched by
/// `import urllib.request`, `from urllib import request` and `from urllib.request import urlopen`, and not by `from
/// urllib import parse`), and the functions of `asyncio` that start a process or a connection, matched when imported
/// by name. The private twins (`_socket`) are the C modules the public ones are made of.
pub const PYTHON: &[&str] = &[
    // The network
    "socket",
    "socketserver",
    "ssl",
    "select",
    "selectors",
    "http.client",
    "http.server",
    "urllib.request",
    "ftplib",
    "smtplib",
    "imaplib",
    "poplib",
    "xmlrpc.client",
    "xmlrpc.server",
    "wsgiref.simple_server",
    "nntplib",
    "telnetlib",
    "asyncore",
    "asynchat",
    "smtpd",
    // Storage and the file system
    "sqlite3",
    "dbm",
    "shelve",
    "mailbox",
    "shutil",
    "tempfile",
    "filecmp",
    "fileinput",
    "glob",
    "netrc",
    "zipapp",
    "py_compile",
    "compileall",
    "linecache",
    "importlib.resources",
    "importlib.metadata",
    "logging.handlers",
    "logging.config",
    // Processes, and what the processes of one program share
    "subprocess",
    "multiprocessing.connection",
    "multiprocessing.shared_memory",
    "asyncio.streams",
    "asyncio.subprocess",
    "asyncio.create_subprocess_exec",
    "asyncio.create_subprocess_shell",
    "asyncio.open_connection",
    "asyncio.start_server",
    "asyncio.open_unix_connection",
    "asyncio.start_unix_server",
    // The terminal, the console and the system around the program
    "pty",
    "tty",
    "termios",
    "fcntl",
    "curses",
    "readline",
    "msvcrt",
    "winreg",
    "winsound",
    "syslog",
    "getpass",
    "webbrowser",
    "tkinter",
    "turtle",
    "pwd",
    "grp",
    "resource",
    "pdb",
    "code",
    "pydoc",
    "mailcap",
    "nis",
    "spwd",
    "ossaudiodev",
    "msilib",
    "antigravity",
    "argparse",
    "optparse",
    "platform",
    "locale",
    // The private twins the standard library holds
    "_socket",
    "_ssl",
    "_sqlite3",
    "_posixsubprocess",
    "_winapi",
    "_multiprocessing",
    "_posixshmem",
    "_curses",
    "_curses_panel",
    "_tkinter",
    "_dbm",
    "_gdbm",
    "_locale",
    "_msi",
    // And the private modules of the same kind, without a public name to be a twin of: the C half of asyncio's
    // Windows event loop, the subprocess a build starts Python with, the Windows management interface, the macOS proxy
    // settings and the terminal of the interactive interpreter
    "_overlapped",
    "_bootsubprocess",
    "_wmi",
    "_scproxy",
    "_pyrepl",
];

/// The modules of Node that do I/O wholly, with or without `node:`.
pub const NODE: &[&str] = &[
    "fs",
    "net",
    "dgram",
    "dns",
    "tls",
    "http",
    "https",
    "http2",
    "child_process",
    "cluster",
    "readline",
    "repl",
    "tty",
    "wasi",
    "process",
    "_http_agent",
    "_http_client",
    "_http_common",
    "_http_incoming",
    "_http_outgoing",
    "_http_server",
    "_tls_common",
    "_tls_wrap",
];

/// The subpaths of [`NODE`]'s modules that are the module's too: `fs/promises` is `fs`.
pub const NODE_SUBPATHS: &[(&str, &str)] = &[
    ("fs", "fs/promises"),
    ("dns", "dns/promises"),
    ("readline", "readline/promises"),
];

/// The modules of Node that exist with `node:` alone: a bare `sqlite` is a package of npm.
pub const NODE_PREFIXED: &[&str] = &["sqlite"];

/// The platforms of `std::os`, each of which may have an `fs`, a `net` and a `process`. A platform a toolchain lacks
/// is a path that names nothing there, so the list is the names the standard library has anywhere.
pub const RUST_PLATFORMS: &[&str] = &[
    "aix",
    "android",
    "cygwin",
    "dragonfly",
    "emscripten",
    "espidf",
    "freebsd",
    "fuchsia",
    "haiku",
    "hermit",
    "horizon",
    "hurd",
    "illumos",
    "ios",
    "l4re",
    "linux",
    "macos",
    "netbsd",
    "nto",
    "openbsd",
    "redox",
    "solaris",
    "solid",
    "uefi",
    "unix",
    "vita",
    "vxworks",
    "wasi",
    "watchos",
    "windows",
];

/// The modules of `std::os::<platform>` that do I/O wholly, as `std::os::<module>`.
pub const RUST_OS_MODULES: &[&str] = &["fs", "net", "process"];

/// One row of Rust's table: the key an approval is made for, the module its names are of, and the names of it that do
/// I/O (`None`: every name of the module does).
pub struct RustRow {
    pub key: &'static str,
    pub module: &'static [&'static str],
    pub names: Option<&'static [&'static str]>,
}

/// The rows of Rust's table but for `std::os`, which has a row for each of [`RUST_OS_MODULES`] across the platforms.
/// `std::env` has none: it is judged by an allowlist of the names that are pure (`rust.rs`).
pub const RUST: &[RustRow] = &[
    RustRow {
        key: "std::fs",
        module: &["std", "fs"],
        names: None,
    },
    RustRow {
        key: "std::net",
        module: &["std", "net"],
        names: Some(&[
            "TcpListener",
            "TcpStream",
            "UdpSocket",
            "ToSocketAddrs",
            "Incoming",
        ]),
    },
    RustRow {
        key: "std::process",
        module: &["std", "process"],
        names: Some(&[
            "Command",
            "Child",
            "ChildStdin",
            "ChildStdout",
            "ChildStderr",
            "Stdio",
        ]),
    },
    RustRow {
        key: "std::io",
        module: &["std", "io"],
        names: Some(&[
            "stdin",
            "stdout",
            "stderr",
            "Stdin",
            "Stdout",
            "Stderr",
            "StdinLock",
            "StdoutLock",
            "StderrLock",
            "IsTerminal",
        ]),
    },
    RustRow {
        key: "std::path",
        module: &["std", "path"],
        names: Some(&["absolute"]),
    },
];

/// The macros of Rust that print, which count under `std::io`.
pub const RUST_MACROS: &[&str] = &["print", "println", "eprint", "eprintln", "dbg"];

/// The key under which the print macros and the terminal items of `std::io` are approved.
pub const RUST_IO_KEY: &str = "std::io";

/// The table of a stack as the guide writes it: what is listed, in Markdown, one list item for each part.
pub fn listing(language: Language) -> String {
    let join = |names: &[&str]| {
        names
            .iter()
            .map(|name| format!("`{name}`"))
            .collect::<Vec<_>>()
            .join(", ")
    };
    match language {
        Language::Python => {
            let (twins, public): (Vec<&str>, Vec<&str>) = PYTHON
                .iter()
                .copied()
                .partition(|name| name.starts_with('_'));
            format!(
                "  - Modules, each with what is under it (`urllib.request` is matched by `import urllib.request`, \
                 `from urllib import request` and `from urllib.request import urlopen`, and `from urllib import \
                 parse` is not): {}.\n  - The private modules of the standard library that these are made of, and \
                 the like: {}.\n",
                join(&public),
                join(&twins)
            )
        }
        Language::TypeScript => format!(
            "  - Modules of Node, with or without `node:` (`fs` and `fs/promises` are `fs`, `dns` and `dns/promises` \
             are `dns`, `readline` and `readline/promises` are `readline`), and a bare name is Node's even when a \
             package of that name is listed in `package.json`: {}; and, with `node:` alone, {}.\n  - Any other scheme \
             (`bun:`, `npm:`, `jsr:`, a URL): what it names cannot be judged, so it fails too.\n",
            join(NODE),
            join(NODE_PREFIXED)
        ),
        Language::Rust => {
            let rows: Vec<String> = RUST
                .iter()
                .map(|row| match row.names {
                    None => format!("`{}` (all of it)", row.key),
                    Some(names) => format!("`{}`: {}", row.key, join(names)),
                })
                .collect();
            format!(
                "  - {}; the print macros `print!`, `println!`, `eprint!`, `eprintln!` and `dbg!`, also as \
                 `std::println!` (with `std::io`'s terminal items, under `std::io`); and the modules `fs`, `net` and \
                 `process` of every platform of `std::os` (`std::os::unix::fs`, `std::os::windows::process`), under \
                 `std::os::fs`, `std::os::net` and `std::os::process`.\n  - A glob of one of these modules, or of a \
                 module above them (`use std::fs::*;`, `use std::*;`), whose names cannot be read.\n",
                rows.join("; ")
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::python_stdlib::STANDARD_LIBRARY;

    #[test]
    fn every_module_of_pythons_table_is_one_of_the_standard_library() {
        for name in PYTHON {
            let first = name.split('.').next().unwrap();
            assert!(STANDARD_LIBRARY.contains(&first), "{name}");
        }
    }

    #[test]
    fn nothing_is_listed_twice() {
        for table in [PYTHON, NODE] {
            let mut seen = std::collections::BTreeSet::new();
            for name in table {
                assert!(seen.insert(*name), "{name}");
            }
        }
        assert!(NODE_PREFIXED.iter().all(|name| !NODE.contains(name)));
    }

    #[test]
    fn the_private_twins_are_the_ones_the_standard_library_holds() {
        for twin in [
            "_socket",
            "_ssl",
            "_sqlite3",
            "_posixsubprocess",
            "_winapi",
            "_multiprocessing",
            "_posixshmem",
            "_curses",
            "_curses_panel",
            "_tkinter",
            "_dbm",
            "_gdbm",
            "_locale",
            "_msi",
        ] {
            assert!(PYTHON.contains(&twin), "{twin}");
            assert!(STANDARD_LIBRARY.contains(&twin), "{twin}");
        }
    }

    #[test]
    fn the_listing_names_every_entry_of_each_stack() {
        let python = listing(Language::Python);
        for name in PYTHON {
            assert!(python.contains(&format!("`{name}`")), "{name}");
        }
        let typescript = listing(Language::TypeScript);
        for name in NODE.iter().chain(NODE_PREFIXED) {
            assert!(typescript.contains(&format!("`{name}`")), "{name}");
        }
        let rust = listing(Language::Rust);
        for row in RUST {
            assert!(rust.contains(&format!("`{}`", row.key)), "{}", row.key);
            for name in row.names.unwrap_or(&[]) {
                assert!(rust.contains(&format!("`{name}`")), "{name}");
            }
        }
        for name in RUST_MACROS {
            assert!(rust.contains(&format!("`{name}!`")), "{name}");
        }
    }
}
