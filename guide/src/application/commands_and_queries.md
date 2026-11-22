# Application: Commands & Queries

Everything the outside world asks of the application is either a command or a query, as in CQRS:

- a **command** asks to change the state of the application: open an account, deposit money;
- a **query** asks to read it, without changing anything: list the accounts of an owner.

Each request is a type of yours, holding what the request needs to know. In `croisiere`, commands
implement the [`Command`] trait, and queries the [`Query`] trait. The two traits are the same, only
their meaning differs.

## The traits

```rust,ignore
pub trait Command {
    type Context;
    type Response;
    type Error;

    fn name(&self) -> &'static str;
    fn handle(&self, context: Self::Context) -> Result<Self::Response, Self::Error>;
}
```

[`Query`] has the same associated types and functions.

| Item       | What it is                                                                |
|------------|---------------------------------------------------------------------------|
| `Context`  | What the request needs to run, given to `handle`, such as a unit of work. |
| `Response` | What the request returns on success, an enum of yours.                    |
| `Error`    | What the request returns on failure, usually your application error.      |
| `name`     | The name of the request, for logging or dispatching.                      |
| `handle`   | Runs the request with its context.                                        |

### The context

The context is up to you. A good choice is built around a [unit of work](./units_of_work.md), started
for this one request by whoever runs it:

```rust,ignore
use croisiere::application::UnitOfWork;

pub type Uow = Box<dyn UnitOfWork<Error = RepositoryError, Registry = Registry>>;
```

The request then reaches the repositories through its [registry](./units_of_work.md#the-registry).

Alternatively, pass the shared `Factory` as the context and call `factory.begin()?` inside `handle`.
A read-only query uses the registry without registering actions or committing. Both approaches keep
storage construction in infrastructure.

A query only needs the unit of work. A command also needs somewhere to send the events its aggregates
emit: the [event bus](./event_bus.md), which sends them to every event service.

```rust,ignore
use croisiere::application::{EventBus, EventBusSender};

pub struct CommandContext {
    pub uow: Uow,
    pub bus: EventBusSender<AppEvent>,
}
```

### The response

Prefer an enum of yours as the response, rather than returning data directly. Its variants say what
happened, and new ones can be added later without changing what the existing ones mean.

## Commands

A command loads the aggregates it changes, runs their
[aggregate functions](../domain/aggregates.md#aggregate-functions), registers the resulting snapshots
on its unit of work and commits. It then publishes the events the aggregates emitted on the bus.

Its response usually just says it went well. A command creating something can say so instead:

```rust,ignore
#[derive(Debug)]
pub enum CommandResponse {
    Ok,
    Created,
}
```

```rust,ignore
use std::collections::HashSet;

use croisiere::application::{Command, SnapshotAction};

pub struct Deposit {
    pub id: u32,
    pub cents: i64,
}

impl Command for Deposit {
    type Context = CommandContext;
    type Response = CommandResponse;
    type Error = AppError;

    fn name(&self) -> &'static str {
        "Deposit"
    }

    fn handle(&self, context: CommandContext) -> Result<CommandResponse, AppError> {
        let registry = context.uow.registry();

        let snapshot = registry
            .read_accounts()
            .get(HashSet::from([AccountFilter::ById(self.id)]), None, None)?
            .pop()
            .ok_or(AppError::NotFound(self.id))?;

        let (actionable, events) = snapshot.do_and_make_actionable::<AppEvent, AppError, _>(
            registry.write_accounts(),
            SnapshotAction::Save,
            |account| account.deposit(self.cents),
        )?;

        context.uow.register_action(Box::new(actionable));
        context.uow.commit_and_rollback()?;

        for event in events {
            context.bus.send(event).map_err(|_| AppError::BusClosed)?;
        }
        Ok(CommandResponse::Ok)
    }
}
```

Events are published only once the unit of work has committed, so nothing hears about changes that were
not stored.

A command creating an aggregate checks what it needs to first, then saves the new aggregate:

```rust,ignore
pub struct OpenAccount {
    pub id: u32,
    pub owner: String,
}

impl Command for OpenAccount {
    type Context = CommandContext;
    type Response = CommandResponse;
    type Error = AppError;

    fn name(&self) -> &'static str {
        "OpenAccount"
    }

    fn handle(&self, context: CommandContext) -> Result<CommandResponse, AppError> {
        let registry = context.uow.registry();

        let existing = registry
            .read_accounts()
            .count(HashSet::from([AccountFilter::ById(self.id)]))?;
        if existing > 0 {
            return Err(AppError::AlreadyExists(self.id));
        }

        let (aggregate, events) = AccountAggregate::open(self.id, self.owner.clone())?;
        let actionable = VersionedSnapshot::<AccountSnapshot>::assemble(aggregate, 0)
            .make_actionable(registry.write_accounts(), SnapshotAction::Save);

        context.uow.register_action(Box::new(actionable));
        context.uow.commit_and_rollback()?;

        for event in events {
            context.bus.send(event.into()).map_err(|_| AppError::BusClosed)?;
        }
        Ok(CommandResponse::Created)
    }
}
```

All the changes of a command go through one unit of work, so they are stored together or not at all.

## Queries

A query reads snapshots and responds with what was asked. It never registers changes, and never
commits its unit of work. Its response is an enum too, whose variants carry what was found:

```rust,ignore
use croisiere::application::{Pagination, Query};

#[derive(Debug)]
pub enum AccountsResponse {
    Found(Vec<AccountSnapshot>),
    Empty,
}

pub struct AccountsOf {
    pub owner: String,
    pub pagination: Option<Pagination>,
}

impl Query for AccountsOf {
    type Context = Uow;
    type Response = AccountsResponse;
    type Error = AppError;

    fn name(&self) -> &'static str {
        "AccountsOf"
    }

    fn handle(&self, uow: Uow) -> Result<AccountsResponse, AppError> {
        let found = uow.registry().read_accounts().get(
            HashSet::from([AccountFilter::ByOwner(self.owner.clone())]),
            Some(AccountSorter::ById),
            self.pagination.as_ref().map(|page| Pagination {
                limit: page.limit,
                offset: page.offset,
            }),
        )?;

        if found.is_empty() {
            return Ok(AccountsResponse::Empty);
        }
        Ok(AccountsResponse::Found(
            found.into_iter().map(|versioned| versioned.snapshot).collect(),
        ))
    }
}
```

A query can respond with the snapshots themselves, or with a view of its own shaped for whoever asked.

## Application-wide commands and queries

> [!TIP]
> **Define a global command enum and a global query enum, and implement `Command` and `Query`
> for them.** Each variant wraps a specific request and delegates to its implementation. This gives
> the application a common entry point for each kind of request. A shared handling routine can
> fetch or construct common context once per request before dispatching it, rather than repeating
> that setup in every presentation entry point.

The commands above already share their context, response and error types, so their enum can
delegate directly:

```rust,ignore
pub enum AppCommand {
    OpenAccount(OpenAccount),
    Deposit(Deposit),
}

impl Command for AppCommand {
    type Context = CommandContext;
    type Response = CommandResponse;
    type Error = AppError;

    fn name(&self) -> &'static str {
        match self {
            Self::OpenAccount(command) => command.name(),
            Self::Deposit(command) => command.name(),
        }
    }

    fn handle(&self, context: CommandContext) -> Result<CommandResponse, AppError> {
        match self {
            Self::OpenAccount(command) => command.handle(context),
            Self::Deposit(command) => command.handle(context),
        }
    }
}
```

Apply the same pattern to queries. If their responses differ, wrap each response in a variant of a
global response enum:

```rust,ignore
pub enum AppQuery {
    AccountsOf(AccountsOf),
}

pub enum QueryResponse {
    Accounts(AccountsResponse),
}

impl Query for AppQuery {
    type Context = Uow;
    type Response = QueryResponse;
    type Error = AppError;

    fn name(&self) -> &'static str {
        match self {
            Self::AccountsOf(query) => query.name(),
        }
    }

    fn handle(&self, context: Uow) -> Result<QueryResponse, AppError> {
        match self {
            Self::AccountsOf(query) => query.handle(context).map(QueryResponse::Accounts),
        }
    }
}
```

Add variants as the application gains requests. Keep the enums focused on dispatch; business rules
stay in the domain, and each concrete command or query keeps its own use-case implementation.

## Running them

Whoever receives requests from the outside world, such as a web server or a command-line interface,
starts a unit of work and hands it to the request:

```rust,ignore
pub fn execute<C>(factory: &Factory, bus: &EventBus<AppEvent>, command: C) -> Result<C::Response, C::Error>
where
    C: Command<Context = CommandContext>,
{
    command.handle(CommandContext {
        uow: factory.begin()?,
        bus: bus.sender(),
    })
}

pub fn ask<Q>(factory: &Factory, query: Q) -> Result<Q::Response, Q::Error>
where
    Q: Query<Context = Uow>,
{
    query.handle(factory.begin()?)
}

let response = execute(&factory, &bus, Deposit { id: 1, cents: 500 })?;
let accounts = ask(&factory, AccountsOf { owner: "Ada".into(), pagination: None })?;
```

The same routines accept the global enums, since those enums implement the traits too:

```rust,ignore
let response = execute(
    &factory,
    &bus,
    AppCommand::Deposit(Deposit { id: 1, cents: 500 }),
)?;
let accounts = ask(
    &factory,
    AppQuery::AccountsOf(AccountsOf { owner: "Ada".into(), pagination: None }),
)?;
```

These routines prepare a fresh context for each request. They are also a place to fetch shared
request information, such as the authenticated actor, and use `name` to log requests consistently.
If every caller should go through the global enums, accept `AppCommand` and `AppQuery` directly
instead of generic parameters.

[`Command`]: https://docs.rs/croisiere/latest/croisiere/application/trait.Command.html
[`Query`]: https://docs.rs/croisiere/latest/croisiere/application/trait.Query.html
