# Infrastructure: PostgreSQL Storage

`croisiere::infrastructure::persistence::diesel_pg` provides units of work and a factory for
PostgreSQL storage over [Diesel](https://diesel.rs), enabled by the `diesel-pg` feature. It
mirrors the [SQLite backend](./diesel_sqlite.md) exactly: the factory owns a connection pool;
every unit of work owns exactly one connection checked out of that pool; each commit runs in
one Diesel transaction on it, so an action error rolls the whole batch back natively.

Read this page for the naming and the PostgreSQL-specific notes; the
[SQLite page](./diesel_sqlite.md) explains the shared mechanics in detail — connection
ownership, the read/write connection split, transaction boundaries and version checks.

## Factory and connection ownership

`PostgresUnitOfWorkFactory<R, E>` owns the pool and the commit gate, with the same three
construction paths as the SQLite factory:

| Constructor | Pool behavior |
|-------------|---------------|
| `new(database_url, registry)` | Builds the pool with default settings and clones a registry template on each `begin()`. Requires `R: Clone + Send + Sync + 'static`. Building the pool opens and tests a connection, so it is fallible. |
| `from_registry_factory(database_url, builder)` | Same pool, but calls a builder for each `begin()`, supporting a chosen error `E` and registries that need not be `Clone`, `Send` or `Sync`. Fallible for the same reason. |
| `from_pool(pool, builder)` | Accepts a caller-built pool, for tuning size, timeouts and connection setup. Infallible. |

`begin()` checks one connection out of the pool, parks it in a slot and hands the builder the
shared pool, a weak handle to that slot and the commit gate. It is fallible: pool exhaustion or
a connection failure is reported instead of a unit of work. The unit of work keeps its
connection until it is dropped, which returns it to the pool.

```rust,ignore
use croisiere::infrastructure::persistence::diesel_pg::PostgresUnitOfWorkFactory;

let factory = PostgresUnitOfWorkFactory::from_registry_factory(
    "postgres://croisiere:secret@localhost/croisiere",
    |pool, writer, gate| {
        let connection = PostgresConnection { pool, writer, gate };
        // Build `Rc<RefCell<_>>` repositories holding `connection.clone()`.
        Registry::new(/* ... */)
    },
)?;
```

## Reads and writes use different connections

`PostgresConnection` holds the pool, the weak slot of its unit of work's connection and the
commit gate. The access paths match the SQLite backend:

| Method | Connection | Use |
|--------|-----------|-----|
| `with_read_connection` | A fresh pooled connection | Select operations. Sees committed storage, works after the unit of work is dropped. Holds the commit gate, so it cannot observe a partial commit. |
| `with_connection` | The unit of work's connection | Insert, update, delete and existence checks. During a commit, the operation runs inside the open transaction. |

Do not call `with_read_connection` during a unit-of-work commit: the commit holds the gate and
the read would deadlock.

## Transactions and version checks

PostgreSQL uses the same ANSI transaction manager as SQLite: `commit_and_rollback()` opens one
transaction (`BEGIN`), runs queued actions in order and commits, and an action error rolls the
transaction back — undoing every write of the batch — before the original error is returned.
DTO write operations keep the version checks: include `version = incoming_version - 1` in the
`WHERE` predicate and treat a zero affected-row count as a conflict, failing the transaction.
See [checking versions](./repositories.md#checking-versions).

Units of work from one factory serialize their commits through the shared gate, as for SQLite;
every writer to the same pool must respect it.

## Typed backend errors

`PostgresRepositoryError` describes backend failures, with the same variants as
`SqliteRepositoryError`:

| Variant | Meaning |
|---------|---------|
| `ConnectionDropped` | The unit of work's connection was released to the pool before the operation ran. |
| `ConnectionPoisoned` | A connection mutex was poisoned by a previous panic. |
| `Pool(r2d2::Error)` | The pool could not provide a connection within its timeout. |
| `Diesel(diesel::result::Error)` | A query or transaction operation failed. |
| `ActionFailed(String)` | An operation reported a failure; DTOs use this for version conflicts. |

Keep these concrete errors in infrastructure and convert them into the application's
database-agnostic error, as for [in-memory storage](./in_memory.md#typed-backend-errors). `begin()`
reports pool failures through the unit-of-work error type `E`, which must convert from
`PostgresRepositoryError`.
