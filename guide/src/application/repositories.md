# Application: Repositories

A repository stores the [snapshots](./snapshots.md) of one type. It is how the application reads
aggregates from storage and writes them back, without knowing what the storage is: a database, files,
memory, another service.

The application only describes what a repository can do, with the [`ReadRepository`] and
[`WriteRepository`] traits. Implementing them for an actual storage is the
[infrastructure's job](../infrastructure/repositories.md).

## Versioned snapshots

Repositories don't store bare snapshots, but [`VersionedSnapshot`]s: a snapshot together with its
version.

```rust,ignore
pub struct VersionedSnapshot<S: Snapshot> {
    pub snapshot: S,
    pub version: u128,
}
```

The version is used for optimistic concurrency. It goes up by one every time the snapshot is handed
over to be saved or deleted. When saving, a repository compares it to the version it has stored: if the stored version
is not the one right before, someone else saved the same aggregate in the meantime, and the save is
rejected instead of silently overwriting their change.

A snapshot that was never saved starts at version `0`, and is saved with version `1`.

### Working on the aggregate behind a snapshot

[`VersionedSnapshot`] can convert to and from its aggregate while keeping the version:

- `dismantle()` turns it into its aggregate and version, failing if the snapshot is not a valid
  aggregate.
- `VersionedSnapshot::assemble(aggregate, version)` does the opposite.
- `do_as_aggregate(f)` does both around `f`, usually one of the
  [aggregate's functions](../domain/aggregates.md#aggregate-functions), and returns the updated
  snapshot with the events `f` emitted:

```rust,ignore
let (updated, events) = versioned
    .do_as_aggregate::<AppEvent, AppError>(|account| account.deposit(500))?;
```

`do_as_aggregate` converts the events into `V`, and the aggregate's and the conversion's errors into
`O`, so they fit your application's own event and error types.

## Reading

[`ReadRepository`] reads snapshots of type `S`:

```rust,ignore
pub trait ReadRepository<S: Snapshot> {
    type Error;

    fn get(
        &mut self,
        filters: HashSet<S::Filter>,
        sorter: Option<S::Sorter>,
        pagination: Option<Pagination>,
    ) -> Result<Vec<VersionedSnapshot<S>>, Self::Error>;

    fn count(
        &mut self,
        filters: HashSet<S::Filter>,
    ) -> Result<usize, Self::Error>;
}
```

- `filters` are the snapshot's [filters](./snapshots.md#filters). How they combine, for example
  whether a snapshot must match all of them or any, is up to the implementation.
- `sorter` is the snapshot's [sorter](./snapshots.md#sorters). Without one, snapshots come in the
  storage's own order.
- `pagination` restricts the result to a page. Without one, every matching snapshot is returned.

`count` takes only filters: ordering does not affect how many snapshots match, and the total
ignores pagination.

```rust,ignore
use croisiere::application::Pagination;

let page = repository.get(
    HashSet::from([AccountFilter::ByOwner("Ada".into())]),
    Some(AccountSorter::ById),
    Some(Pagination { limit: 20, offset: 40 }),
)?;
```

`limit` is the maximum number of snapshots returned, and `offset` the number of matching snapshots
skipped before the first one.

## Writing

[`WriteRepository`] writes snapshots of type `S`:

```rust,ignore
pub trait WriteRepository<S: Snapshot> {
    type Error;

    fn save(&mut self, snapshots: Vec<&VersionedSnapshot<S>>) -> Result<(), Self::Error>;
    fn delete(&mut self, snapshots: Vec<&VersionedSnapshot<S>>) -> Result<(), Self::Error>;
}
```

- `save` inserts the snapshots, or updates them if they are already stored.
- `delete` removes them.

Both should reject a snapshot whose version does not follow the stored one, as explained in
[versioned snapshots](#versioned-snapshots). Their error type covers that conflict, as well as any
failure of the storage.

You will usually not call `save` and `delete` yourself: the application registers what to save and
delete on a unit of work, which calls them together when it commits.

## Extensions

> [!TIP]
> **Define application-owned extension traits for common repository operations.** Helpers such as
> `get_unique` and `exists` can build on `ReadRepository` and return your own error type. Give the
> trait a blanket implementation so every compatible repository gets those helpers automatically,
> without requiring infrastructure implementations to repeat them.
> Prefer adding variants to your existing `RepositoryError` for extension failures instead of
> introducing a separate error type.

For example, the application can distinguish a storage failure from an unexpected number of matches:

```rust,ignore
use std::collections::HashSet;

use croisiere::application::{Pagination, ReadRepository, Snapshot, VersionedSnapshot};

pub trait ReadRepositoryExt<S: Snapshot>:
    ReadRepository<S, Error = RepositoryError>
{
    fn get_unique(
        &mut self,
        filters: HashSet<S::Filter>,
    ) -> Result<VersionedSnapshot<S>, RepositoryError> {
        // Two matches are enough to reject a nonunique result.
        let mut matches = self.get(
            filters,
            None,
            Some(Pagination { limit: 2, offset: 0 }),
        )?;
        if matches.len() > 1 {
            return Err(RepositoryError::NotUnique);
        }
        matches.pop().ok_or(RepositoryError::NotFound)
    }

    fn exists(&mut self, filters: HashSet<S::Filter>) -> Result<bool, RepositoryError> {
        self.count(filters).map(|count| count > 0)
    }
}

impl<S, R> ReadRepositoryExt<S> for R
where
    S: Snapshot,
    R: ReadRepository<S, Error = RepositoryError> + ?Sized,
{}
```

Import `ReadRepositoryExt` wherever you want to call these methods. The blanket implementation
covers every repository with the matching snapshot and error types, including trait objects through
`?Sized`. Use the same pattern for helpers built on `WriteRepository`.

Add `NotFound` and `NotUnique` to the application's `RepositoryError`, as shown below. Callers can
distinguish lookup expectations such as “exactly one match” from storage failures through variants
of that same error type. In particular, `exists` must propagate read errors rather than treat them
as `false`. These helpers describe the result of a read; they do not guarantee uniqueness or prevent
another request from changing the data before a later write.

## Errors

### The repository error

A unit of work commits the actions of every repository it holds, and reports their errors as one
type. All your repositories therefore share a single error type, covering persistence failures and
repository extension failures. Domain failures belong in `DomainError`. The repository error must
convert from [`RepositoryDropped`]:

```rust,ignore
use croisiere::application::RepositoryDropped;

#[derive(Debug)]
pub enum RepositoryError {
    /// A lookup requiring one match found none.
    NotFound,
    /// A lookup requiring one match found several.
    NotUnique,
    /// The snapshot was changed by someone else since it was read.
    Conflict,
    /// A failure specific to the persistence implementation.
    ImplementationError(anyhow::Error),
    RepositoryDropped,
}

impl From<RepositoryDropped> for RepositoryError {
    fn from(_: RepositoryDropped) -> Self {
        Self::RepositoryDropped
    }
}
```

The application defines this error without importing a database client or backend error.
Infrastructure keeps its own typed errors and converts them at the boundary:

```rust,ignore
// In infrastructure, where both types are available:
impl From<InMemoryRepositoryError> for RepositoryError {
    fn from(error: InMemoryRepositoryError) -> Self {
        Self::ImplementationError(anyhow::Error::new(error))
    }
}
```

`anyhow::Error::new` retains the original error. Formatting it to a string first would discard
its concrete type and error chain. Infrastructure can inspect the wrapped cause with
`downcast_ref::<InMemoryRepositoryError>()`; application code can report the error without
knowing which backend produced it. Typed backend errors remain useful for matching and diagnosis
inside infrastructure.

`Conflict` is an optional application-level category when callers need to distinguish concurrent
writes. The counter example instead wraps its backend's conflict failure in `ImplementationError`.
DTO conversion failures also convert directly into `RepositoryError` with `From<SpecificError>`;
an infallible conversion uses `From<Infallible>`.

### The application error

The application's own error type is then built on top of it, alongside the errors of the domain and of
the application itself:

```rust,ignore
#[derive(Debug)]
pub enum AppError {
    Domain(AccountError),
    Repository(RepositoryError),
    NotFound(u32),
    AlreadyExists(u32),
    InvalidSnapshot,
    BusClosed,
}

impl From<RepositoryError> for AppError {
    fn from(error: RepositoryError) -> Self {
        Self::Repository(error)
    }
}
```

Converting from the aggregate's error and from the snapshot's conversion error too lets
[`do_as_aggregate`](#working-on-the-aggregate-behind-a-snapshot) report them as `AppError`:

```rust,ignore
impl From<AccountError> for AppError {
    fn from(error: AccountError) -> Self {
        Self::Domain(error)
    }
}

impl From<InvalidSnapshot> for AppError {
    fn from(_: InvalidSnapshot) -> Self {
        Self::InvalidSnapshot
    }
}
```

With these, `?` turns a repository's `RepositoryError` into an `AppError` wherever the application
returns one.

## Holding repositories

The application holds repositories as trait objects, so it does not depend on their implementation.
Naming the two trait objects, with the repository error filled in, keeps their uses short:

```rust,ignore
/// The read repositories handed out by a unit of work.
pub type DynReadRepository<S> = dyn ReadRepository<S, Error = RepositoryError>;

/// The write repositories handed out by a unit of work.
pub type DynWriteRepository<S> = dyn WriteRepository<S, Error = RepositoryError>;
```

The application gets them from the registry of a
[unit of work](./units_of_work.md#the-registry).

[`ReadRepository`]: https://docs.rs/croisiere/latest/croisiere/application/trait.ReadRepository.html
[`WriteRepository`]: https://docs.rs/croisiere/latest/croisiere/application/trait.WriteRepository.html
[`VersionedSnapshot`]: https://docs.rs/croisiere/latest/croisiere/application/struct.VersionedSnapshot.html
[`RepositoryDropped`]: https://docs.rs/croisiere/latest/croisiere/application/struct.RepositoryDropped.html
