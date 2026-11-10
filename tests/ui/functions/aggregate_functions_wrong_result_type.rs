#[derive(Debug, Default)]
pub struct Counter;
pub struct E;
pub struct F;

croisiere::domain::make_aggregate!(
    #[emits = E]
    #[fails = F]
    Counter
);

#[croisiere::domain::aggregate_functions]
impl CounterAggregate {
    pub fn act(&mut self) -> ActionResult {
        Err(E)
    }
}

fn main() {}
