//! Example of the application layer, written only against the abstract
//! `croisiere::application` traits.
//!
//! Commands and queries receive a [`UnitOfWorkFactory`](croisiere::application::UnitOfWorkFactory)
//! and reach the repositories through its [`Registry`](registry::Registry).
//! Implementing the factory and the repositories (a database, an in-memory
//! store...) is the job of the infrastructure layer, so nothing here runs on
//! its own.

pub mod commands;
pub mod errors;
pub mod event_services;
pub mod events;
pub mod queries;
pub mod registry;
pub mod snapshots;
