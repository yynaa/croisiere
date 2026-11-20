//! Application-owned helpers shared by all compatible read repositories.

use std::collections::HashSet;

use croisiere::application::{Pagination, ReadRepository, Snapshot, VersionedSnapshot};

use super::errors::RepositoryError;

/// Adds unique lookups and existence checks.
///
/// The blanket implementation includes read repository trait objects, so callers
/// can use these helpers on readers returned by the registry.
pub trait ReadRepositoryExt<S: Snapshot>: ReadRepository<S, Error = RepositoryError> {
    /// Returns the unique match, preserving its concurrency version.
    ///
    /// Returns `NotFound` for no matches and `NotUnique` for multiple matches.
    /// Requests at most two rows, enough to distinguish those outcomes, and
    /// propagates storage errors unchanged. This check does not enforce storage
    /// uniqueness or prevent changes by another request after the read.
    fn get_unique(
        &mut self,
        filters: HashSet<S::Filter>,
    ) -> Result<VersionedSnapshot<S>, RepositoryError> {
        let mut matches = self.get(
            filters,
            None,
            Some(Pagination {
                limit: 2,
                offset: 0,
            }),
        )?;
        if matches.len() > 1 {
            return Err(RepositoryError::NotUnique);
        }
        matches.pop().ok_or(RepositoryError::NotFound)
    }

    /// Returns whether any snapshot matches, propagating storage failures.
    ///
    /// This read does not reserve a name or prevent a concurrent insert;
    /// persistence must still enforce uniqueness when saving.
    fn exists(&mut self, filters: HashSet<S::Filter>) -> Result<bool, RepositoryError> {
        self.count(filters).map(|count| count > 0)
    }
}

impl<S, R> ReadRepositoryExt<S> for R
where
    S: Snapshot,
    R: ReadRepository<S, Error = RepositoryError> + ?Sized,
{
}
