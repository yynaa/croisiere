use std::fmt::Debug;

/// Aggregate, as defined by DDD.
///
/// It links an entity (its root) to the entities and value objects it is
/// made of, and is the unit its functions change the state of, through
/// `#[aggregate_functions]`.
///
/// You should not implement this yourself, use [`make_aggregate`].
pub trait Aggregate: Debug + Default {
    /// Name of the aggregate type, set to the generated type's identifier by
    /// [`make_aggregate`] (or to the name given with `#[alias]`).
    const NAME: &'static str;
    /// Error returned by the aggregate's functions, set with `#[fails]` in
    /// [`make_aggregate`].
    type Error;
    /// Events emitted by the aggregate's functions, set with `#[emits]` in
    /// [`make_aggregate`].
    type Event;
}

// The aggregate DSL itself lives in `croisiere-proc`; it is re-exported here so
// users can access it as `croisiere::domain::make_aggregate`.
pub use croisiere_proc::aggregate_functions;
pub use croisiere_proc::make_aggregate;
