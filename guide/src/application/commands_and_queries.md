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

A query only needs the unit of work. A command also needs somewhere to send the events its aggregates
emit: the [event bus](./event_services.md#the-event-bus), which sends them to every event service.

```rust,ignore
pub struct CommandContext {
    pub uow: Uow,
    pub bus: EventBus,
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
            context.bus.publish(event)?;
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
            .count(HashSet::from([AccountFilter::ById(self.id)]), None)?;
        if existing > 0 {
            return Err(AppError::AlreadyExists(self.id));
        }

        let (aggregate, events) = AccountAggregate::open(self.id, self.owner.clone())?;
        let actionable = VersionedSnapshot::<AccountSnapshot>::assemble(aggregate, 0)
            .make_actionable(registry.write_accounts(), SnapshotAction::Save);

        context.uow.register_action(Box::new(actionable));
        context.uow.commit_and_rollback()?;

        for event in events {
            context.bus.publish(event.into())?;
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

## Running them

Whoever receives requests from the outside world, such as a web server or a command-line interface,
starts a unit of work and hands it to the request:

```rust,ignore
pub fn execute<C>(factory: &Factory, bus: &EventBus, command: C) -> Result<C::Response, C::Error>
where
    C: Command<Context = CommandContext>,
{
    command.handle(CommandContext {
        uow: factory.begin(),
        bus: bus.clone(),
    })
}

pub fn ask<Q>(factory: &Factory, query: Q) -> Result<Q::Response, Q::Error>
where
    Q: Query<Context = Uow>,
{
    query.handle(factory.begin())
}

let response = execute(&factory, &bus, Deposit { id: 1, cents: 500 })?;
let accounts = ask(&factory, AccountsOf { owner: "Ada".into(), pagination: None })?;
```

This is also the place to use `name`, for example to log every request.

[`Command`]: https://docs.rs/croisiere/latest/croisiere/application/trait.Command.html
[`Query`]: https://docs.rs/croisiere/latest/croisiere/application/trait.Query.html
