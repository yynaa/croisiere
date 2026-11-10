use croisiere::domain::DomainService;

pub struct E;
pub struct F;

#[derive(Debug, Default, DomainService)]
#[service(event = E, error = F)]
pub struct Transfer;

#[croisiere::domain::domain_service_functions(event = E)]
impl Transfer {}

fn main() {}
