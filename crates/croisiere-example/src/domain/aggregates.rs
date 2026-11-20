//! Counter behavior, domain events and operation errors.
//!
//! `CounterAggregate` groups the `Counter` entity with its `CounterData` value
//! object. Creation accepts an unvalidated string and constructs a validated
//! `CounterName`; subsequent operations enforce rules that depend on the count.

use croisiere::domain::{aggregate_functions, make_aggregate};

use crate::domain::{
    entities::Counter,
    value_objects::{CounterData, CounterName, CounterNameError},
};

make_aggregate!(
    #[emits = CounterEvent]
    #[fails = CounterError]
    Counter { data: CounterData }
);

/// Facts emitted by successful counter operations.
#[derive(Debug, Clone)]
pub enum CounterEvent {
    /// A counter was created with a count of zero.
    Created {
        /// Domain name of the new counter.
        name: String,
    },
    /// A counter's count increased by one.
    Incremented {
        /// Domain name of the counter whose count increased.
        name: String,
    },
}

/// Failures reported by counter operations.
#[derive(Debug, thiserror::Error)]
pub enum CounterError {
    /// Name validation failed; preserve the value object's error.
    #[error(transparent)]
    InvalidName(#[from] CounterNameError),
    /// A zero count cannot be decreased.
    #[error("cannot decrement a counter below zero")]
    CannotDecrement,
}

#[aggregate_functions]
impl CounterAggregate {
    /// Creates a counter starting at 0.
    ///
    /// Validates the supplied name through `CounterName` and emits `Created`.
    /// An invalid name is returned as `CounterError::InvalidName`.
    pub fn create(name: String) -> CreationResult {
        let name = CounterName::try_from(name)?;
        let events = vec![CounterEvent::Created {
            name: name.as_str().to_owned(),
        }];
        let aggregate = Self {
            root: Counter { name },
            ..Default::default()
        };
        Ok((aggregate, events))
    }

    /// Increases the count by one and emits `Incremented`.
    pub fn increment(&mut self) -> ActionResult {
        self.data.root.counter += 1;
        Ok(vec![CounterEvent::Incremented {
            name: self.root.name.as_str().to_owned(),
        }])
    }

    /// Decreases a positive count by one without emitting an event.
    ///
    /// Returns `CannotDecrement` without changing the state if the count is zero.
    pub fn decrement(&mut self) -> ActionResult {
        if self.data.root.counter == 0 {
            return Err(CounterError::CannotDecrement);
        }
        self.data.root.counter -= 1;
        Ok(vec![])
    }
}
