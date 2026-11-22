//! SQLite storage backend over Diesel, enabled by the `diesel-sqlite` feature.
//!
//! The factory owns a [`r2d2::Pool`] and a commit gate. Every unit of work
//! checks out exactly one connection from the pool and keeps it for its whole
//! lifetime: a commit opens one transaction on that connection, so an action
//! error rolls the whole batch back, and the connection returns to the pool
//! when the unit of work is dropped.
//!
//! [`SqliteDTO`] supplies a blanket [`DTO`] implementation with the backend's
//! [`SqliteConnection`] handle. Selection runs on a fresh pooled connection,
//! so readers see committed storage and can outlive their unit of work;
//! writes run on the unit of work's connection, inside the open transaction
//! during a commit. Selection and write operations still need DTO-specific
//! implementations.
//!
//! Units of work from one factory serialize their commits through the shared
//! gate: two open write transactions would contend for SQLite's write lock.
//! Callers must serialize writes during commit, as for the in-memory backend.

use std::sync::Mutex;

use diesel::r2d2::ConnectionManager;
use thiserror::Error;

mod connection;
mod unit_of_work;

pub use connection::SqliteConnection;
pub use unit_of_work::SqliteUnitOfWork;
pub use unit_of_work::SqliteUnitOfWorkFactory;

use crate::infrastructure::persistence::BackendDTO;

/// Failure to access a connection or to execute a Diesel query.
#[derive(Debug, Error)]
#[cfg_attr(docsrs, doc(cfg(feature = "diesel-sqlite")))]
pub enum SqliteRepositoryError {
    /// An application action failed; the unit of work rolls back its writes.
    #[error("action failed: {0}")]
    ActionFailed(String),

    /// The weak slot pointer can no longer be upgraded, or the unit of work
    /// released its connection before the operation ran.
    #[error("the unit of work's connection is no longer available")]
    ConnectionDropped,

    /// A previous panic poisoned a connection mutex.
    #[error("a connection mutex is poisoned")]
    ConnectionPoisoned,

    /// The pool could not provide a connection within its timeout.
    #[error(transparent)]
    Pool(#[from] r2d2::Error),

    /// A Diesel query or transaction operation failed.
    #[error(transparent)]
    Diesel(#[from] diesel::result::Error),
}

/// Connection manager of the factory's pool.
#[cfg_attr(docsrs, doc(cfg(feature = "diesel-sqlite")))]
pub type SqliteConnectionManager = ConnectionManager<diesel::sqlite::SqliteConnection>;

/// A pooled SQLite connection checked out by a unit of work or a read.
///
/// Dropping it returns the connection to the pool it came from.
#[cfg_attr(docsrs, doc(cfg(feature = "diesel-sqlite")))]
pub type SqlitePooledConnection = r2d2::PooledConnection<SqliteConnectionManager>;

/// Shared slot parking a unit of work's checked-out connection.
///
/// The unit of work parks its connection here from construction until it is
/// dropped, which releases it back to the pool. DTO operations upgrade a weak
/// pointer to this slot and lock it; `None` means the connection was released.
#[cfg_attr(docsrs, doc(cfg(feature = "diesel-sqlite")))]
pub type SqliteSlot = Mutex<Option<SqlitePooledConnection>>;

/// Marker supplying backend metadata for this backend.
///
/// Implementing this marker with [`BackendDTO`] supplies a blanket [`DTO`]
/// implementation with `ConnectionData = SqliteConnection` and
/// `Error = SqliteRepositoryError`. This does not implement DTO operations;
/// those must route reads through [`SqliteConnection::with_read_connection`]
/// and writes through [`SqliteConnection::with_connection`] themselves.
#[cfg_attr(docsrs, doc(cfg(feature = "diesel-sqlite")))]
pub trait SqliteDTO:
    BackendDTO<ConnectionData = SqliteConnection, Error = SqliteRepositoryError>
{
}
