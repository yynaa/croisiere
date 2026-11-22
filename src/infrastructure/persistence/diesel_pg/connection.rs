//! Pool and commit-gate handles for PostgreSQL repositories.

use std::{
    fmt,
    sync::{Arc, Mutex, Weak},
};

use diesel::r2d2::Pool;

use super::{PostgresConnectionManager, PostgresRepositoryError, PostgresSlot};

/// Cloneable connection handle shared by DTO operations and repositories.
///
/// Construct it from the pool, slot and gate supplied by
/// [`PostgresUnitOfWorkFactory::from_registry_factory`](super::PostgresUnitOfWorkFactory::from_registry_factory).
/// Cloning shares all three handles; the pool keeps working after the unit of
/// work that created this handle is dropped.
#[derive(Clone)]
#[cfg_attr(docsrs, doc(cfg(feature = "diesel-pg")))]
pub struct PostgresConnection {
    /// Pool providing fresh connections for reads.
    pub pool: Pool<PostgresConnectionManager>,
    /// Weak slot of the unit of work's connection; upgrade it for writes.
    ///
    /// Upgrading fails once the unit of work is dropped and its connection
    /// returned to the pool. DTO writes performed during a unit-of-work commit
    /// run on this connection, inside the transaction that commit opened.
    pub writer: Weak<PostgresSlot>,
    /// Gate shared with the factory's commits and rollback.
    pub gate: Arc<Mutex<()>>,
}

impl fmt::Debug for PostgresConnection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PostgresConnection")
            .finish_non_exhaustive()
    }
}

impl PostgresConnection {
    /// Runs one operation on the unit of work's connection.
    ///
    /// Access failures convert into the caller's error type through `From`;
    /// errors returned by the operation propagate unchanged.
    ///
    /// This locks only the slot's mutex. Do not acquire the commit gate
    /// inside the operation: during a unit-of-work commit, that commit
    /// already holds the gate and the operation runs inside its transaction.
    /// Mutations outside a unit-of-work commit are not rolled back if the
    /// operation returns an error; rollback inside a commit is the
    /// responsibility of the enclosing unit of work.
    pub fn with_connection<T, E>(
        &self,
        f: impl FnOnce(&mut diesel::pg::PgConnection) -> Result<T, E>,
    ) -> Result<T, E>
    where
        E: From<PostgresRepositoryError>,
    {
        let slot = self
            .writer
            .upgrade()
            .ok_or(PostgresRepositoryError::ConnectionDropped)?;
        let mut slot = slot
            .lock()
            .map_err(|_| PostgresRepositoryError::ConnectionPoisoned)?;
        let connection = slot
            .as_mut()
            .ok_or(PostgresRepositoryError::ConnectionDropped)?;
        f(&mut **connection)
    }

    /// Checks a fresh connection out of the pool and runs one read on it.
    ///
    /// The commit gate is held while the connection is checked out, so the
    /// read cannot observe a partial commit and cannot collide with an open
    /// write transaction. Do not call this during a unit-of-work commit: that
    /// commit holds the gate and the read would deadlock. Reads outliving
    /// their unit of work keep working as long as the factory's pool does.
    pub fn with_read_connection<T, E>(
        &self,
        f: impl FnOnce(&mut diesel::pg::PgConnection) -> Result<T, E>,
    ) -> Result<T, E>
    where
        E: From<PostgresRepositoryError>,
    {
        let _guard = self
            .gate
            .lock()
            .map_err(|_| PostgresRepositoryError::ConnectionPoisoned)?;
        let mut connection = self.pool.get().map_err(PostgresRepositoryError::from)?;
        f(&mut connection)
    }
}
