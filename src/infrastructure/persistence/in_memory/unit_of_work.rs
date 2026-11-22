//! Unit-of-work and factory implementation for in-memory storage.

use std::{
    cell::RefCell,
    collections::VecDeque,
    marker::PhantomData,
    sync::{Arc, Mutex, Weak},
};

use crate::{
    application::{AbstractActionableSnapshot, UnitOfWork, UnitOfWorkFactory},
    infrastructure::persistence::in_memory::{InMemoryDatabase, InMemoryRepositoryError},
};

/// Registry and FIFO queue of actions intended for an in-memory transaction.
///
/// Registration only queues actions; it does not change the database.
/// The writer is weak and does not keep the database alive.
///
/// # Transaction boundaries
///
/// Use [`Self::with_commit_gate`] with a shared lock, or prevent concurrent
/// writes externally. Actions acquire the database lock themselves, so the
/// database lock cannot be held across the whole batch.
/// Rollback restores cloned rows and follows their `Clone` semantics.
#[cfg_attr(docsrs, doc(cfg(feature = "in-memory")))]
pub struct InMemoryUnitOfWork<R, E = InMemoryRepositoryError> {
    registry: R,

    writer: Weak<Mutex<InMemoryDatabase>>,
    actions: RefCell<VecDeque<Box<dyn AbstractActionableSnapshot<Error = E>>>>,
    commit_gate: Option<Arc<Mutex<()>>>,
}

impl<R, E> InMemoryUnitOfWork<R, E> {
    /// Creates a unit of work with `registry`, a weak writer and no actions.
    pub fn new(registry: R, writer: Weak<Mutex<InMemoryDatabase>>) -> Self {
        Self {
            registry,
            writer,
            actions: RefCell::new(VecDeque::new()),
            commit_gate: None,
        }
    }

    /// Shares a lock across commits and, optionally, repository reads.
    /// Every writer to this database must use the same gate.
    pub fn with_commit_gate(mut self, gate: Arc<Mutex<()>>) -> Self {
        self.commit_gate = Some(gate);
        self
    }
}

impl<R, E: From<InMemoryRepositoryError>> UnitOfWork for InMemoryUnitOfWork<R, E> {
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

    /// Executes queued actions in FIFO order, restoring a saved database on error.
    ///
    /// The entire batch is consumed on success or failure. No database lock is
    /// held while actions run. A configured commit gate is held across the
    /// entire batch and rollback; otherwise callers must serialize writes.
    /// Action panics propagate without rollback.
    fn commit_and_rollback(&self) -> Result<(), Self::Error> {
        let actions = std::mem::take(&mut *self.actions.borrow_mut());
        let _guard = self
            .commit_gate
            .as_ref()
            .map(|gate| {
                gate.lock()
                    .map_err(|_| InMemoryRepositoryError::DatabasePoisoned)
            })
            .transpose()?;
        let database = self
            .writer
            .upgrade()
            .ok_or(InMemoryRepositoryError::DatabaseDropped)?;
        let backup = database
            .lock()
            .map_err(|_| InMemoryRepositoryError::DatabasePoisoned)?
            .clone();

        for mut action in actions {
            if let Err(error) = action.act() {
                // Restore even if an action caught a panic that poisoned the mutex.
                // Leave the poison flag intact and preserve the original action error.
                *database
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner()) = backup;
                return Err(error);
            }
        }
        Ok(())
    }
}

/// Factory creating registries and units of work over a shared database.
///
/// The builder is shared across threads, but the registries it creates need
/// not be `Send`, `Sync` or `Clone`. Each `begin` invokes the builder with the
/// weak database handle and the factory's commit gate. Repositories may use
/// the same gate for reads. Every writer must respect it.
#[cfg_attr(docsrs, doc(cfg(feature = "in-memory")))]
pub struct InMemoryUnitOfWorkFactory<R, E = InMemoryRepositoryError> {
    registry_builder: Box<dyn Fn(Weak<Mutex<InMemoryDatabase>>, Arc<Mutex<()>>) -> R + Send + Sync>,
    database: Arc<Mutex<InMemoryDatabase>>,
    commit_gate: Arc<Mutex<()>>,
    error: PhantomData<fn() -> E>,
}

impl<R: Clone + Send + Sync + 'static> InMemoryUnitOfWorkFactory<R> {
    /// Creates a factory cloning a registry template for each unit of work.
    pub fn new(registry: R, database: Arc<Mutex<InMemoryDatabase>>) -> Self {
        Self::from_registry_factory(database, move |_, _| registry.clone())
    }
}

impl<R, E> InMemoryUnitOfWorkFactory<R, E> {
    /// Creates fresh registries using the database and commit gate supplied
    /// by the factory. The builder must not retain registries or repositories
    /// across calls if they should expire with their unit of work.
    pub fn from_registry_factory(
        database: Arc<Mutex<InMemoryDatabase>>,
        builder: impl Fn(Weak<Mutex<InMemoryDatabase>>, Arc<Mutex<()>>) -> R + Send + Sync + 'static,
    ) -> Self {
        Self {
            registry_builder: Box::new(builder),
            database,
            commit_gate: Arc::new(Mutex::new(())),
            error: PhantomData,
        }
    }
}

impl<R: 'static, E: From<InMemoryRepositoryError> + 'static> UnitOfWorkFactory
    for InMemoryUnitOfWorkFactory<R, E>
{
    type Error = E;
    type Registry = R;

    /// Builds a registry and creates an empty unit of work sharing the commit gate.
    fn begin(
        &self,
    ) -> Result<Box<dyn UnitOfWork<Error = E, Registry = R>>, E> {
        let weak = Arc::downgrade(&self.database);
        let registry = (self.registry_builder)(weak.clone(), self.commit_gate.clone());
        Ok(Box::new(
            InMemoryUnitOfWork::new(registry, weak).with_commit_gate(self.commit_gate.clone()),
        ))
    }
}
