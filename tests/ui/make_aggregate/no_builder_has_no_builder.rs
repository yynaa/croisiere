use croisiere::domain::make_aggregate;

#[derive(Debug, Default)]
pub struct A;
#[derive(Debug, Default)]
pub struct B;
#[derive(Debug, Default)]
pub struct C;
pub struct E;
pub struct F;

make_aggregate!(
    #[no_builder]
    #[emits = E]
    #[fails = F]
    A { b: B { c: C } }
);

fn main() {
    let _ = AAggregate::builder();
    let _ = AAggregateBBuilder::default();
}
