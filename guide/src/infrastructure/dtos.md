# Infrastructure: DTOs

A DTO is the representation your storage reads and writes. An application
[snapshot](../application/snapshots.md) describes persisted application state; a DTO describes how
that state is represented by a particular storage. They can differ in field types, identifiers and
layout without exposing those storage details to the domain.

All the traits on this page are imported from `croisiere::infrastructure::persistence`.

## Describing a DTO

`DTO` requires `Debug + Clone`, a name, a connection type and an error type:

```rust,ignore
use croisiere::infrastructure::persistence::{DTO, InternalId};

#[derive(Debug, Clone)]
pub struct AccountDTO {
    pub internal_id: InternalId<i64>,
    pub id: u32,
    pub owner: String,
    pub cents: i64,
    pub version: u128,
}

impl DTO for AccountDTO {
    const NAME: &'static str = "accounts";

    type ConnectionData = AccountConnection;
    type Error = StorageError;
}
```

`AccountConnection` and `StorageError` are types supplied by your backend. The connection data may
be a pooled connection or a handle to a transaction owned elsewhere. DTO operations receive a mutable
reference to it.

`NAME` identifies the representation. The repository adapters do not interpret it as a table name or
check it against the repository's name; that meaning belongs to your implementation.

`InternalId<T>` is an alias for `Option<T>`. It can represent an unassigned storage identifier with
`None`, and an assigned one with `Some(id)`. It does not allocate identifiers or control whether a
write inserts or updates. The adapter asks the repository's `exists` method for that decision.

## Converting snapshots

The read adapter requires `TryFrom<AccountDTO> for VersionedSnapshot<AccountSnapshot>`. The write
adapter requires `From<VersionedSnapshot<AccountSnapshot>> for AccountDTO`, as well as the read
conversion bound to determine its error type. Write conversion is therefore infallible at this
boundary.

For an account snapshot with the corresponding fields, the conversions can look like this:

```rust,ignore
use croisiere::application::VersionedSnapshot;

impl From<VersionedSnapshot<AccountSnapshot>> for AccountDTO {
    fn from(value: VersionedSnapshot<AccountSnapshot>) -> Self {
        Self {
            internal_id: None,
            id: value.snapshot.id,
            owner: value.snapshot.owner,
            cents: value.snapshot.cents,
            version: value.version,
        }
    }
}

impl TryFrom<AccountDTO> for VersionedSnapshot<AccountSnapshot> {
    type Error = InvalidSnapshot;

    fn try_from(value: AccountDTO) -> Result<Self, Self::Error> {
        Ok(Self {
            snapshot: AccountSnapshot {
                id: value.id,
                owner: value.owner,
                cents: value.cents,
            },
            version: value.version,
        })
    }
}
```

This example assumes writes locate accounts by the application's `id`, since the snapshot carries
no `internal_id`. If your storage requires its internal key for updates, provide a way to recover it
or carry it in the snapshot. The adapters do not write generated identifiers back into snapshots.

Validate storage values during `TryFrom` when they cannot always form a valid snapshot. A conversion
error fails the entire read; the caller does not receive partial results. Conversion to an aggregate
and validation of its invariants remain the snapshot's separate responsibility.

## Selecting DTOs

`DTOSelect` adds the storage's filter and sorter types and the query operation:

```rust,ignore
pub trait DTOSelect: DTO {
    type Filter: Eq + Hash;
    type Sorter;

    fn select(
        conn: &mut Self::ConnectionData,
        filters: HashSet<Self::Filter>,
        sorter: Option<Self::Sorter>,
        pagination: Option<Pagination>,
    ) -> Result<Vec<Self>, <Self as DTO>::Error>;
}
```

Implement `From<S::Filter>` and `From<S::Sorter>` for the DTO's filter and sorter so the read adapter
can translate the application's query. Snapshot criteria and storage criteria may be different
types, and several snapshots may translate into the same DTO criteria.

`select` must apply filtering, sorting and pagination itself. The adapter forwards `Pagination`
unchanged and preserves the returned row order; it does not apply another query pass. Define how
filters combine, what an empty filter set means, and what ordering is used without a sorter.

## Writing DTOs

Each operation has its own trait:

```rust,ignore
pub trait DTOInsert: DTO {
    fn insert(self, conn: &mut Self::ConnectionData) -> Result<(), Self::Error>;
}

pub trait DTOUpdate: DTO {
    fn update(self, conn: &mut Self::ConnectionData) -> Result<(), Self::Error>;
}

pub trait DTODelete: DTO {
    fn delete(self, conn: &mut Self::ConnectionData) -> Result<(), Self::Error>;
}
```

The operations consume a DTO. The write adapter creates that DTO from a clone of the supplied
versioned snapshot, so it leaves the caller's snapshot untouched.

These implementations must enforce identity constraints and
[optimistic concurrency](../application/repositories.md#versioned-snapshots). An update or delete
must include `version = incoming_version - 1` in the write's `WHERE` clause, alongside the row's
identity. If no row is modified, return a version-mismatch error and roll back the whole unit-of-work
transaction. See the [SQL example](./repositories.md#checking-versions). The DTO traits do not require
an identifier field, a version field or a conflict error variant; the storage implementation supplies
them.

For atomic batches, operations must participate in the transaction managed by the
[unit of work](./units_of_work.md). The repository adapter stops on an error but does not undo earlier
operations itself.
