# Surface Water: Application

The application layer coordinates use cases through commands and queries. It uses domain types
for business decisions and ports for persistence and other external capabilities.

## Port descriptor

> [!TIP]
> **Create a port descriptor holding weak references to important adapters.** Group access to
> capabilities such as the unit-of-work factory and event bus in an application-owned structure.
> Keep the strong references in the main crate that wires the application together. The descriptor
> lets handlers find their dependencies without keeping those adapters alive indefinitely.

For example, a descriptor can refer to the abstract unit-of-work factory and the shared event bus:

```rust,ignore
use std::sync::Weak;

use croisiere::application::{EventBus, UnitOfWorkFactory};

pub type DynFactory = dyn UnitOfWorkFactory<Error = RepositoryError, Registry = Registry>;

#[derive(Clone)]
pub struct Ports {
    pub uow_factory: Weak<DynFactory>,
    pub event_bus: Weak<EventBus<AppEvent>>,
}
```

The main crate owns the corresponding `Arc`s and constructs the descriptor with `Arc::downgrade`.
Keep its fields expressed in application-facing types rather than concrete database repositories
or other infrastructure implementations.

The [shared command and query handling routines](./application/commands_and_queries.md#running-them)
upgrade the ports they need, then prepare request context: a fresh unit of work, an event bus sender
for commands and any common request information. If an adapter is no longer available, return an
application error instead of unwrapping the weak reference. Retain the upgraded strong references
for as long as the request needs the adapters.

The descriptor contains shared capabilities; the command or query context contains what one request
needs to run. Keeping those roles separate also makes it possible to supply alternative adapters
when testing the application.
