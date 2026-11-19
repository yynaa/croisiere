//! Unit-of-work and factory implementation for in-memory storage.

use std::{
    cell::RefCell,
    collections::VecDeque,
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
/// Callers must prevent concurrent writes while committing: actions acquire
/// the database lock themselves, so it cannot be held across the whole batch.
/// Rollback restores cloned rows and follows their `Clone` semantics.
pub struct InMemoryUnitOfWork<R> {
    registry: R,

    writer: Weak<Mutex<InMemoryDatabase>>,
    actions:
        RefCell<VecDeque<Box<dyn AbstractActionableSnapshot<Error = InMemoryRepositoryError>>>>,
}

impl<R> InMemoryUnitOfWork<R> {
    /// Creates a unit of work with `registry`, a weak writer and no actions.
    pub fn new(registry: R, writer: Weak<Mutex<InMemoryDatabase>>) -> Self {
        Self {
            registry,
            writer,
            actions: RefCell::new(VecDeque::new()),
        }
    }
}

impl<R> UnitOfWork for InMemoryUnitOfWork<R> {
    type Registry = R;
    type Error = InMemoryRepositoryError;

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
    /// held while actions run. Callers must serialize writes during this method.
    /// Action panics propagate without rollback.
    fn commit_and_rollback(&self) -> Result<(), Self::Error> {
        let actions = std::mem::take(&mut *self.actions.borrow_mut());
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

/// Factory holding a registry template and the shared database.
///
/// `begin` clones the registry and gives the new unit of work a weak database
/// pointer. It does not construct repositories or open a transaction. Registry
/// cloning semantics determine which repository state is shared.
///
/// Commits require callers to
/// serialize database writes; see [`InMemoryUnitOfWork`].
pub struct InMemoryUnitOfWorkFactory<R>
where
    R: Clone,
{
    registry: R,
    database: Arc<Mutex<InMemoryDatabase>>,
}

impl<R: Clone> InMemoryUnitOfWorkFactory<R> {
    /// Creates a factory using a registry template and a shared database.
    pub fn new(registry: R, database: Arc<Mutex<InMemoryDatabase>>) -> Self {
        Self { registry, database }
    }
}

impl<R: Clone + Send + Sync + 'static> UnitOfWorkFactory for InMemoryUnitOfWorkFactory<R> {
    type Error = InMemoryRepositoryError;
    type Registry = R;

    /// Clones the registry and creates a unit of work with an empty queue.
    fn begin(&self) -> Box<dyn UnitOfWork<Error = Self::Error, Registry = Self::Registry>> {
        let weak = Arc::downgrade(&self.database);
        Box::new(InMemoryUnitOfWork::new(self.registry.clone(), weak))
    }
}
