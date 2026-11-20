//! Query dispatch and read-only use of a fresh unit of work.

use std::collections::HashSet;

use croisiere::application::{Pagination, Query};

use crate::application::{
    errors::AppError,
    registry::{Factory, Uow},
    snapshots::{CounterSnapshot, CounterSorter},
};

/// All queries exposed by the counter application.
pub enum AppQuery {
    /// Select counters using the requested ordering and page.
    ListCounters(ListCounters),
}

/// Application-wide query responses, wrapping each query's own response.
#[derive(Debug)]
pub enum QueryResponse {
    /// Result produced by `ListCounters`.
    Counters(ListCountersResponse),
}

/// Result of listing counters, distinguishing an empty selection explicitly.
#[derive(Debug)]
pub enum ListCountersResponse {
    /// Snapshots in repository order, restricted to the requested page.
    Found(Vec<CounterSnapshot>),
    /// No snapshots were returned, including when the requested page is empty.
    Empty,
}

impl Query for AppQuery {
    type Context = Uow;
    type Response = QueryResponse;
    type Error = AppError;

    fn name(&self) -> &'static str {
        match self {
            Self::ListCounters(query) => query.name(),
        }
    }

    fn handle(&self, context: Uow) -> Result<QueryResponse, AppError> {
        match self {
            Self::ListCounters(query) => query.handle(context).map(QueryResponse::Counters),
        }
    }
}

/// Prepares a fresh read context before dispatching a query.
///
/// The query neither registers writes nor commits; its unit of work is dropped
/// when handling finishes. Query responses do not include concurrency versions.
pub fn ask(factory: &Factory, query: AppQuery) -> Result<QueryResponse, AppError> {
    query.handle(factory.begin())
}

/// Lists counters, optionally sorted and paginated.
///
/// Reads every matching counter when no page is supplied. Results are plain
/// snapshots: listing does not restore aggregates or validate domain values.
pub struct ListCounters {
    /// Optional ordering; `None` preserves storage insertion order.
    pub sorter: Option<CounterSorter>,
    /// Optional offset and limit, applied after sorting.
    pub pagination: Option<Pagination>,
}

impl Query for ListCounters {
    type Context = Uow;
    type Response = ListCountersResponse;
    type Error = AppError;

    fn name(&self) -> &'static str {
        "ListCounters"
    }

    fn handle(&self, uow: Uow) -> Result<ListCountersResponse, AppError> {
        // Only the read registry is used; queries never commit.
        let pagination = self.pagination.as_ref().map(|page| Pagination {
            limit: page.limit,
            offset: page.offset,
        });
        let found =
            uow.registry()
                .read_counters()
                .get(HashSet::new(), self.sorter.clone(), pagination)?;

        if found.is_empty() {
            return Ok(ListCountersResponse::Empty);
        }
        Ok(ListCountersResponse::Found(
            found
                .into_iter()
                .map(|versioned| versioned.snapshot)
                .collect(),
        ))
    }
}
