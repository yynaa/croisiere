# Application: Event Bus

The [`EventBus`] broadcasts application events to independent readers. Commands send events after
committing their changes, and [event services](./event_services.md) read them and may send events in
response. The bus delivers each event to every active subscription.

## Creating a bus

Create a bus with `Default::default`, subscribe each consumer, and get a sender for each publisher:

```rust
use croisiere::application::EventBus;

#[derive(Debug, Clone, PartialEq)]
enum AppEvent {
    Deposited { id: u32, cents: i64 },
}

let bus = EventBus::<AppEvent>::default();
let notifications = bus.subscribe();
let audit = bus.subscribe();
let command_sender = bus.sender();
let service_sender = bus.sender();

let event = AppEvent::Deposited { id: 1, cents: 500 };
command_sender.send(event.clone()).unwrap();

assert_eq!(notifications.try_recv().unwrap(), event);
assert_eq!(audit.try_recv().unwrap(), event);
```

| Type | Role |
|------|------|
| `EventBus<T>` | Owns the subscriptions and creates readers and senders. |
| `EventBusSender<T>` | Sends an event to every active reader. |
| `EventBusReader<T>` | Reads one subscription's queued events without waiting. |

`send` requires `T: Clone`, because each reader receives its own clone. The event type belongs to the
application: it can be an enum covering several aggregates, or an envelope carrying an actor and the
event. See [the events of the application](./event_services.md#the-events-of-the-application).

Call `sender()` for each publisher; the bus and its senders do not implement `Clone`. A
[command context](./commands_and_queries.md#the-context) usually holds an `EventBusSender<AppEvent>`,
while the application keeps the `EventBus<AppEvent>` itself alive.

## Delivery and buffering

Each call to `subscribe` creates a separate, unbounded `crossbeam-channel` queue. A reader sees only
events sent after it subscribed. There is no history to replay, so register the readers before
publishing events they must see. Consuming an event from one reader leaves the others' copies in
their queues.

Sending clones the event directly into each active queue; there is no relay thread to start. Sends
and subscriptions share a mutex, so concurrent sends are serialized and all readers receive those
events in that order. A slow reader does not make a publisher wait for it to consume events. Its
queue keeps growing, so consumers must keep up to avoid accumulating events in memory.

`send` returns `Ok(())` even when there are no readers. Success means delivery to the active queues,
not that any service has processed the event. The bus stores events in memory, with no persistence,
acknowledgment, or retry mechanism. Committing a unit of work and sending its events are separate
operations.

## Reading and shutdown

`try_recv` returns the next queued event immediately. When there is no event, it distinguishes an
empty queue from a disconnected one:

```rust
use croisiere::application::EventBus;
use crossbeam_channel::TryRecvError;

let bus = EventBus::<String>::default();
let reader = bus.subscribe();
let sender = bus.sender();

assert_eq!(reader.try_recv(), Err(TryRecvError::Empty));
sender.send("deposited".to_owned()).unwrap();
drop(bus);

// Events already queued can still be consumed after shutdown.
assert_eq!(reader.try_recv().unwrap(), "deposited");
assert_eq!(reader.try_recv(), Err(TryRecvError::Disconnected));

let error = sender.send("closed".to_owned()).unwrap_err();
assert_eq!(error.0, "closed");
```

Senders hold weak references to the bus. Keeping senders or readers alive does not keep its
subscriptions connected. Dropping the bus releases the subscriptions once any send already in
progress finishes. Readers can drain their queues and then receive `TryRecvError::Disconnected`.
Later sends fail with `crossbeam_channel::SendError<T>`, which contains the original event.

Dropping a reader discards its queued events; a subsequent send removes its disconnected subscription.
An event service receives its own emitted events too, so its handlers must avoid endless cycles.

[`EventBus`]: https://docs.rs/croisiere/latest/croisiere/application/struct.EventBus.html
