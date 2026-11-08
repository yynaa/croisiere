use croisiere::application::{Filter, Snapshot, Sorter};

use crate::domain::{
    aggregates::{CounterAggregate, CounterAggregateCounterData},
    entities::Counter,
    value_objects::CounterData,
};

#[derive(Debug, Snapshot)]
#[snapshot(aggregate = CounterAggregate, filter = CounterFilter, sorter = CounterSorter)]
pub struct CounterSnapshot {
    pub name: String,
    pub counter: u64,
}

impl From<CounterAggregate> for CounterSnapshot {
    fn from(value: CounterAggregate) -> Self {
        Self {
            name: value.root.name,
            counter: value.data.root.counter,
        }
    }
}

impl TryFrom<CounterSnapshot> for CounterAggregate {
    type Error = anyhow::Error;

    fn try_from(value: CounterSnapshot) -> Result<Self, Self::Error> {
        Ok(Self {
            root: Counter { name: value.name },
            data: CounterAggregateCounterData {
                root: CounterData {
                    counter: value.counter,
                },
            },
        })
    }
}

/// Criteria to select counters by. `Eq` is needed to put them in the
/// `HashSet` repositories take.
#[derive(Hash, PartialEq, Eq, Filter)]
pub enum CounterFilter {
    ByName(String),
}

/// Orders to list counters in.
#[derive(Clone, Sorter)]
pub enum CounterSorter {
    ByName,
    ByCountDescending,
}
