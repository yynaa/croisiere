//! Owned-reader and weak-writer lifetimes through the example's real wiring.

// Exercise the example's wiring without introducing a separate library crate.
#[allow(dead_code)]
#[path = "../src/application/mod.rs"]
mod application;
#[allow(dead_code)]
#[path = "../src/domain/mod.rs"]
mod domain;
#[allow(dead_code)]
#[path = "../src/infrastructure/mod.rs"]
mod infrastructure;

use std::collections::HashSet;

use croisiere::application::{
    AbstractActionableSnapshot, SnapshotAction, UnitOfWorkFactory, VersionedSnapshot,
};

use application::{errors::RepositoryError, snapshots::CounterSnapshot};
use domain::aggregates::CounterAggregate;
use infrastructure::persistence::{CounterFactory, counter_factory};

#[test]
fn owned_readers_see_committed_data_and_outlive_the_unit_of_work() {
    let factory: CounterFactory = counter_factory();
    let uow = factory.begin();
    let mut first_reader = uow.registry().read_counters();
    let mut second_reader = uow.registry().read_counters();
    let writer = uow.registry().write_counters();

    let (counter, _) = CounterAggregate::create("visits".to_owned()).unwrap();
    let action = VersionedSnapshot::<CounterSnapshot>::assemble(counter, 0)
        .make_actionable(writer.clone(), SnapshotAction::Save);
    uow.register_action(Box::new(action));

    assert_eq!(first_reader.count(HashSet::new()).unwrap(), 0);
    assert_eq!(second_reader.count(HashSet::new()).unwrap(), 0);

    uow.commit_and_rollback().unwrap();
    drop(uow);

    assert!(writer.upgrade().is_none());
    assert_eq!(first_reader.count(HashSet::new()).unwrap(), 1);
    assert_eq!(second_reader.count(HashSet::new()).unwrap(), 1);
}

#[test]
fn pending_actions_cannot_keep_the_registry_writer_alive() {
    let factory = counter_factory();
    let uow = factory.begin();
    let mut reader = uow.registry().read_counters();
    let (counter, _) = CounterAggregate::create("visits".to_owned()).unwrap();
    let mut action = VersionedSnapshot::<CounterSnapshot>::assemble(counter, 0)
        .make_actionable(uow.registry().write_counters(), SnapshotAction::Save);

    drop(uow);

    assert!(matches!(
        action.act(),
        Err(RepositoryError::RepositoryDropped)
    ));
    assert_eq!(reader.count(HashSet::new()).unwrap(), 0);
}
