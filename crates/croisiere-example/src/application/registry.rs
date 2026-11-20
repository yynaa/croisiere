//! Separate read and write repository access within a unit of work.

use std::{
    cell::RefCell,
    rc::{Rc, Weak},
    sync::Arc,
};

use croisiere::application::{ReadRepository, UnitOfWork, UnitOfWorkFactory, WriteRepository};

use crate::application::{errors::RepositoryError, snapshots::CounterSnapshot};

/// Read access using the application's shared repository error.
pub type DynReadRepository<S> = dyn ReadRepository<S, Error = RepositoryError>;

/// Write access using the application's shared repository error.
pub type DynWriteRepository<S> = dyn WriteRepository<S, Error = RepositoryError>;

/// Creates an owned reader of committed storage, independent of the registry.
pub type ReadRepositoryFactory<S> = Box<dyn Fn() -> Box<DynReadRepository<S>>>;

/// Writer owned by the registry and borrowed weakly by pending actions.
pub type SharedWriteRepository<S> = Rc<RefCell<DynWriteRepository<S>>>;

/// Repositories available within a unit of work, one per snapshot type.
///
/// Readers are created on demand and owned by their callers. Writers stay
/// owned by the registry, which only hands out weak pointers to them, so
/// pending actions cannot extend their lifetime beyond the unit of work.
pub struct Registry {
    /// Builds readers that observe committed rows through the shared gate.
    read_counters: ReadRepositoryFactory<CounterSnapshot>,
    /// Sole strong owner of the writer used by this unit of work's actions.
    write_counters: SharedWriteRepository<CounterSnapshot>,
}

impl Registry {
    /// Called by the infrastructure when it starts a unit of work.
    pub fn new(
        read_counters: ReadRepositoryFactory<CounterSnapshot>,
        write_counters: SharedWriteRepository<CounterSnapshot>,
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
    pub fn read_counters(&self) -> Box<DynReadRepository<CounterSnapshot>> {
        (self.read_counters)()
    }

    /// Borrows the writer weakly for `make_actionable` and related helpers.
    pub fn write_counters(&self) -> Weak<RefCell<DynWriteRepository<CounterSnapshot>>> {
        Rc::downgrade(&self.write_counters)
    }
}

/// Request-scoped unit of work supplied to command and query handlers.
pub type Uow = Box<dyn UnitOfWork<Error = RepositoryError, Registry = Registry>>;

/// Shared factory used by the command and query handling routines.
pub type Factory = Arc<dyn UnitOfWorkFactory<Error = RepositoryError, Registry = Registry>>;
