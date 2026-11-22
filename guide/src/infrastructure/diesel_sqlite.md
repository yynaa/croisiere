# Infrastructure: SQLite Storage

`croisiere::infrastructure::persistence::diesel_sqlite` provides units of work and a factory for
SQLite storage over [Diesel](https://diesel.rs), enabled by the `diesel-sqlite` feature. The
factory owns a connection pool; every unit of work owns exactly one connection checked out of
that pool, and each commit runs in one Diesel transaction on it, so an action error rolls the
whole batch back natively.

For a complete integration, follow the [counter example](../examples/counter_example.md); the
DTO and repository layers work identically over the in-memory backend.

## Factory and connection ownership

`SqliteUnitOfWorkFactory<R, E>` owns the pool and the commit gate. Two construction paths:

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
use croisiere::infrastructure::persistence::diesel_sqlite::SqliteUnitOfWorkFactory;

let factory = SqliteUnitOfWorkFactory::from_registry_factory(
    "file:data.db",
    |pool, writer, gate| {
        let connection = SqliteConnection { pool, writer, gate };
        // Build `Rc<RefCell<_>>` repositories holding `connection.clone()`.
        Registry::new(/* ... */)
    },
)?;
```

## Reads and writes use different connections

`SqliteConnection` holds the pool, the weak slot of its unit of work's connection and the
commit gate. DTO operations choose between two access paths:

| Method | Connection | Use |
|--------|-----------|-----|
| `with_read_connection` | A fresh pooled connection | Select operations. Sees committed storage, works after the unit of work is dropped. Holds the commit gate, so it cannot observe a partial commit. |
| `with_connection` | The unit of work's connection | Insert, update, delete and existence checks. During a commit, the operation runs inside the open transaction. |

Reads on a fresh connection make readers independent of the registry: an owned reader outlives
its unit of work and keeps working as long as the factory's pool does. Writes always land on
the unit of work's connection, so the commit's transaction covers them. Do not call
`with_read_connection` during a unit-of-work commit: the commit holds the gate and the read
would deadlock.

```rust,ignore
impl DTOSelect for CounterDTO {
    type Filter = CounterDTOFilter;
    type Sorter = CounterDTOSorter;

    fn select(conn: &mut Connection, filters: HashSet<CounterDTOFilter>, sorter: Option<CounterDTOSorter>, pagination: Option<Pagination>) -> Result<Vec<Self>, RepositoryError> {
        conn.with_read_connection(|connection| {
            // Translate filters and run the query with Diesel here.
        })
    }
}
```

## Transactions and version checks

`commit_and_rollback()` opens one Diesel transaction (`BEGIN`), runs queued actions in order and
commits. On the first action error it rolls the transaction back — undoing every write of the
batch, including writes of earlier actions — and returns the original error. Action panics
propagate without rollback. The batch is consumed on success or failure.

Because rollback is native, DTO write operations need no snapshot backup, but they keep the
version checks: include `version = incoming_version - 1` in the `WHERE` predicate and treat a
zero affected-row count as a conflict, failing the transaction. See
[checking versions](./repositories.md#checking-versions).

Units of work from one factory serialize their commits through the shared gate: two open write
transactions would contend for SQLite's write lock and fail with `SQLITE_BUSY`. Every writer to
the same pool must respect that gate; independent factories create independent gates.

## Typed backend errors

`SqliteRepositoryError` describes backend failures:

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
`SqliteRepositoryError`.
