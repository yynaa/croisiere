use std::{
    cell::RefCell,
    rc::{Rc, Weak},
    sync::Arc,
};

use croisiere::application::{ReadRepository, UnitOfWorkFactory, WriteRepository};

use crate::application::{errors::AppError, snapshots::CounterSnapshot};

/// Read and write access to counters, as one trait object.
pub trait CounterRepository:
    ReadRepository<CounterSnapshot, Error = AppError> + WriteRepository<CounterSnapshot, Error = AppError>
{
}

impl<R> CounterRepository for R where
    R: ReadRepository<CounterSnapshot, Error = AppError>
        + WriteRepository<CounterSnapshot, Error = AppError>
{
}

/// Repositories available within a unit of work, one per snapshot type.
///
/// The registry is the only owner of its repositories: it hands out weak
/// pointers only, so pending actions (and anything else) never keep a
/// repository alive past its unit of work.
pub struct Registry {
    counters: Rc<RefCell<dyn CounterRepository>>,
}

impl Registry {
    /// Called by the infrastructure when it starts a unit of work.
    pub fn new(counters: Rc<RefCell<dyn CounterRepository>>) -> Self {
        Self { counters }
    }

    /// The counter repository, e.g. for `make_actionable`, or to read from
    /// with [`upgrade`].
    pub fn counters(&self) -> Weak<RefCell<dyn CounterRepository>> {
        Rc::downgrade(&self.counters)
    }
}

/// Upgrades a repository pointer for a single call, failing with
/// [`AppError::RepositoryDropped`] if its unit of work is gone.
pub fn upgrade<T: ?Sized>(repository: &Weak<T>) -> Result<Rc<T>, AppError> {
    repository.upgrade().ok_or(AppError::RepositoryDropped)
}

/// What commands and queries receive: the infrastructure provides the
/// implementation.
pub type Factory = Arc<dyn UnitOfWorkFactory<Error = AppError, Registry = Registry>>;
