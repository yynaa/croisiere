//! Domain layer: value objects, entities, and the aggregates that tie them
//! together.

mod aggregate;
mod entity;
mod value_object;

pub use aggregate::aggregate_functions;
pub use aggregate::make_aggregate;
pub use aggregate::Aggregate;
pub use entity::Entity;
pub use value_object::ValueObject;
