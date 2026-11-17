/// Job doing actions on domain objects.
///
/// The event type is up to the implementor, and can carry more information
/// than the domain event itself, such as the original caller or actor.
pub trait Job {
    /// Events emitted in response.
    type Event;
    /// Error returned when handling fails.
    type Error;

    /// Acts on aggregates or domain services, then
    /// returns the events emitted in response.
    fn handle(&mut self) -> Result<Vec<Self::Event>, Self::Error>;
}

/// Service reacting to events.
///
/// It reads events from a bus provided to it, acts upon some of them and
/// emits other events in response.
///
/// The event type is up to the implementor, and can carry more information
/// than the domain event itself, such as the original caller or actor.
pub trait EventService: Job {}
