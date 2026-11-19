//! DTO persistence using the shared in-memory backend. Data lasts for the
//! lifetime of the factory. Commits and reads share a gate so readers cannot
//! observe partial commits and rollback cannot overwrite another commit.

use std::{
    cell::RefCell,
    collections::HashSet,
    convert::Infallible,
    rc::Rc,
    sync::{Arc, Mutex, Weak},
};

use croisiere::{
    application::{
        AbstractActionableSnapshot, Pagination, ReadRepository, SnapshotAction, UnitOfWork,
        UnitOfWorkFactory, VersionedSnapshot, WriteRepository,
    },
    infrastructure::persistence::{
        DTODelete, DTOInsert, DTOReadRepository, DTORepository, DTORepositoryError, DTOSelect,
        DTOUpdate, DTOWriteRepository,
        in_memory::{InMemoryDTO, InMemoryDatabase, InMemoryRepositoryError, InMemoryUnitOfWork},
    },
};

use crate::application::{
    errors::AppError,
    registry::Registry,
    snapshots::{CounterFilter, CounterSnapshot, CounterSorter},
};

type Connection = Weak<Mutex<InMemoryDatabase>>;
type StorageError = InMemoryRepositoryError;

#[derive(Debug, Clone)]
struct CounterDTO {
    name: String,
    counter: u64,
    version: u128,
}

impl InMemoryDTO for CounterDTO {
    const NAME: &'static str = "counters";
}

impl From<VersionedSnapshot<CounterSnapshot>> for CounterDTO {
    fn from(value: VersionedSnapshot<CounterSnapshot>) -> Self {
        Self {
            name: value.snapshot.name,
            counter: value.snapshot.counter,
            version: value.version,
        }
    }
}

impl From<CounterDTO> for VersionedSnapshot<CounterSnapshot> {
    fn from(value: CounterDTO) -> Self {
        Self {
            snapshot: CounterSnapshot {
                name: value.name,
                counter: value.counter,
            },
            version: value.version,
        }
    }
}

fn with_database<T>(
    connection: &Connection,
    f: impl FnOnce(&mut InMemoryDatabase) -> Result<T, StorageError>,
) -> Result<T, StorageError> {
    let database = connection.upgrade().ok_or(StorageError::DatabaseDropped)?;
    let mut database = database
        .lock()
        .map_err(|_| StorageError::DatabasePoisoned)?;
    f(&mut database)
}

fn rows(database: &InMemoryDatabase) -> Result<Vec<CounterDTO>, StorageError> {
    match database.read_rows(CounterDTO::NAME) {
        Err(StorageError::TableNotFound(_)) => Ok(Vec::new()),
        result => result,
    }
}

impl DTOSelect for CounterDTO {
    // The in-memory representation can use the application's criteria directly.
    type Filter = CounterFilter;
    type Sorter = CounterSorter;

    fn select(
        conn: &mut Connection,
        filters: HashSet<CounterFilter>,
        sorter: Option<CounterSorter>,
        pagination: Option<Pagination>,
    ) -> Result<Vec<Self>, StorageError> {
        let mut selected = with_database(conn, |database| rows(database))?;
        // All filters must match; no filters selects every counter.
        selected.retain(|row| {
            filters.iter().all(|filter| match filter {
                CounterFilter::ByName(name) => row.name == *name,
            })
        });
        match sorter {
            Some(CounterSorter::ByName) => selected.sort_by(|a, b| a.name.cmp(&b.name)),
            Some(CounterSorter::ByCountDescending) => {
                selected.sort_by(|a, b| b.counter.cmp(&a.counter).then_with(|| a.name.cmp(&b.name)))
            }
            None => {}
        }
        Ok(match pagination {
            Some(page) => selected
                .into_iter()
                .skip(page.offset)
                .take(page.limit)
                .collect(),
            None => selected,
        })
    }
}

fn conflict(name: &str) -> StorageError {
    StorageError::ActionFailed(format!("version conflict for counter {name:?}"))
}

impl DTOInsert for CounterDTO {
    fn insert(self, conn: &mut Connection) -> Result<(), StorageError> {
        with_database(conn, |database| {
            if self.version != 1 || rows(database)?.iter().any(|row| row.name == self.name) {
                return Err(conflict(&self.name));
            }
            database.insert(Self::NAME, self);
            Ok(())
        })
    }
}

impl DTOUpdate for CounterDTO {
    fn update(self, conn: &mut Connection) -> Result<(), StorageError> {
        with_database(conn, |database| {
            let changed = database.update(
                Self::NAME,
                |row: &Self| {
                    row.name == self.name && row.version.checked_add(1) == Some(self.version)
                },
                |row| *row = self.clone(),
            )?;
            if changed != 1 {
                return Err(conflict(&self.name));
            }
            Ok(())
        })
    }
}

impl DTODelete for CounterDTO {
    fn delete(self, conn: &mut Connection) -> Result<(), StorageError> {
        with_database(conn, |database| {
            let changed = database.delete(Self::NAME, |row: &Self| {
                row.name == self.name && row.version.checked_add(1) == Some(self.version)
            })?;
            if changed != 1 {
                return Err(conflict(&self.name));
            }
            Ok(())
        })
    }
}

struct CounterDTORepository(Connection);

impl DTORepository for CounterDTORepository {
    const NAME: &'static str = "CounterRepository";
    type ConnectionData = Connection;
    type Error = StorageError;

    fn connection(&mut self) -> Result<Connection, StorageError> {
        Ok(self.0.clone())
    }
}

impl DTOReadRepository<CounterSnapshot> for CounterDTORepository {
    type DTO = CounterDTO;

    fn count(
        &mut self,
        filters: HashSet<CounterFilter>,
        sorter: Option<CounterSorter>,
    ) -> Result<usize, StorageError> {
        Ok(CounterDTO::select(&mut self.connection()?, filters, sorter, None)?.len())
    }
}

impl DTOWriteRepository<CounterSnapshot> for CounterDTORepository {
    type DTO = CounterDTO;

    fn exists(&mut self, dto: &CounterDTO) -> bool {
        // This trait cannot return errors. A failed lookup selects insert,
        // which repeats the lookup and propagates the storage error.
        with_database(&self.0, |database| {
            Ok(rows(database)?.iter().any(|row| row.name == dto.name))
        })
        .unwrap_or(false)
    }
}

impl From<StorageError> for AppError {
    fn from(error: StorageError) -> Self {
        Self::Storage(error.to_string())
    }
}

impl From<DTORepositoryError<StorageError, Infallible>> for AppError {
    fn from(error: DTORepositoryError<StorageError, Infallible>) -> Self {
        match error {
            DTORepositoryError::Repository(error) => error.into(),
            DTORepositoryError::Conversion(never) => match never {},
        }
    }
}

/// Gives the DTO adapter the application's error type. Writes are only used
/// by actions while the unit of work holds the commit gate.
struct CounterRepository {
    inner: CounterDTORepository,
    gate: Arc<Mutex<()>>,
}

impl ReadRepository<CounterSnapshot> for CounterRepository {
    type Error = AppError;

    fn get(
        &mut self,
        filters: HashSet<CounterFilter>,
        sorter: Option<CounterSorter>,
        pagination: Option<Pagination>,
    ) -> Result<Vec<VersionedSnapshot<CounterSnapshot>>, AppError> {
        let _guard = self
            .gate
            .lock()
            .map_err(|_| AppError::Storage("commit gate poisoned".into()))?;
        Ok(ReadRepository::get(
            &mut self.inner,
            filters,
            sorter,
            pagination,
        )?)
    }

    fn count(
        &mut self,
        filters: HashSet<CounterFilter>,
        sorter: Option<CounterSorter>,
    ) -> Result<usize, AppError> {
        let _guard = self
            .gate
            .lock()
            .map_err(|_| AppError::Storage("commit gate poisoned".into()))?;
        Ok(ReadRepository::count(&mut self.inner, filters, sorter)?)
    }
}

impl WriteRepository<CounterSnapshot> for CounterRepository {
    type Error = AppError;

    fn save(
        &mut self,
        snapshots: Vec<&VersionedSnapshot<CounterSnapshot>>,
    ) -> Result<(), AppError> {
        Ok(WriteRepository::save(&mut self.inner, snapshots)?)
    }

    fn delete(
        &mut self,
        snapshots: Vec<&VersionedSnapshot<CounterSnapshot>>,
    ) -> Result<(), AppError> {
        Ok(WriteRepository::delete(&mut self.inner, snapshots)?)
    }
}

// Retain the original application error while signalling failure to the
// backend, which owns rollback and keeps its concrete storage error type.
struct ActionAdapter {
    inner: Box<dyn AbstractActionableSnapshot<Error = AppError>>,
    error: Rc<RefCell<Option<AppError>>>,
}

impl ActionAdapter {
    fn run(&mut self) -> Result<(), StorageError> {
        self.inner.act().map_err(|error| {
            *self.error.borrow_mut() = Some(error);
            StorageError::ActionFailed("counter persistence action failed".into())
        })
    }
}

impl AbstractActionableSnapshot for ActionAdapter {
    type Error = StorageError;
    fn get_name(&self) -> &str {
        self.inner.get_name()
    }
    fn get_action(&self) -> &SnapshotAction {
        self.inner.get_action()
    }
    fn save(&mut self) -> Result<(), StorageError> {
        self.run()
    }
    fn delete(&mut self) -> Result<(), StorageError> {
        self.run()
    }
}

struct CounterUnitOfWork {
    inner: InMemoryUnitOfWork<Registry>,
    gate: Arc<Mutex<()>>,
    error: Rc<RefCell<Option<AppError>>>,
}

impl UnitOfWork for CounterUnitOfWork {
    type Error = AppError;
    type Registry = Registry;

    fn registry(&self) -> &Registry {
        self.inner.registry()
    }

    fn register_action(&self, action: Box<dyn AbstractActionableSnapshot<Error = AppError>>) {
        self.inner.register_action(Box::new(ActionAdapter {
            inner: action,
            error: self.error.clone(),
        }));
    }

    fn commit_and_rollback(&self) -> Result<(), AppError> {
        let _guard = self
            .gate
            .lock()
            .map_err(|_| AppError::Storage("commit gate poisoned".into()))?;
        let result = self.inner.commit_and_rollback();
        if let Some(error) = self.error.borrow_mut().take() {
            return Err(error);
        }
        result.map_err(Into::into)
    }
}

/// Owns the database and creates fresh repositories for each unit of work.
#[derive(Default)]
pub struct CounterFactory {
    database: Arc<Mutex<InMemoryDatabase>>,
    gate: Arc<Mutex<()>>,
}

impl UnitOfWorkFactory for CounterFactory {
    type Error = AppError;
    type Registry = Registry;

    fn begin(&self) -> Box<dyn UnitOfWork<Error = AppError, Registry = Registry>> {
        let connection = Arc::downgrade(&self.database);
        let registry = Registry::new(Rc::new(RefCell::new(CounterRepository {
            inner: CounterDTORepository(connection.clone()),
            gate: self.gate.clone(),
        })));
        Box::new(CounterUnitOfWork {
            inner: InMemoryUnitOfWork::new(registry, connection),
            gate: self.gate.clone(),
            error: Rc::new(RefCell::new(None)),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::{
        commands::{CreateCounter, DeleteCounter, IncrementCounter},
        queries::ListCounters,
        registry::{Factory, upgrade},
    };
    use croisiere::application::{Command, Query};

    fn create(factory: &Factory, name: &str) {
        CreateCounter {
            actor: "test".into(),
            name: name.into(),
        }
        .handle(factory.clone())
        .unwrap();
    }

    fn find(
        uow: &dyn UnitOfWork<Error = AppError, Registry = Registry>,
        name: &str,
    ) -> VersionedSnapshot<CounterSnapshot> {
        upgrade(&uow.registry().counters())
            .unwrap()
            .borrow_mut()
            .get(
                HashSet::from([CounterFilter::ByName(name.into())]),
                None,
                None,
            )
            .unwrap()
            .pop()
            .unwrap()
    }

    #[test]
    fn commands_persist_and_queries_filter_sort_and_paginate() {
        let factory: Factory = Arc::new(CounterFactory::default());
        assert!(
            ListCounters {
                sorter: None,
                pagination: None
            }
            .handle(factory.clone())
            .unwrap()
            .is_empty()
        );
        create(&factory, "orders");
        create(&factory, "visits");
        assert!(matches!(
            CreateCounter {
                actor: "test".into(),
                name: "visits".into()
            }
            .handle(factory.clone()),
            Err(AppError::AlreadyExists(_))
        ));
        IncrementCounter {
            actor: "test".into(),
            name: "visits".into(),
        }
        .handle(factory.clone())
        .unwrap();

        let uow = factory.begin();
        let visits = find(uow.as_ref(), "visits");
        assert_eq!((visits.snapshot.counter, visits.version), (1, 2));
        let page = ListCounters {
            sorter: Some(CounterSorter::ByCountDescending),
            pagination: Some(Pagination {
                limit: 1,
                offset: 0,
            }),
        }
        .handle(factory.clone())
        .unwrap();
        assert_eq!(page[0].name, "visits");
        let page = ListCounters {
            sorter: Some(CounterSorter::ByName),
            pagination: Some(Pagination {
                limit: 1,
                offset: 1,
            }),
        }
        .handle(factory.clone())
        .unwrap();
        assert_eq!(page[0].name, "visits");
        assert!(
            upgrade(&uow.registry().counters())
                .unwrap()
                .borrow_mut()
                .get(
                    HashSet::from([CounterFilter::ByName("missing".into())]),
                    None,
                    None
                )
                .unwrap()
                .is_empty()
        );

        DeleteCounter {
            name: "orders".into(),
        }
        .handle(factory.clone())
        .unwrap();
        assert!(matches!(
            DeleteCounter {
                name: "orders".into()
            }
            .handle(factory.clone()),
            Err(AppError::NotFound(_))
        ));
        assert_eq!(
            ListCounters {
                sorter: None,
                pagination: None
            }
            .handle(factory)
            .unwrap()
            .len(),
            1
        );
    }

    #[test]
    fn conflict_rolls_back_earlier_writes_and_discards_pending_actions() {
        let factory: Factory = Arc::new(CounterFactory::default());
        create(&factory, "orders");
        create(&factory, "visits");
        let stale = factory.begin();
        let visits = find(stale.as_ref(), "visits");
        let mut orders = find(stale.as_ref(), "orders");
        orders.snapshot.counter = 99;
        stale.register_action(Box::new(
            orders.make_actionable(stale.registry().counters(), SnapshotAction::Save),
        ));
        stale.register_action(Box::new(
            visits.make_actionable(stale.registry().counters(), SnapshotAction::Delete),
        ));
        // This action must be discarded when the preceding delete conflicts.
        stale.register_action(Box::new(
            find(stale.as_ref(), "orders")
                .make_actionable(stale.registry().counters(), SnapshotAction::Delete),
        ));

        IncrementCounter {
            actor: "test".into(),
            name: "visits".into(),
        }
        .handle(factory.clone())
        .unwrap();
        assert!(
            matches!(stale.commit_and_rollback(), Err(AppError::Storage(message)) if message.contains("version conflict"))
        );
        stale.commit_and_rollback().unwrap();
        let fresh = factory.begin();
        assert_eq!(find(fresh.as_ref(), "orders").snapshot.counter, 0);
        assert_eq!(find(fresh.as_ref(), "visits").snapshot.counter, 1);
    }

    #[test]
    fn stale_update_is_rejected_and_uncommitted_actions_are_discarded() {
        let factory: Factory = Arc::new(CounterFactory::default());
        create(&factory, "visits");
        let stale = factory.begin();
        stale.register_action(Box::new(
            find(stale.as_ref(), "visits")
                .make_actionable(stale.registry().counters(), SnapshotAction::Save),
        ));
        IncrementCounter {
            actor: "test".into(),
            name: "visits".into(),
        }
        .handle(factory.clone())
        .unwrap();
        assert!(
            matches!(stale.commit_and_rollback(), Err(AppError::Storage(message)) if message.contains("version conflict"))
        );

        let abandoned = factory.begin();
        let repository = abandoned.registry().counters();
        abandoned.register_action(Box::new(
            find(abandoned.as_ref(), "visits")
                .make_actionable(repository.clone(), SnapshotAction::Delete),
        ));
        drop(abandoned);
        assert!(repository.upgrade().is_none());
        assert_eq!(find(factory.begin().as_ref(), "visits").snapshot.counter, 1);
    }
}
