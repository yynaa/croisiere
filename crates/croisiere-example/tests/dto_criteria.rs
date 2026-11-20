//! Storage-only UUID criteria and translation from application criteria.

#[allow(dead_code)]
#[path = "../src/application/mod.rs"]
mod application;
#[allow(dead_code)]
#[path = "../src/domain/mod.rs"]
mod domain;
// Load the private persistence modules in this test's scope to exercise
// infrastructure-only criteria without making them application APIs.
#[allow(dead_code)]
#[path = "../src/infrastructure/persistence/dtos.rs"]
mod dtos;
#[allow(dead_code)]
#[path = "../src/infrastructure/persistence/repositories.rs"]
mod repositories;

use std::{
    collections::HashSet,
    sync::{Arc, Mutex},
};

use croisiere::{
    application::{Pagination, ReadRepository},
    infrastructure::persistence::{
        DTO, DTOSelect,
        in_memory::{
            InMemoryConnection as Connection, InMemoryDatabase,
            InMemoryRepositoryError as StorageError,
        },
    },
};
use uuid::Uuid;

use application::snapshots::{CounterFilter, CounterSnapshot, CounterSorter};
use dtos::{CounterDTO, CounterDTOFilter, CounterDTOSorter};
use repositories::CounterRepository;

fn storage() -> (Arc<Mutex<InMemoryDatabase>>, Connection) {
    let mut database = InMemoryDatabase::default();
    for (id, name, counter) in [(3, "visits", 5), (1, "orders", 2), (2, "alerts", 5)] {
        database.insert(
            CounterDTO::NAME,
            CounterDTO {
                internal_id: Some(Uuid::from_u128(id)),
                name: name.into(),
                counter,
                version: 1,
            },
        );
    }
    let database = Arc::new(Mutex::new(database));
    let connection = Connection {
        database: Arc::downgrade(&database),
        gate: Arc::new(Mutex::new(())),
    };
    (database, connection)
}

#[test]
fn storage_criteria_filter_and_sort_by_internal_uuid() {
    let (_database, mut connection) = storage();
    let found = CounterDTO::select(
        &mut connection,
        HashSet::from([CounterDTOFilter::ByInternalId(Uuid::from_u128(2))]),
        None,
        None,
    )
    .unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].name, "alerts");

    let found = CounterDTO::select(
        &mut connection,
        HashSet::from([
            CounterDTOFilter::ByInternalId(Uuid::from_u128(2)),
            CounterDTOFilter::ByName("visits".into()),
        ]),
        None,
        None,
    )
    .unwrap();
    assert!(found.is_empty());

    let found = CounterDTO::select(
        &mut connection,
        HashSet::new(),
        Some(CounterDTOSorter::ByInternalId),
        None,
    )
    .unwrap();
    assert_eq!(
        found
            .iter()
            .map(|row| row.name.as_str())
            .collect::<Vec<_>>(),
        ["orders", "alerts", "visits"]
    );

    let page = CounterDTO::select(
        &mut connection,
        HashSet::new(),
        Some(CounterDTOSorter::ByInternalId),
        Some(Pagination {
            limit: 1,
            offset: 1,
        }),
    )
    .unwrap();
    assert_eq!(page.len(), 1);
    assert_eq!(page[0].name, "alerts");
}

#[test]
fn repository_translates_application_filters_and_sorters() {
    let (_database, connection) = storage();
    let mut repository = CounterRepository::new(connection);
    let filters = || HashSet::from([CounterFilter::ByName("visits".into())]);
    let found =
        ReadRepository::<CounterSnapshot>::get(&mut repository, filters(), None, None).unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].snapshot.name, "visits");
    assert_eq!(
        ReadRepository::<CounterSnapshot>::count(&mut repository, filters()).unwrap(),
        1
    );

    for (sorter, expected) in [
        (CounterSorter::ByName, ["alerts", "orders", "visits"]),
        (
            CounterSorter::ByCountDescending,
            ["alerts", "visits", "orders"],
        ),
    ] {
        let found = ReadRepository::<CounterSnapshot>::get(
            &mut repository,
            HashSet::new(),
            Some(sorter),
            None,
        )
        .unwrap();
        assert_eq!(
            found
                .iter()
                .map(|value| value.snapshot.name.as_str())
                .collect::<Vec<_>>(),
            expected
        );
    }
}
