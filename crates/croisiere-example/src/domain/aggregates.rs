use croisiere::domain::{aggregate_functions, make_aggregate};

use crate::domain::{
  entities::{Counter, CounterGroup},
  value_objects::CounterData,
};

make_aggregate!(
    CounterGroup {
        counters: Vec<Counter {
            data: CounterData
        }>
    }
);

make_aggregate!(Counter { data: CounterData });

pub enum CounterEvent {
  Incremented { name: String },
}

pub enum CounterError {
  CannotDecrement,
}

#[aggregate_functions(CounterEvent, CounterError)]
impl CounterAggregate {
  pub fn increment(&mut self) -> DomainResult {
    self.data.root.counter += 1;
    Ok(vec![CounterEvent::Incremented {
      name: self.root.name.clone(),
    }])
  }

  pub fn decrement(&mut self) -> DomainResult {
    if self.data.root.counter == 0 {
      return Err(CounterError::CannotDecrement);
    }
    self.data.root.counter -= 1;
    Ok(vec![])
  }
}
