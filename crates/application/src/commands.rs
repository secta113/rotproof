//! The input port of `protocol.inbound`, implemented: each command opens the project at its root through the machine
//! `composition` passes in, and runs its use case.

use std::path::Path;

use domain::approvals::{APPROVALS, Approved, Kept};
use domain::declaration::{DECLARATION, MISSING};
use domain::layers::RECORDS_ONLY;
use domain::markers::{MARKERS, either};
use domain::readers::FILE;
use domain::version::VERSION;
use protocol::inbound::{
    Commands, Indexed, Initialized, LogEntry, Logged, Made, Names, NewDocument, Pinned, Removed,
    Report, Started,
};
use protocol::outbound::{Files, Machine};

use crate::readers::Change;

/// Every command of Rotproof, on `machine`.
pub struct Rotproof<'a> {
    machine: &'a dyn Machine,
}

impl<'a> Rotproof<'a> {
    pub fn new(machine: &'a dyn Machine) -> Self {
        Rotproof { machine }
    }
}

impl Rotproof<'_> {
    /// Write the list of who reads the shared units, or take it away, and say which into `written` or `removed`.
    fn list_readers(
        &self,
        files: &dyn Files,
        written: &mut Vec<String>,
        removed: &mut Vec<String>,
    ) -> Result<(), String> {
        match crate::readers::refresh(files, self.machine.parsers(), files)? {
            Some(Change::Written) => written.push(FILE.to_string()),
            Some(Change::Removed) => removed.push(FILE.to_string()),
            None => {}
        }
        Ok(())
    }
}

impl Commands for Rotproof<'_> {
    fn names(&self) -> Names {
        Names {
            version: VERSION,
            declaration: DECLARATION,
            approvals: APPROVALS,
            records_only: RECORDS_ONLY,
            markers: either(&MARKERS),
        }
    }

    fn init(
        &self,
        root: &Path,
        stack: Option<&str>,
        ui: Option<&str>,
    ) -> Result<Initialized, String> {
        let files = self.machine.files(root);
        let mut initialized =
            crate::init::init(&*files, &*files, stack, ui, &files.name()?, VERSION)?;
        // An upgrade makes what the project lacks as `rotproof create` does, so it writes the list too, and drafts the
        // document of the units once, which reads the imports between them
        if let Initialized::Upgraded(upgraded) = &mut initialized {
            let made = &mut upgraded.made;
            self.list_readers(&*files, &mut made.written, &mut made.removed)?;
            crate::init::draft(&*files, self.machine.parsers(), &*files, upgraded)?;
            crate::init::note_map(&*files, upgraded);
        }
        Ok(initialized)
    }

    fn create(&self, root: &Path, yes: bool) -> Result<Made, String> {
        let files = self.machine.files(root);
        let mut made = crate::create::create(&*files, &*files, &files.name()?, yes)?;
        self.list_readers(&*files, &mut made.written, &mut made.removed)?;
        Ok(made)
    }

    fn check(&self, root: &Path) -> Result<Report, String> {
        let files = self.machine.files(root);
        // Without a declaration nothing can be checked against it: that is no broken rule, as a file that cannot be
        // read is none, and CI tells a project that has not adopted Rotproof from one that breaks its rules
        if crate::layers::declaration(&*files)
            .map_err(|e| e.to_string())?
            .is_none()
        {
            return Err(MISSING.into());
        }
        crate::check::check(&*files, self.machine.parsers(), self.machine.now())
            .map_err(|e| e.to_string())
    }

    fn guide(&self, root: &Path, stack: Option<&str>, ui: Option<&str>) -> Result<String, String> {
        crate::project::guide_for(&*self.machine.files(root), stack, ui)
    }

    fn index(&self, root: &Path) -> Result<Indexed, String> {
        let files = self.machine.files(root);
        let mut indexed = crate::index::index(&*files, &*files)?;
        self.list_readers(&*files, &mut indexed.written, &mut indexed.removed)?;
        Ok(indexed)
    }

    fn pending(&self, root: &Path, file: &str, import: &str) -> Result<Vec<String>, String> {
        let files = self.machine.files(root);
        let pending = crate::approvals::pending(&*files, self.machine.parsers(), file, import)?;
        Ok(pending.into_iter().map(|f| f.detail).collect())
    }

    fn user_name(&self, root: &Path) -> Option<String> {
        self.machine.user_name(root)
    }

    fn approve(
        &self,
        root: &Path,
        file: &str,
        imports: &[String],
        reason: &str,
        by: &str,
    ) -> Result<String, String> {
        let files = self.machine.files(root);
        let at = self
            .machine
            .now()
            .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        let entries: Vec<Kept> = imports
            .iter()
            .map(|import| Kept {
                from: file.to_string(),
                import: import.clone(),
                reason: reason.to_string(),
                approved: Approved {
                    by: by.to_string(),
                    at: at.clone(),
                },
            })
            .collect();
        crate::approvals::approve(&*files, &*files, &entries).map(String::from)
    }

    fn prune(&self, root: &Path) -> Result<Vec<Removed>, String> {
        let files = self.machine.files(root);
        let removed = crate::approvals::prune(&*files, self.machine.parsers(), &*files)?;
        Ok(removed
            .into_iter()
            .map(|entry| Removed {
                import: entry.import,
                from: entry.from,
            })
            .collect())
    }

    fn pin(&self, root: &Path, document: &str, keys: &[String]) -> Result<Pinned, String> {
        let files = self.machine.files(root);
        crate::follows::pin(&*files, &*files, self.machine.parsers(), document, keys)
    }

    fn start(&self, root: &Path, document: &NewDocument) -> Result<Started, String> {
        let files = self.machine.files(root);
        crate::scaffold::start(&*files, &*files, self.machine.parsers(), document)
    }

    fn log(&self, root: &Path, entry: &LogEntry) -> Result<Logged, String> {
        let files = self.machine.files(root);
        crate::log::write(&*files, &*files, self.machine.now(), entry)
    }
}
