# Counter Example

`crates/croisiere-example` is a runnable example of the domain, application and persistence layers
working together. Run it from the workspace root:

```sh
cargo run -p croisiere-example
```

It creates `visits` and `orders`, increments `visits`, lists counters by descending count, deletes
`orders`, and lists the remaining counter. Storage is in memory and lasts for the factory's lifetime.
The example keeps its source focused on the implementation; it has no test module.

## Source layout

The infrastructure follows the same separation as the book:

```text
src/
  domain/
    entities.rs
    value_objects.rs
    aggregates.rs
  application/
    snapshots.rs
    registry.rs
    errors.rs
    commands.rs
    queries.rs
  infrastructure/
    mod.rs
    persistence/
      mod.rs
      dtos.rs
      repositories.rs
      units_of_work.rs
```

`persistence/mod.rs` declares the child modules, imports the library's `InMemoryConnection` and
exports factory construction. Database access uses the connection's `with_database` helper.
DTO operations belong in `dtos.rs`, repository
implementations and backend error conversions in `repositories.rs`, and library factory setup in
`units_of_work.rs`.

## Identities and rows

The counter's name is its domain identity. It is a value object on the entity:

```rust,ignore
#[derive(Debug, Default, Clone, PartialEq, Eq, Hash, ValueObject)]
pub struct CounterName(pub String);

#[derive(Debug, Default, Entity)]
pub struct Counter {
    pub name: CounterName,
}
```

The snapshot stores that name as a string alongside the count. Aggregate-to-snapshot conversion
extracts the value object's string; snapshot-to-aggregate conversion reconstructs the value object.
Commands, filters and queries continue to identify the counter by name.

The storage representation adds its own identifier and the optimistic-concurrency version:

```rust,ignore
#[derive(Debug, Clone)]
struct CounterDTO {
    internal_id: InternalId<Uuid>,
    name: String,
    counter: u64,
    version: u128,
}
```

`InternalId<Uuid>` means `Option<Uuid>`. Conversion from a versioned snapshot sets it to `None`.
`DTOInsert` assigns `Some(Uuid::new_v4())` and inserts the row into `counters`. The example uses
`uuid` 1.29.3 with its `v4` feature. There is one table for this DTO, with no sequence or auxiliary
ID table.

`DTOUpdate` locates the row by name, checks the expected previous version, and changes the count
and version. It keeps the stored internal UUID. Deleting and recreating a counter assigns a new
UUID, even when the domain name is the same. Neither snapshots nor domain entities carry this
storage key.

## Repository behavior

One concrete counter repository implements `DTORepository`, `DTOReadRepository<CounterSnapshot>`
and `DTOWriteRepository<CounterSnapshot>`. Blanket implementations supply its application read
and write traits. There is no outer repository forwarding calls to another DTO repository.

| Operation | Example behavior |
|-----------|------------------|
| Select | Applies every filter with AND; no filters selects all rows. A missing table is treated as empty. |
| Sort | Uses insertion order without a sorter; otherwise sorts by name or descending count, breaking count ties by name. |
| Paginate | Applies offset and limit after filtering and sorting. |
| Count | Counts all matches without pagination. |
| Exists | Returns `Result<bool, RepositoryError>`, locating rows by domain name and propagating lookup failures. |
| Insert | Rejects a duplicate domain name or a version other than 1, then assigns a UUID. |
| Update/delete | Requires the incoming version to follow the stored version by one; a mismatch fails the commit. |

The existence check chooses insert versus update. Each write still enforces its own identity and
version constraints, since another request can change a row between reading it and committing.

## Error boundary

The application defines `RepositoryError`, including `RepositoryDropped` and
`ImplementationError(anyhow::Error)`. DTO operations, repositories and the unit of work share that
type. Commands and queries report their broader `AppError`, converting repository failures with `?`.

Infrastructure retains the typed `InMemoryRepositoryError` and implements:

```rust,ignore
impl From<InMemoryRepositoryError> for RepositoryError {
    fn from(error: InMemoryRepositoryError) -> Self {
        Self::ImplementationError(anyhow::Error::new(error))
    }
}
```

This preserves the concrete backend error for diagnostics and downcasting without importing it
into the application. Since DTO-to-snapshot conversion is infallible here, the repository error
also implements `From<Infallible>`. Backend errors are not formatted into strings at this boundary.

## Constructing the factory

The source uses the library factory directly. Its registry-builder callback receives the database
handle and commit gate, and creates fresh repositories for each unit of work:

```rust,ignore
{{#include ../../../crates/croisiere-example/src/infrastructure/persistence/units_of_work.rs}}
```

Only the callback must be `Send + Sync`; the returned registry can own `Rc<RefCell<_>>` repositories.
The registry creates owned read repositories on demand and holds the strong reference to its write
repository, handing out weak write references for pending actions. Dropping a unit of work releases
its writer, so pending actions cannot keep it alive; owned readers can outlive the unit of work.
No custom factory, action adapter, registry trait or unit-of-work implementation is needed.

## Running a request

The shared handling routines prepare a fresh unit of work and dispatch `AppCommand` or `AppQuery`.
Command context also supplies the actor and an event bus sender. Creation checks for an existing name, builds
an aggregate, assembles a version-0 snapshot and makes it actionable, advancing it to version 1.
Increment loads a snapshot and uses `do_and_make_actionable` to run the aggregate function and
queue the resulting save. Delete queues a `SnapshotAction::Delete` after loading the snapshot.
Each command commits before publishing its events and returns a `CommandResponse` outcome.

`ListCounters` only uses the registry's read access. It does not register actions or commit.
Its response distinguishes found counters from an empty selection. Events published by commands carry
the context's actor; the binary subscribes before sending requests and prints the received events.

The library factory gives all its units of work the same commit gate. The DTO selector acquires
that gate before reading, while DTO writes use the gate already held by the commit. Database
operations still lock the database mutex individually. This protects a whole commit and its rollback
without trying to reacquire the same database mutex recursively.

Returned action errors roll back earlier writes and discard the rest of the batch. Dropping an
uncommitted unit of work discards its actions. Panics follow the backend's behavior and do not roll
back. See [units of work](../infrastructure/units_of_work.md) for transaction boundaries and limitations.
