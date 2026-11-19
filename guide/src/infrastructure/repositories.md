# Infrastructure: Repositories

The application describes [repositories](../application/repositories.md) with the [`ReadRepository`]
and [`WriteRepository`] traits. The infrastructure implements them for an actual storage: a database,
files, memory, another service.

The framework provides DTO adapters for implementing those traits, described first below. The rest
of the page shows a hand-written implementation with storage kept in memory. Its types are examples,
separate from the crate's [in-memory backend](./in_memory.md).

## DTO repository adapters

Import the adapter traits from `croisiere::infrastructure::persistence`. Implement `DTORepository`
for the type that provides storage access:

```rust,ignore
impl DTORepository for AccountRepository {
    const NAME: &'static str = "accounts";

    type ConnectionData = AccountConnection;
    type Error = StorageError;

    fn connection(&mut self) -> Result<AccountConnection, StorageError> {
        self.storage.connection()
    }
}
```

`AccountRepository`, its `storage` field and the connection acquisition above are backend-specific.
The DTO and repository must have the same connection and storage-error types. The adapter does not
use `NAME` to select storage or match it against the DTO's name.

### Reading through a DTO

Implement `DTOReadRepository<S>` by choosing a [DTO](./dtos.md) and implementing `count`:

```rust,ignore
impl DTOReadRepository<AccountSnapshot> for AccountRepository {
    type DTO = AccountDTO;

    fn count(
        &mut self,
        filters: HashSet<AccountFilter>,
        sorter: Option<AccountSorter>,
    ) -> Result<usize, StorageError> {
        let mut conn = self.connection()?;
        // Backend-specific count operation using storage-level criteria.
        conn.count_accounts(
            filters.into_iter().map(Into::into).collect(),
            sorter.map(Into::into),
        )
    }
}
```

The blanket `ReadRepository<S>` implementation is available when:

- The DTO implements `DTOSelect`, with the repository's connection and error types.
- `VersionedSnapshot<S>` implements `TryFrom` for the DTO.
- The DTO's filter and sorter implement `From<S::Filter>` and `From<S::Sorter>`.

For `get`, the adapter acquires one connection, converts filters and sorter, and calls `DTOSelect::select`
with pagination unchanged. It converts the selected DTOs into versioned snapshots, preserving their
order. If acquiring the connection, selecting rows or converting any row fails, the call returns an
error instead of partial results.

For `count`, the adapter delegates directly to your `DTOReadRepository::count` implementation. It
does not acquire a connection or convert criteria for you. The method receives the application's
filters and sorter and must supply its own storage query; it receives no pagination.

The same repository and DTO can support several snapshot types through separate
`DTOReadRepository<S>` implementations and conversions. Use `ReadRepository::<S>::get` or
`ReadRepository::<S>::count` to disambiguate calls when needed.

### Writing through a DTO

Implement `DTOWriteRepository<S>` with a DTO and an existence check:

```rust,ignore
impl DTOWriteRepository<AccountSnapshot> for AccountRepository {
    type DTO = AccountDTO;

    fn exists(&mut self, dto: &AccountDTO) -> bool {
        // Consult backend-specific state to choose insert or update.
        self.known_account_ids.contains(&dto.id)
    }
}
```

The example assumes the repository already knows which account identifiers exist. `exists` returns
a `bool`, cannot report an error, and is not passed the batch's connection. Your implementation must
provide its own way to determine existence. This check chooses an operation; version validation still
belongs in the write itself.

The blanket `WriteRepository<S>` implementation requires `DTOInsert`, `DTOUpdate` and `DTODelete`
for the DTO, matching connection and error types, `From<VersionedSnapshot<S>>` for the DTO, and
`TryFrom<DTO>` for `VersionedSnapshot<S>`. The last bound supplies the adapter's conversion-error
type; writes do not perform that conversion back to snapshots.

For each `save` or `delete` call, the adapter acquires one connection, including for an empty batch.
It clones each supplied snapshot and converts the clone into a DTO. `save` calls `exists` for each
DTO and chooses `insert` or `update`; `delete` calls the DTO's delete operation without checking
existence first. Each batch runs in input order and stops at the first error.

The adapter does not create a transaction or roll back earlier writes. Supply a connection tied to
the unit of work's transaction so a failed write can roll back every change in that unit of work.

### Adapter errors

Both adapters use `DTORepositoryError<R, C>`, where `R` is the storage error and `C` is the error
from converting a DTO into a versioned snapshot:

| Variant | Cause |
|---------|-------|
| `Repository(R)` | Connection acquisition, select, count, insert, update or delete failed. |
| `Conversion(C)` | A selected DTO could not become a versioned snapshot. |

Write calls only return the `Repository` variant. Its current display message says
`repository read failed` even for write failures; match the variant to distinguish storage failures
from conversion failures.

When using the adapters with actionable snapshots, the final write-repository error must also
convert from `RepositoryDropped`, as required by the
[application's persistence API](../application/units_of_work.md#errors). `DTORepositoryError` does
not currently provide that conversion; integration needs an error wrapper or another adapter that
does. DTO adapters alone are therefore not a complete unit-of-work implementation.

## A hand-written repository

The following example implements the application traits directly, without DTO adapters.

## The storage

The storage holds the committed snapshots, with their versions. It is shared by every unit of work,
possibly across threads, hence the [`Arc`] and the [`Mutex`]:

```rust,ignore
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

use croisiere::application::VersionedSnapshot;

#[derive(Default)]
pub struct Database {
    pub accounts: BTreeMap<u32, VersionedSnapshot<AccountSnapshot>>,
}

pub type SharedDatabase = Arc<Mutex<Database>>;
```

With an actual database, this is the connection pool, or whatever your database client provides.

## Reading

A read repository reads what is committed. It applies the snapshot's
[filters and sorter](../application/snapshots.md#filters-and-sorters): what each of them means is
decided here.

```rust,ignore
use std::collections::HashSet;

use croisiere::application::{Pagination, ReadRepository};

pub struct AccountReader {
    pub database: SharedDatabase,
}

fn matches(filter: &AccountFilter, snapshot: &AccountSnapshot) -> bool {
    match filter {
        AccountFilter::ById(id) => snapshot.id == *id,
        AccountFilter::ByOwner(owner) => snapshot.owner == *owner,
    }
}

impl ReadRepository<AccountSnapshot> for AccountReader {
    type Error = RepositoryError;

    fn get(
        &mut self,
        filters: HashSet<AccountFilter>,
        sorter: Option<AccountSorter>,
        pagination: Option<Pagination>,
    ) -> Result<Vec<VersionedSnapshot<AccountSnapshot>>, RepositoryError> {
        let database = self.database.lock().map_err(|_| RepositoryError::Storage("poisoned".into()))?;

        // A snapshot must match every filter.
        let mut found: Vec<_> = database
            .accounts
            .values()
            .filter(|vs| filters.iter().all(|filter| matches(filter, &vs.snapshot)))
            .cloned()
            .collect();

        match sorter {
            None | Some(AccountSorter::ById) => found.sort_by_key(|vs| vs.snapshot.id),
            Some(AccountSorter::ByBalanceDescending) => {
                found.sort_by_key(|vs| std::cmp::Reverse(vs.snapshot.cents))
            }
        }

        Ok(match pagination {
            Some(Pagination { limit, offset }) => {
                found.into_iter().skip(offset).take(limit).collect()
            }
            None => found,
        })
    }

    fn count(
        &mut self,
        filters: HashSet<AccountFilter>,
        sorter: Option<AccountSorter>,
    ) -> Result<usize, RepositoryError> {
        Ok(self.get(filters, sorter, None)?.len())
    }
}
```

With an actual database, filters, sorters and pagination usually become parts of the query sent to it,
rather than being applied in memory.

## Writing

A write repository does not write to the storage right away. Its writes belong to its unit of work,
and must only be stored if the whole unit of work commits. So it stages them, and the
[unit of work](./units_of_work.md) applies them when it commits:

```rust,ignore
use std::{cell::RefCell, rc::Rc};

use croisiere::application::WriteRepository;

/// A write waiting for its unit of work to commit.
pub enum StagedWrite {
    Save(VersionedSnapshot<AccountSnapshot>),
    Delete(VersionedSnapshot<AccountSnapshot>),
}

pub struct AccountWriter {
    /// Shared with the unit of work, which applies them on commit.
    pub staged: Rc<RefCell<Vec<StagedWrite>>>,
}

impl WriteRepository<AccountSnapshot> for AccountWriter {
    type Error = RepositoryError;

    fn save(&mut self, snapshots: Vec<&VersionedSnapshot<AccountSnapshot>>) -> Result<(), RepositoryError> {
        let mut staged = self.staged.borrow_mut();
        staged.extend(snapshots.into_iter().cloned().map(StagedWrite::Save));
        Ok(())
    }

    fn delete(&mut self, snapshots: Vec<&VersionedSnapshot<AccountSnapshot>>) -> Result<(), RepositoryError> {
        let mut staged = self.staged.borrow_mut();
        staged.extend(snapshots.into_iter().cloned().map(StagedWrite::Delete));
        Ok(())
    }
}
```

With an actual database, the write repository usually writes through the database transaction of its
unit of work instead, which the unit of work commits or rolls back.

## Checking versions

Whenever a write is stored, its version must follow the stored one, as described in
[versioned snapshots](../application/repositories.md#versioned-snapshots). Otherwise the snapshot was
changed by someone else since it was read, and the write is rejected:

```rust,ignore
impl Database {
    pub fn apply(&mut self, write: StagedWrite) -> Result<(), RepositoryError> {
        match write {
            StagedWrite::Save(vs) => {
                // A new account is stored at version 0, and saved at version 1.
                let stored = self.accounts.get(&vs.snapshot.id).map_or(0, |s| s.version);
                if stored + 1 != vs.version {
                    return Err(RepositoryError::Conflict);
                }
                self.accounts.insert(vs.snapshot.id, vs);
            }
            StagedWrite::Delete(vs) => match self.accounts.get(&vs.snapshot.id) {
                Some(stored) if stored.version + 1 == vs.version => {
                    self.accounts.remove(&vs.snapshot.id);
                }
                _ => return Err(RepositoryError::Conflict),
            },
        }
        Ok(())
    }
}
```

With a database, check the version in the write itself. An update or delete includes the row's
identity and `version = incoming_version - 1` in its `WHERE` clause:

```sql
UPDATE accounts
SET owner = :owner, cents = :cents, version = :incoming_version
WHERE id = :id AND version = :incoming_version - 1;

DELETE FROM accounts
WHERE id = :id AND version = :incoming_version - 1;
```

Inspect the affected-row count. If no row is modified, treat the write as a version mismatch and
return a conflict error. The unit of work must then roll back the entire transaction, including
earlier successful writes. Do not perform a separate version read as the concurrency check: another
transaction could change the row between that read and the write.

New snapshots are inserted at version `1`; use the storage's uniqueness constraints to reject a
competing insertion for the same identity. For updates and deletes, the version predicate remains
necessary even if an earlier existence check succeeded.

[`ReadRepository`]: https://docs.rs/croisiere/latest/croisiere/application/trait.ReadRepository.html
[`WriteRepository`]: https://docs.rs/croisiere/latest/croisiere/application/trait.WriteRepository.html
[`Arc`]: https://doc.rust-lang.org/std/sync/struct.Arc.html
[`Mutex`]: https://doc.rust-lang.org/std/sync/struct.Mutex.html
