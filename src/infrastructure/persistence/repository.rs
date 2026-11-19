//! Blanket application-repository implementations backed by DTO operations.

use std::collections::HashSet;

use crate::{
    application::{Pagination, ReadRepository, Snapshot, VersionedSnapshot, WriteRepository},
    infrastructure::persistence::{DTO, DTOInsert, DTOSelect, DTOUpdate, dto::DTODelete},
};

/// Storage or DTO-to-snapshot conversion failure from a repository adapter.
///
/// `R` is the repository's storage error and `C` the error from
/// `VersionedSnapshot<S>::try_from(dto)`. Both read and write adapters use
/// this type, although writes only produce [`Repository`](Self::Repository).
#[derive(Debug, thiserror::Error)]
pub enum DTORepositoryError<R, C> {
    /// Acquiring a connection or performing a storage operation failed.
    #[error("repository read failed: {0}")]
    Repository(R),
    /// A selected DTO could not be converted into a versioned snapshot.
    #[error("DTO conversion failed: {0}")]
    Conversion(C),
}

/// Provides the storage handle shared by a repository's DTO operations.
///
/// For the application-repository blanket implementations, `ConnectionData`
/// and `Error` must be the same as those of the associated DTO.
pub trait DTORepository {
    /// Name identifying this repository; not interpreted by the adapters.
    const NAME: &'static str;

    /// Owned handle passed to DTO operations, for example a pooled connection
    /// or a handle to a transaction owned by a unit of work.
    type ConnectionData;
    /// Connection-acquisition and storage-operation error.
    type Error;

    /// Acquires the handle for one repository call.
    ///
    /// `get`, `save` and `delete` each call this once, including for an empty
    /// write batch. The adapter does not start or commit a transaction.
    fn connection(&mut self) -> Result<Self::ConnectionData, Self::Error>;
}

/// Adapts selected DTOs to application snapshots of type `S`.
///
/// Provides [`ReadRepository`] when `VersionedSnapshot<S>: TryFrom<Self::DTO>`,
/// the DTO's filter and sorter implement `From<S::Filter>` and
/// `From<S::Sorter>`, and the connection and error types match.
///
/// `get` acquires one connection, converts the criteria, calls
/// [`DTOSelect::select`] and converts the returned DTOs in order. A failure
/// returns an error instead of partial results. A repository can implement
/// this trait for multiple snapshots using the same DTO.
pub trait DTOReadRepository<S>: DTORepository
where
    S: Snapshot,
{
    /// DTO selected from storage and converted into `VersionedSnapshot<S>`.
    type DTO: DTOSelect;

    /// Returns the number of snapshots matching application-level criteria.
    ///
    /// Unlike `get`, the blanket [`ReadRepository::count`] implementation
    /// forwards filters and sorter here without conversion or acquiring a
    /// connection. Implement connection access and criteria translation here.
    fn count(
        &mut self,
        filters: HashSet<S::Filter>,
        sorter: Option<S::Sorter>,
    ) -> Result<usize, Self::Error>;
}

impl<
    C,
    S: Snapshot,
    D: DTOSelect + DTO<ConnectionData = C, Error = R::Error>,
    R: DTOReadRepository<S, DTO = D, ConnectionData = C>,
> ReadRepository<S> for R
where
    VersionedSnapshot<S>: TryFrom<D>,
    D::Filter: From<S::Filter>,
    D::Sorter: From<S::Sorter>,
{
    type Error = DTORepositoryError<R::Error, <VersionedSnapshot<S> as TryFrom<D>>::Error>;

    fn get(
        &mut self,
        filters: HashSet<<S as Snapshot>::Filter>,
        sorter: Option<<S as Snapshot>::Sorter>,
        pagination: Option<Pagination>,
    ) -> Result<Vec<VersionedSnapshot<S>>, Self::Error> {
        let dtos = D::select(
            &mut R::connection(self).map_err(DTORepositoryError::Repository)?,
            filters.into_iter().map(Into::into).collect(),
            sorter.map(Into::into),
            pagination,
        )
        .map_err(DTORepositoryError::Repository)?;

        dtos.into_iter()
            .map(|dto| VersionedSnapshot::try_from(dto).map_err(DTORepositoryError::Conversion))
            .collect()
    }

    fn count(
        &mut self,
        filters: HashSet<<S as Snapshot>::Filter>,
        sorter: Option<<S as Snapshot>::Sorter>,
    ) -> Result<usize, Self::Error> {
        DTOReadRepository::count(self, filters, sorter).map_err(DTORepositoryError::Repository)
    }
}

/// Adapts application snapshots of type `S` to DTO write operations.
///
/// Provides [`WriteRepository`] when the DTO implements the three write
/// operation traits and `From<VersionedSnapshot<S>>`, connection and error
/// types match, and `VersionedSnapshot<S>: TryFrom<Self::DTO>`. The latter
/// bound supplies the conversion error type; writes do not convert DTOs back.
///
/// Each batch acquires one connection, clones and converts each snapshot,
/// and processes them in input order. Saving uses [`exists`](Self::exists)
/// to choose insert or update; deleting calls [`DTODelete::delete`] directly.
/// Processing stops at the first error. Earlier writes are not rolled back
/// by this adapter: atomicity and version checks require a storage transaction
/// and DTO implementations that enforce them.
pub trait DTOWriteRepository<S>: DTORepository
where
    S: Snapshot,
{
    /// Storage representation receiving cloned versioned snapshots.
    type DTO: DTOInsert + DTOUpdate + DTODelete;

    /// Whether saving `dto` should update an existing row rather than insert.
    ///
    /// Called once per DTO during `save`, after the batch connection is
    /// acquired. No connection is passed to this method and it cannot return
    /// an error; the implementation supplies its own access to storage.
    fn exists(&mut self, dto: &Self::DTO) -> bool;
}

impl<
    C,
    S: Snapshot,
    D: DTOInsert
        + DTOUpdate
        + DTODelete
        + DTO<ConnectionData = C, Error = R::Error>
        + From<VersionedSnapshot<S>>,
    R: DTOWriteRepository<S, DTO = D, ConnectionData = C>,
> WriteRepository<S> for R
where
    VersionedSnapshot<S>: TryFrom<D>,
{
    type Error = DTORepositoryError<R::Error, <VersionedSnapshot<S> as TryFrom<D>>::Error>;

    fn save(&mut self, snapshots: Vec<&VersionedSnapshot<S>>) -> Result<(), Self::Error> {
        let mut conn = R::connection(self).map_err(DTORepositoryError::Repository)?;
        for s in snapshots {
            let d = s.clone().into();
            match self.exists(&d) {
                false => D::insert(d, &mut conn),
                true => D::update(d, &mut conn),
            }
            .map_err(DTORepositoryError::Repository)?;
        }
        Ok(())
    }

    fn delete(&mut self, snapshots: Vec<&VersionedSnapshot<S>>) -> Result<(), Self::Error> {
        let mut conn = R::connection(self).map_err(DTORepositoryError::Repository)?;
        for s in snapshots {
            let d = s.clone().into();
            D::delete(d, &mut conn).map_err(DTORepositoryError::Repository)?;
        }
        Ok(())
    }
}
