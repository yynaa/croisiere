//! DTO persistence using the shared in-memory backend. Data lasts for the
//! lifetime of the factory. Commits and reads share a gate so readers cannot
//! observe partial commits and rollback cannot overwrite another commit.
//!
//! The library provides `InMemoryConnection` and its database-access helper.
//! This module names the backend error locally and exports factory construction;
//! child modules define table DTOs, repository adapters and registry wiring.

use croisiere::infrastructure::persistence::in_memory::{
    InMemoryConnection as Connection, InMemoryRepositoryError,
};

mod dtos;
mod repositories;
mod units_of_work;

pub use units_of_work::{CounterFactory, counter_factory};

/// Typed backend failure converted to the application's repository error.
type StorageError = InMemoryRepositoryError;
