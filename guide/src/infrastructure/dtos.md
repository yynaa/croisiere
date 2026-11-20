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
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct AccountDTO {
    pub internal_id: InternalId<Uuid>,
    pub id: u32,
    pub owner: String,
    pub cents: i64,
    pub version: u128,
}

impl DTO for AccountDTO {
    const NAME: &'static str = "accounts";

    type ConnectionData = AccountConnection;
    type Error = RepositoryError;
}
```

`AccountConnection` is supplied by your backend. `RepositoryError` is defined in the application
and shared by DTO operations, repositories and the unit of work. Infrastructure converts typed
backend errors into it with `From<BackendError>`. The connection data may be a pooled connection or a handle to a transaction
owned elsewhere. DTO operations receive a mutable reference to it.

`NAME` identifies the representation. The repository adapters do not interpret it as a table name or
check it against the repository's name; that meaning belongs to your implementation.

`InternalId<T>` is an alias for `Option<T>`. It can represent an unassigned storage identifier with
`None`, and an assigned one with `Some(id)`. It does not allocate identifiers or control whether a
write inserts or updates. The adapter asks the repository's `exists` method for that decision.

## Domain and storage identifiers

An entity's domain identity and a row's internal key serve different purposes. In the counter
example, the entity has a `CounterName` value object, the snapshot has a `name: String`, and the
DTO stores both that name and `internal_id: InternalId<Uuid>`. Commands locate counters by name;
the UUID belongs to persistence.

Use one table per DTO in this example. The counter's internal UUID is a field of each row in the
`counters` table. `DTOInsert` assigns `Some(Uuid::new_v4())`; no sequence table is needed. The example
uses the `uuid` dependency with its `v4` feature.

Converting a snapshot to a DTO sets `internal_id` to `None` because the snapshot does not carry it.
An update finds the stored row by domain identity, checks its version, and changes only mutable
fields and the version. Replacing the entire row with the new DTO would erase its internal UUID.
See the [counter walkthrough](../examples/counter_example.md#identities-and-rows) for the concrete fields.

## Converting snapshots

The read adapter requires `TryFrom<AccountDTO> for VersionedSnapshot<AccountSnapshot>`. The write
adapter only requires `From<VersionedSnapshot<AccountSnapshot>> for AccountDTO`; it does not
require the read conversion. Conversion from a versioned snapshot into a DTO is infallible at this
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

## One snapshot across multiple tables

An aggregate often contains data stored in several tables. Its snapshot still represents the whole
aggregate; it does not need to mirror the database layout. In this design, the type mappings are:

- Each aggregate has one corresponding snapshot type.
- Each table has one corresponding DTO type.
- One snapshot can map to several DTOs, including collections of rows.

For example, an order aggregate and its snapshot may include order lines, while storage separates
them into `orders` and `order_lines` tables. Keep that separation in infrastructure. The application
continues to load and save one versioned order snapshot.

### An intermediate structure

Introduce a structure that groups the DTOs needed to represent the complete snapshot:

```rust,ignore
#[derive(Debug, Clone)]
pub struct OrderRows {
    pub order: OrderDTO,
    pub lines: Vec<OrderLineDTO>,
}
```

`OrderDTO` describes the `orders` table, and `OrderLineDTO` describes `order_lines`. `OrderRows`
describes their composition for this snapshot; it has no table of its own. Convert between
`OrderRows` and `VersionedSnapshot<OrderSnapshot>` to assemble or split the data.

With the [DTO repository adapters](./repositories.md#dto-repository-adapters), the intermediate
structure must implement `DTO` and the operation traits required by the adapter. Use it as the
repository's associated `DTO` type. Its `NAME` identifies the composite representation rather than
a physical table, which the adapters allow. Its selector gathers the related rows; its write
operations coordinate writes to the individual table DTOs. The adapters do not discover nested
DTOs or generate those operations for you.

This keeps each table DTO focused on its own storage representation and puts the knowledge of how
they form a snapshot in a separate type. A hand-written repository can also use this intermediate
structure without implementing the adapter traits on it.

### DTOs containing other DTOs

> [!WARNING]
> **Nesting DTOs is not recommended for scalability.** As relationships and contexts grow, nested
> DTOs can couple otherwise independent storage representations and encourage loading or writing
> larger object graphs than an operation needs. Prefer an intermediate structure that explicitly
> groups the DTOs required by a particular snapshot, keeping table DTOs independently reusable.

Alternatively, let a DTO contain the related DTOs directly. For example, `OrderDTO` can include a
`Vec<OrderLineDTO>` alongside the fields of its own row. The outer DTO then provides the complete
representation used for snapshot conversion and coordinates the related storage operations.

This can work when those rows belong to the same context and are normally loaded and saved together.
It also couples the outer DTO to the related DTOs and their loading requirements. Be careful about
context separation: embedding a DTO from another context can pull that context's representation and
storage responsibilities into this one. Prefer an intermediate structure when that composition
should be specific to one repository or snapshot rather than part of the table DTO itself.

### Reading and writing the whole snapshot

With either approach, return one complete snapshot per aggregate. Count and paginate aggregates,
rather than joined rows: an order with five lines is one result, not five. If loading related data
requires several queries, use the backend's appropriate read-consistency mechanism so the assembled
snapshot does not combine incompatible states.

Save all participating rows through the same unit-of-work transaction. Preserve the aggregate's
optimistic-concurrency check and roll back every table change if any write fails. Coordinate
inserts, updates and removals of child rows explicitly; updating an existing order may still require
inserting a new line or deleting an old one. Composition alone does not provide cascading writes
or atomicity.

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

> [!TIP]
> **Let infrastructure criteria express storage needs.** DTO filters and sorters can reuse the
> application types when their needs coincide, but often benefit from being a superset or a
> different representation. Include storage-only criteria such as filtering or sorting by an
> internal row ID without exposing that ID in application snapshots or queries. Implement `From`
> conversions for all application criteria so the repository adapter can translate them; the reverse
> conversion need not exist. Keep the meaning of each application criterion intact during translation.

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
