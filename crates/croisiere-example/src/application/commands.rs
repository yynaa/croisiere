//! Command dispatch, per-request context and event publication after commit.

use std::collections::HashSet;

use croisiere::application::{
    Command, EventBus, EventBusSender, SnapshotAction, VersionedSnapshot,
};

use crate::{
    application::{
        errors::AppError,
        events::{AppEvent, Envelope},
        registry::{Factory, Uow},
        repositories::ReadRepositoryExt,
        snapshots::{CounterFilter, CounterSnapshot},
    },
    domain::aggregates::CounterAggregate,
};

/// Context prepared once for a command, shared by all command implementations.
pub struct CommandContext {
    /// Fresh unit of work for this request.
    pub uow: Uow,
    /// Publisher used only after committing state changes.
    pub bus: EventBusSender<Envelope>,
    /// Actor attached to every event emitted by this request.
    pub actor: String,
}

/// All commands exposed by the counter application.
pub enum AppCommand {
    /// Create a counter with a validated name and an initial count of zero.
    CreateCounter(CreateCounter),
    /// Increase an existing counter's count.
    IncrementCounter(IncrementCounter),
    /// Remove an existing counter.
    DeleteCounter(DeleteCounter),
}

/// Command outcomes; emitted events travel through the bus instead.
#[derive(Debug, PartialEq, Eq)]
pub enum CommandResponse {
    /// A new counter was committed and its creation event was published.
    Created,
    /// The operation committed and any events it produced were published.
    Ok,
}

impl Command for AppCommand {
    type Context = CommandContext;
    type Response = CommandResponse;
    type Error = AppError;

    fn name(&self) -> &'static str {
        match self {
            Self::CreateCounter(command) => command.name(),
            Self::IncrementCounter(command) => command.name(),
            Self::DeleteCounter(command) => command.name(),
        }
    }

    fn handle(&self, context: CommandContext) -> Result<CommandResponse, AppError> {
        match self {
            Self::CreateCounter(command) => command.handle(context),
            Self::IncrementCounter(command) => command.handle(context),
            Self::DeleteCounter(command) => command.handle(context),
        }
    }
}

/// Prepares a fresh unit of work and publisher before dispatching a command.
///
/// A publication failure is reported as `BusClosed`; persistence has already
/// committed at that point and is not rolled back by a failed send.
pub fn execute(
    factory: &Factory,
    bus: &EventBus<Envelope>,
    actor: String,
    command: AppCommand,
) -> Result<CommandResponse, AppError> {
    command.handle(CommandContext {
        uow: factory.begin()?,
        bus: bus.sender(),
        actor,
    })
}

/// Creates a counter, starting at 0.
///
/// Refuses an existing domain name, delegates name validation to the aggregate
/// and publishes `Created` after saving version 1. Storage also checks uniqueness
/// during commit because the preliminary existence check can race another request.
pub struct CreateCounter {
    /// Unvalidated domain name supplied by the caller.
    pub name: String,
}

impl Command for CreateCounter {
    type Context = CommandContext;
    type Response = CommandResponse;
    type Error = AppError;

    fn name(&self) -> &'static str {
        "CreateCounter"
    }

    fn handle(&self, context: CommandContext) -> Result<CommandResponse, AppError> {
        let uow = context.uow;

        // Read side: refuse duplicates.
        let filters = HashSet::from([CounterFilter::ByName(self.name.clone())]);
        if uow.registry().read_counters().exists(filters)? {
            return Err(AppError::AlreadyExists(self.name.clone()));
        }

        // The aggregate creates itself (and may refuse to), then is assembled
        // into a snapshot at version 0; `make_actionable` bumps it to 1.
        let (aggregate, events) = CounterAggregate::create(self.name.clone())?;
        let actionable = VersionedSnapshot::<CounterSnapshot>::assemble(aggregate, 0)
            .make_actionable(uow.registry().write_counters(), SnapshotAction::Save);

        uow.register_action(Box::new(actionable));
        uow.commit_and_rollback()?;

        // Publication follows persistence; a rejected commit emits nothing.
        for event in events {
            context
                .bus
                .send(Envelope {
                    actor: context.actor.clone(),
                    event: event.into(),
                })
                .map_err(|_| AppError::BusClosed)?;
        }
        Ok(CommandResponse::Created)
    }
}

/// Increments a counter.
///
/// Requires exactly one matching snapshot, restores the aggregate, runs its
/// increment operation and queues the next snapshot version. Publishes
/// `Incremented` with the context's actor after committing.
pub struct IncrementCounter {
    /// Exact domain name to look up; no normalization is applied.
    pub name: String,
}

impl Command for IncrementCounter {
    type Context = CommandContext;
    type Response = CommandResponse;
    type Error = AppError;

    fn name(&self) -> &'static str {
        "IncrementCounter"
    }

    fn handle(&self, context: CommandContext) -> Result<CommandResponse, AppError> {
        let uow = context.uow;
        let snapshot = uow
            .registry()
            .read_counters()
            .get_unique(HashSet::from([CounterFilter::ByName(self.name.clone())]))?;

        // Snapshot -> aggregate -> run the aggregate function -> snapshot,
        // ready to be saved. Domain events come back converted into
        // `AppEvent`, errors into `AppError`.
        let (actionable, events) = snapshot.do_and_make_actionable::<AppEvent, AppError, _>(
            uow.registry().write_counters(),
            SnapshotAction::Save,
            CounterAggregate::increment,
        )?;

        uow.register_action(Box::new(actionable));
        uow.commit_and_rollback()?;

        for event in events {
            context
                .bus
                .send(Envelope {
                    actor: context.actor.clone(),
                    event,
                })
                .map_err(|_| AppError::BusClosed)?;
        }
        Ok(CommandResponse::Ok)
    }
}

/// Deletes a counter.
///
/// Requires exactly one matching snapshot and queues a version-checked delete.
/// This example does not define a deletion event, so success publishes nothing.
pub struct DeleteCounter {
    /// Exact domain name of the counter to remove.
    pub name: String,
}

impl Command for DeleteCounter {
    type Context = CommandContext;
    type Response = CommandResponse;
    type Error = AppError;

    fn name(&self) -> &'static str {
        "DeleteCounter"
    }

    fn handle(&self, context: CommandContext) -> Result<CommandResponse, AppError> {
        let uow = context.uow;
        let actionable = uow
            .registry()
            .read_counters()
            .get_unique(HashSet::from([CounterFilter::ByName(self.name.clone())]))?
            .make_actionable(uow.registry().write_counters(), SnapshotAction::Delete);

        uow.register_action(Box::new(actionable));
        uow.commit_and_rollback()?;
        Ok(CommandResponse::Ok)
    }
}
