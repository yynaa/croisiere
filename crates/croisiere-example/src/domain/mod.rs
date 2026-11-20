//! Counter business values, identity, behavior, errors and events.
//!
//! Names validate their own contents. The counter entity carries domain
//! identity, and its aggregate owns the count and the operations that change it.
//! Public error and event enums provide a common boundary for callers.

pub mod aggregates;
pub mod entities;
pub mod errors;
pub mod events;
pub mod value_objects;

pub use errors::DomainError;
pub use events::DomainEvent;
