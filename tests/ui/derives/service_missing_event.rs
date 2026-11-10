use croisiere::domain::DomainService;

pub struct E;
pub struct F;

#[derive(Debug, Default, DomainService)]
#[service(error = F)]
pub struct Transfer;

fn main() {}
