/// Service reacting to events.
///
/// It reads events from a bus provided to it, acts upon some of them and
/// emits other events in response.
///
/// The event type is up to the implementor, and can carry more information
/// than the domain event itself, such as the original caller or actor.
pub trait EventService {
    /// Events read from the bus and emitted in response.
    type Event;
    /// Error returned when handling fails.
    type Error;

    /// Reads the pending events from the bus, acts upon the relevant ones and
    /// returns the events emitted in response.
    fn handle(&mut self) -> Result<Vec<Self::Event>, Self::Error>;
}
