# Application: Event Services

An event service reacts to what happened in the application. It reads events from the event bus, acts
upon the ones it cares about, and emits other events in response.

[Commands](./commands_and_queries.md#commands) publish the events of their aggregates on the bus once
their changes are stored. Event services are on the other end: notifying someone when money is
deposited, updating a read model, starting the next step of a longer process.

In `croisiere`, event services are described by the [`EventService`] trait.

## The events of the application

The bus carries the events of the application, an enum of yours. It holds the events of every
aggregate, plus the ones only the application knows about, such as those emitted by event services:

```rust,ignore
#[derive(Debug, Clone)]
pub enum AppEvent {
    /// Emitted by an `AccountAggregate` function.
    Account(AccountEvent),
    /// Emitted by the `DepositNotifier` event service.
    Notification { message: String },
}

impl From<AccountEvent> for AppEvent {
    fn from(event: AccountEvent) -> Self {
        Self::Account(event)
    }
}
```

Converting from each aggregate's events lets
[`do_and_make_actionable`](./units_of_work.md#actionable-snapshots) return them as `AppEvent`. Every
event service gets its own copy of each event, hence `Clone`.

The events on the bus can carry more than the domain event itself, such as who caused it. Wrap them in
a type of yours, and send that on the bus instead:

```rust,ignore
#[derive(Debug, Clone)]
pub struct Envelope {
    pub actor: String,
    pub event: AppEvent,
}
```

## The event bus

Every event service must see every event, and both commands and event services publish events. The
bus is therefore a broadcast channel with several senders, built from two crates:

- a [`crossbeam-channel`] channel, whose sender can be cloned for every publisher;
- a [`bus`] broadcast, which gives every event to each of its readers.

A relay moves events from the first to the second. Publishers only ever see the sending end:

```rust,ignore
use crossbeam_channel::Sender;

/// Where commands and event services publish events.
#[derive(Clone)]
pub struct EventBus {
    sender: Sender<AppEvent>,
}

impl EventBus {
    pub fn new(sender: Sender<AppEvent>) -> Self {
        Self { sender }
    }

    pub fn publish(&self, event: AppEvent) -> Result<(), AppError> {
        self.sender.send(event).map_err(|_| AppError::BusClosed)
    }
}
```

Each event service gets its own reader of the broadcast to read from, and the bus to emit through.

## The trait

```rust,ignore
pub trait EventService {
    type Event;
    type Error;

    fn handle(&mut self) -> Result<Vec<Self::Event>, Self::Error>;
}
```

| Item     | What it is                                                                         |
|----------|------------------------------------------------------------------------------------|
| `Event`  | The events the service reads, and emits in response.                               |
| `Error`  | What the service returns when handling fails.                                      |
| `handle` | Reads the pending events, acts upon the relevant ones, and returns what it emitted. |

## Writing an event service

Here is a service turning every deposit into a notification:

```rust,ignore
use bus::BusReader;
use croisiere::application::EventService;

pub struct DepositNotifier {
    pub reader: BusReader<AppEvent>,
    pub bus: EventBus,
}

impl EventService for DepositNotifier {
    type Event = AppEvent;
    type Error = AppError;

    fn handle(&mut self) -> Result<Vec<AppEvent>, AppError> {
        let mut emitted = Vec::new();
        while let Ok(event) = self.reader.try_recv() {
            if let AppEvent::Account(AccountEvent::Deposited { id, cents }) = event {
                let notification = AppEvent::Notification {
                    message: format!("{cents} cents were deposited on account {id}"),
                };
                self.bus.publish(notification.clone())?;
                emitted.push(notification);
            }
        }
        Ok(emitted)
    }
}
```

`try_recv` reads the events waiting for the service without blocking, so `handle` returns once they
are all handled. Events the service doesn't care about are simply dropped.

The service publishes what it emits itself, through the bus. What `handle` returns is a report of
those events, for example to log them.

Since every event goes to every service, a service also receives the events it emitted. Make sure it
does not react to its own events in a way that never ends.

A service that needs to change the application, rather than only emit events, can hold what a
[command](./commands_and_queries.md) needs, such as the unit of work factory, and run commands from
`handle`.

## Running event services

Create the channel and the broadcast once. Add one reader per service, then start the relay, which
broadcasts every event published on the channel:

```rust,ignore
use bus::Bus;

let (sender, receiver) = crossbeam_channel::unbounded();
let mut broadcast = Bus::new(1024);

let mut notifier = DepositNotifier {
    reader: broadcast.add_rx(),
    bus: EventBus::new(sender.clone()),
};
let mut audit = AuditLog {
    reader: broadcast.add_rx(),
    bus: EventBus::new(sender.clone()),
};

std::thread::spawn(move || {
    for event in receiver {
        broadcast.broadcast(event);
    }
});

let bus = EventBus::new(sender);
```

A reader only gets the events broadcast after it was added, so add every reader before anything is
published. The relay stops once every `EventBus` is dropped.

The broadcast holds a fixed number of events, `1024` here. When a reader falls that far behind, the
relay waits for it, while publishers keep sending to the channel.

The last `bus` is then handed to [commands](./commands_and_queries.md#running-them).

Whoever runs the application calls `handle` on each service regularly, for example each on its own
thread:

```rust,ignore
std::thread::spawn(move || -> Result<(), AppError> {
    loop {
        for event in notifier.handle()? {
            println!("emitted {event:?}");
        }
        // Wait for more events, e.g. by sleeping.
    }
});
```

[`EventService`]: https://docs.rs/croisiere/latest/croisiere/application/trait.EventService.html
[`crossbeam-channel`]: https://docs.rs/crossbeam-channel/latest/crossbeam_channel/
[`bus`]: https://docs.rs/bus/latest/bus/
