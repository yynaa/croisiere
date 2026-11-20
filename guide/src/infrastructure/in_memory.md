# Infrastructure: In-memory Storage

`croisiere::infrastructure::persistence::in_memory` provides tables, units of work and a factory for
storage in memory. It is available with the default `in-memory` feature. Building with
`--no-default-features` disables this module and keeps the general DTO repository traits available.

For a complete integration, follow the [counter example](../examples/counter_example.md). It uses the library
factory and unit of work directly, with application-specific DTOs and repositories.

## Database tables

Create an empty database with `InMemoryDatabase::default()`. Share it through an
`Arc<Mutex<InMemoryDatabase>>` when repositories and units of work need the same storage.

Each named table holds owned rows in insertion order. Rows are cloneable, type-erased values;
operations recover their concrete type by downcasting. Use one concrete row type per table.
The counter example uses one table for its DTO, with its UUID stored in each row.

| Method | Behavior |
|--------|----------|
| `insert(table, row)` | Appends a row, creating the table if needed. Requires `Any + Clone + Send + Sync`. |
| `read_rows::<O>(table)` | Clones rows in insertion order. Requires `O: Clone + 'static`. |
| `update::<O>(table, predicate, update)` | Updates matching rows and returns how many matched. |
| `delete::<O>(table, predicate)` | Removes matching rows, preserving the remaining order, and returns how many were removed. |

Read, update and delete return `TableNotFound(name)` if the table has never been created. After
its last row is deleted, the table remains present and empty. The counter selector treats a missing
`counters` table as an empty result so queries also work before the first insertion.

Insertion does not check uniqueness or row types. If incompatible types are mixed in one table,
subsequent operations can return `DowncastFromAnyImpossible`. Update and delete check row types
before modifying rows, so a downcast failure leaves the table unchanged.

The database methods do not allocate identifiers, interpret filters, enforce versions or provide
locking. Those responsibilities belong to DTO operations and their owner. Cloning the database
clones its tables and rows. Rollback follows each row's `Clone` semantics: shared mutable state
inside a row remains shared if its clone shares it.

## DTO metadata and connections

For operations that use the default backend connection and error, implement `InMemoryDTO`:

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

This supplies a blanket `DTO` implementation with:

- `ConnectionData = Weak<Mutex<InMemoryDatabase>>`;
- `Error = InMemoryRepositoryError`;
- `NAME` forwarded from the marker.

It does not implement select, insert, update, delete or snapshot conversions. Implement those for
your DTO. Do not also implement `DTO` directly on a type implementing `InMemoryDTO`, because the
implementations would overlap.

If you need an application repository error or a connection that also carries the commit gate,
implement `DTO` directly instead of the marker. The library's `InMemoryConnection` holds the weak
database handle and shared commit gate. The counter example uses:

```rust,ignore
use croisiere::infrastructure::persistence::in_memory::InMemoryConnection;

impl DTO for CounterDTO {
    const NAME: &'static str = "counters";
    type ConnectionData = InMemoryConnection;
    type Error = RepositoryError;
}
```

Construct `InMemoryConnection { database, gate }` from the handles supplied to the factory's
registry builder. Cloning it shares both handles without keeping the database alive.
Use `connection.with_database(|database| ...)` to upgrade and lock the database for an operation.
The callback returns `Result<T, E>`, where `E: From<InMemoryRepositoryError>`, so it can use your
application's repository error. Access failures convert into that error type; callback errors
propagate unchanged. The helper only acquires the database mutex and does not roll back changes
itself. Select operations also acquire the commit gate first. Write operations run
inside a unit-of-work commit, which already holds that gate, so they must not acquire it again.

## Typed backend errors

`InMemoryRepositoryError` describes backend failures:

| Variant | Meaning |
|---------|---------|
| `DatabaseDropped` | The weak database handle could not be upgraded. |
| `DatabasePoisoned` | A required mutex was poisoned. |
| `TableNotFound(String)` | The named table has not been created. |
| `DowncastFromAnyImpossible` | A stored row has an unexpected concrete type. |
| `ActionFailed(String)` | An operation reported a failure; the counter DTO uses this for version conflicts. |

Keep these concrete errors in infrastructure. To expose the application's database-agnostic error,
implement `From<InMemoryRepositoryError>` there and wrap the original error in
`RepositoryError::ImplementationError(anyhow::Error)`. This retains its type and error chain.
Do not turn it into a string before wrapping it.

## Units of work

`InMemoryUnitOfWork<R, E>` holds a registry and pending actions returning the shared error `E`.
The error must implement `From<InMemoryRepositoryError>` and defaults to that backend error type.
For actionable snapshots, choose an error that also converts from `RepositoryDropped`.

`InMemoryUnitOfWork::new(registry, writer)` receives a registry and weak database handle.
`with_commit_gate(gate)` configures a shared lock across commit and rollback.

- `registry()` returns the supplied registry.
- `register_action()` queues a change without writing it.
- `commit_and_rollback()` consumes the batch, saves a database clone, and runs actions in order.
  On the first returned error it restores that clone and returns the original error.

The batch is consumed on success or failure, including actions not executed after an error.
Dropping an uncommitted unit of work discards pending changes. Action panics do not trigger
rollback. See [transaction boundaries](./units_of_work.md#commit-and-rollback) for the full behavior.

## Factory and registry construction

`InMemoryUnitOfWorkFactory<R, E>` owns the database and a commit gate. It returns the library's
`InMemoryUnitOfWork<R, E>` directly, so the example needs no factory or unit-of-work implementation.
The factory requires `R: 'static` and `E: From<InMemoryRepositoryError> + 'static` to implement
`UnitOfWorkFactory`.

There are two construction paths:

| Constructor | Registry behavior |
|-------------|-------------------|
| `new(registry, database)` | Clones a template on each `begin()`. Requires `R: Clone + Send + Sync + 'static` and uses the default backend error. |
| `from_registry_factory(database, builder)` | Calls a builder for each `begin()`. Supports a chosen error `E` and registries that need not be `Clone`, `Send` or `Sync`. |

The builder receives `Weak<Mutex<InMemoryDatabase>>` and `Arc<Mutex<()>>`, then returns `R`.
It must implement `Fn + Send + Sync + 'static`. It can create `Rc<RefCell<_>>` repositories on the
calling thread because those repositories are returned to the unit of work, not stored in the
shared factory. Avoid retaining them across calls when they should expire with their unit of work.
The registry is an application struct; no registry-construction trait is required.

All units of work from a factory use its shared gate for the whole commit and rollback. Give
repository reads the same gate to prevent partial reads. Every writer to the same database must
respect it; two independently constructed factories have different gates. The database mutex alone
only protects individual operations. See [why the commit gate is separate](./units_of_work.md#database-locks-and-the-commit-gate).
