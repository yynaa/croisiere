# Infrastructure: In-memory Storage

The `croisiere::infrastructure::persistence::in_memory` module contains an in-memory backend. It is available with the `in-memory` feature, which is enabled by default. Building with
`--no-default-features` disables the module while retaining the DTO repository adapters.

The module exports `InMemoryDatabase`, `InMemoryRepositoryError`, `InMemoryDTO`,
`InMemoryUnitOfWork` and `InMemoryUnitOfWorkFactory`. Create an empty database with `InMemoryDatabase::default()` and a factory with
`InMemoryUnitOfWorkFactory::new(registry, database)`. Units of work support snapshot rollback on returned action errors, provided
callers serialize writes during commit.
The hand-written backend in the [repositories](./repositories.md) and
[units of work](./units_of_work.md) chapters is an implementation example, separate from these types.

## Database tables

`InMemoryDatabase` holds a map from table names to vectors of rows. Each row is stored as
a cloneable, type-erased value, so its concrete type is recovered by downcasting during reads, updates
and deletes. Use one concrete row type per table: insertion permits mixed types, but subsequent
operations fail if any row cannot be downcast to the requested type.

| Method | Behavior |
|--------|----------|
| `insert(table, row)` | Creates the table if necessary and appends a row. Requires `Any + Clone + Send + Sync`; performs no uniqueness or version checks. |
| `read_rows::<O>(table)` | Clones all rows in insertion order. Requires `O: Clone + 'static`. |
| `delete::<O>(table, predicate)` | Removes matching rows, preserves the order of remaining rows, and returns the number removed. |
| `update::<O>(table, predicate, update)` | Applies the callback to matching rows and returns the number matched. |

Read, delete and update return `TableNotFound(name)` for an unknown table. They return
`DowncastFromAnyImpossible` if any row has the wrong concrete type. Reading an existing empty table
returns an empty vector; deleting or updating it returns zero. Deleting every row leaves the table
in place.

Deletion evaluates all downcasts and predicates before removing rows. Updating downcasts all rows
before running any predicate or mutation callback. A downcast error therefore leaves the table
unchanged. This is not transaction support: callback panics are not rolled back, and separate
database operations do not commit atomically.

The database itself has no locks. Sharing and locking belong to its owner, typically an
`Arc<Mutex<InMemoryDatabase>>`. Use `InMemoryDatabase::default()` to create the initial empty database.

Cloning the database clones every table and row. Rollback follows each row's `Clone` semantics:
shared mutable state inside a row remains shared if cloning that row shares it.

## In-memory DTOs

`InMemoryDTO` requires `Debug + Clone` and a `NAME`. Implementing it supplies a blanket `DTO`
implementation:

```rust,ignore
use croisiere::infrastructure::persistence::in_memory::InMemoryDTO;

#[derive(Debug, Clone)]
struct AccountRow {
    id: u32,
    cents: i64,
    version: u128,
}

impl InMemoryDTO for AccountRow {
    const NAME: &'static str = "accounts";
}
```

The resulting `DTO` has the same name, `ConnectionData = Weak<Mutex<InMemoryDatabase>>`, and
`Error = InMemoryRepositoryError`. Implementors must not also implement `DTO` directly, because
that would conflict with the blanket implementation.

The weak handle does not keep the database alive. DTO operations must upgrade it and lock the mutex
before accessing rows. In addition to table and downcast errors, the error enum provides
`DatabaseDropped` and `DatabasePoisoned`. DTO operations can report version conflicts using `ActionFailed(String)`.

`InMemoryDTO` supplies metadata only. It does not implement `DTOSelect`, `DTOInsert`, `DTOUpdate`
or `DTODelete`, nor does it provide snapshot conversions. Those remain backend-specific work.
Filtering, ordering, pagination and version checks are not built into the database table methods.

## Units of work and their factory

`InMemoryUnitOfWork::new(registry, writer)` takes a registry and a weak database handle. It starts
with an empty `VecDeque` of actionable snapshots:

- `registry()` returns the supplied registry.
- `register_action()` appends an action without executing it.
- `commit_and_rollback()` clones the database and executes actions in registration order. It stops
  at the first returned error, restores the clone and returns that error. The entire batch is
  consumed on success or failure, including actions that were not executed.

No database lock is held while actions run, since DTO operations acquire the lock themselves.
Callers must prevent concurrent writes for the duration of commit; otherwise rollback could
overwrite another writer's changes. Action panics propagate without rollback. A dropped database
or an initially poisoned mutex returns the corresponding error before executing any action.

Actions must have `Error = InMemoryRepositoryError`. The error type also does not yet implement
`From<RepositoryDropped>`, which standard `ActionableSnapshot` values require for execution.

`InMemoryUnitOfWorkFactory<R>` owns an `Arc<Mutex<InMemoryDatabase>>` and a registry template.
Its `UnitOfWorkFactory` implementation requires `R: Clone + Send + Sync + 'static`. `begin()`
clones the registry, downgrades the database pointer and creates a unit of work with an empty queue.
It does not construct repositories or start a transaction. How much repository state is shared
depends on the registry's `Clone` implementation.

Construct it with `InMemoryUnitOfWorkFactory::new(registry, database)`. Applications whose registry
contains `Rc` repositories can instead construct fresh repositories in their own factory and use
`InMemoryUnitOfWork::new` directly. See `crates/croisiere-example` for a runnable integration that
adapts application errors and serializes commits and reads.
