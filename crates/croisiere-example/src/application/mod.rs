//! Example of the application layer, written only against the abstract
//! `croisiere::application` traits.
//!
//! Shared dispatch routines prepare a unit of work for each command or query.
//! Commands also receive an actor and event bus sender. Handlers reach the
//! repositories through the unit of work's [`Registry`](registry::Registry).
//! The infrastructure layer supplies the in-memory factory and repositories;
//! the binary wires them into a runnable persistence example.

pub mod commands;
pub mod errors;
pub mod event_services;
pub mod events;
pub mod queries;
pub mod registry;
pub mod repositories;
pub mod snapshots;
