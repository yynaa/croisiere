//! Example of the application layer, written only against the abstract
//! `croisiere::application` traits.
//!
//! Commands and queries receive a [`UnitOfWorkFactory`](croisiere::application::UnitOfWorkFactory)
//! and reach the repositories through its [`Registry`](registry::Registry).
//! The infrastructure layer supplies the in-memory factory and repositories;
//! the binary wires them into a runnable persistence example.

pub mod commands;
pub mod errors;
pub mod event_services;
pub mod events;
pub mod queries;
pub mod registry;
pub mod snapshots;
