use croisiere::domain::{aggregate_functions, make_aggregate};

use crate::domain::{entities::Counter, value_objects::CounterData};

make_aggregate!(Counter { data: CounterData } emits CounterEvent fails CounterError);

#[derive(Debug)]
pub enum CounterEvent {
    Created { name: String },
    Incremented { name: String },
}

#[derive(Debug)]
pub enum CounterError {
    EmptyName,
    CannotDecrement,
}

#[aggregate_functions]
impl CounterAggregate {
    /// Creates a counter starting at 0.
    pub fn create(name: String) -> CreationResult {
        if name.is_empty() {
            return Err(CounterError::EmptyName);
        }
        let events = vec![CounterEvent::Created { name: name.clone() }];
        let aggregate = Self {
            root: Counter { name },
            ..Default::default()
        };
        Ok((aggregate, events))
    }

    pub fn increment(&mut self) -> ActionResult {
        self.data.root.counter += 1;
        Ok(vec![CounterEvent::Incremented {
            name: self.root.name.clone(),
        }])
    }

    pub fn decrement(&mut self) -> ActionResult {
        if self.data.root.counter == 0 {
            return Err(CounterError::CannotDecrement);
        }
        self.data.root.counter -= 1;
        Ok(vec![])
    }
}
