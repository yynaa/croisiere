pub mod domain;

fn main() {}

#[cfg(test)]
mod tests {
    use croisiere::domain::{Aggregate, Entity, ValueObject};

    use crate::domain::{
        aggregates::{CounterAggregate, CounterGroupAggregate},
        entities::{Counter, CounterGroup},
        value_objects::CounterData,
    };

    #[test]
    fn value_object_name() {
        assert_eq!(CounterData::NAME, "CounterData");
    }

    #[test]
    fn entity_name() {
        assert_eq!(Counter::NAME, "Counter");
        assert_eq!(CounterGroup::NAME, "CounterGroup");
    }

    #[test]
    fn aggregate_name() {
        assert_eq!(CounterAggregate::NAME, "CounterAggregate");
        assert_eq!(CounterGroupAggregate::NAME, "CounterGroupAggregate");
    }

    mod custom_name {
        use croisiere::domain::{make_aggregate, Aggregate};

        use crate::domain::{entities::Counter, value_objects::CounterData};

        make_aggregate!(Counter as Tally { data: CounterData });

        #[test]
        fn aggregate_custom_name() {
            assert_eq!(Tally::NAME, "Tally");
            // Nested wrappers are prefixed with the custom name.
            let tally = Tally::default();
            let _: &TallyCounterData = &tally.data;
        }
    }
}
