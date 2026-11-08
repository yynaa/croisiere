use croisiere::application::RepositoryDropped;

use crate::domain::aggregates::CounterError;

/// Single error type of the application layer.
///
/// Every repository and unit of work shares it, so actions on snapshots of
/// different types can be registered on the same unit of work.
#[derive(Debug)]
pub enum AppError {
    /// No counter has this name.
    NotFound(String),
    /// A counter with this name already exists.
    AlreadyExists(String),
    /// The aggregate refused the operation.
    Domain(CounterError),
    /// A stored snapshot could not be turned back into its aggregate.
    InvalidSnapshot(anyhow::Error),
    /// A pending action outlived its repository.
    RepositoryDropped,
    /// Reported by the infrastructure (connection lost, version conflict...).
    Storage(String),
}

/// Lets `do_as_aggregate` / `do_and_make_actionable` convert aggregate errors.
impl From<CounterError> for AppError {
    fn from(error: CounterError) -> Self {
        Self::Domain(error)
    }
}

/// Lets `do_as_aggregate` / `do_and_make_actionable` convert snapshot
/// conversion errors (`CounterSnapshot`'s `TryFrom` error).
impl From<anyhow::Error> for AppError {
    fn from(error: anyhow::Error) -> Self {
        Self::InvalidSnapshot(error)
    }
}

/// Required to register an `ActionableSnapshot` on a unit of work.
impl From<RepositoryDropped> for AppError {
    fn from(_: RepositoryDropped) -> Self {
        Self::RepositoryDropped
    }
}
