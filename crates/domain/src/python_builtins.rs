//! The names Python binds without being asked, the builtins: what `builtins.<name>` is, for a name no scope of a file
//! binds (`open`, `print`, `input`, `getattr`, `__import__`).
//!
//! The list is the union of the names of `dir(builtins)` of Python 3.10 to 3.14, as `python_stdlib` is for modules: a
//! name a version dropped or one a later version brought (`aiter`, `ExceptionGroup`, `PythonFinalizationError`) is a
//! name a project may meet on any version it runs. `exit`, `quit`, `copyright`, `credits`, `license` and `help` are
//! there: `site` adds them when Python starts, and a program reaches them as the builtins. The module's own names
//! (`__name__`, `__doc__`, `__file__`) are no builtins and are not in it. It is a constant, not read from the Python
//! that runs the check, since the answer must be the same on every machine.

pub const BUILTINS: [&str; 151] = [
    "ArithmeticError",
    "AssertionError",
    "AttributeError",
    "BaseException",
    "BaseExceptionGroup",
    "BlockingIOError",
    "BrokenPipeError",
    "BufferError",
    "BytesWarning",
    "ChildProcessError",
    "ConnectionAbortedError",
    "ConnectionError",
    "ConnectionRefusedError",
    "ConnectionResetError",
    "DeprecationWarning",
    "EOFError",
    "Ellipsis",
    "EncodingWarning",
    "EnvironmentError",
    "Exception",
    "ExceptionGroup",
    "FileExistsError",
    "FileNotFoundError",
    "FloatingPointError",
    "FutureWarning",
    "GeneratorExit",
    "IOError",
    "ImportError",
    "ImportWarning",
    "IndentationError",
    "IndexError",
    "InterruptedError",
    "IsADirectoryError",
    "KeyError",
    "KeyboardInterrupt",
    "LookupError",
    "MemoryError",
    "ModuleNotFoundError",
    "NameError",
    "NotADirectoryError",
    "NotImplemented",
    "NotImplementedError",
    "OSError",
    "OverflowError",
    "PendingDeprecationWarning",
    "PermissionError",
    "ProcessLookupError",
    "PythonFinalizationError",
    "RecursionError",
    "ReferenceError",
    "ResourceWarning",
    "RuntimeError",
    "RuntimeWarning",
    "StopAsyncIteration",
    "StopIteration",
    "SyntaxError",
    "SyntaxWarning",
    "SystemError",
    "SystemExit",
    "TabError",
    "TimeoutError",
    "TypeError",
    "UnboundLocalError",
    "UnicodeDecodeError",
    "UnicodeEncodeError",
    "UnicodeError",
    "UnicodeTranslateError",
    "UnicodeWarning",
    "UserWarning",
    "ValueError",
    "Warning",
    "WindowsError",
    "ZeroDivisionError",
    "__build_class__",
    "__debug__",
    "__import__",
    "abs",
    "aiter",
    "all",
    "anext",
    "any",
    "ascii",
    "bin",
    "bool",
    "breakpoint",
    "bytearray",
    "bytes",
    "callable",
    "chr",
    "classmethod",
    "compile",
    "complex",
    "copyright",
    "credits",
    "delattr",
    "dict",
    "dir",
    "divmod",
    "enumerate",
    "eval",
    "exec",
    "exit",
    "filter",
    "float",
    "format",
    "frozenset",
    "getattr",
    "globals",
    "hasattr",
    "hash",
    "help",
    "hex",
    "id",
    "input",
    "int",
    "isinstance",
    "issubclass",
    "iter",
    "len",
    "license",
    "list",
    "locals",
    "map",
    "max",
    "memoryview",
    "min",
    "next",
    "object",
    "oct",
    "open",
    "ord",
    "pow",
    "print",
    "property",
    "quit",
    "range",
    "repr",
    "reversed",
    "round",
    "set",
    "setattr",
    "slice",
    "sorted",
    "staticmethod",
    "str",
    "sum",
    "super",
    "tuple",
    "type",
    "vars",
    "zip",
];

/// Whether `name` is a builtin of Python in any version from 3.10 to 3.14.
pub fn is_builtin(name: &str) -> bool {
    BUILTINS.binary_search(&name).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_names_are_in_byte_order_without_repeats_so_that_a_search_finds_each() {
        assert!(BUILTINS.windows(2).all(|pair| pair[0] < pair[1]));
        for name in BUILTINS {
            assert!(is_builtin(name), "{name}");
        }
    }

    #[test]
    fn a_name_of_any_version_from_3_10_to_3_14_is() {
        for name in [
            "open",
            "print",
            "input",
            "getattr",
            "eval",
            "exec",
            "__import__",
            "len",
            "ValueError",
            "OSError",
            "WindowsError",
            // Added in 3.10, 3.11 and 3.13
            "aiter",
            "anext",
            "EncodingWarning",
            "ExceptionGroup",
            "BaseExceptionGroup",
            "PythonFinalizationError",
        ] {
            assert!(is_builtin(name), "{name}");
        }
    }

    #[test]
    fn a_name_that_is_no_builtin_is_not() {
        // The module's own names, a name of another case, a dotted name and a name of the standard library
        for name in [
            "__name__",
            "__file__",
            "__doc__",
            "Open",
            "os",
            "builtins.open",
            "None",
            "self",
            "",
        ] {
            assert!(!is_builtin(name), "{name}");
        }
    }
}
