# Counter persistence example

Run from the workspace root:

```sh
cargo run -p croisiere-example
```

The demo creates `visits` and `orders`, increments `visits`, lists counters by
descending count, deletes `orders`, and lists the remaining counter.
Storage is in memory and lasts for the lifetime of the factory.

`src/infrastructure/persistence.rs` implements the DTO operations and repository
adapters. `CounterFactory` creates fresh repositories for each unit of work,
using the library's `InMemoryUnitOfWork` for rollback. Application adapters
preserve `AppError` while the backend keeps `InMemoryRepositoryError`.

Inserts require version 1. Updates and deletes require the stored version to
precede the incoming version by one. Filters are combined with AND; an empty
filter set selects everything. Sorting by descending count uses the name to
break ties, and pagination follows filtering and sorting.

A shared gate serializes commits and reads, preventing partial reads and
rollback from overwriting another commit. Writes are performed by registered
actions during commit. Dropping an uncommitted unit of work discards its actions.
Returned action errors trigger rollback; panics follow the backend's behavior
and do not roll back.

```sh
cargo test -p croisiere-example
```
