# Infrastructure: Units of Work

A unit of work owns the repositories and pending snapshot actions for one request. Its
`commit_and_rollback()` method runs those actions as one transaction: an error must undo earlier
writes in the same batch. The application registers changes against the
[`UnitOfWork`](../application/units_of_work.md) trait; infrastructure supplies the transaction.

## Using the in-memory implementation

For storage in memory, use the library's `InMemoryUnitOfWork<R, E>` and
`InMemoryUnitOfWorkFactory<R, E>`. Implement the DTO operations and construct your application's
registry. The library handles the action queue, commit and rollback.

The factory's registry builder receives the database handle and commit gate. Each `begin()` creates
fresh repositories connected to that storage and returns a library unit of work. This supports a
shared factory with registries containing `Rc<RefCell<_>>` repositories local to each request.
See [factory construction](./in_memory.md#factory-and-registry-construction) and the
[counter example](../examples/counter_example.md#constructing-the-factory).

You can also construct a library unit of work directly:

```rust,ignore
use croisiere::infrastructure::persistence::in_memory::InMemoryUnitOfWork;

let uow = InMemoryUnitOfWork::<Registry, RepositoryError>::new(registry, database_handle)
    .with_commit_gate(shared_gate);
```

Here, `database_handle` is a `Weak<Mutex<InMemoryDatabase>>` and `shared_gate` is an
`Arc<Mutex<()>>`. Every writer to the same database must respect the same gate. The library factory
supplies it automatically to the units of work it creates and to their registry builders.

## One repository error

DTO operations, repositories and registered actions share the application's `RepositoryError`.
Use that same type as the unit of work's `E`. It converts from `RepositoryDropped` for actionable
snapshots and from `InMemoryRepositoryError` for backend failures.

The application can remain database-agnostic:

```rust,ignore
// Application layer:
#[derive(Debug)]
pub enum RepositoryError {
    RepositoryDropped,
    ImplementationError(anyhow::Error),
}

// Infrastructure layer:
impl From<InMemoryRepositoryError> for RepositoryError {
    fn from(error: InMemoryRepositoryError) -> Self {
        Self::ImplementationError(anyhow::Error::new(error))
    }
}
```

The concrete infrastructure error is retained inside `anyhow::Error`. An action's repository error
is returned unchanged after rollback. Commands and queries convert it into their broader `AppError`
with `From<RepositoryError>`. There is no need for an action adapter or a second unit of work just
to translate errors. See [repository errors](../application/repositories.md#the-repository-error).

## Commit and rollback

Registration queues an action without performing a write. On commit, the in-memory implementation
consumes the pending batch, acquires its commit gate if configured, and clones the database. It runs
actions in registration order. If an action returns an error, it restores the saved database and
returns that error. All actions in the batch are discarded, including those not reached after a
failure; a later commit cannot accidentally retry them.

Dropping an uncommitted unit of work discards its actions. Action panics propagate without rollback.
Cloned rows must not share mutable state if rollback is expected to restore that state. The storage
lasts only as long as an owner retains the database, normally the factory.

## Database locks and the commit gate

The database mutex protects individual table operations. DTO operations lock and unlock it
separately, so it does not make a whole batch atomic. Without commit serialization, another request
could write between two actions. If the first request then failed, restoring its backup could erase
the second request's successful write.

The commit gate covers the whole batch, including backup and rollback. Readers can share it to avoid
seeing intermediate writes. Holding the database mutex across the batch would also exclude other
writers, but the current DTO operations would deadlock when they tried to lock that mutex again.
The separate gate follows from this connection design. A transaction-scoped connection that owns
one database lock for the whole commit could use that lock instead.

Use one factory per shared database, or otherwise arrange for every writer to use the same gate.
Independent factories create independent gates even when they receive the same database handle.

## Other storage backends

For a SQL backend or another storage system, implement `UnitOfWork` and `UnitOfWorkFactory` around
its transaction API. `begin()` is fallible: report the failure that prevented the unit of work — for
example a connection pool that could not provide a connection. Connections returned by the
repositories must participate in that same transaction. Execute every action, commit only if all
succeed, and roll back on the first error.
The DTO repository blanket implementations stop on errors but do not provide transactions.

The library provides such implementations for SQLite and PostgreSQL: see
[SQLite storage](./diesel_sqlite.md) and [PostgreSQL storage](./diesel_pg.md).

Updates and deletes must check identity and `version = incoming_version - 1` in the write itself.
A zero affected-row count is a conflict that must fail the transaction, undoing earlier writes too.
An existence check only decides insert versus update; it cannot replace this concurrency check.
See [checking versions](./repositories.md#checking-versions).
