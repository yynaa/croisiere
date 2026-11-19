use std::sync::{Arc, Mutex, Weak};

use crossbeam_channel::{Receiver, SendError, Sender, TryRecvError, unbounded};

/// In-memory broadcast of events to independent subscribers.
///
/// Create a bus with [`Default::default`], register readers with
/// [`subscribe`](Self::subscribe), and give publishers their own
/// [`EventBusSender`] with [`sender`](Self::sender). Sending requires `T: Clone`:
/// each active reader gets its own clone of every event sent after it subscribes.
/// Events sent before a subscription are not replayed.
///
/// Each reader has an unbounded queue. Sending does not wait for readers to
/// consume events, so a slow reader can accumulate an arbitrary number of them.
/// Sends and subscriptions share a mutex; concurrent sends are serialized, and
/// readers receive events in that order. There is no relay thread to run.
///
/// The bus owns the subscriptions. Senders hold only weak references and do not
/// keep the bus alive. Dropping the bus disconnects its readers once any send
/// already in progress releases its reference; queued events can still be read.
///
/// # Examples
///
/// ```
/// use croisiere::application::EventBus;
/// use crossbeam_channel::TryRecvError;
///
/// let bus = EventBus::<String>::default();
/// let first = bus.subscribe();
/// let second = bus.subscribe();
/// let sender = bus.sender();
///
/// sender.send("deposited".to_owned()).unwrap();
/// assert_eq!(first.try_recv().unwrap(), "deposited");
/// assert_eq!(second.try_recv().unwrap(), "deposited");
/// assert_eq!(first.try_recv(), Err(TryRecvError::Empty));
///
/// drop(bus);
/// assert!(sender.send("closed".to_owned()).is_err());
/// assert_eq!(first.try_recv(), Err(TryRecvError::Disconnected));
/// ```
pub struct EventBus<T> {
    subscribers: Arc<Mutex<Vec<Sender<T>>>>,
}

impl<T> Default for EventBus<T> {
    fn default() -> Self {
        Self {
            subscribers: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

impl<T> EventBus<T> {
    /// Creates a publisher that does not keep the bus alive.
    ///
    /// Call this for each publisher; senders do not implement `Clone`.
    pub fn sender(&self) -> EventBusSender<T> {
        EventBusSender {
            subscribers: Arc::downgrade(&self.subscribers),
        }
    }

    /// Registers an independent, unbounded queue for future events.
    ///
    /// Dropping the reader discards its queued events. Its subscription is
    /// removed from the bus on a subsequent send.
    ///
    /// # Panics
    ///
    /// Panics if the subscription mutex is poisoned.
    pub fn subscribe(&self) -> EventBusReader<T> {
        let (sender, receiver) = unbounded();
        self.subscribers.lock().unwrap().push(sender);
        EventBusReader { receiver }
    }
}

/// Publisher obtained from [`EventBus::sender`].
///
/// Holds a weak reference to the bus, so keeping a sender does not keep the bus
/// or its subscriptions alive.
pub struct EventBusSender<T> {
    subscribers: Weak<Mutex<Vec<Sender<T>>>>,
}

impl<T: Clone> EventBusSender<T> {
    /// Clones `event` into every active subscriber's queue.
    ///
    /// Disconnected subscriptions are removed. Returns `Ok(())` even if there
    /// are no readers; success means the event was queued for active readers,
    /// not that any service has handled it. Slow readers do not block delivery,
    /// though concurrent sends and subscriptions contend for the same mutex.
    ///
    /// # Errors
    ///
    /// Returns [`SendError`] containing the original event if the bus has been
    /// dropped and its shared state has been released.
    ///
    /// # Panics
    ///
    /// Panics if the subscription mutex is poisoned or `T::clone` panics. A
    /// panic while cloning can leave the event delivered to only some readers
    /// and poisons the mutex.
    pub fn send(&self, event: T) -> Result<(), SendError<T>> {
        let Some(subscribers) = self.subscribers.upgrade() else {
            return Err(SendError(event));
        };

        subscribers
            .lock()
            .unwrap()
            .retain(|sender| sender.send(event.clone()).is_ok());
        Ok(())
    }
}

/// Independent subscription obtained from [`EventBus::subscribe`].
///
/// Holds events until they are received or the reader is dropped. Each reader
/// sees its own copy of the broadcast; reading does not consume another
/// reader's events.
pub struct EventBusReader<T> {
    receiver: Receiver<T>,
}

impl<T> EventBusReader<T> {
    /// Returns the next queued event without waiting.
    ///
    /// # Errors
    ///
    /// Returns [`TryRecvError::Empty`] if the queue is empty but still connected,
    /// or [`TryRecvError::Disconnected`] once the bus's shared state has been
    /// released and all queued events have been read.
    pub fn try_recv(&self) -> Result<T, TryRecvError> {
        self.receiver.try_recv()
    }
}
