//! Illustrative notification job, separate from the binary's request flow.
//!
//! The job drains a supplied channel and returns follow-up events. Its caller
//! is responsible for feeding that channel and publishing the returned events.
//! The binary does not construct or schedule this service.

use std::convert::Infallible;

use croisiere::application::{EventService, Job};
use crossbeam_channel::Receiver;

use crate::{
    application::events::{AppEvent, Envelope},
    domain::{DomainEvent, aggregates::CounterEvent},
};

/// Turns queued counter increments into notification events with the same actor.
///
/// Ignores other events and processes only those available when polled. It
/// returns notification data without delivering messages or sending bus events.
pub struct IncrementNotifier {
    /// Input queue supplied by the service's caller.
    pub bus: Receiver<Envelope>,
}

impl EventService for IncrementNotifier {}

impl Job for IncrementNotifier {
    type Event = Envelope;
    type Error = Infallible;

    fn handle(&mut self) -> Result<Vec<Envelope>, Infallible> {
        Ok(self
            .bus
            .try_iter()
            .filter_map(|Envelope { actor, event }| match event {
                AppEvent::Domain(DomainEvent::Counter(CounterEvent::Incremented { name })) => {
                    Some(Envelope {
                        event: AppEvent::Notification {
                            message: format!("{name} was incremented by {actor}"),
                        },
                        actor,
                    })
                }
                _ => None,
            })
            .collect())
    }
}
