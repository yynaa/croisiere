//! Runtime behavior of the persistence types: versioned snapshots, actionable
//! snapshots and how they reach their repository.

use std::{cell::RefCell, rc::Rc};

use croisiere::{
    application::{
        AbstractActionableSnapshot, RepositoryDropped, Snapshot, SnapshotAction, VersionedSnapshot,
        WriteRepository,
    },
    domain::{Entity, ValueObject, aggregate_functions, make_aggregate},
};

// --- Domain ------------------------------------------------------------------

#[derive(Debug, Default, Entity)]
pub struct Counter {
    pub name: String,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Hash, ValueObject)]
pub struct CounterData {
    pub counter: u64,
}

make_aggregate!(
    #[emits = CounterEvent]
    #[fails = CounterError]
    Counter { data: CounterData }
);

#[derive(Debug, PartialEq, Eq)]
pub enum CounterEvent {
    Incremented,
}

#[derive(Debug, PartialEq, Eq)]
pub enum CounterError {
    CannotDecrement,
}

#[aggregate_functions]
impl CounterAggregate {
    pub fn increment(&mut self) -> ActionResult {
        self.data.root.counter += 1;
        Ok(vec![CounterEvent::Incremented])
    }

    pub fn decrement(&mut self) -> ActionResult {
        if self.data.root.counter == 0 {
            return Err(CounterError::CannotDecrement);
        }
        self.data.root.counter -= 1;
        Ok(vec![])
    }
}

// --- Snapshot ----------------------------------------------------------------

/// Name a snapshot can't be turned back into an aggregate with.
const CORRUPT: &str = "corrupt";

#[derive(Debug, Clone, PartialEq, Eq, Snapshot)]
#[snapshot(aggregate = CounterAggregate)]
pub struct CounterSnapshot {
    pub name: String,
    pub counter: u64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct InvalidSnapshot;

impl From<CounterAggregate> for CounterSnapshot {
    fn from(value: CounterAggregate) -> Self {
        Self {
            name: value.root.name,
            counter: value.data.root.counter,
        }
    }
}

impl TryFrom<CounterSnapshot> for CounterAggregate {
    type Error = InvalidSnapshot;

    fn try_from(value: CounterSnapshot) -> Result<Self, Self::Error> {
        if value.name == CORRUPT {
            return Err(InvalidSnapshot);
        }
        Ok(CounterAggregate::builder()
            .root(Counter { name: value.name })
            .data(CounterData {
                counter: value.counter,
            })
            .build())
    }
}

fn snapshot(name: &str, counter: u64, version: u128) -> VersionedSnapshot<CounterSnapshot> {
    VersionedSnapshot {
        snapshot: CounterSnapshot {
            name: name.to_owned(),
            counter,
        },
        version,
    }
}

// --- Application error and repository ---------------------------------------

#[derive(Debug, PartialEq, Eq)]
pub enum AppError {
    Domain(CounterError),
    InvalidSnapshot,
    RepositoryDropped,
}

impl From<CounterError> for AppError {
    fn from(error: CounterError) -> Self {
        Self::Domain(error)
    }
}

impl From<InvalidSnapshot> for AppError {
    fn from(_: InvalidSnapshot) -> Self {
        Self::InvalidSnapshot
    }
}

impl From<RepositoryDropped> for AppError {
    fn from(_: RepositoryDropped) -> Self {
        Self::RepositoryDropped
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Call {
    Save(CounterSnapshot, u128),
    Delete(CounterSnapshot, u128),
}

/// Records every call it gets instead of storing anything.
#[derive(Default)]
struct RecordingRepository {
    calls: Vec<Call>,
}

impl WriteRepository<CounterSnapshot> for RecordingRepository {
    type Error = AppError;

    fn save(
        &mut self,
        snapshots: Vec<&VersionedSnapshot<CounterSnapshot>>,
    ) -> Result<(), AppError> {
        self.calls.extend(
            snapshots
                .into_iter()
                .map(|vs| Call::Save(vs.snapshot.clone(), vs.version)),
        );
        Ok(())
    }

    fn delete(
        &mut self,
        snapshots: Vec<&VersionedSnapshot<CounterSnapshot>>,
    ) -> Result<(), AppError> {
        self.calls.extend(
            snapshots
                .into_iter()
                .map(|vs| Call::Delete(vs.snapshot.clone(), vs.version)),
        );
        Ok(())
    }
}

fn repository() -> Rc<RefCell<RecordingRepository>> {
    Rc::new(RefCell::new(RecordingRepository::default()))
}

// --- VersionedSnapshot -------------------------------------------------------

#[test]
fn snapshot_name_is_the_type_name() {
    assert_eq!(CounterSnapshot::NAME, "CounterSnapshot");
}

#[test]
fn assemble_then_dismantle_round_trips() {
    let aggregate = CounterAggregate::builder()
        .root(Counter {
            name: "a".to_owned(),
        })
        .data(CounterData { counter: 3 })
        .build();

    let vs = VersionedSnapshot::<CounterSnapshot>::assemble(aggregate, 7);
    assert_eq!(vs.version, 7);
    assert_eq!(
        vs.snapshot,
        CounterSnapshot {
            name: "a".to_owned(),
            counter: 3
        }
    );

    let (aggregate, version) = vs.dismantle().unwrap();
    assert_eq!(version, 7);
    assert_eq!(aggregate.root.name, "a");
    assert_eq!(aggregate.data.root.counter, 3);
}

#[test]
fn dismantle_returns_the_conversion_error() {
    let result = snapshot(CORRUPT, 0, 1).dismantle();
    assert_eq!(result.unwrap_err(), InvalidSnapshot);
}

#[test]
fn do_as_aggregate_returns_updated_snapshot_and_events() {
    let (vs, events) = snapshot("a", 1, 4)
        .do_as_aggregate::<CounterEvent, AppError>(CounterAggregate::increment)
        .unwrap();

    assert_eq!(vs.snapshot.counter, 2);
    assert_eq!(vs.version, 4, "do_as_aggregate must not change the version");
    assert_eq!(events, vec![CounterEvent::Incremented]);
}

#[test]
fn do_as_aggregate_converts_events() {
    #[derive(Debug, PartialEq, Eq)]
    struct Wrapped(CounterEvent);

    impl From<CounterEvent> for Wrapped {
        fn from(event: CounterEvent) -> Self {
            Self(event)
        }
    }

    let (_, events) = snapshot("a", 0, 0)
        .do_as_aggregate::<Wrapped, AppError>(CounterAggregate::increment)
        .unwrap();

    assert_eq!(events, vec![Wrapped(CounterEvent::Incremented)]);
}

#[test]
fn do_as_aggregate_returns_the_aggregate_error() {
    let result =
        snapshot("a", 0, 0).do_as_aggregate::<CounterEvent, AppError>(CounterAggregate::decrement);

    assert_eq!(
        result.err(),
        Some(AppError::Domain(CounterError::CannotDecrement))
    );
}

#[test]
fn do_as_aggregate_returns_the_snapshot_error_without_running_f() {
    let mut called = false;
    let result = snapshot(CORRUPT, 0, 0).do_as_aggregate::<CounterEvent, AppError>(|a| {
        called = true;
        a.increment()
    });

    assert_eq!(result.err(), Some(AppError::InvalidSnapshot));
    assert!(!called);
}

// --- make_actionable / do_and_make_actionable --------------------------------

#[test]
fn make_actionable_increments_the_version() {
    let repo = repository();
    let actionable =
        snapshot("a", 0, 5).make_actionable(Rc::downgrade(&repo), SnapshotAction::Save);

    assert_eq!(actionable.snapshot.version, 6);
    assert!(matches!(actionable.action, SnapshotAction::Save));
    assert!(
        repo.borrow().calls.is_empty(),
        "nothing is saved before act()"
    );
}

#[test]
fn do_and_make_actionable_runs_f_then_increments_the_version() {
    let repo = repository();
    let (actionable, events) = snapshot("a", 1, 5)
        .do_and_make_actionable::<CounterEvent, AppError, _>(
            Rc::downgrade(&repo),
            SnapshotAction::Save,
            CounterAggregate::increment,
        )
        .unwrap();

    assert_eq!(actionable.snapshot.snapshot.counter, 2);
    assert_eq!(actionable.snapshot.version, 6);
    assert_eq!(events, vec![CounterEvent::Incremented]);
}

#[test]
fn do_and_make_actionable_returns_the_aggregate_error() {
    let repo = repository();
    let result = snapshot("a", 0, 5).do_and_make_actionable::<CounterEvent, AppError, _>(
        Rc::downgrade(&repo),
        SnapshotAction::Save,
        CounterAggregate::decrement,
    );

    // `ActionableSnapshot` isn't `Debug`, so `unwrap_err` isn't available.
    let Err(error) = result else {
        panic!("expected an error");
    };
    assert_eq!(error, AppError::Domain(CounterError::CannotDecrement));
    assert!(repo.borrow().calls.is_empty());
}

// --- AbstractActionableSnapshot ----------------------------------------------

#[test]
fn act_save_saves_the_snapshot() {
    let repo = repository();
    let mut actionable =
        snapshot("a", 1, 0).make_actionable(Rc::downgrade(&repo), SnapshotAction::Save);

    actionable.act().unwrap();

    assert_eq!(
        repo.borrow().calls,
        vec![Call::Save(
            CounterSnapshot {
                name: "a".to_owned(),
                counter: 1
            },
            1
        )]
    );
}

#[test]
fn act_delete_deletes_the_snapshot() {
    let repo = repository();
    let mut actionable =
        snapshot("a", 1, 0).make_actionable(Rc::downgrade(&repo), SnapshotAction::Delete);

    actionable.act().unwrap();

    assert_eq!(
        repo.borrow().calls,
        vec![Call::Delete(
            CounterSnapshot {
                name: "a".to_owned(),
                counter: 1
            },
            1
        )]
    );
}

#[test]
fn act_none_does_nothing() {
    let repo = repository();
    let mut actionable =
        snapshot("a", 1, 0).make_actionable(Rc::downgrade(&repo), SnapshotAction::None);

    actionable.act().unwrap();

    assert!(repo.borrow().calls.is_empty());
}

#[test]
fn get_name_is_the_snapshot_name() {
    let repo = repository();
    let actionable =
        snapshot("a", 0, 0).make_actionable(Rc::downgrade(&repo), SnapshotAction::None);

    assert_eq!(actionable.get_name(), "CounterSnapshot");
}

#[test]
fn acting_after_the_repository_is_dropped_fails() {
    for action in [SnapshotAction::Save, SnapshotAction::Delete] {
        let repo = repository();
        let mut actionable = snapshot("a", 0, 0).make_actionable(Rc::downgrade(&repo), action);
        drop(repo);

        assert_eq!(actionable.act(), Err(AppError::RepositoryDropped));
    }
}

#[test]
fn acting_with_none_after_the_repository_is_dropped_succeeds() {
    let repo = repository();
    let mut actionable =
        snapshot("a", 0, 0).make_actionable(Rc::downgrade(&repo), SnapshotAction::None);
    drop(repo);

    assert_eq!(actionable.act(), Ok(()));
}

#[test]
fn works_through_trait_objects() {
    // The repository behind a `dyn WriteRepository`, and the actionable
    // snapshot behind a `dyn AbstractActionableSnapshot`, as a unit of work
    // would hold them.
    let repo = repository();
    let dyn_repo: Rc<RefCell<dyn WriteRepository<CounterSnapshot, Error = AppError>>> =
        repo.clone();

    let mut pending: Vec<Box<dyn AbstractActionableSnapshot<Error = AppError>>> = vec![
        Box::new(
            snapshot("a", 1, 0).make_actionable(Rc::downgrade(&dyn_repo), SnapshotAction::Save),
        ),
        Box::new(
            snapshot("b", 2, 3).make_actionable(Rc::downgrade(&dyn_repo), SnapshotAction::Delete),
        ),
    ];

    for actionable in &mut pending {
        actionable.act().unwrap();
    }

    assert_eq!(
        repo.borrow().calls,
        vec![
            Call::Save(
                CounterSnapshot {
                    name: "a".to_owned(),
                    counter: 1
                },
                1
            ),
            Call::Delete(
                CounterSnapshot {
                    name: "b".to_owned(),
                    counter: 2
                },
                4
            ),
        ]
    );
}

// --- RepositoryDropped -------------------------------------------------------

#[test]
fn repository_dropped_display() {
    assert_eq!(
        RepositoryDropped.to_string(),
        "the repository of the actionable snapshot was dropped"
    );
}
