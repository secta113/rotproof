//! Connections to the outside: the file system, git, the parsers of code and the clock. Each implements an output port
//! of `protocol.outbound`.
//!
//! May import `protocol.outbound` and `domain`.

pub mod cargo;
pub mod disk;
pub mod git;
pub mod python;
pub mod readers;
pub mod typescript;

use std::path::{Path, PathBuf};

use domain::schema::Time;
use protocol::outbound::{Changes, Files, Machine, Parsers};

use crate::outbound::disk::{Disk, project_root};
use crate::outbound::git::Git;
use crate::outbound::readers::Readers;

/// The machine Rotproof runs on: the disk, git, Ruff, oxc and toml_edit, and the local clock.
pub struct Local;

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
        &Readers
    }

    fn user_name(&self, root: &Path) -> Option<String> {
        Git::new(root).user_name()
    }

    fn changes(&self, root: &Path) -> Option<Box<dyn Changes>> {
        // git runs only when the hook's decision asks for it, so a root outside a repository is not an error here
        project_root(root).map(|root| Box::new(Git::new(&root)) as Box<dyn Changes>)
    }

    fn project(&self, start: &Path) -> Option<PathBuf> {
        project_root(start)
    }

    fn now(&self) -> Time {
        chrono::Local::now().fixed_offset()
    }
}
