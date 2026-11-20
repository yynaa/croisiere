# Application: Snapshots

A snapshot is the persistable representation of an [aggregate](../domain/aggregates.md). Aggregates
are shaped for the domain: wrappers, nested parts, functions that guard their rules. Snapshots are
shaped for storage: plain data that a repository can read and write.

Repositories never see aggregates, only snapshots. The application turns an aggregate into a snapshot
to save it, and a snapshot back into an aggregate to work on it.

In `croisiere`, snapshots are described by the [`Snapshot`] trait.

A snapshot represents one aggregate, even when infrastructure stores its data across several
tables. Keep table-specific DTO composition in infrastructure; see
[one snapshot across multiple tables](../infrastructure/dtos.md#one-snapshot-across-multiple-tables)
for the two composition approaches.

## Declaring a snapshot

Derive [`Snapshot`] on a struct or an enum, and give it the aggregate it is a snapshot of with the
`#[snapshot(...)]` attribute:

```rust,ignore
use croisiere::application::Snapshot;

#[derive(Debug, Clone, Snapshot)]
#[snapshot(aggregate = AccountAggregate)]
pub struct AccountSnapshot {
    pub id: u32,
    pub owner: String,
    pub cents: i64,
}
```

`#[snapshot(...)]` takes:

| Argument    | Required | Default         | Becomes               |
|-------------|----------|-----------------|-----------------------|
| `aggregate` | yes      |                 | `Snapshot::Aggregate` |
| `filter`    | no       | [`EmptyFilter`] | `Snapshot::Filter`    |
| `sorter`    | no       | [`EmptySorter`] | `Snapshot::Sorter`    |

[Filters and sorters](#filters-and-sorters) are explained below.

The derive only implements [`Snapshot`] itself. The supertraits have to be derived (or implemented) by
you:

| Trait                    | Why                                                          |
|--------------------------|--------------------------------------------------------------|
| `From<Aggregate>`        | An aggregate can always be turned into a snapshot.           |
| `TryInto<Aggregate>`     | A snapshot can be turned back into an aggregate, or fail.    |
| `Clone`                  | Snapshots are plain data, freely copied around.              |
| `Debug`                  | Every snapshot can be inspected.                             |

Forgetting one of them is a compile error.

### `NAME`

[`Snapshot`] carries a constant, `NAME`, which the derive sets to the type's identifier:

```rust,ignore
assert_eq!(AccountSnapshot::NAME, "AccountSnapshot");
```

## Converting

### From an aggregate

Turning an aggregate into a snapshot cannot fail: the aggregate is always valid, since its functions
guard its rules. Implement [`From`]:

```rust,ignore
impl From<AccountAggregate> for AccountSnapshot {
    fn from(aggregate: AccountAggregate) -> Self {
        Self {
            id: aggregate.root.id,
            owner: aggregate.root.owner,
            cents: aggregate.balance.root.cents,
        }
    }
}
```

### Back to an aggregate

Turning a snapshot back into an aggregate can fail: stored data may no longer satisfy the aggregate's
rules, for example after those rules changed, or if the storage was edited by hand. Implement
[`TryFrom`] on the aggregate, which gives the snapshot [`TryInto`]:

> [!TIP]
> **Use `DomainError` as the snapshot-to-aggregate conversion error.** Restoration should only
> assemble domain objects and delegate validation to them. If it needs another error type,
> check whether domain logic has leaked into the application layer. Keep validation rules and
> their errors in the domain, and propagate them through the conversion with `?`.

For an aggregate with a domain-defined `restore` function:

```rust,ignore
impl TryFrom<AccountSnapshot> for AccountAggregate {
    type Error = DomainError;

    fn try_from(snapshot: AccountSnapshot) -> Result<Self, Self::Error> {
        Ok(Self::restore(snapshot.id, snapshot.owner, snapshot.cents)?)
    }
}
```

The domain's `restore` function constructs validated value objects, checks rules involving the
aggregate's state and assembles it using the
[aggregate's builder](../domain/aggregates.md#building-an-aggregate). Unlike creating a new account,
restoring an existing one should not emit an account-opened event. For aggregates whose rules are
fully enforced by their value objects, the conversion can construct those values with `try_into()?`
and assemble them directly; it still returns `DomainError`.

## Filters and sorters

A snapshot type comes with the ways repositories can select and order it. When reading snapshots, a
repository takes a set of filters and an optional sorter of that snapshot's types.

### Filters

A filter is a criterion snapshots can be selected by. Derive [`Filter`] on it, usually an enum with one
variant per criterion:

```rust,ignore
use croisiere::application::Filter;

#[derive(Hash, PartialEq, Eq, Filter)]
pub enum AccountFilter {
    ById(u32),
    ByOwner(String),
}
```

Repositories take filters as a `HashSet`, so a filter needs `Hash`, which [`Filter`] requires, and
`PartialEq` and `Eq`, which the `HashSet` requires.

### Sorters

A sorter is an order snapshots can be returned in. Derive [`Sorter`] on it:

```rust,ignore
use croisiere::application::Sorter;

#[derive(Sorter)]
pub enum AccountSorter {
    ById,
    ByBalanceDescending,
}
```

### Giving them to the snapshot

```rust,ignore
#[derive(Debug, Clone, Snapshot)]
#[snapshot(aggregate = AccountAggregate, filter = AccountFilter, sorter = AccountSorter)]
pub struct AccountSnapshot {
    pub id: u32,
    pub owner: String,
    pub cents: i64,
}
```

A snapshot that cannot be filtered or sorted leaves them out: it then uses [`EmptyFilter`] and
[`EmptySorter`].

What each filter and sorter means is up to the repositories: they are the ones that apply them to the
stored snapshots.

[`Snapshot`]: https://docs.rs/croisiere/latest/croisiere/application/trait.Snapshot.html
[`Filter`]: https://docs.rs/croisiere/latest/croisiere/application/trait.Filter.html
[`Sorter`]: https://docs.rs/croisiere/latest/croisiere/application/trait.Sorter.html
[`EmptyFilter`]: https://docs.rs/croisiere/latest/croisiere/application/struct.EmptyFilter.html
[`EmptySorter`]: https://docs.rs/croisiere/latest/croisiere/application/struct.EmptySorter.html
