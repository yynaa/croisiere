# Infrastructure: Units of Work

The application describes [units of work](../application/units_of_work.md) with the [`UnitOfWork`]
and [`UnitOfWorkFactory`] traits. The infrastructure implements them for its storage, together with
the [repositories](./repositories.md) they hand out.

This page continues the storage kept in memory from the repositories page.

## The unit of work

A unit of work holds the registry it hands out, and the changes registered on it until it commits:

```rust,ignore
use std::{cell::RefCell, rc::Rc};

use croisiere::application::{AbstractActionableSnapshot, UnitOfWork};

type Actions = Vec<Box<dyn AbstractActionableSnapshot<Error = RepositoryError>>>;

pub struct InMemoryUnitOfWork {
    database: SharedDatabase,
    registry: Registry,
    /// Shared with the write repositories of the registry.
    staged: Rc<RefCell<Vec<StagedWrite>>>,
    actions: RefCell<Actions>,
}
```

`registry` and `register_action` only hand out and keep what they are given:

```rust,ignore
impl UnitOfWork for InMemoryUnitOfWork {
    type Error = RepositoryError;
    type Registry = Registry;

    fn registry(&self) -> &Registry {
        &self.registry
    }

    fn register_action(&self, actionable_snapshot: Box<dyn AbstractActionableSnapshot<Error = RepositoryError>>) {
        self.actions.borrow_mut().push(actionable_snapshot);
    }

    fn commit_and_rollback(&self) -> Result<(), RepositoryError> {
        // ...
    }
}
```

### Committing

`commit_and_rollback` stores every registered change as one transaction: if any of them fails, none of
them is stored, and the error is returned.

Each change is performed by calling `act()` on it, which calls `save` or `delete` on its write
repository. Here, that only stages the writes. They are then applied to a copy of the storage, which
replaces it only if every write succeeded:

```rust,ignore
fn commit_and_rollback(&self) -> Result<(), RepositoryError> {
    let actions = std::mem::take(&mut *self.actions.borrow_mut());
    for mut action in actions {
        action.act()?;
    }
    let staged = std::mem::take(&mut *self.staged.borrow_mut());

    let mut database = self.database.lock().map_err(|_| RepositoryError::Storage("poisoned".into()))?;
    let mut transaction = Database {
        accounts: database.accounts.clone(),
    };
    for write in staged {
        transaction.apply(write)?;
    }
    *database = transaction;
    Ok(())
}
```

The storage stays locked from the first write to the last, so no other unit of work can commit in
between.

With an actual database, `commit_and_rollback` calls `act()` on every change, which writes through the
database transaction, then commits that transaction. If anything fails, it rolls the transaction back
instead.

## The factory

The factory starts units of work. It is shared by every request, possibly across threads, hence the
`Sync + Send` bound of [`UnitOfWorkFactory`]. Here, it only holds the shared storage:

```rust,ignore
use croisiere::application::UnitOfWorkFactory;

#[derive(Clone, Default)]
pub struct InMemoryFactory {
    database: SharedDatabase,
}
```

`begin` builds the repositories of the new unit of work and gives them to the application's
[registry](../application/units_of_work.md#the-registry):

```rust,ignore
impl UnitOfWorkFactory for InMemoryFactory {
    type Error = RepositoryError;
    type Registry = Registry;

    fn begin(&self) -> Box<dyn UnitOfWork<Error = RepositoryError, Registry = Registry>> {
        let staged = Rc::new(RefCell::new(Vec::new()));

        let database = Arc::clone(&self.database);
        let read_accounts: ReadRepositoryFactory<AccountSnapshot> = Box::new(move || {
            Box::new(AccountReader {
                database: Arc::clone(&database),
            })
        });
        let write_accounts: SharedWriteRepository<AccountSnapshot> =
            Rc::new(RefCell::new(AccountWriter {
                staged: Rc::clone(&staged),
            }));

        Box::new(InMemoryUnitOfWork {
            database: Arc::clone(&self.database),
            registry: Registry::new(read_accounts, write_accounts),
            staged,
            actions: RefCell::new(Vec::new()),
        })
    }
}
```

- Every read repository reads the committed storage, so it does not see the changes of its unit of
  work that have not been committed yet.
- The write repository shares the staged writes with its unit of work, which applies them on commit.

With an actual database, `begin` opens a database transaction and gives it to the write repositories,
while the read repositories use their own connections.

The application only ever sees the factory as a
`dyn UnitOfWorkFactory<Error = RepositoryError, Registry = Registry>`:

```rust,ignore
let factory: Factory = Arc::new(InMemoryFactory::default());
```

[`UnitOfWork`]: https://docs.rs/croisiere/latest/croisiere/application/trait.UnitOfWork.html
[`UnitOfWorkFactory`]: https://docs.rs/croisiere/latest/croisiere/application/trait.UnitOfWorkFactory.html
