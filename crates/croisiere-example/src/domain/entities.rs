use croisiere::domain::Entity;

#[derive(Debug, Default, Entity)]
pub struct Counter {
	pub name: String
}

#[derive(Debug, Default, Entity)]
pub struct CounterGroup {
	pub name: String
}

