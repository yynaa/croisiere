# Application: Event Services

An event service reacts to what happened in the application. It reads events from the event bus, acts
upon the ones it cares about, and emits other events in response.

[Commands](./commands_and_queries.md#commands) publish the events of their aggregates on the bus once
their changes are stored. Event services are on the other end: notifying someone when money is
deposited, updating a read model, starting the next step of a longer process.

In `croisiere`, event services are jobs: [`EventService`] extends the [`Job`] trait. A job acts on
aggregates or domain services and returns the events it emitted. An event service is a job that
reacts to events read from a bus.

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

Use [`EventBus<AppEvent>`](./event_bus.md) to broadcast the application's events. Each event service
gets its own `EventBusReader<AppEvent>` to read from and an `EventBusSender<AppEvent>` to emit through.
Keep the bus alive for as long as the services need it.

## The trait

```rust,ignore
pub trait Job {
    type Event;
    type Error;

    fn handle(&mut self) -> Result<Vec<Self::Event>, Self::Error>;
}

pub trait EventService: Job {}
```

| Item     | What it is                                                                         |
|----------|------------------------------------------------------------------------------------|
| `Event`  | The events the job emits in response.                                               |
| `Error`  | What the job returns when handling fails.                                           |
| `handle` | Acts on aggregates or domain services and returns the events emitted in response.   |

These items belong to `Job`. `EventService` is a marker trait: it adds no methods or associated
types. Implement `Job` for the service's behavior, then `EventService` to mark it as reacting to
events. The bus and the events it reads are provided by the implementation.

## Writing an event service

Here is a service turning every deposit into a notification:

```rust,ignore
use croisiere::application::{EventBusReader, EventBusSender, EventService, Job};

pub struct DepositNotifier {
    pub reader: EventBusReader<AppEvent>,
    pub bus: EventBusSender<AppEvent>,
}

impl Job for DepositNotifier {
    type Event = AppEvent;
    type Error = AppError;

    fn handle(&mut self) -> Result<Vec<AppEvent>, AppError> {
        let mut emitted = Vec::new();
        while let Ok(event) = self.reader.try_recv() {
            if let AppEvent::Account(AccountEvent::Deposited { id, cents }) = event {
                let notification = AppEvent::Notification {
                    message: format!("{cents} cents were deposited on account {id}"),
                };
                self.bus.send(notification.clone()).map_err(|_| AppError::BusClosed)?;
                emitted.push(notification);
            }
        }
        Ok(emitted)
    }
}

impl EventService for DepositNotifier {}
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

Create the bus once and subscribe each service before publishing events:

```rust,ignore
use croisiere::application::EventBus;

let bus = EventBus::<AppEvent>::default();
let mut notifier = DepositNotifier {
    reader: bus.subscribe(),
    bus: bus.sender(),
};
let mut audit = AuditLog {
    reader: bus.subscribe(),
    bus: bus.sender(),
};
```

There is no relay thread. Each reader has an unbounded queue, so services must keep up with the event
rate to avoid accumulating events in memory. See [Event Bus](./event_bus.md) for delivery and shutdown
behavior.

Keep `bus` in the application and hand a sender to each publisher. The bus can also supply a fresh
sender for each [command](./commands_and_queries.md#running-them).

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
[`Job`]: https://docs.rs/croisiere/latest/croisiere/application/trait.Job.html
