//! Public error boundary for the counter domain.

use super::{aggregates::CounterError, value_objects::CounterNameError};

/// All domain failures, preserving their specific error types and messages.
///
/// Individual domain operations keep their focused error types. Callers can
/// convert them into this enum when crossing the domain boundary.
#[derive(Debug, thiserror::Error)]
pub enum DomainError {
    /// A counter operation failed.
    #[error(transparent)]
    Counter(#[from] CounterError),
    /// Constructing a counter name failed.
    #[error(transparent)]
    CounterName(#[from] CounterNameError),
}
