# croisière

an *opinionated* domain-driven framework

## usage

### domain

#### value objects

create value objects. they cannot contain other value objects.
```rs
//! domain/value_objects.rs
#[derive(Debug, Default, Clone, PartialEq, Eq, Hash)]
pub struct CounterData {
	pub counter: u64
}
```

#### entities

create entities. they have a lifecycle, and cannot contain other entities and value objects.  
```rs
//! domain/entities.rs
#[derive(Debug, Default)]
pub struct Counter {
	pub name: String
}

#[derive(Debug, Default)]
pub struct CounterGroup {
	pub name: String
}
```

#### aggregates

create aggregates. they contain links between entities and value objects.
```rs
//! domain/aggregates.rs
make_aggregate!(
    CounterGroup {
        counters: Vec<Counter {
            data: CounterData
        }>
    }
);

make_aggregate!(
	Counter {
		data: CounterData
	}
);
```

associate functions. they return a vector of custom events, or a custom error.
```rs
//! domain/aggregates.rs
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
```

## faq

### why `croisière`?

the *titan*ic sank so that the croisière could sail!
