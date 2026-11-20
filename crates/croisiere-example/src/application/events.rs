//! Application event boundary and actor metadata carried on the event bus.
//!
//! Events are cloneable because the in-memory bus broadcasts a copy to every
//! subscriber. Commands publish envelopes after committing their state changes.

use crate::domain::{DomainEvent, aggregates::CounterEvent};

/// Events of the application: domain events, plus the ones only the
/// application layer knows about.
#[derive(Debug, Clone)]
pub enum AppEvent {
    /// Emitted by a domain operation.
    Domain(DomainEvent),
    /// Emitted by the `IncrementNotifier` event service.
    Notification {
        /// Text prepared by the notification job; no delivery is performed here.
        message: String,
    },
}

/// Lets `do_as_aggregate` / `do_and_make_actionable` convert domain events.
impl From<CounterEvent> for AppEvent {
    fn from(event: CounterEvent) -> Self {
        Self::from(DomainEvent::from(event))
    }
}

impl From<DomainEvent> for AppEvent {
    fn from(event: DomainEvent) -> Self {
        Self::Domain(event)
    }
}

/// What actually travels on the bus: an event with who caused it.
#[derive(Debug, Clone)]
pub struct Envelope {
    /// Actor responsible for the original operation.
    pub actor: String,
    /// Domain or application fact being delivered.
    pub event: AppEvent,
}
