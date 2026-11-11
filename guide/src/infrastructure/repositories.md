# Infrastructure: Repositories

The application describes [repositories](../application/repositories.md) with the [`ReadRepository`]
and [`WriteRepository`] traits. The infrastructure implements them for an actual storage: a database,
files, memory, another service.

This page implements them for a storage kept in memory. It is the simplest storage to show, but the
same parts are found in any other.

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

With an actual database, this is usually a condition on the version in the `UPDATE` or `DELETE`
statement: if no row was changed, the version did not match.

[`ReadRepository`]: https://docs.rs/croisiere/latest/croisiere/application/trait.ReadRepository.html
[`WriteRepository`]: https://docs.rs/croisiere/latest/croisiere/application/trait.WriteRepository.html
[`Arc`]: https://doc.rust-lang.org/std/sync/struct.Arc.html
[`Mutex`]: https://doc.rust-lang.org/std/sync/struct.Mutex.html
