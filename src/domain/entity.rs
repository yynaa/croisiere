use std::fmt::Debug;

/// Entity, as defined by DDD.
///
/// It has an identity and a lifecycle: two entities with the same fields are
/// still two different entities.
///
/// It can contain value objects and other entities. Be thoughtful about which
/// ones you embed: an embedded entity lives and changes along with the one
/// containing it. Links between entities that live independently are made by
/// an [`Aggregate`](crate::domain::Aggregate).
///
/// Value objects cannot contain entities.
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
