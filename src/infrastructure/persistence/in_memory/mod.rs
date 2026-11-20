//! In-memory storage backend, enabled by the `in-memory` feature.
//!
//! Tables contain type-erased rows. [`InMemoryDTO`] supplies a blanket
//! [`DTO`] implementation using a weak pointer to a mutex-protected database.
//! Selection and write operations still need DTO-specific implementations.
//!
//! The `in-memory` feature is enabled by default. Units of work restore a
//! database snapshot on action errors; callers must serialize writes during
//! commit to prevent rollback from overwriting concurrent changes.

use std::{
    fmt::Debug,
    sync::{Mutex, Weak},
};

use thiserror::Error;

mod connection;
mod database;
mod unit_of_work;

pub use connection::InMemoryConnection;
pub use database::InMemoryDatabase;
pub use unit_of_work::InMemoryUnitOfWork;
pub use unit_of_work::InMemoryUnitOfWorkFactory;

use crate::infrastructure::persistence::DTO;

/// Failure to access the database, find a table or interpret a stored row.
#[derive(Debug, Error)]
#[cfg_attr(docsrs, doc(cfg(feature = "in-memory")))]
pub enum InMemoryRepositoryError {
    /// An application action failed; the unit of work rolls back its writes.
    #[error("action failed: {0}")]
    ActionFailed(String),

    /// The weak database pointer can no longer be upgraded.
    #[error("the in-memory database was dropped")]
    DatabaseDropped,

    /// A previous panic poisoned the database mutex.
    #[error("the in-memory database mutex is poisoned")]
    DatabasePoisoned,

    /// The requested table has never been created by an insertion.
    #[error("table not found: {0}")]
    TableNotFound(String),

    /// A row's concrete type differs from the operation's requested type.
    #[error("could not downcast from Any")]
    DowncastFromAnyImpossible,
}

/// Marker supplying [`DTO`] metadata for this backend.
///
/// Implementors receive `ConnectionData = Weak<Mutex<InMemoryDatabase>>` and
/// `Error = InMemoryRepositoryError`. This does not implement DTO operations;
/// those must upgrade the weak pointer and lock the database themselves.
#[cfg_attr(docsrs, doc(cfg(feature = "in-memory")))]
pub trait InMemoryDTO: Debug + Clone {
    /// Storage representation name, forwarded to [`DTO::NAME`].
    const NAME: &'static str;
}

impl<D: InMemoryDTO> DTO for D {
    const NAME: &'static str = D::NAME;

    type ConnectionData = Weak<Mutex<InMemoryDatabase>>;
    type Error = InMemoryRepositoryError;
}
