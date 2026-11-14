//! The input port of `protocol.inbound`, implemented: each command opens the project at its root through the machine
//! `composition` passes in, and runs its use case.

use std::path::Path;

use domain::approvals::{APPROVALS, Approved, Kept};
use domain::declaration::{DECLARATION, MISSING};
use domain::layers::RECORDS_ONLY;
use domain::markers::{MARKERS, either};
use domain::upgrade::VERSION;
use protocol::inbound::{
    Commands, Drift, Indexed, Initialized, Made, Names, Pinned, Removed, Report,
};
use protocol::outbound::Machine;

/// Every command of Rotproof, on `machine`.
pub struct Rotproof<'a> {
    machine: &'a dyn Machine,
}

impl<'a> Rotproof<'a> {
    pub fn new(machine: &'a dyn Machine) -> Self {
        Rotproof { machine }
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
        crate::init::init(&*files, &*files, stack, ui, &files.name()?, VERSION)
    }

    fn create(&self, root: &Path, yes: bool) -> Result<Made, String> {
        let files = self.machine.files(root);
        crate::create::create(&*files, &*files, &files.name()?, yes)
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

    fn guide(&self, root: &Path, stack: Option<&str>) -> Result<String, String> {
        crate::project::guide_for(&*self.machine.files(root), stack)
    }

    fn index(&self, root: &Path) -> Result<Indexed, String> {
        let files = self.machine.files(root);
        crate::index::index(&*files, &*files)
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
        import: &str,
        reason: &str,
        by: &str,
    ) -> Result<String, String> {
        let files = self.machine.files(root);
        let entry = Kept {
            from: file.to_string(),
            import: import.to_string(),
            reason: reason.to_string(),
            approved: Approved {
                by: by.to_string(),
                at: self
                    .machine
                    .now()
                    .to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            },
        };
        crate::approvals::approve(&*files, &*files, &entry).map(String::from)
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

    fn changed(&self, root: &Path) -> Result<Vec<Drift>, String> {
        crate::follows::changed(&*self.machine.files(root), self.machine.parsers())
    }

    fn repin(&self, root: &Path, drifts: &[Drift]) -> Result<Vec<String>, String> {
        let files = self.machine.files(root);
        crate::follows::repin(&*files, &*files, drifts)
    }

    fn pin(&self, root: &Path, document: &str, keys: &[String]) -> Result<Pinned, String> {
        let files = self.machine.files(root);
        crate::follows::pin(&*files, &*files, self.machine.parsers(), document, keys)
    }
}
