//! The readers of code: [`Parsers`] with Ruff for Python, oxc for TypeScript and toml_edit for a `Cargo.toml`, and
//! [`Remembering`], which reads a file once however many checks ask for it.

use std::cell::RefCell;
use std::collections::HashMap;
use std::io;

use domain::code::{Aliases, Import, Manifest, Source};
use domain::definitions::Definition;
use domain::dependencies::Listing;
use domain::layers::Language;
use protocol::outbound::Parsers;
use protocol::outbound::Tree;

/// Every reader Rotproof has.
#[derive(Debug, Default, Clone, Copy)]
pub struct Readers;

impl Parsers for Readers {
    fn python(&self, source: &str, path: &str) -> Source<Vec<String>> {
        crate::outbound::python::read(source, path)
    }

    fn outline(
        &self,
        language: Language,
        source: &str,
        path: &str,
    ) -> Result<Vec<Definition>, (usize, String)> {
        match language {
            Language::Python => Ok(crate::outbound::python::outline(source)),
            Language::TypeScript => crate::outbound::typescript::outline(source, path),
            Language::Rust => Ok(domain::rust::outline(source)),
        }
    }

    fn typescript(&self, source: &str, path: &str) -> Source<Import> {
        crate::outbound::typescript::read(source, path)
    }

    fn typescript_aliases(&self, tree: &dyn Tree, roots: &[String]) -> io::Result<Aliases> {
        crate::outbound::typescript::aliases(tree, roots)
    }

    fn manifest(&self, source: &str) -> Result<Manifest, (usize, String)> {
        crate::outbound::cargo::read(source)
    }

    fn listing(&self, language: Language, source: &str) -> Result<Listing, (usize, String)> {
        match language {
            Language::Python => crate::outbound::pyproject::read(source),
            Language::TypeScript => crate::outbound::typescript::dependencies::read(source),
            Language::Rust => crate::outbound::cargo::listing(source),
        }
    }
}

/// What was read of each path, with the text it was read from.
type Kept<I> = RefCell<HashMap<String, (String, I)>>;

/// Readers of `P` that remember what they read of a Python or TypeScript file: a check that asks again for the same
/// path and text is given what the first was, as the readers would give it, and does not parse the file again. A run
/// of `rotproof check` asks for each file once for every check that reads code (the direction, the I/O of the inner
/// layers, the dependencies, the markers, the environment), and the reading of the names costs more than the parse, so
/// without this a file is read five times or more. The text is part of the key: a file written between two askings is
/// read again.
#[derive(Debug, Default)]
pub struct Remembering<P> {
    inner: P,
    python: Kept<Source<Vec<String>>>,
    typescript: Kept<Source<Import>>,
}

impl<P> Remembering<P> {
    pub fn new(inner: P) -> Self {
        Remembering {
            inner,
            python: RefCell::default(),
            typescript: RefCell::default(),
        }
    }

    /// The readers that do the reading.
    pub fn inner(&self) -> &P {
        &self.inner
    }
}

/// What `cache` holds for `path` when it was read from `source`, else what `read` gives, kept for the next asking.
fn remembered<I: Clone>(cache: &Kept<I>, source: &str, path: &str, read: impl FnOnce() -> I) -> I {
    if let Some((text, found)) = cache.borrow().get(path)
        && text == source
    {
        return found.clone();
    }
    let found = read();
    cache
        .borrow_mut()
        .insert(path.to_string(), (source.to_string(), found.clone()));
    found
}

impl<P: Parsers> Parsers for Remembering<P> {
    fn python(&self, source: &str, path: &str) -> Source<Vec<String>> {
        remembered(&self.python, source, path, || {
            self.inner.python(source, path)
        })
    }

    fn outline(
        &self,
        language: Language,
        source: &str,
        path: &str,
    ) -> Result<Vec<Definition>, (usize, String)> {
        self.inner.outline(language, source, path)
    }

    fn typescript(&self, source: &str, path: &str) -> Source<Import> {
        remembered(&self.typescript, source, path, || {
            self.inner.typescript(source, path)
        })
    }

    fn typescript_aliases(&self, tree: &dyn Tree, roots: &[String]) -> io::Result<Aliases> {
        self.inner.typescript_aliases(tree, roots)
    }

    fn manifest(&self, source: &str) -> Result<Manifest, (usize, String)> {
        self.inner.manifest(source)
    }

    fn listing(&self, language: Language, source: &str) -> Result<Listing, (usize, String)> {
        self.inner.listing(language, source)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    /// Readers that count how many times each reader of code is asked.
    #[derive(Default)]
    struct Counting {
        python: Cell<usize>,
        typescript: Cell<usize>,
    }

    impl Parsers for Counting {
        fn python(&self, source: &str, path: &str) -> Source<Vec<String>> {
            self.python.set(self.python.get() + 1);
            Readers.python(source, path)
        }

        fn outline(
            &self,
            language: Language,
            source: &str,
            path: &str,
        ) -> Result<Vec<Definition>, (usize, String)> {
            Readers.outline(language, source, path)
        }

        fn typescript(&self, source: &str, path: &str) -> Source<Import> {
            self.typescript.set(self.typescript.get() + 1);
            Readers.typescript(source, path)
        }

        fn typescript_aliases(&self, tree: &dyn Tree, roots: &[String]) -> io::Result<Aliases> {
            Readers.typescript_aliases(tree, roots)
        }

        fn manifest(&self, source: &str) -> Result<Manifest, (usize, String)> {
            Readers.manifest(source)
        }

        fn listing(&self, language: Language, source: &str) -> Result<Listing, (usize, String)> {
            Readers.listing(language, source)
        }
    }

    #[test]
    fn a_file_is_read_once_however_many_times_it_is_asked_for_and_again_when_its_text_changes() {
        let readers = Remembering::new(Counting::default());
        let source = "import os\nx = os.getcwd()\n";
        let first = readers.python(source, "domain/a.py");
        for _ in 0..4 {
            let again = readers.python(source, "domain/a.py");
            assert_eq!(again.imports, first.imports);
            assert_eq!(again.uses, first.uses);
            assert_eq!(again.methods, first.methods);
        }
        assert_eq!(readers.inner().python.get(), 1);
        // Another file, and the same file with another text, are read
        readers.python(source, "domain/b.py");
        assert_eq!(readers.inner().python.get(), 2);
        let changed = readers.python("import sys\n", "domain/a.py");
        assert_eq!(readers.inner().python.get(), 3);
        assert_eq!(changed.imports, [(1, vec!["sys".to_string()])]);
        // The text now held is the new one
        readers.python("import sys\n", "domain/a.py");
        assert_eq!(readers.inner().python.get(), 3);

        let source = "import fs from 'fs';\nfs.readFileSync('x');\n";
        let first = readers.typescript(source, "src/a.ts");
        let again = readers.typescript(source, "src/a.ts");
        assert_eq!(readers.inner().typescript.get(), 1);
        assert_eq!(again.imports, first.imports);
        assert_eq!(again.uses, first.uses);
        readers.typescript(source, "src/b.ts");
        assert_eq!(readers.inner().typescript.get(), 2);
    }
}
