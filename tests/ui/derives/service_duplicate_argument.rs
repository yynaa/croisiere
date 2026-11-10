use croisiere::domain::DomainService;

pub struct E;
pub struct F;

#[derive(Debug, Default, DomainService)]
#[service(event = E, event = E, error = F)]
pub struct Transfer;

fn main() {}
