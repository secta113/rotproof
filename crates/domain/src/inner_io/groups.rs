//! What the I/O check of every language shares beyond the table of what does I/O wholly: the groups of one act, what a
//! finding says to do about each, and the exclusion of the method net.
//!
//! - **A group is one act that more than one name does** (the clock, randomness, the terminal, the command-line
//!   arguments), and it is one entry of the approvals file, whichever names the file reaches it by: `clock` approves
//!   `datetime.datetime.now` and `time.time` of one file together, and the approval prints the names the file uses
//!   under it now. A name of a module judged by an allowlist that is in no group (`os.getcwd`, `sys.platform`,
//!   `logging.basicConfig`) is an entry of its own.
//! - **What a finding of a group says to do** is the same in every language, with the example of the language: the
//!   value comes in as an argument, or from a port in `protocol.outbound` that the use case is given, and a port may be
//!   a callable alias.
//! - **The method net's exclusion** is the one the specification gives for the net of a language: a method name is left
//!   out of the net when the project itself defines a method of the same name and the same number of parameters,
//!   anywhere in the project, and every name left out is printed on every run.

use std::collections::BTreeSet;

use crate::layers::Language;

/// One act that several names do, and one entry of the approvals file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Group {
    /// The time now: `datetime.datetime.now`, `time.time`, `Date.now`, `SystemTime::now`
    Clock,
    /// Randomness and the ids made of it: `random.choice`, `uuid.uuid4`, `os.urandom`, `Math.random`
    Random,
    /// The terminal, and a file opened by a builtin: `print`, `input`, `open`, `sys.stdout`, `console.log`
    Terminal,
    /// The command-line arguments: `sys.argv`, `process.argv`, `std::env::args`
    Argv,
}

impl Group {
    /// The key of the group in the approvals file.
    pub fn key(self) -> &'static str {
        match self {
            Group::Clock => "clock",
            Group::Random => "random",
            Group::Terminal => "terminal",
            Group::Argv => "argv",
        }
    }

    /// What the code does, as a finding says it.
    pub fn act(self) -> &'static str {
        match self {
            Group::Clock => "reads the clock",
            Group::Random => "draws on randomness",
            Group::Terminal => "uses the terminal or opens a file",
            Group::Argv => "reads the command-line arguments",
        }
    }

    /// What the value is called where a screen's model is told where to get it.
    fn value(self) -> &'static str {
        match self {
            Group::Clock => "the time now",
            Group::Random => "the id or the random value",
            Group::Terminal => "what it shows or reads",
            Group::Argv => "the value of the argument",
        }
    }

    /// What to do about it in `layer` of a project in `language`: the first thing a finding of the group says.
    pub fn remedy(self, layer: &str, language: Language) -> String {
        if layer == "presentation" {
            let hook = match language {
                Language::TypeScript => ", through a hook in `ui.atoms`",
                Language::Python | Language::Rust => "",
            };
            return format!(
                "a screen's model does no I/O: it gets {} from its page{hook}, or through a use case of \
                 protocol.inbound it is given",
                self.value()
            );
        }
        let what = match self {
            Group::Clock => "pass the time now in as an argument",
            Group::Random => "pass the id or the random value in as an argument",
            Group::Terminal => {
                "return what is to be shown, and read what is typed in infrastructure"
            }
            Group::Argv => {
                "read the arguments in infrastructure and pass the value in as an argument"
            }
        };
        let port = match self {
            Group::Argv => String::new(),
            Group::Terminal => format!(
                ", or give it to a port in protocol.outbound that the use case is given (a port may be a callable \
                 alias, {})",
                self.example(language)
            ),
            Group::Clock | Group::Random => format!(
                ", or take it from a port in protocol.outbound that the use case is given (a port may be a callable \
                 alias, {})",
                self.example(language)
            ),
        };
        format!("{what}{port}")
    }

    /// A port written as a callable alias, in the language of the project.
    pub fn example(self, language: Language) -> &'static str {
        match (language, self) {
            (Language::Python, Group::Clock) => "`Clock = Callable[[], datetime]`",
            (Language::Python, Group::Random) => "`NewId = Callable[[], UUID]`",
            (Language::Python, _) => "`Say = Callable[[str], None]`",
            (Language::TypeScript, Group::Clock) => "`() => Date`",
            (Language::TypeScript, Group::Random) => "`() => string`",
            (Language::TypeScript, _) => "`(text: string) => void`",
            (Language::Rust, Group::Clock) => "`fn() -> SystemTime`",
            (Language::Rust, Group::Random) => "`fn() -> u64`",
            (Language::Rust, _) => "`fn(&str)`",
        }
    }
}

/// What the guide says of a value that the clock, randomness, the terminal or the command line gives, for a project in
/// `language`: it is passed in.
pub fn passed_in(language: Language) -> String {
    let hook = match language {
        Language::TypeScript => ", through a hook in `ui.atoms`",
        Language::Python | Language::Rust => "",
    };
    format!(
        " **A value that the clock, randomness, the terminal or the command line gives is passed in:** as an argument \
         (the time now, the id, the value), or from a port, which may be a callable alias ({}); a screen's model gets it \
         from its page{hook}, or through a use case it is given.",
        Group::Clock.example(language)
    )
}

/// The entries of a TypeScript project's approvals file besides the table's: a global is an entry by its name, and
/// `process` by its member.
pub const KEYS_TYPESCRIPT: &str = ", and `fetch`, `localStorage` or `process.cwd` for a global judged by its name, `eval` \
                                  for what cannot be read, and `clock`, `random`, `terminal` or `argv` for an act more \
                                  than one name does";

/// The entries of the approvals file besides the table's, as the guide lists them after the table's.
pub const KEYS: &str = ", and `os.getcwd` or `sys.platform` for a name of a module judged by its names, `read_text` for \
                       a method judged by its name, `eval` for what cannot be read, and `clock`, `random`, `terminal` \
                       or `argv` for an act more than one name does";

/// The methods of the project that the net of method names leaves out, each with the number of parameters: a method
/// the project defines itself is not the standard library's.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Excluded(BTreeSet<(String, usize)>);

impl Excluded {
    /// The methods the net leaves out: each name with the number of parameters (not counting `self`) of a method the
    /// project defines, for the calls of that many arguments that the net would judge.
    pub fn new(methods: Vec<(String, usize)>) -> Self {
        Excluded(methods.into_iter().collect())
    }

    /// Whether a call of `name` with `arguments` arguments is a call of a method the project defines.
    pub fn contains(&self, name: &str, arguments: usize) -> bool {
        self.0.contains(&(name.to_string(), arguments))
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The line `rotproof check` prints on every run when the project defines a method the net leaves out, which is the
    /// only place the exclusion is seen: `None` when it leaves out nothing.
    pub fn line(&self) -> Option<String> {
        if self.0.is_empty() {
            return None;
        }
        let names: Vec<String> = self
            .0
            .iter()
            .map(|(name, parameters)| format!("{name}({parameters})"))
            .collect();
        Some(format!(
            "I/O net: left out, as the project defines a method of the same name and number of parameters: {}",
            names.join(", ")
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_group_has_its_own_key_and_says_what_to_do_in_the_language_of_the_project() {
        let keys: Vec<&str> = [Group::Clock, Group::Random, Group::Terminal, Group::Argv]
            .iter()
            .map(|group| group.key())
            .collect();
        assert_eq!(keys, ["clock", "random", "terminal", "argv"]);
        assert!(
            Group::Clock
                .remedy("domain", Language::Python)
                .contains("`Clock = Callable[[], datetime]`")
        );
        assert!(
            Group::Clock
                .remedy("application", Language::TypeScript)
                .contains("`() => Date`")
        );
        assert!(Group::Clock.remedy("protocol", Language::Python).starts_with(
            "pass the time now in as an argument, or take it from a port in protocol.outbound"
        ));
        // The arguments are read where the program starts: no port
        assert!(
            !Group::Argv
                .remedy("domain", Language::Python)
                .contains("port")
        );
    }

    #[test]
    fn a_screens_model_is_told_to_get_the_value_from_its_page() {
        let said = Group::Random.remedy("presentation", Language::Python);
        assert!(
            said.starts_with(
                "a screen's model does no I/O: it gets the id or the random value from its page, or"
            ),
            "{said}"
        );
        assert!(!said.contains("hook"));
        assert!(
            Group::Clock
                .remedy("presentation", Language::TypeScript)
                .contains(
                    "from its page, through a hook in `ui.atoms`, or through a use case of protocol.inbound"
                )
        );
    }

    #[test]
    fn the_exclusion_is_by_the_name_and_the_number_of_parameters_and_is_printed() {
        let excluded = Excluded::new(vec![
            ("exists".to_string(), 0),
            ("read_text".to_string(), 2),
        ]);
        assert!(excluded.contains("exists", 0));
        assert!(!excluded.contains("exists", 1));
        assert!(excluded.contains("read_text", 2));
        assert!(!excluded.contains("read_text", 1));
        assert_eq!(
            excluded.line().as_deref(),
            Some(
                "I/O net: left out, as the project defines a method of the same name and number of parameters: \
                 exists(0), read_text(2)"
            )
        );
        assert_eq!(Excluded::default().line(), None);
    }
}
