use std::{fmt::Debug, hash::Hash};

/// Value Object, as defined by DDD
///
/// It is recommended to implement a builder for implementors of this trait.
/// You can use a crate like [`builder_pattern`](https://docs.rs/builder-pattern/latest/builder_pattern/).
pub trait ValueObject: Debug + Clone + PartialEq + Eq + Default + Hash {
    const NAME: &'static str;
}

pub use croisiere_proc::ValueObject;
