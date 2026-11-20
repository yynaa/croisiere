//! Counter identity, with field validation delegated to value objects.

use croisiere::domain::Entity;

use super::value_objects::CounterName;

/// The entity at the root of the counter aggregate.
///
/// Its name is its domain identity. `CounterName` ensures the name is nonempty;
/// the entity does not duplicate that validation. The count and the operations
/// that change it belong to the aggregate.
#[derive(Debug, Default, Entity)]
pub struct Counter {
    /// Validated domain name, independent of the storage UUID.
    pub name: CounterName,
}
