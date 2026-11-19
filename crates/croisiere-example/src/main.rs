pub mod application;
pub mod domain;
pub mod infrastructure;

use std::sync::Arc;

use application::{
    commands::{CreateCounter, DeleteCounter, IncrementCounter},
    errors::AppError,
    queries::ListCounters,
    registry::Factory,
    snapshots::CounterSorter,
};
use croisiere::application::{Command, Query};
use infrastructure::persistence::CounterFactory;

fn main() -> Result<(), AppError> {
    let factory: Factory = Arc::new(CounterFactory::default());
    for name in ["visits", "orders"] {
        CreateCounter {
            actor: "demo".into(),
            name: name.into(),
        }
        .handle(factory.clone())?;
    }
    IncrementCounter {
        actor: "demo".into(),
        name: "visits".into(),
    }
    .handle(factory.clone())?;
    let query = ListCounters {
        sorter: Some(CounterSorter::ByCountDescending),
        pagination: None,
    };
    println!(
        "After creating and incrementing: {:?}",
        query.handle(factory.clone())?
    );
    DeleteCounter {
        name: "orders".into(),
    }
    .handle(factory.clone())?;
    println!("After deleting orders: {:?}", query.handle(factory)?);
    Ok(())
}
