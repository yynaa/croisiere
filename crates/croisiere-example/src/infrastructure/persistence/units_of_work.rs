//! Construction of the library factory for the counter application.

use std::{
    cell::RefCell,
    rc::Rc,
    sync::{Arc, Mutex},
};

use croisiere::{
    application::ReadRepositoryFactory,
    infrastructure::persistence::in_memory::{InMemoryDatabase, InMemoryUnitOfWorkFactory},
};

use super::{Connection, repositories::CounterRepository};
use crate::application::{errors::RepositoryError, registry::Registry, snapshots::CounterSnapshot};

/// Library factory specialized to the application's registry and shared error.
pub type CounterFactory = InMemoryUnitOfWorkFactory<Registry, RepositoryError>;

/// Creates an empty database and a factory owning it for its entire lifetime.
///
/// Every `begin()` builds a fresh registry with a writer and a factory for
/// owned readers. All connections borrow the same database weakly and share
/// the factory's commit gate. Registries use `Rc<RefCell<_>>` locally; only the
/// registry-building callback must be `Send + Sync`.
pub fn counter_factory() -> CounterFactory {
    let database = Arc::new(Mutex::new(InMemoryDatabase::default()));

    InMemoryUnitOfWorkFactory::from_registry_factory(database, |database, gate| {
        let connection = Connection { database, gate };
        let write_counters = Rc::new(RefCell::new(CounterRepository::new(connection.clone())));
        let read_counters: ReadRepositoryFactory<CounterSnapshot, RepositoryError> =
            Box::new(move || Box::new(CounterRepository::new(connection.clone())));

        Registry::new(read_counters, write_counters)
    })
}
