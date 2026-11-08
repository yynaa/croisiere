use std::{cell::RefCell, collections::HashSet, rc::Weak};

use croisiere::application::{Command, SnapshotAction, VersionedSnapshot};

use crate::{
    application::{
        errors::AppError,
        events::{AppEvent, Envelope},
        registry::{upgrade, CounterRepository, Factory},
        snapshots::{CounterFilter, CounterSnapshot},
    },
    domain::aggregates::CounterAggregate,
};

/// Creates a counter, starting at 0.
pub struct CreateCounter {
    pub actor: String,
    pub name: String,
}

impl Command for CreateCounter {
    type Context = Factory;
    type Response = Vec<Envelope>;
    type Error = AppError;

    fn name(&self) -> &'static str {
        "CreateCounter"
    }

    fn handle(&self, factory: Factory) -> Result<Vec<Envelope>, AppError> {
        let uow = factory.begin();
        let counters = uow.registry().counters();

        // Read side: refuse duplicates.
        let filters = HashSet::from([CounterFilter::ByName(self.name.clone())]);
        if upgrade(&counters)?.borrow_mut().count(filters, None)? > 0 {
            return Err(AppError::AlreadyExists(self.name.clone()));
        }

        // The aggregate creates itself (and may refuse to), then is assembled
        // into a snapshot at version 0; `make_actionable` bumps it to 1.
        let (aggregate, events) = CounterAggregate::create(self.name.clone())?;
        let actionable = VersionedSnapshot::<CounterSnapshot>::assemble(aggregate, 0)
            .make_actionable(counters, SnapshotAction::Save);

        uow.register_action(Box::new(actionable));
        uow.commit_and_rollback()?;

        Ok(events
            .into_iter()
            .map(|event| Envelope {
                actor: self.actor.clone(),
                event: event.into(),
            })
            .collect())
    }
}

/// Increments a counter.
pub struct IncrementCounter {
    pub actor: String,
    pub name: String,
}

impl Command for IncrementCounter {
    type Context = Factory;
    type Response = Vec<Envelope>;
    type Error = AppError;

    fn name(&self) -> &'static str {
        "IncrementCounter"
    }

    fn handle(&self, factory: Factory) -> Result<Vec<Envelope>, AppError> {
        let uow = factory.begin();
        let counters = uow.registry().counters();
        let snapshot = find_by_name(&counters, &self.name)?;

        // Snapshot -> aggregate -> run the aggregate function -> snapshot,
        // ready to be saved. Domain events come back converted into
        // `AppEvent`, errors into `AppError`.
        let (actionable, events) = snapshot.do_and_make_actionable::<AppEvent, AppError, _>(
            counters,
            SnapshotAction::Save,
            CounterAggregate::increment,
        )?;

        uow.register_action(Box::new(actionable));
        uow.commit_and_rollback()?;

        Ok(events
            .into_iter()
            .map(|event| Envelope {
                actor: self.actor.clone(),
                event,
            })
            .collect())
    }
}

/// Deletes a counter.
pub struct DeleteCounter {
    pub name: String,
}

impl Command for DeleteCounter {
    type Context = Factory;
    type Response = ();
    type Error = AppError;

    fn name(&self) -> &'static str {
        "DeleteCounter"
    }

    fn handle(&self, factory: Factory) -> Result<(), AppError> {
        let uow = factory.begin();
        let counters = uow.registry().counters();
        let actionable = find_by_name(&counters, &self.name)?
            .make_actionable(counters, SnapshotAction::Delete);

        uow.register_action(Box::new(actionable));
        uow.commit_and_rollback()
    }
}

fn find_by_name(
    counters: &Weak<RefCell<dyn CounterRepository>>,
    name: &str,
) -> Result<VersionedSnapshot<CounterSnapshot>, AppError> {
    let filters = HashSet::from([CounterFilter::ByName(name.to_owned())]);
    let found = upgrade(counters)?.borrow_mut().get(filters, None, None)?;
    found
        .into_iter()
        .next()
        .ok_or_else(|| AppError::NotFound(name.to_owned()))
}
