//! DTO-based adapters for application repositories.
//!
//! Implement [`DTO`] and its operation traits for a storage representation,
//! then [`DTORepository`] and [`DTOReadRepository`] or [`DTOWriteRepository`]
//! for the repository holding access to that storage. Blanket implementations
//! provide [`ReadRepository`](crate::application::ReadRepository) and
//! [`WriteRepository`](crate::application::WriteRepository) when their
//! conversion and connection bounds are satisfied.
//!
//! Reads convert DTOs into versioned snapshots with `TryFrom`; writes convert
//! cloned versioned snapshots into DTOs with `From`. Connections, transaction
//! boundaries and optimistic-concurrency checks belong to the storage
//! implementation, not to these adapters.
//!
//! The optional `in_memory` backend supports snapshot rollback on action
//! errors. Callers must serialize writes during commit. It is enabled by
//! the default `in-memory` feature.

mod dto;
#[cfg(feature = "in-memory")]
pub mod in_memory;
mod repository;

pub use dto::DTO;
pub use dto::DTODelete;
pub use dto::DTOInsert;
pub use dto::DTOSelect;
pub use dto::DTOUpdate;
pub use dto::InternalId;
pub use repository::DTOReadRepository;
pub use repository::DTORepository;
pub use repository::DTORepositoryError;
pub use repository::DTOWriteRepository;
