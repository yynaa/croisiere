use std::fmt::Debug;

/// Aggregate, as defined by DDD.
///
/// You should not implement this yourself, use [`make_aggregate`].
pub trait Aggregate: Debug + Default {
    const NAME: &'static str;
}

// The aggregate DSL itself lives in `croisiere-proc`; it is re-exported here so
// users can access it as `croisiere::domain::make_aggregate`.
pub use croisiere_proc::aggregate_functions;
pub use croisiere_proc::make_aggregate;
