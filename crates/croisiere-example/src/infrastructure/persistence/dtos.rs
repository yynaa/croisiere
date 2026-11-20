//! Counter storage representation, snapshot conversions and DTO operations.

use std::collections::HashSet;

use uuid::Uuid;

use croisiere::{
    application::{Pagination, VersionedSnapshot},
    infrastructure::persistence::{
        DTO, DTODelete, DTOInsert, DTOSelect, DTOUpdate, InternalId, in_memory::InMemoryDatabase,
    },
};

use super::{Connection, StorageError};
use crate::application::errors::RepositoryError;
use crate::application::snapshots::{CounterFilter, CounterSnapshot, CounterSorter};

/// One stored counter row, including storage identity and concurrency version.
///
/// The domain name identifies the counter to application callers. The UUID is
/// allocated on insertion and retained across updates, even though snapshots
/// do not carry it. This example stores these rows in the `counters` table.
#[derive(Debug, Clone)]
pub(super) struct CounterDTO {
    /// Unassigned on snapshot conversion; populated before storage insertion.
    pub(super) internal_id: InternalId<Uuid>,
    /// Domain identity used for uniqueness checks, updates and deletes.
    pub(super) name: String,
    /// Persisted count, copied to and from the application snapshot.
    pub(super) counter: u64,
    /// Incoming version; writes require the stored version to be one lower.
    pub(super) version: u128,
}

/// Storage criteria, including identifiers that are absent from snapshots.
#[derive(Debug, PartialEq, Eq, Hash)]
pub(super) enum CounterDTOFilter {
    /// Match the domain name exactly.
    ByName(String),
    /// Match an assigned internal UUID.
    #[allow(
        dead_code,
        reason = "available to infrastructure queries, not application requests"
    )]
    ByInternalId(Uuid),
}

impl From<CounterFilter> for CounterDTOFilter {
    fn from(filter: CounterFilter) -> Self {
        match filter {
            CounterFilter::ByName(name) => Self::ByName(name),
        }
    }
}

/// Storage ordering, extending the application's orders with internal UUIDs.
pub(super) enum CounterDTOSorter {
    /// Ascending lexicographic domain name.
    ByName,
    /// Descending count, breaking ties by ascending domain name.
    ByCountDescending,
    /// Ascending UUID order. Unassigned IDs sort before assigned IDs.
    #[allow(
        dead_code,
        reason = "available to infrastructure queries, not application requests"
    )]
    ByInternalId,
}

impl From<CounterSorter> for CounterDTOSorter {
    fn from(sorter: CounterSorter) -> Self {
        match sorter {
            CounterSorter::ByName => Self::ByName,
            CounterSorter::ByCountDescending => Self::ByCountDescending,
        }
    }
}

impl DTO for CounterDTO {
    const NAME: &'static str = "counters";
    type ConnectionData = Connection;
    type Error = RepositoryError;
}

/// Copies application data and version without inventing a storage UUID.
impl From<VersionedSnapshot<CounterSnapshot>> for CounterDTO {
    fn from(value: VersionedSnapshot<CounterSnapshot>) -> Self {
        Self {
            internal_id: None,
            name: value.snapshot.name,
            counter: value.snapshot.counter,
            version: value.version,
        }
    }
}

/// Drops the storage UUID while preserving domain data and concurrency version.
///
/// This conversion is infallible. Domain validation happens when the snapshot
/// is subsequently restored into its aggregate.
impl From<CounterDTO> for VersionedSnapshot<CounterSnapshot> {
    fn from(value: CounterDTO) -> Self {
        Self {
            snapshot: CounterSnapshot {
                name: value.name,
                counter: value.counter,
            },
            version: value.version,
        }
    }
}

/// Clones stored rows, treating a not-yet-created table as an empty collection.
///
/// Other backend failures propagate, including rows of an unexpected type.
pub(super) fn rows(database: &InMemoryDatabase) -> Result<Vec<CounterDTO>, StorageError> {
    match database.read_rows(CounterDTO::NAME) {
        Err(StorageError::TableNotFound(_)) => Ok(Vec::new()),
        result => result,
    }
}

impl DTOSelect for CounterDTO {
    type Filter = CounterDTOFilter;
    type Sorter = CounterDTOSorter;

    /// Reads committed rows under the shared gate, requiring every filter to
    /// match. No filters select all rows; no sorter preserves insertion order.
    /// Pagination is applied after filtering and sorting.
    fn select(
        conn: &mut Connection,
        filters: HashSet<CounterDTOFilter>,
        sorter: Option<CounterDTOSorter>,
        pagination: Option<Pagination>,
    ) -> Result<Vec<Self>, RepositoryError> {
        let _guard = conn
            .gate
            .lock()
            .map_err(|_| StorageError::DatabasePoisoned)?;
        let mut selected =
            conn.with_database(|database| rows(database).map_err(RepositoryError::from))?;
        // All filters must match; no filters selects every counter.
        selected.retain(|row| {
            filters.iter().all(|filter| match filter {
                CounterDTOFilter::ByName(name) => row.name == *name,
                CounterDTOFilter::ByInternalId(id) => row.internal_id == Some(*id),
            })
        });
        match sorter {
            Some(CounterDTOSorter::ByName) => selected.sort_by(|a, b| a.name.cmp(&b.name)),
            Some(CounterDTOSorter::ByCountDescending) => {
                selected.sort_by(|a, b| b.counter.cmp(&a.counter).then_with(|| a.name.cmp(&b.name)))
            }
            Some(CounterDTOSorter::ByInternalId) => selected.sort_by_key(|row| row.internal_id),
            None => {}
        }
        Ok(match pagination {
            Some(page) => selected
                .into_iter()
                .skip(page.offset)
                .take(page.limit)
                .collect(),
            None => selected,
        })
    }
}

/// Reports rejected insertions and version mismatches through the backend error.
fn conflict(name: &str) -> StorageError {
    StorageError::ActionFailed(format!("version conflict for counter {name:?}"))
}

impl DTOInsert for CounterDTO {
    /// Requires version 1 and a unique domain name, then assigns a new UUID.
    /// The caller's unit-of-work commit holds the gate across this operation.
    fn insert(mut self, conn: &mut Connection) -> Result<(), RepositoryError> {
        conn.with_database(|database| {
            if self.version != 1 || rows(database)?.iter().any(|row| row.name == self.name) {
                return Err(conflict(&self.name).into());
            }
            self.internal_id = Some(Uuid::new_v4());
            database.insert(Self::NAME, self);
            Ok(())
        })
    }
}

impl DTOUpdate for CounterDTO {
    /// Updates exactly one row by name and expected previous version.
    /// Keeps its UUID and rejects missing rows or stale versions. The enclosing
    /// commit owns the gate and rolls back the batch on a returned error.
    fn update(self, conn: &mut Connection) -> Result<(), RepositoryError> {
        conn.with_database(|database| {
            let changed = database.update(
                Self::NAME,
                |row: &Self| {
                    row.name == self.name && row.version.checked_add(1) == Some(self.version)
                },
                |row| {
                    // Snapshots carry the domain ID only. Retain the stored ID.
                    row.counter = self.counter;
                    row.version = self.version;
                },
            )?;
            if changed != 1 {
                return Err(conflict(&self.name).into());
            }
            Ok(())
        })
    }
}

impl DTODelete for CounterDTO {
    /// Deletes exactly one row matching the domain name and previous version.
    /// The enclosing commit owns the gate and rolls back on a mismatch.
    fn delete(self, conn: &mut Connection) -> Result<(), RepositoryError> {
        conn.with_database(|database| {
            let changed = database.delete(Self::NAME, |row: &Self| {
                row.name == self.name && row.version.checked_add(1) == Some(self.version)
            })?;
            if changed != 1 {
                return Err(conflict(&self.name).into());
            }
            Ok(())
        })
    }
}
