//! Connections to the outside: the file system, git, the parsers of code and the clock. Each implements an output port
//! of `protocol.outbound`.
//!
//! May import `protocol.outbound` and `domain`.

pub mod cargo;
pub mod disk;
pub mod git;
pub mod pyproject;
pub mod python;
pub mod readers;
pub mod typescript;

use std::path::Path;

use domain::schema::Time;
use protocol::outbound::{Files, Machine, Parsers};

use crate::outbound::disk::Disk;
use crate::outbound::git::Git;
use crate::outbound::readers::{Readers, Remembering};

/// The machine Rotproof runs on: the disk, git, Ruff, oxc and toml_edit (which remember each file they read, so that
/// one run reads it once), and the local clock.
#[derive(Default)]
pub struct Local {
    readers: Remembering<Readers>,
}

impl Files for Disk {
    fn name(&self) -> Result<String, String> {
        Disk::name(self)
    }
}

impl Machine for Local {
    fn files(&self, root: &Path) -> Box<dyn Files> {
        Box::new(Disk::new(root))
    }

    fn parsers(&self) -> &dyn Parsers {
        &self.readers
    }

    fn user_name(&self, root: &Path) -> Option<String> {
        Git::new(root).user_name()
    }

    fn now(&self) -> Time {
        chrono::Local::now().fixed_offset()
    }
}
