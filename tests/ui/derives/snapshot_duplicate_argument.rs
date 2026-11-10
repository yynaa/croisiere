use croisiere::application::Snapshot;

#[derive(Debug, Default)]
pub struct Order;
pub struct E;
pub struct F;

croisiere::domain::make_aggregate!(
    #[emits = E]
    #[fails = F]
    Order
);

impl From<OrderAggregate> for OrderSnapshot {
    fn from(_: OrderAggregate) -> Self {
        Self
    }
}

impl From<OrderSnapshot> for OrderAggregate {
    fn from(_: OrderSnapshot) -> Self {
        Self::default()
    }
}

#[derive(Debug, Clone, Snapshot)]
#[snapshot(aggregate = OrderAggregate, aggregate = OrderAggregate)]
pub struct OrderSnapshot;

fn main() {}
