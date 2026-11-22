//! Unit-of-work and factory implementation for PostgreSQL storage over Diesel.

use std::{
    cell::RefCell,
    collections::VecDeque,
    marker::PhantomData,
    sync::{Arc, Mutex, Weak},
};

use diesel::connection::{AnsiTransactionManager, TransactionManager};
use diesel::r2d2::Pool;

use crate::{
    application::{AbstractActionableSnapshot, UnitOfWork, UnitOfWorkFactory},
    infrastructure::persistence::diesel_pg::{
        PostgresPooledConnection, PostgresRepositoryError, PostgresSlot,
    },
};

/// Registry and FIFO queue of actions intended for a PostgreSQL transaction.
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
#[cfg_attr(docsrs, doc(cfg(feature = "diesel-pg")))]
pub struct PostgresUnitOfWork<R, E = PostgresRepositoryError> {
    registry: R,

    connection: Arc<PostgresSlot>,
    actions: RefCell<VecDeque<Box<dyn AbstractActionableSnapshot<Error = E>>>>,
    commit_gate: Option<Arc<Mutex<()>>>,
}

impl<R, E> Drop for PostgresUnitOfWork<R, E> {
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

impl<R, E> PostgresUnitOfWork<R, E> {
    /// Creates a unit of work owning `connection`, parked in a shared slot,
    /// with `registry` and no actions.
    pub fn new(registry: R, connection: Arc<PostgresSlot>) -> Self {
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
    fn lock(slot: &PostgresSlot) -> std::sync::MutexGuard<'_, Option<PostgresPooledConnection>> {
        slot.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

impl<R, E: From<PostgresRepositoryError>> UnitOfWork for PostgresUnitOfWork<R, E> {
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
                    .map_err(|_| PostgresRepositoryError::ConnectionPoisoned)
            })
            .transpose()?;

        {
            let mut slot = Self::lock(&self.connection);
            let connection = slot
                .as_mut()
                .ok_or(PostgresRepositoryError::ConnectionDropped)?;
            AnsiTransactionManager::begin_transaction(&mut **connection)
                .map_err(PostgresRepositoryError::from)?;
        }

        for mut action in actions {
            if let Err(error) = action.act() {
                let mut slot = Self::lock(&self.connection);
                let connection = slot
                    .as_mut()
                    .ok_or(PostgresRepositoryError::ConnectionDropped)?;
                AnsiTransactionManager::rollback_transaction(&mut **connection)
                    .map_err(PostgresRepositoryError::from)?;
                return Err(error);
            }
        }

        let mut slot = Self::lock(&self.connection);
        let connection = slot
            .as_mut()
            .ok_or(PostgresRepositoryError::ConnectionDropped)?;
        AnsiTransactionManager::commit_transaction(&mut **connection)
            .map_err(PostgresRepositoryError::from)?;
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
#[cfg_attr(docsrs, doc(cfg(feature = "diesel-pg")))]
pub struct PostgresUnitOfWorkFactory<R, E = PostgresRepositoryError> {
    registry_builder: Box<
        dyn Fn(Pool<super::PostgresConnectionManager>, Weak<PostgresSlot>, Arc<Mutex<()>>) -> R
            + Send
            + Sync,
    >,
    pool: Pool<super::PostgresConnectionManager>,
    commit_gate: Arc<Mutex<()>>,
    error: PhantomData<fn() -> E>,
}

impl<R: Clone + Send + Sync + 'static> PostgresUnitOfWorkFactory<R> {
    /// Creates a factory cloning a registry template for each unit of work.
    ///
    /// Building the pool opens and tests one connection, so this can fail.
    /// Use [`Self::from_pool`] to tune the pool.
    pub fn new(database_url: impl Into<String>, registry: R) -> Result<Self, r2d2::Error> {
        Self::from_registry_factory(database_url, move |_, _, _| registry.clone())
    }
}

impl<R, E> PostgresUnitOfWorkFactory<R, E> {
    /// Creates the pool from `database_url` with default settings, then
    /// behaves like [`Self::from_pool`]. Building the pool opens and tests
    /// one connection, so this can fail.
    pub fn from_registry_factory(
        database_url: impl Into<String>,
        builder: impl Fn(
            Pool<super::PostgresConnectionManager>,
            Weak<PostgresSlot>,
            Arc<Mutex<()>>,
        ) -> R
            + Send
            + Sync
            + 'static,
    ) -> Result<Self, r2d2::Error> {
        let pool = Pool::builder().build(super::PostgresConnectionManager::new(database_url));
        Ok(Self::from_pool(pool?, builder))
    }

    /// Creates fresh registries using a caller-built pool. Useful for tuning
    /// size, timeouts and connection setup. The builder must not retain
    /// registries or repositories across calls if they should expire with
    /// their unit of work; it receives the shared pool, a weak handle to the
    /// unit of work's connection slot and the commit gate.
    pub fn from_pool(
        pool: Pool<super::PostgresConnectionManager>,
        builder: impl Fn(
            Pool<super::PostgresConnectionManager>,
            Weak<PostgresSlot>,
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

impl<R: 'static, E: From<PostgresRepositoryError> + 'static> UnitOfWorkFactory
    for PostgresUnitOfWorkFactory<R, E>
{
    type Error = E;
    type Registry = R;

    /// Checks out a connection, builds a registry around it and returns an
    /// empty unit of work sharing the commit gate. Pool exhaustion or a
    /// connection failure is reported instead of a unit of work.
    fn begin(&self) -> Result<Box<dyn UnitOfWork<Error = E, Registry = R>>, E> {
        let connection = self.pool.get().map_err(PostgresRepositoryError::from)?;
        let slot: Arc<PostgresSlot> = Arc::new(Mutex::new(Some(connection)));
        let registry = (self.registry_builder)(
            self.pool.clone(),
            Arc::downgrade(&slot),
            self.commit_gate.clone(),
        );
        Ok(Box::new(
            PostgresUnitOfWork::new(registry, Arc::clone(&slot))
                .with_commit_gate(self.commit_gate.clone()),
        ))
    }
}
