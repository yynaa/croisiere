use std::{fmt::Debug, hash::Hash};

/// Value Object, as defined by DDD
///
/// It has no identity: two value objects with the same fields are equal, hence
/// the [`PartialEq`], [`Eq`] and [`Hash`] bounds.
///
/// It can contain other value objects. Be thoughtful about which ones you
/// embed: an embedded value object is part of this one's value, and is
/// compared, hashed and replaced along with it.
///
/// It cannot contain entities.
///
/// Implement it with `#[derive(ValueObject)]`, alongside the derives for its
/// supertraits.
///
/// It is recommended to implement a builder for implementors of this trait.
/// You can use a crate like [`builder_pattern`](https://docs.rs/builder-pattern/latest/builder_pattern/).
pub trait ValueObject: Debug + Clone + PartialEq + Eq + Default + Hash {
    /// Name of the value object type, set to the type's identifier by the
    /// derive.
    const NAME: &'static str;
}

pub use croisiere_proc::ValueObject;
