//! Separate read and write repository access within a unit of work.

use std::{rc::Rc, sync::Arc};

use croisiere::application::{
    ReadRepositoryBox, ReadRepositoryFactory, SharedWriteRepository, UnitOfWork, UnitOfWorkFactory,
    WriteRepositoryWeak,
};

use crate::application::{errors::RepositoryError, snapshots::CounterSnapshot};

/// Repositories available within a unit of work, one per snapshot type.
///
/// Readers are created on demand and owned by their callers. Writers stay
/// owned by the registry, which only hands out weak pointers to them, so
/// pending actions cannot extend their lifetime beyond the unit of work.
pub struct Registry {
    /// Builds readers that observe committed rows through the shared gate.
    read_counters: ReadRepositoryFactory<CounterSnapshot, RepositoryError>,
    /// Sole strong owner of the writer used by this unit of work's actions.
    write_counters: SharedWriteRepository<CounterSnapshot, RepositoryError>,
}

impl Registry {
    /// Called by the infrastructure when it starts a unit of work.
    pub fn new(
        read_counters: ReadRepositoryFactory<CounterSnapshot, RepositoryError>,
        write_counters: SharedWriteRepository<CounterSnapshot, RepositoryError>,
    ) -> Self {
        Self {
            read_counters,
            write_counters,
        }
    }

    /// Creates a reader that can outlive this unit of work.
    ///
    /// Reads see committed storage, not this unit of work's pending actions.
    /// The backing factory must remain alive for database access to succeed.
    pub fn read_counters(&self) -> ReadRepositoryBox<CounterSnapshot, RepositoryError> {
        (self.read_counters)()
    }

    /// Borrows the writer weakly for `make_actionable` and related helpers.
    pub fn write_counters(&self) -> WriteRepositoryWeak<CounterSnapshot, RepositoryError> {
        Rc::downgrade(&self.write_counters)
    }
}

/// Request-scoped unit of work supplied to command and query handlers.
pub type Uow = Box<dyn UnitOfWork<Error = RepositoryError, Registry = Registry>>;

/// Shared factory used by the command and query handling routines.
pub type Factory = Arc<dyn UnitOfWorkFactory<Error = RepositoryError, Registry = Registry>>;
