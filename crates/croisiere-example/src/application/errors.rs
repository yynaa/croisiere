//! Application failures and the shared repository error boundary.
//!
//! Domain and repository errors retain their concrete types. Infrastructure
//! converts backend failures into `RepositoryError` without flattening them
//! into strings; command and query handlers report `AppError`.

use croisiere::application::RepositoryDropped;

use crate::domain::{DomainError, aggregates::CounterError};

/// Single error type of the application layer.
///
/// Commands and queries report domain, persistence and application failures
/// through this type. Repositories and units of work share `RepositoryError`.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    /// A counter with this name already exists.
    #[error("a counter named {0:?} already exists")]
    AlreadyExists(String),
    /// The domain refused the operation.
    #[error(transparent)]
    Domain(#[from] DomainError),
    /// Persistence failed.
    #[error(transparent)]
    Repository(#[from] RepositoryError),
    /// Event publication failed after the command committed its changes.
    #[error("event bus is closed; command changes were already committed")]
    BusClosed,
}

/// Lets `do_as_aggregate` / `do_and_make_actionable` convert aggregate errors.
impl From<CounterError> for AppError {
    fn from(error: CounterError) -> Self {
        Self::from(DomainError::from(error))
    }
}

/// Database-agnostic failures shared by repositories and units of work.
#[derive(Debug, thiserror::Error)]
pub enum RepositoryError {
    /// A lookup requiring exactly one snapshot found none.
    #[error("no snapshot matched the lookup")]
    NotFound,
    /// A lookup requiring exactly one snapshot found multiple matches.
    #[error("more than one snapshot matched the lookup")]
    NotUnique,
    /// A pending action's weak writer outlived the registry that owned it.
    #[error("the repository was dropped")]
    RepositoryDropped,
    /// Backend failure, preserving its original type and source chain.
    #[error("{0}")]
    ImplementationError(#[source] anyhow::Error),
}

impl From<RepositoryDropped> for RepositoryError {
    fn from(_: RepositoryDropped) -> Self {
        Self::RepositoryDropped
    }
}
