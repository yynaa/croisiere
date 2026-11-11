# Application: Units of Work

A unit of work groups the changes made while handling one request, and persists them as a single
transaction: either every change is stored, or none of them is.

Changing two accounts in a transfer, for example, means saving two snapshots. If the second save
fails, the first one must not stay stored. The unit of work takes care of that: the application
registers what to save and delete on it, then commits them all at once.

In `croisiere`, units of work are described by the [`UnitOfWork`] trait, and started by a
[`UnitOfWorkFactory`]. As for [repositories](./repositories.md), the application only describes them:
implementing them for an actual storage is the
[infrastructure's job](../infrastructure/units_of_work.md).

## The registry

A unit of work gives access to the repositories its changes go through, with a registry. The registry
is a type of yours, usually a struct with, for each snapshot type, a way to read and a way to write.

A good pattern is to have the registry hold these:

```rust,ignore
use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};

/// Starts a new read repository. Each one is owned by whoever asked for it, so it can be kept
/// around, but it does not see the changes of its unit of work that have not been committed yet.
pub type ReadRepositoryFactory<S> = Box<dyn Fn() -> Box<DynReadRepository<S>>>;

/// A write repository, which actionable snapshots save and delete through while their unit of
/// work commits.
pub type SharedWriteRepository<S> = Rc<RefCell<DynWriteRepository<S>>>;
```

`DynReadRepository` and `DynWriteRepository` are the trait objects from
[the repositories page](./repositories.md#holding-repositories). The registry then has one of each per
snapshot type:

```rust,ignore
pub struct Registry {
    read_accounts: ReadRepositoryFactory<AccountSnapshot>,
    write_accounts: SharedWriteRepository<AccountSnapshot>,
}

impl Registry {
    /// Called by the infrastructure when it starts a unit of work.
    pub fn new(
        read_accounts: ReadRepositoryFactory<AccountSnapshot>,
        write_accounts: SharedWriteRepository<AccountSnapshot>,
    ) -> Self {
        Self { read_accounts, write_accounts }
    }

    pub fn read_accounts(&self) -> Box<DynReadRepository<AccountSnapshot>> {
        (self.read_accounts)()
    }

    pub fn write_accounts(&self) -> Weak<RefCell<DynWriteRepository<AccountSnapshot>>> {
        Rc::downgrade(&self.write_accounts)
    }
}
```

Read repositories are handed out to keep: they only read what is already stored.

Write repositories are owned by the registry alone, which hands out [`Weak`] pointers to them. Pending
changes hold these pointers, so nothing keeps a write repository alive after its unit of work is gone.
Acting through one whose unit of work was dropped fails with [`RepositoryDropped`].

## Actionable snapshots

A change waiting for the unit of work to commit is an [`ActionableSnapshot`]: a
[versioned snapshot](./repositories.md#versioned-snapshots), the repository to store it with, and the
[`SnapshotAction`] to perform on it:

| Action                 | On commit                                        |
|------------------------|--------------------------------------------------|
| `SnapshotAction::Save` | the snapshot is saved with `WriteRepository::save`   |
| `SnapshotAction::Delete` | the snapshot is deleted with `WriteRepository::delete` |
| `SnapshotAction::None` | nothing happens                                  |

They are made from a [`VersionedSnapshot`] in one of two ways:

- `make_actionable(repository, action)` pairs it with the repository and action as is.
- `do_and_make_actionable(repository, action, f)` first runs `f` on the aggregate behind the snapshot,
  like [`do_as_aggregate`](./repositories.md#working-on-the-aggregate-behind-a-snapshot), then makes
  the updated snapshot actionable. It also returns the events `f` emitted.

Both increment the snapshot's version, which is what lets the repository detect a concurrent change
when saving.

## Using a unit of work

A unit of work is started from a factory, for one request. Get the repositories from its registry,
read the snapshots you need, register the changes, then commit:

```rust,ignore
use croisiere::application::{SnapshotAction, UnitOfWorkFactory};

pub type Factory = Arc<dyn UnitOfWorkFactory<Error = RepositoryError, Registry = Registry>>;

pub fn deposit(factory: &Factory, id: u32, cents: i64) -> Result<Vec<AppEvent>, AppError> {
    let uow = factory.begin();
    let registry = uow.registry();

    let snapshot = registry
        .read_accounts()
        .get(HashSet::from([AccountFilter::ById(id)]), None, None)?
        .into_iter()
        .next()
        .ok_or(AppError::NotFound(id))?;

    let (actionable, events) = snapshot.do_and_make_actionable::<AppEvent, AppError, _>(
        registry.write_accounts(),
        SnapshotAction::Save,
        |account| account.deposit(cents),
    )?;

    uow.register_action(Box::new(actionable));
    uow.commit_and_rollback()?;
    Ok(events)
}
```

Nothing is stored before `commit_and_rollback`. If the request fails before it, through `?` for
example, the unit of work is dropped with its pending changes, and the storage is left untouched.

### Creating an aggregate

A new aggregate has no snapshot yet. Create it with its
[aggregate function](../domain/aggregates.md#aggregate-functions), assemble it into a snapshot at
version `0`, and save it: making it actionable brings it to version `1`.

```rust,ignore
let (aggregate, events) = AccountAggregate::open(id, owner)?;
let actionable = VersionedSnapshot::<AccountSnapshot>::assemble(aggregate, 0)
    .make_actionable(registry.write_accounts(), SnapshotAction::Save);

uow.register_action(Box::new(actionable));
uow.commit_and_rollback()?;
```

### Deleting an aggregate

Read its snapshot, and make it actionable with `SnapshotAction::Delete`:

```rust,ignore
let actionable = snapshot.make_actionable(registry.write_accounts(), SnapshotAction::Delete);

uow.register_action(Box::new(actionable));
uow.commit_and_rollback()?;
```

### Several changes

Register as many changes as the request needs, on any of the registry's repositories, before
committing. They are stored together, or not at all:

```rust,ignore
uow.register_action(Box::new(withdrawn));
uow.register_action(Box::new(deposited));
uow.commit_and_rollback()?;
```

## Errors

A unit of work holds changes for every repository of its registry, and reports their failures as its
own error type. Registered changes must therefore fail with exactly that type: the unit of work and
every repository share the same [repository error](./repositories.md#the-repository-error), which
converts from [`RepositoryDropped`].

The application then turns it into its own
[application error](./repositories.md#the-application-error), as `deposit` does above with `?`.

[`UnitOfWork`]: https://docs.rs/croisiere/latest/croisiere/application/trait.UnitOfWork.html
[`UnitOfWorkFactory`]: https://docs.rs/croisiere/latest/croisiere/application/trait.UnitOfWorkFactory.html
[`ActionableSnapshot`]: https://docs.rs/croisiere/latest/croisiere/application/struct.ActionableSnapshot.html
[`SnapshotAction`]: https://docs.rs/croisiere/latest/croisiere/application/enum.SnapshotAction.html
[`VersionedSnapshot`]: https://docs.rs/croisiere/latest/croisiere/application/struct.VersionedSnapshot.html
[`RepositoryDropped`]: https://docs.rs/croisiere/latest/croisiere/application/struct.RepositoryDropped.html
[`Weak`]: https://doc.rust-lang.org/std/rc/struct.Weak.html
