//! Infrastructure layer: storage representations and adapters for the
//! application's persistence traits.
//!
//! [`persistence`] keeps storage-specific DTOs and connections separate from
//! application snapshots. Its repository adapters implement the application's
//! read and write repository traits through DTO operations.

pub mod persistence;
