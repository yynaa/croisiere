//! Runnable counter application with domain, application and persistence layers.
//!
//! Run with `cargo run -p croisiere-example` from the workspace root. The binary
//! creates two counters, increments one, lists them, deletes the other and
//! prints the events published by successful commands. Storage is in memory
//! and lasts only while the factory is alive.
//!
//! `main` owns the factory and event bus. Request dispatch prepares contexts;
//! domain operations enforce business rules; DTO adapters handle storage.
//! The illustrative notification job in `application::event_services` is not
//! started by this binary.

pub mod application;
pub mod domain;
pub mod infrastructure;

use std::sync::Arc;

use application::{
    commands::{AppCommand, CreateCounter, DeleteCounter, IncrementCounter, execute},
    errors::AppError,
    queries::{AppQuery, ListCounters, ask},
    registry::Factory,
    snapshots::CounterSorter,
};
use croisiere::application::EventBus;
use infrastructure::persistence::counter_factory;

/// Wires the adapters and runs the demonstration through application dispatch.
fn main() -> Result<(), AppError> {
    let factory: Factory = Arc::new(counter_factory());
    let bus = EventBus::default();
    // Subscribe before publishing: the in-memory bus does not replay old events.
    let events = bus.subscribe();
    for name in ["visits", "orders"] {
        execute(
            &factory,
            &bus,
            "demo".into(),
            AppCommand::CreateCounter(CreateCounter { name: name.into() }),
        )?;
    }
    execute(
        &factory,
        &bus,
        "demo".into(),
        AppCommand::IncrementCounter(IncrementCounter {
            name: "visits".into(),
        }),
    )?;
    // Build each query afresh; every dispatch receives its own unit of work.
    let query = || {
        AppQuery::ListCounters(ListCounters {
            sorter: Some(CounterSorter::ByCountDescending),
            pagination: None,
        })
    };
    println!(
        "After creating and incrementing: {:?}",
        ask(&factory, query())?
    );
    execute(
        &factory,
        &bus,
        "demo".into(),
        AppCommand::DeleteCounter(DeleteCounter {
            name: "orders".into(),
        }),
    )?;
    println!("After deleting orders: {:?}", ask(&factory, query())?);
    while let Ok(event) = events.try_recv() {
        println!("Event: {event:?}");
    }
    Ok(())
}
