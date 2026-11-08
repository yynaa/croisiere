use croisiere::domain::ValueObject;

#[derive(Debug, Default, Clone, PartialEq, Eq, Hash, ValueObject)]
pub struct CounterData {
    pub counter: u64,
}
