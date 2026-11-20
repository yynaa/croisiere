//! Request dispatch, publication ordering, lookup helpers and failure behavior.

#[allow(dead_code)]
#[path = "../src/application/mod.rs"]
mod application;
#[allow(dead_code)]
#[path = "../src/domain/mod.rs"]
mod domain;
#[allow(dead_code)]
#[path = "../src/infrastructure/mod.rs"]
mod infrastructure;

use std::{collections::HashSet, sync::Arc};

use croisiere::application::{AbstractActionableSnapshot, Command, EventBus, UnitOfWork};

use application::{
    commands::{
        AppCommand, CommandContext, CommandResponse, CreateCounter, DeleteCounter,
        IncrementCounter, execute,
    },
    errors::{AppError, RepositoryError},
    events::{AppEvent, Envelope},
    queries::{AppQuery, ListCounters, ListCountersResponse, QueryResponse, ask},
    registry::{Factory, Registry, Uow},
    repositories::ReadRepositoryExt,
    snapshots::CounterFilter,
};
use domain::{DomainError, DomainEvent, aggregates::CounterEvent};
use infrastructure::persistence::{CounterFactory, counter_factory};

fn factory() -> Factory {
    let factory: CounterFactory = counter_factory();
    Arc::new(factory)
}

fn list(factory: &Factory) -> ListCountersResponse {
    match ask(
        factory,
        AppQuery::ListCounters(ListCounters {
            sorter: None,
            pagination: None,
        }),
    )
    .unwrap()
    {
        QueryResponse::Counters(response) => response,
    }
}

#[test]
fn dispatch_returns_outcomes_and_publishes_actor_events() {
    let factory = factory();
    let bus = EventBus::default();
    let events = bus.subscribe();
    let create = AppCommand::CreateCounter(CreateCounter {
        name: "visits".into(),
    });
    assert_eq!(create.name(), "CreateCounter");
    assert_eq!(
        execute(&factory, &bus, "creator".into(), create).unwrap(),
        CommandResponse::Created
    );
    assert!(matches!(
        events.try_recv().unwrap(),
        Envelope {
            actor,
            event: AppEvent::Domain(DomainEvent::Counter(CounterEvent::Created { name })),
        } if actor == "creator" && name == "visits"
    ));

    assert_eq!(
        execute(
            &factory,
            &bus,
            "visitor".into(),
            AppCommand::IncrementCounter(IncrementCounter {
                name: "visits".into()
            }),
        )
        .unwrap(),
        CommandResponse::Ok
    );
    assert!(matches!(
        events.try_recv().unwrap(),
        Envelope {
            actor,
            event: AppEvent::Domain(DomainEvent::Counter(CounterEvent::Incremented { name })),
        } if actor == "visitor" && name == "visits"
    ));
    match list(&factory) {
        ListCountersResponse::Found(counters) => {
            assert_eq!(counters.len(), 1);
            assert_eq!(counters[0].name, "visits");
            assert_eq!(counters[0].counter, 1);
        }
        ListCountersResponse::Empty => panic!("the committed counter should be listed"),
    }

    assert_eq!(
        execute(
            &factory,
            &bus,
            "creator".into(),
            AppCommand::DeleteCounter(DeleteCounter {
                name: "visits".into()
            }),
        )
        .unwrap(),
        CommandResponse::Ok
    );
    assert!(matches!(list(&factory), ListCountersResponse::Empty));
    assert!(matches!(
        events.try_recv(),
        Err(crossbeam_channel::TryRecvError::Empty)
    ));
}

#[test]
fn rejected_commands_publish_no_events() {
    let factory = factory();
    let bus = EventBus::default();
    let events = bus.subscribe();
    assert!(matches!(
        execute(
            &factory,
            &bus,
            "demo".into(),
            AppCommand::CreateCounter(CreateCounter {
                name: String::new()
            }),
        ),
        Err(AppError::Domain(DomainError::Counter(_)))
    ));
    assert!(matches!(list(&factory), ListCountersResponse::Empty));
    assert!(matches!(
        events.try_recv(),
        Err(crossbeam_channel::TryRecvError::Empty)
    ));

    execute(
        &factory,
        &bus,
        "demo".into(),
        AppCommand::CreateCounter(CreateCounter {
            name: "visits".into(),
        }),
    )
    .unwrap();
    events.try_recv().unwrap();
    assert!(matches!(
        execute(
            &factory,
            &bus,
            "demo".into(),
            AppCommand::CreateCounter(CreateCounter {
                name: "visits".into()
            }),
        ),
        Err(AppError::AlreadyExists(_))
    ));
    assert!(matches!(
        events.try_recv(),
        Err(crossbeam_channel::TryRecvError::Empty)
    ));
}

/// Inject a commit failure while retaining the real registry and pending actions.
struct RejectCommit(Uow);

impl UnitOfWork for RejectCommit {
    type Error = RepositoryError;
    type Registry = Registry;

    fn registry(&self) -> &Registry {
        self.0.registry()
    }

    fn register_action(
        &self,
        action: Box<dyn AbstractActionableSnapshot<Error = RepositoryError>>,
    ) {
        self.0.register_action(action);
    }

    fn commit_and_rollback(&self) -> Result<(), RepositoryError> {
        Err(RepositoryError::ImplementationError(anyhow::anyhow!(
            "commit failed"
        )))
    }
}

#[test]
fn failed_commit_does_not_publish_events() {
    let factory = factory();
    let bus = EventBus::default();
    let events = bus.subscribe();
    let result = CreateCounter {
        name: "visits".into(),
    }
    .handle(CommandContext {
        uow: Box::new(RejectCommit(factory.begin())),
        bus: bus.sender(),
        actor: "demo".into(),
    });
    assert!(matches!(result, Err(AppError::Repository(_))));
    assert!(matches!(list(&factory), ListCountersResponse::Empty));
    assert!(matches!(
        events.try_recv(),
        Err(crossbeam_channel::TryRecvError::Empty)
    ));
}

#[test]
fn publication_failure_is_reported_after_the_data_is_committed() {
    let factory = factory();
    let bus = EventBus::default();
    let sender = bus.sender();
    drop(bus);
    let result = CreateCounter {
        name: "visits".into(),
    }
    .handle(CommandContext {
        uow: factory.begin(),
        bus: sender,
        actor: "demo".into(),
    });
    assert!(matches!(result, Err(AppError::BusClosed)));
    assert!(matches!(list(&factory), ListCountersResponse::Found(_)));
}

#[test]
fn unique_lookup_distinguishes_missing_unique_and_multiple_matches() {
    let factory = factory();
    let bus = EventBus::default();
    let mut reader = factory.begin().registry().read_counters();
    assert!(matches!(
        reader.get_unique(HashSet::new()),
        Err(RepositoryError::NotFound)
    ));

    for name in ["visits", "orders"] {
        execute(
            &factory,
            &bus,
            "demo".into(),
            AppCommand::CreateCounter(CreateCounter { name: name.into() }),
        )
        .unwrap();
    }

    let counter = reader
        .get_unique(HashSet::from([CounterFilter::ByName("visits".into())]))
        .unwrap();
    assert_eq!(counter.snapshot.name, "visits");
    assert_eq!(counter.snapshot.counter, 0);
    assert_eq!(counter.version, 1);
    assert!(matches!(
        reader.get_unique(HashSet::new()),
        Err(RepositoryError::NotUnique)
    ));
}

#[test]
fn unique_lookup_preserves_storage_failures() {
    let factory = factory();
    let mut reader = factory.begin().registry().read_counters();
    drop(factory);

    assert!(matches!(
        reader.get_unique(HashSet::new()),
        Err(RepositoryError::ImplementationError(_))
    ));
}

#[test]
fn commands_report_missing_snapshots_through_repository_error() {
    let factory = factory();
    let bus = EventBus::default();
    let events = bus.subscribe();
    for command in [
        AppCommand::IncrementCounter(IncrementCounter {
            name: "missing".into(),
        }),
        AppCommand::DeleteCounter(DeleteCounter {
            name: "missing".into(),
        }),
    ] {
        assert!(matches!(
            execute(&factory, &bus, "demo".into(), command),
            Err(AppError::Repository(RepositoryError::NotFound))
        ));
    }
    assert!(matches!(
        events.try_recv(),
        Err(crossbeam_channel::TryRecvError::Empty)
    ));
}

#[test]
fn existence_checks_distinguish_missing_matches_and_storage_failures() {
    let factory = factory();
    let bus = EventBus::default();
    let mut reader = factory.begin().registry().read_counters();
    assert!(!reader.exists(HashSet::new()).unwrap());

    execute(
        &factory,
        &bus,
        "demo".into(),
        AppCommand::CreateCounter(CreateCounter {
            name: "visits".into(),
        }),
    )
    .unwrap();
    assert!(
        reader
            .exists(HashSet::from([CounterFilter::ByName("visits".into())]))
            .unwrap()
    );
    assert!(
        !reader
            .exists(HashSet::from([CounterFilter::ByName("missing".into())]))
            .unwrap()
    );

    drop(factory);
    assert!(matches!(
        reader.exists(HashSet::new()),
        Err(RepositoryError::ImplementationError(_))
    ));
}
