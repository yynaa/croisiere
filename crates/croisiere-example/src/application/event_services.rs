use std::convert::Infallible;

use croisiere::application::EventService;
use crossbeam_channel::Receiver;

use crate::{
    application::events::{AppEvent, Envelope},
    domain::aggregates::CounterEvent,
};

/// Turns every counter increment read from the bus into a notification,
/// keeping the actor that caused it.
pub struct IncrementNotifier {
    pub bus: Receiver<Envelope>,
}

impl EventService for IncrementNotifier {
    type Event = Envelope;
    type Error = Infallible;

    fn handle(&mut self) -> Result<Vec<Envelope>, Infallible> {
        Ok(self
            .bus
            .try_iter()
            .filter_map(|Envelope { actor, event }| match event {
                AppEvent::Counter(CounterEvent::Incremented { name }) => Some(Envelope {
                    event: AppEvent::Notification {
                        message: format!("{name} was incremented by {actor}"),
                    },
                    actor,
                }),
                _ => None,
            })
            .collect())
    }
}
