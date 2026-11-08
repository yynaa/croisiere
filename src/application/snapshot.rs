use std::{fmt::Debug, hash::Hash};

use crate::domain::Aggregate;

/// Persistable representation of an [`Aggregate`].
///
/// A snapshot is what repositories read and write: it converts from its
/// aggregate with [`From`] and back with [`TryInto`] (the way back may fail,
/// e.g. on data that no longer satisfies the aggregate's invariants).
///
/// Implement it with `#[derive(Snapshot)]`, giving the associated types with
/// the `#[snapshot(aggregate = ..., filter = ..., sorter = ...)]` attribute;
/// `filter` and `sorter` default to [`EmptyFilter`] and [`EmptySorter`].
pub trait Snapshot: Debug + From<Self::Aggregate> + TryInto<Self::Aggregate> {
    /// Name of the snapshot type, set to the type's identifier by the derive.
    const NAME: &'static str;
    /// The aggregate this snapshot is taken of.
    type Aggregate: Aggregate;
    /// Criteria a [`ReadRepository`](crate::application::ReadRepository) can
    /// select snapshots of this type by.
    type Filter: Filter;
    /// Orderings a [`ReadRepository`](crate::application::ReadRepository) can
    /// return snapshots of this type in.
    type Sorter: Sorter;
}

/// Criterion for selecting snapshots in a
/// [`ReadRepository`](crate::application::ReadRepository).
///
/// Repositories take filters as a [`HashSet`](std::collections::HashSet),
/// hence the [`Hash`] bound. Implement it with `#[derive(Filter)]`.
pub trait Filter: Hash {}

/// [`Filter`] for snapshots that cannot be filtered.
///
/// Default `Snapshot::Filter` when `#[snapshot(...)]` sets no `filter`.
#[derive(Hash, Filter)]
pub struct EmptyFilter;

/// Ordering for snapshots returned by a
/// [`ReadRepository`](crate::application::ReadRepository).
///
/// Implement it with `#[derive(Sorter)]`.
pub trait Sorter {}

/// [`Sorter`] for snapshots that cannot be sorted.
///
/// Default `Snapshot::Sorter` when `#[snapshot(...)]` sets no `sorter`.
#[derive(Sorter)]
pub struct EmptySorter;

pub use croisiere_proc::{Filter, Snapshot, Sorter};
