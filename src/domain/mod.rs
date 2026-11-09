//! Domain layer: value objects, entities, the aggregates that tie them
//! together, and the domain services that work across them.

mod aggregate;
mod entity;
mod service;
mod value_object;

pub use aggregate::aggregate_functions;
pub use aggregate::make_aggregate;
pub use aggregate::Aggregate;
pub use entity::Entity;
pub use service::domain_service_functions;
pub use service::DomainService;
pub use value_object::ValueObject;
