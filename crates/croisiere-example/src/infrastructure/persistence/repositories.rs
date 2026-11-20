//! DTO repository adapters and application error conversions.

use std::{collections::HashSet, convert::Infallible};

use croisiere::infrastructure::persistence::{
    DTOReadRepository, DTORepository, DTOSelect, DTOWriteRepository,
};

use super::{
    Connection, StorageError,
    dtos::{CounterDTO, rows},
};
use crate::application::{
    errors::RepositoryError,
    snapshots::{CounterFilter, CounterSnapshot},
};

/// Connection-backed repository serving both read and write adapter traits.
///
/// The registry exposes each instance through only the appropriate application
/// trait. Blanket implementations translate snapshots and delegate to DTO
/// operations; this type supplies connections, counting and existence checks.
pub(super) struct CounterRepository(Connection);

impl CounterRepository {
    /// Wraps the database and gate handles without taking ownership of storage.
    pub(super) fn new(connection: Connection) -> Self {
        Self(connection)
    }
}

impl DTORepository for CounterRepository {
    const NAME: &'static str = "CounterRepository";
    type ConnectionData = Connection;
    type Error = RepositoryError;

    /// Clones the shared handles; cloning does not keep the database alive.
    fn connection(&mut self) -> Result<Connection, RepositoryError> {
        Ok(self.0.clone())
    }
}

impl DTOReadRepository<CounterSnapshot> for CounterRepository {
    type DTO = CounterDTO;

    /// Converts application filters and counts all selected rows without sorting
    /// or pagination. Reuses selection for clarity rather than a separate count
    /// query optimized for a particular backend.
    fn count(&mut self, filters: HashSet<CounterFilter>) -> Result<usize, RepositoryError> {
        Ok(CounterDTO::select(
            &mut self.connection()?,
            filters.into_iter().map(Into::into).collect(),
            None,
            None,
        )?
        .len())
    }
}

impl DTOWriteRepository<CounterSnapshot> for CounterRepository {
    type DTO = CounterDTO;

    /// Chooses insert or update by domain name during a commit.
    ///
    /// The commit already holds the gate, so this only locks the database.
    /// DTO writes still check identity and version constraints themselves.
    fn exists(&mut self, dto: &CounterDTO) -> Result<bool, RepositoryError> {
        self.0
            .with_database(|database| Ok(rows(database)?.iter().any(|row| row.name == dto.name)))
    }
}

/// Retains the concrete backend error for diagnostics and downcasting.
impl From<StorageError> for RepositoryError {
    fn from(error: StorageError) -> Self {
        Self::ImplementationError(anyhow::Error::new(error))
    }
}

/// Satisfies the read adapter's conversion bound for infallible DTO conversion.
impl From<Infallible> for RepositoryError {
    fn from(never: Infallible) -> Self {
        match never {}
    }
}
