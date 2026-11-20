//! Public event boundary for the counter domain.

use super::aggregates::CounterEvent;

/// All domain events, preserving each aggregate's specific event type.
///
/// Aggregate operations return their own events; callers convert them into
/// this enum when crossing the domain boundary.
#[derive(Debug, Clone, derive_more::From)]
pub enum DomainEvent {
    /// A fact emitted by a counter operation.
    Counter(CounterEvent),
}
