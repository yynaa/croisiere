use std::fmt::Debug;

/// Entity, as defined by DDD.
///
/// It has a lifecycle, and cannot contain other entities or value objects:
/// those links are made by an [`Aggregate`](crate::domain::Aggregate).
///
/// Implement it with `#[derive(Entity)]`, alongside the derives for its
/// supertraits.
///
/// It is recommended to implement a builder for implementors of this trait.
/// You can use a crate like [`builder_pattern`](https://docs.rs/builder-pattern/latest/builder_pattern/).
pub trait Entity: Debug + Default {
    /// Name of the entity type, set to the type's identifier by the derive.
    const NAME: &'static str;
}

pub use croisiere_proc::Entity;
