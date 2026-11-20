//! Plain counter data and repository selection criteria.
//!
//! Snapshot restoration delegates validation to domain types and returns
//! `DomainError`; this module does not define business validation rules.

use croisiere::application::{Filter, Snapshot, Sorter};

use crate::domain::{
    DomainError, aggregates::CounterAggregate, entities::Counter, value_objects::CounterData,
};

/// Persistable counter data, without domain validation or storage identifiers.
///
/// Repositories read and write this representation rather than the aggregate.
/// The name and count are plain values; restoring the aggregate validates them
/// through its domain types. Concurrency versions are carried separately by
/// `VersionedSnapshot<CounterSnapshot>`.
#[derive(Debug, Clone, Snapshot)]
#[snapshot(aggregate = CounterAggregate, filter = CounterFilter, sorter = CounterSorter)]
pub struct CounterSnapshot {
    /// Domain identity as an unvalidated string, rather than the storage UUID.
    pub name: String,
    /// Current count, with nonnegativity represented by the unsigned type.
    pub counter: u64,
}

/// Flattens an aggregate into plain data without validation or allocation of a
/// new name string: ownership of the name is transferred into the snapshot.
impl From<CounterAggregate> for CounterSnapshot {
    fn from(value: CounterAggregate) -> Self {
        Self {
            name: value.root.name.into_inner(),
            counter: value.data.root.counter,
        }
    }
}

/// Restores existing state without emitting creation events.
///
/// Name construction delegates validation to `CounterName`; an empty name
/// returns `DomainError::CounterName`. The count needs no additional checks
/// beyond its `u64` representation in this example. The builder assembles the
/// validated parts without repeating their rules in the application layer.
impl TryFrom<CounterSnapshot> for CounterAggregate {
    type Error = DomainError;

    fn try_from(value: CounterSnapshot) -> Result<Self, Self::Error> {
        Ok(CounterAggregate::builder()
            .root(Counter {
                name: value.name.try_into()?,
            })
            .data(CounterData {
                counter: value.counter,
            })
            .build())
    }
}

/// Criteria repositories use to select counters.
///
/// `Hash` and `Eq` allow criteria to be collected in the repositories' `HashSet`.
/// The repository applies the criteria; constructing a filter does not validate
/// a domain name or restore an aggregate.
#[derive(Hash, PartialEq, Eq, Filter)]
pub enum CounterFilter {
    /// Matches the domain name exactly, including case and whitespace.
    ByName(String),
}

/// Orders to list counters in.
#[derive(Clone, Sorter)]
pub enum CounterSorter {
    /// Ascending lexicographic order of domain names.
    ByName,
    /// Descending count, with ties broken by ascending domain name.
    ByCountDescending,
}
