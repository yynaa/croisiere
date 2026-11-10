use croisiere::domain::DomainService;

pub struct E;
pub struct F;

#[derive(Debug, Default, DomainService)]
#[service(event = E, error = F, name = X)]
pub struct Transfer;

fn main() {}
