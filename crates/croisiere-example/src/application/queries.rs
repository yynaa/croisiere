use std::collections::HashSet;

use croisiere::application::{Pagination, Query};

use crate::application::{
    errors::AppError,
    registry::{upgrade, Factory},
    snapshots::{CounterSnapshot, CounterSorter},
};

/// Lists counters, optionally sorted and paginated.
pub struct ListCounters {
    pub sorter: Option<CounterSorter>,
    pub pagination: Option<Pagination>,
}

impl Query for ListCounters {
    type Context = Factory;
    type Response = Vec<CounterSnapshot>;
    type Error = AppError;

    fn name(&self) -> &'static str {
        "ListCounters"
    }

    fn handle(&self, factory: Factory) -> Result<Vec<CounterSnapshot>, AppError> {
        // A unit of work that is never committed: only its registry is used.
        let uow = factory.begin();

        let pagination = self.pagination.as_ref().map(|page| Pagination {
            limit: page.limit,
            offset: page.offset,
        });
        let found = upgrade(&uow.registry().counters())?.borrow_mut().get(
            HashSet::new(),
            self.sorter.clone(),
            pagination,
        )?;

        Ok(found.into_iter().map(|versioned| versioned.snapshot).collect())
    }
}
