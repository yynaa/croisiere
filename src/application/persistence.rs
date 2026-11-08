use std::{cell::RefCell, collections::HashSet, error::Error, fmt, rc};

use crate::{application::Snapshot, domain::Aggregate};

/// Page of results to return from a [`ReadRepository`].
pub struct Pagination {
    /// Maximum number of snapshots to return.
    pub limit: usize,
    /// Number of matching snapshots to skip before the first one returned.
    pub offset: usize,
}

/// Read side of the storage of snapshots of type `S`.
pub trait ReadRepository<S: Snapshot> {
    /// Error returned when the storage cannot be read.
    type Error;

    /// Returns the snapshots matching `filters`, ordered by `sorter` (storage
    /// order when `None`) and restricted to `pagination` (everything when
    /// `None`).
    fn get(
        &mut self,
        filters: HashSet<S::Filter>,
        sorter: Option<S::Sorter>,
        pagination: Option<Pagination>,
    ) -> Result<Vec<VersionedSnapshot<S>>, Self::Error>;
    /// Returns how many snapshots match `filters`.
    fn count(
        &mut self,
        filters: HashSet<S::Filter>,
        sorter: Option<S::Sorter>,
    ) -> Result<usize, Self::Error>;
}

/// Write side of the storage of snapshots of type `S`.
///
/// Versions are used for optimistic concurrency: implementations should reject
/// a snapshot whose [`version`](VersionedSnapshot::version) does not follow
/// the stored one, as it was changed concurrently in the meantime.
pub trait WriteRepository<S: Snapshot> {
    /// Error returned when the storage cannot be written, including on a
    /// version conflict.
    type Error;

    /// Inserts or updates `snapshots`.
    fn save(&mut self, snapshots: Vec<&VersionedSnapshot<S>>) -> Result<(), Self::Error>;
    /// Removes `snapshots`.
    fn delete(&mut self, snapshots: Vec<&VersionedSnapshot<S>>) -> Result<(), Self::Error>;
}

/// A snapshot together with its version, as stored by repositories.
pub struct VersionedSnapshot<S>
where
    S: Snapshot,
{
    /// The snapshot itself.
    pub snapshot: S,
    /// Optimistic-concurrency version: incremented on every change, and
    /// checked by [`WriteRepository`] against the stored one to detect
    /// concurrent writes.
    pub version: u128,
}

impl<S: Snapshot> VersionedSnapshot<S> {
    /// Marks the snapshot as changed and pairs it with the repository and the
    /// action to perform on it, ready to be registered on a [`UnitOfWork`].
    ///
    /// Increments [`version`](Self::version).
    // this function makes a snapshot into an actionable.
    // the repository cannot be sized, for a plethora of reasons.
    // in short, Sized is implicit by default, and we need it removed, since dyn-compatibility needs no Self: Sized
    // in shorter, this is black magic and you should never EVER EVER need to touch this.
    // if you do, i hope you're okay and everything will be fine.
    pub fn make_actionable<R: WriteRepository<S> + ?Sized>(
        mut self,
        repository: rc::Weak<RefCell<R>>,
        action: SnapshotAction,
    ) -> ActionableSnapshot<S, R> {
        self.version += 1;
        ActionableSnapshot {
            snapshot: self,
            repository,
            action,
        }
    }
}

/// these operations only are available when the aggregate and the snapshot have direct convertions.
impl<S: Snapshot> VersionedSnapshot<S> {
    /// Converts the snapshot back into its aggregate, returning the aggregate
    /// and the version.
    #[allow(unused)]
    pub fn dismantle(self) -> Result<(S::Aggregate, u128), S::Error> {
        self.snapshot
            .try_into()
            .map(|a| (a, self.version))
            .map_err(Into::into)
    }

    /// Builds a versioned snapshot from an aggregate and a version; the
    /// inverse of [`dismantle`](Self::dismantle).
    #[allow(unused)]
    pub fn assemble(aggregate: S::Aggregate, version: u128) -> Self {
        Self {
            snapshot: aggregate.into(),
            version,
        }
    }

    /// Runs `f` (typically an `#[aggregate_functions]` method) on the
    /// aggregate behind this snapshot, and returns the updated snapshot with
    /// the events `f` emitted, converted into `V`.
    ///
    /// The version is kept as is; conversion and aggregate errors are
    /// converted into `O`.
    pub fn do_as_aggregate<
        V: From<<S::Aggregate as Aggregate>::Event>,
        O: From<<S::Aggregate as Aggregate>::Error> + From<S::Error>,
    >(
        self,
        f: impl FnOnce(
            &mut S::Aggregate,
        ) -> Result<
            Vec<<S::Aggregate as Aggregate>::Event>,
            <S::Aggregate as Aggregate>::Error,
        >,
    ) -> Result<(Self, Vec<V>), O> {
        let (mut aggregate, version) = self.dismantle()?;
        let events: Vec<V> = f(&mut aggregate)?.into_iter().map(|e| e.into()).collect();
        let new_vs = VersionedSnapshot::assemble(aggregate, version);
        Ok((new_vs, events))
    }

    /// [`do_as_aggregate`](Self::do_as_aggregate) followed by
    /// [`make_actionable`](Self::make_actionable): runs `f` on the aggregate,
    /// then returns the updated snapshot ready to be registered on a
    /// [`UnitOfWork`], with the emitted events.
    pub fn do_and_make_actionable<
        V: From<<S::Aggregate as Aggregate>::Event>,
        O: From<<S::Aggregate as Aggregate>::Error> + From<S::Error>,
        R: WriteRepository<S> + ?Sized,
    >(
        self,
        repository: rc::Weak<RefCell<R>>,
        action: SnapshotAction,
        f: impl FnOnce(
            &mut S::Aggregate,
        ) -> Result<
            Vec<<S::Aggregate as Aggregate>::Event>,
            <S::Aggregate as Aggregate>::Error,
        >,
    ) -> Result<(ActionableSnapshot<S, R>, Vec<V>), O> {
        let (vs, events) = self.do_as_aggregate::<V, O>(f)?;
        let actionable = vs.make_actionable(repository, action);
        Ok((actionable, events))
    }
}

/// What to do with an [`ActionableSnapshot`] when its [`UnitOfWork`] commits.
pub enum SnapshotAction {
    /// Nothing; the snapshot is left as stored.
    None,
    /// Save the snapshot with [`WriteRepository::save`].
    Save,
    /// Delete the snapshot with [`WriteRepository::delete`].
    Delete,
}

/// Type-erased [`ActionableSnapshot`], so a [`UnitOfWork`] can hold pending
/// actions on snapshots of different types as trait objects.
pub trait AbstractActionableSnapshot {
    /// Error returned when the action fails.
    type Error;

    /// Name of the snapshot type, see [`Snapshot::NAME`].
    fn get_name(&self) -> &str;

    /// The action to perform on commit.
    fn get_action(&self) -> &SnapshotAction;
    /// Saves the snapshot through its repository.
    fn save(&mut self) -> Result<(), Self::Error>;
    /// Deletes the snapshot through its repository.
    fn delete(&mut self) -> Result<(), Self::Error>;

    /// Performs [`get_action`](Self::get_action): calls [`save`](Self::save),
    /// [`delete`](Self::delete), or does nothing.
    fn act(&mut self) -> Result<(), Self::Error> {
        match self.get_action() {
            SnapshotAction::None => Ok(()),
            SnapshotAction::Save => self.save(),
            SnapshotAction::Delete => self.delete(),
        }
    }
}

/// A versioned snapshot with a pending action and the repository to perform
/// it with, built by [`VersionedSnapshot::make_actionable`].
pub struct ActionableSnapshot<S: Snapshot, R: WriteRepository<S> + ?Sized> {
    /// The snapshot to act on.
    pub snapshot: VersionedSnapshot<S>,
    /// Repository to perform the action with. Weak, so pending actions don't
    /// keep it alive.
    pub repository: rc::Weak<RefCell<R>>,
    /// What to do with the snapshot on commit.
    pub action: SnapshotAction,
}

impl<S, R> AbstractActionableSnapshot for ActionableSnapshot<S, R>
where
    S: Snapshot,
    R: WriteRepository<S> + ?Sized,
    R::Error: From<RepositoryDropped>,
{
    type Error = R::Error;

    fn get_name(&self) -> &str {
        S::NAME
    }

    fn get_action(&self) -> &SnapshotAction {
        &self.action
    }

    fn save(&mut self) -> Result<(), Self::Error> {
        let repository = self.repository.upgrade().ok_or(RepositoryDropped)?;
        let result = repository.borrow_mut().save(vec![&self.snapshot]);
        result
    }

    fn delete(&mut self) -> Result<(), Self::Error> {
        let repository = self.repository.upgrade().ok_or(RepositoryDropped)?;
        let result = repository.borrow_mut().delete(vec![&self.snapshot]);
        result
    }
}

/// Returned when an [`ActionableSnapshot`] is acted upon after its repository
/// was dropped.
///
/// The repository's error type must convert from it for the
/// [`AbstractActionableSnapshot`] impl of [`ActionableSnapshot`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RepositoryDropped;

impl fmt::Display for RepositoryDropped {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("the repository of the actionable snapshot was dropped")
    }
}

impl Error for RepositoryDropped {}

/// Groups actions on snapshots so they are persisted as one transaction.
pub trait UnitOfWork {
    /// Error returned when committing fails.
    type Error;
    /// The repositories available within the unit of work, e.g. a struct
    /// holding one repository per snapshot type.
    type Registry;

    /// The repositories available within this unit of work.
    fn registry(&self) -> &Self::Registry;
    /// Queues an action to perform on commit.
    fn register_action(
        &self,
        actionable_snapshot: Box<dyn AbstractActionableSnapshot<Error = Self::Error>>,
    );
    /// Performs every registered action as one transaction: if any of them
    /// fails, all of them are rolled back and the error is returned.
    fn commit_and_rollback(&self) -> Result<(), Self::Error>;
}

/// Starts [`UnitOfWork`]s. Shared across threads, hence `Sync + Send`.
pub trait UnitOfWorkFactory: Sync + Send {
    /// [`UnitOfWork::Error`] of the units of work it starts.
    type Error;
    /// [`UnitOfWork::Registry`] of the units of work it starts.
    type Registry;

    /// Starts a new, empty unit of work.
    fn begin(&self) -> Box<dyn UnitOfWork<Error = Self::Error, Registry = Self::Registry>>;
}
