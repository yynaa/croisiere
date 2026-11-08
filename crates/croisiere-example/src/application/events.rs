use crate::domain::aggregates::CounterEvent;

/// Events of the application: domain events, plus the ones only the
/// application layer knows about.
#[derive(Debug)]
pub enum AppEvent {
    /// Emitted by a `CounterAggregate` function.
    Counter(CounterEvent),
    /// Emitted by the `IncrementNotifier` event service.
    Notification { message: String },
}

/// Lets `do_as_aggregate` / `do_and_make_actionable` convert domain events.
impl From<CounterEvent> for AppEvent {
    fn from(event: CounterEvent) -> Self {
        Self::Counter(event)
    }
}

/// What actually travels on the bus: an event with who caused it.
#[derive(Debug)]
pub struct Envelope {
    pub actor: String,
    pub event: AppEvent,
}
