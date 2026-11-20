//! Shared database and commit-gate handles for in-memory repositories.

use std::sync::{Arc, Mutex, Weak};

use super::{InMemoryDatabase, InMemoryRepositoryError};

/// Cloneable connection shared by DTO operations and repositories.
///
/// Construct it from the handles supplied by
/// [`InMemoryUnitOfWorkFactory::from_registry_factory`](super::InMemoryUnitOfWorkFactory::from_registry_factory).
/// Cloning shares both handles without keeping the database alive.
#[derive(Clone)]
#[cfg_attr(docsrs, doc(cfg(feature = "in-memory")))]
pub struct InMemoryConnection {
    /// Weak database handle; upgrade and lock it before accessing rows.
    pub database: Weak<Mutex<InMemoryDatabase>>,
    /// Gate shared with the factory's commits and rollback.
    ///
    /// Select operations acquire this gate before locking the database, so
    /// they cannot observe a partial commit. DTO writes performed during a
    /// unit-of-work commit must not acquire it again: the commit holds it.
    pub gate: Arc<Mutex<()>>,
}

impl InMemoryConnection {
    /// Upgrades the database handle, locks it and runs one operation.
    ///
    /// Access failures convert into the caller's error type through `From`;
    /// errors returned by the operation propagate unchanged.
    ///
    /// This locks only the database mutex. Reads must acquire the commit gate
    /// first; writes during a unit-of-work commit use the gate already held by
    /// that commit. Do not call this recursively on the same database.
    /// Mutations are not rolled back if the operation returns an error; rollback
    /// is the responsibility of the enclosing unit of work.
    pub fn with_database<T, E>(
        &self,
        f: impl FnOnce(&mut InMemoryDatabase) -> Result<T, E>,
    ) -> Result<T, E>
    where
        E: From<InMemoryRepositoryError>,
    {
        let database = self
            .database
            .upgrade()
            .ok_or(InMemoryRepositoryError::DatabaseDropped)?;
        let mut database = database
            .lock()
            .map_err(|_| InMemoryRepositoryError::DatabasePoisoned)?;
        f(&mut database)
    }
}
