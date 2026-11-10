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
    #[emits = E]
    #[emits = E]
    #[fails = F]
    A
);

fn main() {}
