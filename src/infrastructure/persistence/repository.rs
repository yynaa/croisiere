//! Blanket application-repository implementations backed by DTO operations.

use std::collections::HashSet;

use crate::{
    application::{Pagination, ReadRepository, Snapshot, VersionedSnapshot, WriteRepository},
    infrastructure::persistence::{DTO, DTOInsert, DTOSelect, DTOUpdate, dto::DTODelete},
};

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
    /// Shared error for DTO operations and application repositories.
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
/// `From<S::Sorter>`, and the connection and storage error types match.
/// The shared error must implement `From` for the DTO-to-snapshot conversion
/// error. Connection and storage failures propagate directly.
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
    /// forwards filters here without conversion or acquiring a
    /// connection. Implement connection access and criteria translation here.
    fn count(&mut self, filters: HashSet<S::Filter>) -> Result<usize, Self::Error>;
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
    R::Error: From<<VersionedSnapshot<S> as TryFrom<D>>::Error>,
{
    type Error = R::Error;

    fn get(
        &mut self,
        filters: HashSet<<S as Snapshot>::Filter>,
        sorter: Option<<S as Snapshot>::Sorter>,
        pagination: Option<Pagination>,
    ) -> Result<Vec<VersionedSnapshot<S>>, Self::Error> {
        let dtos = D::select(
            &mut R::connection(self)?,
            filters.into_iter().map(Into::into).collect(),
            sorter.map(Into::into),
            pagination,
        )?;

        dtos.into_iter()
            .map(|dto| VersionedSnapshot::try_from(dto).map_err(Into::into))
            .collect()
    }

    fn count(&mut self, filters: HashSet<<S as Snapshot>::Filter>) -> Result<usize, Self::Error> {
        DTOReadRepository::count(self, filters)
    }
}

/// Adapts application snapshots of type `S` to DTO write operations.
///
/// Provides [`WriteRepository`] when the DTO implements the three write
/// operation traits and `From<VersionedSnapshot<S>>`, connection and error
/// types match. DTO failures propagate directly as the shared error.
/// Writes do not require DTO-to-snapshot conversions.
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
    /// acquired. No connection is passed to this method; the implementation
    /// supplies its own access to storage. Errors stop the save batch before
    /// inserting or updating this DTO.
    fn exists(&mut self, dto: &Self::DTO) -> Result<bool, Self::Error>;
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
{
    type Error = R::Error;

    fn save(&mut self, snapshots: Vec<&VersionedSnapshot<S>>) -> Result<(), Self::Error> {
        let mut conn = R::connection(self)?;
        for s in snapshots {
            let d = s.clone().into();
            match self.exists(&d)? {
                false => D::insert(d, &mut conn),
                true => D::update(d, &mut conn),
            }?;
        }
        Ok(())
    }

    fn delete(&mut self, snapshots: Vec<&VersionedSnapshot<S>>) -> Result<(), Self::Error> {
        let mut conn = R::connection(self)?;
        for s in snapshots {
            let d = s.clone().into();
            D::delete(d, &mut conn)?;
        }
        Ok(())
    }
}
