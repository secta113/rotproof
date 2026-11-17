//! TypeScript's names that do I/O, for the globals: the allowlist of the globals that are pure, the groups of one act
//! (the clock, randomness, the terminal, the command-line arguments), `process`, and what cannot be read.
//!
//! - **An allowlist names what is pure; every other global is I/O.** It starts narrow, since a name added later is a
//!   patch and a name taken away is a major change. A global is a name no scope of the file binds, with `window.`,
//!   `globalThis.`, `self.` and `global.` in front of it taken away by the reader ([`crate::code::Use`]):
//!   `window.fetch` is `fetch`, and a parameter called `fetch` is nothing. A name taken from a module
//!   (`"node:fs".readFile`) is judged by the table of Node's modules and not here.
//! - **A reference counts as a call:** `useState(Date.now)` is the clock as `Date.now()` is, and `const f = fetch` is
//!   I/O as `fetch(x)` is. A name in a type position is never judged (the reader leaves it out), and nor is the operand
//!   of `typeof` (`typeof window`), which runs nothing but the test of what it is.
//! - **The entry is the global,** by its name: `fetch`, `localStorage` (for `localStorage.getItem`, which the finding
//!   names), `document`, and, for `process`, which is judged as a module is, `process.cwd`. An act that more than one
//!   name does is one entry, a group: `clock`, `random`, `terminal` and `argv`.
//! - **What the environment check owns is left to it:** `process.env` and `import.meta.env`, never reported twice.
//! - **What cannot be read fails:** `eval`, `Function` (called, constructed or taken as a value), and an `import()` or
//!   a `require` of what is no literal, which the reader gives in `Source::unread`.
//!
//! What is not seen: a method on an object whose type is not known (`client.fetch()`, `store.getItem()`), a global
//! reached through a name assigned at run time (`const g = globalThis; g.fetch(x)`, `const f = fetch; f(x)` is seen
//! at `fetch`), the clocks and ids of packages (`dayjs()`, `uuid()`, `nanoid()`), and a global another file declares
//! (`declare global`) that this one uses (it is a global, and not on the list, so it fails).

use std::slice;

use crate::code::{How, Use, split_typescript_name, typescript_name};
use crate::environment;

use super::groups::Group;
use super::{Cause, Found};

/// The globals that are pure: ECMAScript's, and the few web values that read nothing from outside. `Date` and `Math`
/// are here and have a clock and a source of randomness among their names, which [`verdict`] takes first.
pub const GLOBALS_PURE: &[&str] = &[
    "Object",
    "Array",
    "Map",
    "Set",
    "WeakMap",
    "WeakSet",
    "WeakRef",
    "FinalizationRegistry",
    "Promise",
    "JSON",
    "Math",
    "Number",
    "String",
    "Boolean",
    "Symbol",
    "BigInt",
    "Error",
    "AggregateError",
    "EvalError",
    "RangeError",
    "ReferenceError",
    "SyntaxError",
    "TypeError",
    "URIError",
    "RegExp",
    "Date",
    "Intl",
    "Reflect",
    "Proxy",
    "ArrayBuffer",
    "SharedArrayBuffer",
    "DataView",
    "Int8Array",
    "Uint8Array",
    "Uint8ClampedArray",
    "Int16Array",
    "Uint16Array",
    "Int32Array",
    "Uint32Array",
    "Float16Array",
    "Float32Array",
    "Float64Array",
    "BigInt64Array",
    "BigUint64Array",
    "Atomics",
    "Iterator",
    "parseInt",
    "parseFloat",
    "isNaN",
    "isFinite",
    "encodeURI",
    "encodeURIComponent",
    "decodeURI",
    "decodeURIComponent",
    "undefined",
    "NaN",
    "Infinity",
    "globalThis",
    "structuredClone",
    "queueMicrotask",
    "setTimeout",
    "clearTimeout",
    "setInterval",
    "clearInterval",
    "AbortController",
    "AbortSignal",
    "URL",
    "URLSearchParams",
    "TextEncoder",
    "TextDecoder",
    "Blob",
    "atob",
    "btoa",
    "DOMException",
    "EventTarget",
    "Event",
    "CustomEvent",
    "Headers",
    "Request",
    "Response",
    "FormData",
];

/// The members of `crypto` that draw on randomness; the rest of it (`crypto.subtle`) is I/O of its own.
pub const CRYPTO_RANDOM: [&str; 2] = ["randomUUID", "getRandomValues"];

/// The streams of `process`: the terminal.
pub const PROCESS_TERMINAL: [&str; 3] = ["stdin", "stdout", "stderr"];

/// What a name comes to.
enum Verdict {
    /// An entry of its own, with the name used under it
    Entry(String, Option<String>),
    /// A group's act, by this name
    Group(Group, String),
    /// A module used as a value
    Module(String),
    /// What cannot be read
    Unread(String),
}

/// What the global `used` is, by the allowlist and the groups.
fn verdict(used: &Use) -> Option<Verdict> {
    let (None, parts) = split_typescript_name(&used.name) else {
        return None;
    };
    let names: Vec<&str> = parts.iter().map(String::as_str).collect();
    let root = *names.first()?;
    let first = names.get(1).copied();
    // The first `n` names, as written
    let named = |n: usize| typescript_name(None, &parts[..n.min(parts.len())]);
    let group = |group: Group, n: usize| Some(Verdict::Group(group, named(n)));
    let called = matches!(used.how, How::Called { .. });
    let constructed = matches!(used.how, How::Constructed { .. });
    match root {
        // `import.meta` is no global
        "import" => None,
        "console" => group(Group::Terminal, 2),
        "performance" => group(Group::Clock, 2),
        "Temporal" if first == Some("Now") => group(Group::Clock, 3),
        "Math" if first == Some("random") => group(Group::Random, 2),
        "crypto" if first.is_some_and(|name| CRYPTO_RANDOM.contains(&name)) => {
            group(Group::Random, 2)
        }
        "Date" if first == Some("now") => group(Group::Clock, 2),
        // `Date()` is the time now as text whatever it is given; `new Date()` the time now, and `new Date(...a)` may
        // be (`new Date(x)` is the moment `x`)
        "Date" if first.is_none() && called => Some(Verdict::Group(Group::Clock, "Date()".into())),
        "Date" if first.is_none() && matches!(used.how, How::Constructed { positional: 0, .. }) => {
            Some(Verdict::Group(Group::Clock, "new Date()".into()))
        }
        "process" => match first {
            None => Some(Verdict::Module("process".into())),
            // The environment check's
            Some("env") => None,
            Some("argv") => group(Group::Argv, 2),
            Some("hrtime") => group(Group::Clock, 2),
            Some(name) if PROCESS_TERMINAL.contains(&name) => group(Group::Terminal, 2),
            Some(_) => Some(Verdict::Entry(named(2), None)),
        },
        // What they run is unknown: calling or constructing them is what the reader says it cannot read
        "eval" | "Function" if names.len() == 1 && (called || constructed) => None,
        "eval" | "Function" => Some(Verdict::Unread(root.to_string())),
        // `require("m")` with a literal is the table's, a name it binds is judged where it is used, and one with no
        // literal is what the reader says it cannot read
        "require" if names.len() == 1 && called => None,
        _ if GLOBALS_PURE.contains(&root) => None,
        _ => Some(Verdict::Entry(
            root.to_string(),
            (names.len() > 1).then(|| named(2)),
        )),
    }
}

/// What is I/O in the use of a global, as a finding: `None` when it passes, and when the name is one the environment
/// check judges, the operand of `typeof`, or a name taken from a module.
pub fn judge(used: &Use) -> Option<Found> {
    if used.in_type
        || used.how == How::TypeOf
        || !environment::in_typescript(slice::from_ref(used)).is_empty()
    {
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
        Verdict::Unread(what) => Found {
            line,
            entry: what.clone(),
            name: None,
            cause: Cause::Unread(what),
        },
    })
}

/// What the guide says of the globals of TypeScript: the allowlist in full, the groups and what fails.
pub fn listing() -> String {
    let join = |names: &[&str]| {
        names
            .iter()
            .map(|name| format!("`{name}`"))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let mut text = String::from(
        "  What a global does only in part, and every global that is not on the list below, is judged by the name used: the \
         lists are Rotproof's, a project cannot change them, and a name taken from a module is judged by the table above \
         and not here.\n",
    );
    text.push_str(&format!(
        "  - **A global is a name no scope of the file binds** (a parameter or a variable called `fetch` is its own), \
         with `window.`, `globalThis.`, `self.` and `global.` in front of it taken away: `window.fetch` is `fetch`. Pure \
         are {} and nothing else; every other global is I/O, an entry of its own, by the global (`fetch`, `document`, \
         `localStorage`, `navigator`, `location`, `matchMedia`, `requestAnimationFrame`, `XMLHttpRequest`, `WebSocket`, \
         `Worker`, the observers, `window` itself, `Buffer`, `require`, `module` and the rest). Constructing `Request`, \
         `Response` and `Headers` is pure; `fetch` is not. `Temporal` is not on the list, so `Temporal.PlainDate` fails; \
         `Temporal.Now` is the clock.\n",
        join(GLOBALS_PURE)
    ));
    text.push_str(
        "  - **A reference counts as a call,** and a name in a type position (`let x: Date`, `type T = typeof window`) or \
         the operand of `typeof` (`typeof window !== \"undefined\"`) is never judged: `useState(Date.now)` is the clock as \
         `Date.now()` is, and `const f = fetch` is `fetch`.\n",
    );
    text.push_str(
        "  - **The clock** (`clock`): `Date.now`, `new Date()` with no argument (and `new Date(...args)`, which may be \
         none), `Date()` called, all of `performance` (`performance.now`, `performance.timeOrigin`, ...), \
         `process.hrtime` and `Temporal.Now.*`. `new Date(x)`, `Date.parse` and `Date.UTC` are pure.\n",
    );
    text.push_str(&format!(
        "  - **Randomness** (`random`): `Math.random`, and {} of `crypto`; the rest of `crypto` (`crypto.subtle`) is I/O \
         under the entry `crypto`.\n",
        join(&CRYPTO_RANDOM)
    ));
    text.push_str(&format!(
        "  - **The terminal** (`terminal`): all of `console` (`console.log`, `console.error`, ...) and {} of `process`.\n",
        PROCESS_TERMINAL
            .iter()
            .map(|name| format!("`process.{name}`"))
            .collect::<Vec<_>>()
            .join(", ")
    ));
    text.push_str("  - **The command-line arguments** (`argv`): `process.argv`.\n");
    text.push_str(
        "  - **`process`** is judged as a module is: `process.env` is the environment check's (it is not approved, and not \
         reported twice), `process.argv`, `process.hrtime` and the streams are the groups above, and every other name of \
         it (`process.cwd`, `process.platform`, `process.exit`, `process.uptime`) is I/O, an entry of its own \
         (`process.cwd`). `process` used as a value (`const p = process`, `check(process)`) fails, as what is done through \
         it cannot be read; `typeof process` does not.\n",
    );
    text.push_str(
        "  - **What cannot be read fails:** `eval` and `Function`, called, constructed (`new Function(...)`) or taken as \
         a value, and `import()` and `require` of what is no string literal (their entries are `eval`, `Function`, \
         `import()` and `require`). A reference to `require` is I/O under the entry `require`.\n",
    );
    text.push_str(
        "  - **A screen's model (`presentation.models`) is judged as the other layers are,** with no exception: the state it \
         needs from the browser (`matchMedia`, `localStorage`, `document.title`) comes from a hook in `ui.atoms` that its \
         page uses, or through a use case, and the model is given the value.\n",
    );
    text.push_str(
        "  - **Not seen:** a method on an object whose type is not known (`client.fetch()`, `store.getItem()`), a global \
         reached through a name assigned at run time (`const g = globalThis; g[name]`), `window[name]` with a name that is no \
         literal, and the clocks and ids of packages (`dayjs()`, `uuid()`, `nanoid()`). Writing the code so that the check \
         does not see it fixes nothing: the I/O is the same.\n",
    );
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::code::First;

    fn used(name: &str, how: How) -> Use {
        Use {
            line: 1,
            name: name.into(),
            how,
            in_type: false,
        }
    }

    fn called(name: &str, positional: usize) -> Use {
        used(
            name,
            How::Called {
                positional,
                keywords: Vec::new(),
                unpacked: false,
                first: First::Other,
            },
        )
    }

    fn constructed(name: &str, positional: usize, unpacked: bool) -> Use {
        used(
            name,
            How::Constructed {
                positional,
                unpacked,
            },
        )
    }

    /// The entry a name is, and the name used under it: `None` for what passes.
    fn said(used: &Use) -> Option<(String, Option<String>)> {
        judge(used).map(|found| (found.entry, found.name))
    }

    fn entry(entry: &str, name: Option<&str>) -> Option<(String, Option<String>)> {
        Some((entry.to_string(), name.map(String::from)))
    }

    fn group(group: Group, name: &str) -> Option<(String, Option<String>)> {
        Some((group.key().to_string(), Some(name.to_string())))
    }

    #[test]
    fn the_pure_globals_pass_however_they_are_used_and_every_other_global_is_io_by_its_name() {
        for name in GLOBALS_PURE {
            // `Date()` called is the clock, which its own test says; everything else of `Date` here is pure
            let hows = if *name == "Date" {
                vec![How::Value]
            } else {
                vec![
                    How::Value,
                    How::Called {
                        positional: 1,
                        keywords: Vec::new(),
                        unpacked: false,
                        first: First::Other,
                    },
                    How::Constructed {
                        positional: 1,
                        unpacked: false,
                    },
                ]
            };
            for how in hows {
                assert_eq!(said(&used(name, how)), None, "{name}");
            }
            assert_eq!(
                said(&used(&format!("{name}.x"), How::Value)),
                None,
                "{name}"
            );
        }
        for name in [
            "fetch",
            "XMLHttpRequest",
            "WebSocket",
            "EventSource",
            "document",
            "window",
            "self",
            "global",
            "localStorage",
            "sessionStorage",
            "indexedDB",
            "navigator",
            "location",
            "history",
            "screen",
            "alert",
            "confirm",
            "prompt",
            "matchMedia",
            "requestAnimationFrame",
            "Worker",
            "BroadcastChannel",
            "Notification",
            "IntersectionObserver",
            "ResizeObserver",
            "Buffer",
            "module",
            "exports",
            "__dirname",
            "Temporal",
            "WebAssembly",
        ] {
            assert_eq!(said(&used(name, How::Value)), entry(name, None), "{name}");
            assert_eq!(said(&called(name, 1)), entry(name, None), "{name}");
        }
    }

    #[test]
    fn the_entry_is_the_global_and_the_finding_names_what_is_used_of_it() {
        assert_eq!(
            said(&called("localStorage.getItem", 1)),
            entry("localStorage", Some("localStorage.getItem"))
        );
        assert_eq!(
            said(&used("document.body.classList.add", How::Value)),
            entry("document", Some("document.body"))
        );
        assert_eq!(said(&called("fetch", 1)), entry("fetch", None));
        // A property that is no identifier is written in brackets
        assert_eq!(
            said(&used("window[\"a-b\"]", How::Value)),
            entry("window", Some("window[\"a-b\"]"))
        );
        // `window.fetch` is `fetch`: the reader takes `window.` away
        assert_eq!(said(&called("fetch", 0)), entry("fetch", None));
    }

    #[test]
    fn typeof_a_type_position_a_name_of_a_module_and_import_meta_are_not_judged() {
        for name in [
            "window",
            "document.title",
            "process.argv",
            "Date.now",
            "fetch",
        ] {
            assert_eq!(said(&used(name, How::TypeOf)), None, "{name}");
            let mut in_type = used(name, How::Value);
            in_type.in_type = true;
            assert_eq!(said(&in_type), None, "{name}");
        }
        for name in [
            "\"node:fs\".readFile",
            "\"fs\"",
            "\"react\".default.useState",
            "\"node:process\".argv",
            "import.meta.url",
            "import.meta.env.VITE_A",
        ] {
            assert_eq!(said(&used(name, How::Value)), None, "{name}");
        }
    }

    #[test]
    fn process_is_judged_by_its_member_and_the_environment_is_left_to_its_check() {
        assert_eq!(
            said(&used("process.argv", How::Value)),
            group(Group::Argv, "process.argv")
        );
        assert_eq!(
            said(&called("process.hrtime.bigint", 0)),
            group(Group::Clock, "process.hrtime")
        );
        for stream in ["stdin", "stdout", "stderr"] {
            assert_eq!(
                said(&called(&format!("process.{stream}.write"), 1)),
                group(Group::Terminal, &format!("process.{stream}"))
            );
        }
        for member in ["cwd", "platform", "exit", "uptime", "chdir", "kill", "pid"] {
            assert_eq!(
                said(&called(&format!("process.{member}"), 0)),
                entry(&format!("process.{member}"), None),
                "{member}"
            );
        }
        // Never reported twice
        for name in ["process.env", "process.env.A", "process.env.get"] {
            assert_eq!(said(&used(name, How::Value)), None, "{name}");
        }
        assert_eq!(said(&called("process.env.get", 1)), None);
        // The object itself, as a value, cannot be read
        let found = judge(&used("process", How::Value)).unwrap();
        assert_eq!(
            (found.entry.as_str(), found.cause),
            ("process", Cause::Module)
        );
    }

    #[test]
    fn the_clock_is_a_call_a_construction_with_no_argument_and_a_reference() {
        for found in [
            called("Date.now", 0),
            used("Date.now", How::Value),
            used("Date.now.bind", How::Value),
        ] {
            assert_eq!(said(&found), group(Group::Clock, "Date.now"));
        }
        assert_eq!(
            said(&constructed("Date", 0, false)),
            group(Group::Clock, "new Date()")
        );
        // `new Date(...args)` may have none
        assert_eq!(
            said(&constructed("Date", 0, true)),
            group(Group::Clock, "new Date()")
        );
        // `Date()` is the time now as text, whatever it is given
        assert_eq!(said(&called("Date", 0)), group(Group::Clock, "Date()"));
        assert_eq!(said(&called("Date", 1)), group(Group::Clock, "Date()"));
        assert_eq!(
            said(&called("performance.now", 0)),
            group(Group::Clock, "performance.now")
        );
        assert_eq!(
            said(&used("performance", How::Value)),
            group(Group::Clock, "performance")
        );
        assert_eq!(
            said(&called("Temporal.Now.instant", 0)),
            group(Group::Clock, "Temporal.Now.instant")
        );
        assert_eq!(
            said(&called("Temporal.Now.plainDateISO", 0)),
            group(Group::Clock, "Temporal.Now.plainDateISO")
        );
        // A moment that is given is pure
        for pure in [
            constructed("Date", 1, false),
            constructed("Date", 3, false),
            constructed("Date", 1, true),
            called("Date.parse", 1),
            called("Date.UTC", 3),
            used("Date", How::Value),
            used("Date.prototype", How::Value),
        ] {
            assert_eq!(said(&pure), None, "{pure:?}");
        }
    }

    #[test]
    fn randomness_is_math_random_and_the_two_names_of_crypto_and_the_rest_of_crypto_is_io() {
        assert_eq!(
            said(&called("Math.random", 0)),
            group(Group::Random, "Math.random")
        );
        assert_eq!(
            said(&used("Math.random", How::Value)),
            group(Group::Random, "Math.random")
        );
        assert_eq!(
            said(&called("crypto.randomUUID", 0)),
            group(Group::Random, "crypto.randomUUID")
        );
        assert_eq!(
            said(&called("crypto.getRandomValues", 1)),
            group(Group::Random, "crypto.getRandomValues")
        );
        assert_eq!(
            said(&called("crypto.subtle.digest", 2)),
            entry("crypto", Some("crypto.subtle"))
        );
        assert_eq!(said(&called("Math.max", 2)), None);
        assert_eq!(said(&used("Math.PI", How::Value)), None);
    }

    #[test]
    fn the_terminal_is_all_of_console() {
        for name in ["console.log", "console.error", "console.table"] {
            assert_eq!(said(&called(name, 1)), group(Group::Terminal, name));
        }
        assert_eq!(
            said(&used("console", How::Value)),
            group(Group::Terminal, "console")
        );
        assert_eq!(
            said(&used("console.log", How::Value)),
            group(Group::Terminal, "console.log")
        );
    }

    #[test]
    fn what_cannot_be_read_fails_as_a_value_and_a_call_is_the_readers() {
        assert_eq!(said(&called("eval", 1)), None);
        assert_eq!(said(&constructed("Function", 1, false)), None);
        assert_eq!(said(&called("Function", 1)), None);
        assert_eq!(said(&called("require", 1)), None);
        for name in ["eval", "Function", "Function.prototype"] {
            let found = judge(&used(name, How::Value)).unwrap();
            let root = name.split('.').next().unwrap();
            assert_eq!(found.entry, root, "{name}");
            assert_eq!(found.cause, Cause::Unread(root.to_string()), "{name}");
        }
        assert_eq!(
            said(&called("eval.call", 1)).map(|(entry, _)| entry),
            Some("eval".into())
        );
        // A reference to `require`, and what it holds
        assert_eq!(said(&used("require", How::Value)), entry("require", None));
        assert_eq!(
            said(&called("require.resolve", 1)),
            entry("require", Some("require.resolve"))
        );
    }
}
