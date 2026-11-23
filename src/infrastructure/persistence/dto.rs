//! Storage representations and the operations repositories delegate to them.

use std::{collections::HashSet, fmt::Debug, hash::Hash};

use crate::application::Pagination;

/// Optional storage identifier: `None` when absent, `Some(id)` when assigned.
///
/// This is an alias for [`Option`]; it does not allocate identifiers or decide
/// whether a repository inserts or updates a DTO.
pub type InternalId<T> = Option<T>;

/// Storage representation of application data, independent of its snapshot.
///
/// A DTO may serve several snapshot types through separate conversions.
/// Preserve the snapshot's version in those conversions when the storage
/// uses optimistic concurrency.
pub trait DTO: Debug + Clone {
    /// Name identifying this storage representation.
    ///
    /// The adapters do not use this value to choose a table or check that it
    /// matches the repository's name.
    const NAME: &'static str;

    /// Connection or transaction handle used by this DTO's operations.
    type ConnectionData;
    /// Failure reported by the underlying storage operations.
    type Error;
}

/// Backend metadata of a storage representation, shared by backend markers.
///
/// Backend modules define marker traits pinning [`ConnectionData`](Self::ConnectionData)
/// and [`Error`](Self::Error) to the backend's types; implementing such a
/// marker on a `BackendDTO` supplies a blanket [`DTO`] implementation.
/// Implement [`DTO`] directly instead when the connection carries extra
/// handles such as a commit gate, or when the application error differs.
pub trait BackendDTO: Debug + Clone {
    /// Name identifying this storage representation, forwarded to [`DTO::NAME`].
    const NAME: &'static str;

    /// Connection or transaction handle used by this DTO's operations.
    type ConnectionData;
    /// Failure reported by the underlying storage operations.
    type Error;
}

/// Backend markers share this single blanket implementation: overlapping
/// blanket implementations cannot coexist when several backends are enabled.
impl<D: BackendDTO> DTO for D {
    const NAME: &'static str = D::NAME;

    type ConnectionData = D::ConnectionData;
    type Error = D::Error;
}

/// Selects DTOs using storage-specific criteria, ordering and pagination.
///
/// The implementation defines how filters combine and what order is used
/// without a sorter. The repository adapter forwards pagination unchanged;
/// it does not filter, sort or paginate the returned DTOs itself.
pub trait DTOSelect: DTO {
    /// Storage criteria, converted from each snapshot's filters by the repository adapter.
    type Filter: Eq + Hash + Clone;
    /// Storage ordering, converted from each snapshot's sorter by the repository adapter.
    type Sorter: Clone;

    /// Returns matching DTOs in the requested order and page.
    ///
    /// Returns a storage error if the query fails. Snapshot conversion is
    /// performed separately by the repository adapter.
    fn select(
        conn: &mut Self::ConnectionData,
        filters: HashSet<Self::Filter>,
        sorter: Option<Self::Sorter>,
        pagination: Option<Pagination>,
    ) -> Result<Vec<Self>, <Self as DTO>::Error>;
}

/// Inserts a DTO through a storage connection or transaction.
pub trait DTOInsert: DTO {
    /// Inserts this DTO, returning a storage error on failure.
    ///
    /// The implementation is responsible for identity and version checks.
    fn insert(self, conn: &mut Self::ConnectionData) -> Result<(), Self::Error>;
}

/// Updates a DTO already present in storage.
pub trait DTOUpdate: DTO {
    /// Updates the stored representation of this DTO.
    ///
    /// Include `version = incoming_version - 1` in the write's `WHERE`
    /// predicate. If no row is modified, return a version-conflict error so
    /// the unit of work rolls back the entire transaction. The repository
    /// adapter does not perform this check or rollback itself.
    fn update(self, conn: &mut Self::ConnectionData) -> Result<(), Self::Error>;
}

/// Deletes a DTO from storage.
pub trait DTODelete: DTO {
    /// Deletes the stored representation of this DTO.
    ///
    /// Include `version = incoming_version - 1` in the write's `WHERE`
    /// predicate. If no row is modified, return a version-conflict error so
    /// the unit of work rolls back the entire transaction. The repository
    /// adapter does not perform this check or rollback itself.
    fn delete(self, conn: &mut Self::ConnectionData) -> Result<(), Self::Error>;
}
