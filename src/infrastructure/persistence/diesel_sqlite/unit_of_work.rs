//! Unit-of-work and factory implementation for SQLite storage over Diesel.

use std::{
    cell::RefCell,
    collections::VecDeque,
    marker::PhantomData,
    sync::{Arc, Mutex, Weak},
};

use diesel::r2d2::Pool;

use crate::{
    application::{AbstractActionableSnapshot, UnitOfWork, UnitOfWorkFactory},
    infrastructure::persistence::diesel_sqlite::{
        SqlitePooledConnection, SqliteRepositoryError, SqliteSlot,
    },
};

use diesel::connection::{AnsiTransactionManager, TransactionManager};

/// Registry and FIFO queue of actions intended for a SQLite transaction.
///
/// Registration only queues actions; it does not touch the connection. The
/// unit of work owns one connection checked out of the factory's pool until
/// it is dropped.
///
/// # Transaction boundaries
///
/// [`commit_and_rollback`](UnitOfWork::commit_and_rollback) opens one Diesel
/// transaction on the owned connection, runs every queued action and commits
/// it. Actions acquire the connection themselves, so the connection cannot be
/// held across the whole batch: use [`Self::with_commit_gate`] with a shared
/// lock, or prevent concurrent writes externally. An action error rolls the
/// opened transaction back, undoing every write of the batch.
#[cfg_attr(docsrs, doc(cfg(feature = "diesel-sqlite")))]
pub struct SqliteUnitOfWork<R, E = SqliteRepositoryError> {
    registry: R,

    connection: Arc<SqliteSlot>,
    actions: RefCell<VecDeque<Box<dyn AbstractActionableSnapshot<Error = E>>>>,
    commit_gate: Option<Arc<Mutex<()>>>,
}

impl<R, E> Drop for SqliteUnitOfWork<R, E> {
    /// Releases the connection back to the pool, even if a registry kept a
    /// strong handle to its slot.
    fn drop(&mut self) {
        let mut slot = self
            .connection
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *slot = None;
    }
}

impl<R, E> SqliteUnitOfWork<R, E> {
    /// Creates a unit of work owning `connection`, parked in a shared slot,
    /// with `registry` and no actions.
    pub fn new(registry: R, connection: Arc<SqliteSlot>) -> Self {
        Self {
            registry,
            connection,
            actions: RefCell::new(VecDeque::new()),
            commit_gate: None,
        }
    }

    /// Shares a lock across commits and, optionally, repository reads.
    /// Every writer to this factory's pool must use the same gate.
    pub fn with_commit_gate(mut self, gate: Arc<Mutex<()>>) -> Self {
        self.commit_gate = Some(gate);
        self
    }

    /// Locks the slot, tolerating a previous panic.
    fn lock(slot: &SqliteSlot) -> std::sync::MutexGuard<'_, Option<SqlitePooledConnection>> {
        slot.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

impl<R, E: From<SqliteRepositoryError>> UnitOfWork for SqliteUnitOfWork<R, E> {
    type Registry = R;
    type Error = E;

    /// Returns the registry supplied at construction.
    fn registry(&self) -> &Self::Registry {
        &self.registry
    }

    /// Appends an action to the queue without executing it.
    fn register_action(
        &self,
        actionable_snapshot: Box<dyn AbstractActionableSnapshot<Error = Self::Error>>,
    ) {
        self.actions.borrow_mut().push_back(actionable_snapshot);
    }

    /// Executes queued actions in FIFO order inside one Diesel transaction.
    ///
    /// The entire batch is consumed on success or failure. No slot lock is
    /// held while actions run; each action locks the slot itself. A configured
    /// commit gate is held across the entire batch; otherwise callers must
    /// serialize writes. An action error rolls the transaction back before
    /// returning it. Action panics propagate without rollback.
    fn commit_and_rollback(&self) -> Result<(), Self::Error> {
        let actions = std::mem::take(&mut *self.actions.borrow_mut());
        let _guard = self
            .commit_gate
            .as_ref()
            .map(|gate| {
                gate.lock()
                    .map_err(|_| SqliteRepositoryError::ConnectionPoisoned)
            })
            .transpose()?;

        {
            let mut slot = Self::lock(&self.connection);
            let connection = slot
                .as_mut()
                .ok_or(SqliteRepositoryError::ConnectionDropped)?;
            AnsiTransactionManager::begin_transaction(&mut **connection)
                .map_err(SqliteRepositoryError::from)?;
        }

        for mut action in actions {
            if let Err(error) = action.act() {
                let mut slot = Self::lock(&self.connection);
                let connection = slot
                    .as_mut()
                    .ok_or(SqliteRepositoryError::ConnectionDropped)?;
                AnsiTransactionManager::rollback_transaction(&mut **connection)
                    .map_err(SqliteRepositoryError::from)?;
                return Err(error);
            }
        }

        let mut slot = Self::lock(&self.connection);
        let connection = slot
            .as_mut()
            .ok_or(SqliteRepositoryError::ConnectionDropped)?;
        AnsiTransactionManager::commit_transaction(&mut **connection)
            .map_err(SqliteRepositoryError::from)?;
        Ok(())
    }
}

/// Factory creating registries and units of work over a connection pool.
///
/// The pool and the commit gate are shared across threads; the registries the
/// builder creates need not be `Send`, `Sync` or `Clone`. Each `begin` checks
/// out one connection, parks it in a fresh slot and invokes the builder with
/// the pool, a weak handle to that slot and the gate. Every writer must
/// respect the gate.
#[cfg_attr(docsrs, doc(cfg(feature = "diesel-sqlite")))]
pub struct SqliteUnitOfWorkFactory<R, E = SqliteRepositoryError> {
    registry_builder: Box<
        dyn Fn(Pool<super::SqliteConnectionManager>, Weak<SqliteSlot>, Arc<Mutex<()>>) -> R
            + Send
            + Sync,
    >,
    pool: Pool<super::SqliteConnectionManager>,
    commit_gate: Arc<Mutex<()>>,
    error: PhantomData<fn() -> E>,
}

impl<R: Clone + Send + Sync + 'static> SqliteUnitOfWorkFactory<R> {
    /// Creates a factory cloning a registry template for each unit of work.
    ///
    /// Building the pool opens and tests one connection, so this can fail.
    /// Use [`Self::from_pool`] to tune the pool.
    pub fn new(database_url: impl Into<String>, registry: R) -> Result<Self, r2d2::Error> {
        Self::from_registry_factory(database_url, move |_, _, _| registry.clone())
    }
}

impl<R, E> SqliteUnitOfWorkFactory<R, E> {
    /// Creates the pool from `database_url` with default settings, then
    /// behaves like [`Self::from_pool`]. Building the pool opens and tests
    /// one connection, so this can fail.
    pub fn from_registry_factory(
        database_url: impl Into<String>,
        builder: impl Fn(
            Pool<super::SqliteConnectionManager>,
            Weak<SqliteSlot>,
            Arc<Mutex<()>>,
        ) -> R
            + Send
            + Sync
            + 'static,
    ) -> Result<Self, r2d2::Error> {
        let pool = Pool::builder().build(super::SqliteConnectionManager::new(database_url));
        Ok(Self::from_pool(pool?, builder))
    }

    /// Creates fresh registries using a caller-built pool. Useful for tuning
    /// size, timeouts and connection setup. The builder must not retain
    /// registries or repositories across calls if they should expire with
    /// their unit of work; it receives the shared pool, a weak handle to the
    /// unit of work's connection slot and the commit gate.
    pub fn from_pool(
        pool: Pool<super::SqliteConnectionManager>,
        builder: impl Fn(
            Pool<super::SqliteConnectionManager>,
            Weak<SqliteSlot>,
            Arc<Mutex<()>>,
        ) -> R
            + Send
            + Sync
            + 'static,
    ) -> Self {
        Self {
            registry_builder: Box::new(builder),
            pool,
            commit_gate: Arc::new(Mutex::new(())),
            error: PhantomData,
        }
    }
}

impl<R: 'static, E: From<SqliteRepositoryError> + 'static> UnitOfWorkFactory
    for SqliteUnitOfWorkFactory<R, E>
{
    type Error = E;
    type Registry = R;

    /// Checks out a connection, builds a registry around it and returns an
    /// empty unit of work sharing the commit gate. Pool exhaustion or a
    /// connection failure is reported instead of a unit of work.
    fn begin(&self) -> Result<Box<dyn UnitOfWork<Error = E, Registry = R>>, E> {
        let connection = self.pool.get().map_err(SqliteRepositoryError::from)?;
        let slot: Arc<SqliteSlot> = Arc::new(Mutex::new(Some(connection)));
        let registry =
            (self.registry_builder)(self.pool.clone(), Arc::downgrade(&slot), self.commit_gate.clone());
        Ok(Box::new(
            SqliteUnitOfWork::new(registry, Arc::clone(&slot))
                .with_commit_gate(self.commit_gate.clone()),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    use diesel::prelude::*;
    use diesel::r2d2::ConnectionManager;
    use diesel::sqlite::SqliteConnection as RawConnection;

    use crate::{
        application::{AbstractActionableSnapshot, SnapshotAction, UnitOfWork},
        infrastructure::persistence::diesel_sqlite::{
            SqliteConnection, SqliteRepositoryError,
        },
    };

    struct TestAction {
        action: SnapshotAction,
        run: Box<dyn FnMut() -> Result<(), SqliteRepositoryError>>,
    }

    impl AbstractActionableSnapshot for TestAction {
        type Error = SqliteRepositoryError;

        fn get_name(&self) -> &str {
            "test"
        }

        fn get_action(&self) -> &SnapshotAction {
            &self.action
        }

        fn save(&mut self) -> Result<(), Self::Error> {
            (self.run)()
        }

        fn delete(&mut self) -> Result<(), Self::Error> {
            (self.run)()
        }
    }

    /// Builds a pool over a shared in-memory database and stashes the handle
    /// the registry builder receives, for direct access in the tests.
    fn pooled_factory(
        name: &str,
        max_size: u32,
    ) -> (
        SqliteUnitOfWorkFactory<(), SqliteRepositoryError>,
        std::sync::Arc<std::sync::Mutex<Option<SqliteConnection>>>,
    ) {
        let pool = Pool::builder()
            .max_size(max_size)
            .connection_timeout(Duration::from_millis(100))
            .build(ConnectionManager::<RawConnection>::new(format!(
                "file:croisiere_diesel_sqlite_{name}?mode=memory&cache=shared"
            )))
            .unwrap();
        let stash = std::sync::Arc::new(std::sync::Mutex::new(None));
        let builder_stash = stash.clone();
        let factory = SqliteUnitOfWorkFactory::from_pool(pool, move |pool, writer, gate| {
            *builder_stash.lock().unwrap() = Some(SqliteConnection { pool, writer, gate });
        });
        (factory, stash)
    }

    /// Takes the connection handle stashed by the registry builder.
    fn take_handle(stash: &std::sync::Mutex<Option<SqliteConnection>>) -> SqliteConnection {
        stash.lock().unwrap().take().unwrap()
    }

    fn create_counters(connection: &mut RawConnection) -> Result<(), SqliteRepositoryError> {
        diesel::sql_query("CREATE TABLE counters (name TEXT NOT NULL, counter INTEGER NOT NULL)")
            .execute(connection)?;
        Ok(())
    }

    fn insert_row(connection: &mut RawConnection) -> Result<(), SqliteRepositoryError> {
        diesel::sql_query("INSERT INTO counters (name, counter) VALUES ('visits', 1)")
            .execute(connection)?;
        Ok(())
    }

    #[derive(Debug, QueryableByName)]
    struct Count {
        #[diesel(sql_type = diesel::sql_types::BigInt)]
        count: i64,
    }

    /// Counts committed rows on a fresh pooled connection.
    fn committed_rows(handle: &SqliteConnection) -> i64 {
        handle
            .with_read_connection(|connection| {
                diesel::sql_query("SELECT COUNT(*) AS count FROM counters")
                    .get_result::<Count>(connection)
                    .map_err(SqliteRepositoryError::from)
            })
            .unwrap()
            .count
    }

    #[test]
    fn commit_runs_actions_in_order_and_consumes_them() {
        let (factory, stash) = pooled_factory("runs_in_order", 2);
        let uow = factory.begin().unwrap();
        let handle = take_handle(&stash);
        handle.with_connection(create_counters).unwrap();

        for (id, action) in [
            (1, SnapshotAction::Save),
            (2, SnapshotAction::Save),
            (3, SnapshotAction::None),
        ] {
            let handle = handle.clone();
            uow.register_action(Box::new(TestAction {
                action,
                run: Box::new(move || {
                    handle.with_connection(|connection| {
                        diesel::sql_query(&format!(
                            "INSERT INTO counters (name, counter) VALUES ('{id}', {id})"
                        ))
                        .execute(connection)?;
                        Ok(())
                    })
                }),
            }));
        }

        uow.commit_and_rollback().unwrap();
        uow.commit_and_rollback().unwrap();
        assert_eq!(
            handle.with_read_connection(|connection| {
                diesel::sql_query("SELECT counter AS count FROM counters ORDER BY rowid")
                    .get_results::<Count>(connection)
                    .map_err(SqliteRepositoryError::from)
            })
            .unwrap()
            .into_iter()
            .map(|count| count.count)
            .collect::<Vec<_>>(),
            vec![1, 2]
        );
    }

    #[test]
    fn commit_rolls_back_every_write_on_an_action_error() {
        let (factory, stash) = pooled_factory("rolls_back", 2);
        let uow = factory.begin().unwrap();
        let handle = take_handle(&stash);
        handle.with_connection(create_counters).unwrap();

        uow.register_action(Box::new(TestAction {
            action: SnapshotAction::Save,
            run: Box::new({
                let handle = handle.clone();
                move || handle.with_connection(insert_row)
            }),
        }));
        uow.register_action(Box::new(TestAction {
            action: SnapshotAction::Save,
            run: Box::new({
                let handle = handle.clone();
                move || {
                    handle.with_connection(insert_row)?;
                    Err(SqliteRepositoryError::ActionFailed("failure".into()))
                }
            }),
        }));

        assert!(matches!(
            uow.commit_and_rollback(),
            Err(SqliteRepositoryError::ActionFailed(message)) if message == "failure"
        ));
        // The rollback undid both writes, including the first action's.
        assert_eq!(committed_rows(&handle), 0);
    }

    #[test]
    fn begin_reports_an_exhausted_pool() {
        let pool = Pool::builder()
            .max_size(1)
            .connection_timeout(Duration::from_millis(100))
            .build(ConnectionManager::<RawConnection>::new(
                "file:croisiere_diesel_sqlite_exhausted?mode=memory&cache=shared",
            ))
            .unwrap();
        let held = pool.get().unwrap();
        let factory = SqliteUnitOfWorkFactory::from_pool(pool, |_, _, _| ());

        assert!(matches!(
            factory.begin(),
            Err(SqliteRepositoryError::Pool(_))
        ));
        drop(held);
        factory.begin().unwrap();
    }

    #[test]
    fn dropping_the_unit_of_work_releases_its_connection() {
        let (factory, stash) = pooled_factory("releases", 1);
        let uow = factory.begin().unwrap();
        let handle = take_handle(&stash);
        handle.with_connection(create_counters).unwrap();

        drop(uow);
        // The write connection is gone with its unit of work.
        assert!(matches!(
            handle.with_connection(|connection| execute_row(connection)),
            Err(SqliteRepositoryError::ConnectionDropped)
        ));
        // Reads keep working: the pool returned the connection to its storage.
        assert_eq!(committed_rows(&handle), 0);

        // A new unit of work checks out again without waiting.
        factory.begin().unwrap();
    }

    fn execute_row(connection: &mut RawConnection) -> Result<(), SqliteRepositoryError> {
        diesel::sql_query("INSERT INTO counters (name, counter) VALUES ('visits', 1)")
            .execute(connection)?;
        Ok(())
    }
}
