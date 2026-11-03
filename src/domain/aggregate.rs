use std::fmt::Debug;

pub trait Aggregate: Debug + Default {}


// The aggregate DSL itself lives in `croisiere-proc`; it is re-exported here so
// users can access it as `croisiere::domain::make_aggregate`.
pub use croisiere_proc::aggregate_functions;
pub use croisiere_proc::make_aggregate;