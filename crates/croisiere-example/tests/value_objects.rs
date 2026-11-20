//! Name validation, snapshot restoration and domain error/event conversions.

// These source modules also expose operations outside this test's scope.
#[allow(dead_code)]
#[path = "../src/domain/mod.rs"]
mod domain;
#[allow(dead_code)]
#[path = "../src/application/snapshots.rs"]
mod snapshots;

use domain::{
    DomainError, DomainEvent,
    aggregates::{CounterAggregate, CounterError, CounterEvent},
    value_objects::{CounterName, CounterNameError},
};
use snapshots::CounterSnapshot;

#[test]
fn domain_event_preserves_the_aggregate_event() {
    let (_, events) = CounterAggregate::create("visits".to_owned()).unwrap();
    let mut events = events.into_iter().map(DomainEvent::from);
    assert!(matches!(
        events.next(),
        Some(DomainEvent::Counter(CounterEvent::Created { name })) if name == "visits"
    ));
    assert!(events.next().is_none());
}

#[test]
fn domain_error_preserves_specific_failures() {
    let name_error = DomainError::from(CounterNameError::EmptyName);
    assert_eq!(name_error.to_string(), "a counter name cannot be empty");
    assert!(matches!(
        name_error,
        DomainError::CounterName(CounterNameError::EmptyName)
    ));

    let operation_error = CounterAggregate::create(String::new()).unwrap_err();
    assert!(matches!(
        DomainError::from(operation_error),
        DomainError::Counter(CounterError::InvalidName(CounterNameError::EmptyName))
    ));
}

#[test]
fn names_reject_empty_strings_and_preserve_nonempty_strings() {
    assert_eq!(
        CounterName::try_from(String::new()),
        Err(CounterNameError::EmptyName)
    );
    let name = CounterName::try_from(" visits ".to_owned()).unwrap();
    assert_eq!(name.as_str(), " visits ");
    assert_eq!(name.into_inner(), " visits ");
    assert_eq!(CounterName::default().as_str(), "-");
}

#[test]
fn aggregate_creation_propagates_name_validation() {
    assert!(matches!(
        CounterAggregate::create(String::new()),
        Err(CounterError::InvalidName(CounterNameError::EmptyName))
    ));
    let (aggregate, _) = CounterAggregate::create("visits".to_owned()).unwrap();
    assert_eq!(aggregate.root.name.as_str(), "visits");
}

#[test]
fn snapshot_restoration_validates_names() {
    let error = CounterAggregate::try_from(CounterSnapshot {
        name: String::new(),
        counter: 3,
    })
    .unwrap_err();
    assert!(matches!(
        error,
        DomainError::CounterName(CounterNameError::EmptyName)
    ));

    let aggregate = CounterAggregate::try_from(CounterSnapshot {
        name: "visits".to_owned(),
        counter: 3,
    })
    .unwrap();
    let snapshot = CounterSnapshot::from(aggregate);
    assert_eq!(snapshot.name, "visits");
    assert_eq!(snapshot.counter, 3);
}
