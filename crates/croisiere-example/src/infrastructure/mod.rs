//! Persistence adapters for the counter application.
//!
//! Only this layer knows the in-memory backend, row UUIDs and DTO operations.
//! It supplies a factory through the application-facing unit-of-work trait.

pub mod persistence;
