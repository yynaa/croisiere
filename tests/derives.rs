//! What the derives generate: `Entity`, `ValueObject`, `DomainService`,
//! `Snapshot`, `Filter` and `Sorter`.

use std::fmt::Debug;

use croisiere::{
    application::{EmptyFilter, EmptySorter, Filter, Snapshot, Sorter},
    domain::{DomainService, Entity, ValueObject},
};

// --- Entity and ValueObject --------------------------------------------------

#[derive(Debug, Default, Entity)]
pub struct Customer {
    pub name: String,
}

#[derive(Debug, Default, Entity)]
pub struct Wrapper<T: Debug + Default> {
    pub inner: T,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Hash, ValueObject)]
pub struct Money {
    pub cents: u64,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Hash, ValueObject)]
pub enum Currency {
    #[default]
    Euro,
}

#[test]
fn entity_name_is_the_type_name() {
    assert_eq!(Customer::NAME, "Customer");
}

#[test]
fn generic_entity_name_has_no_parameters() {
    assert_eq!(<Wrapper<u8> as Entity>::NAME, "Wrapper");
    assert_eq!(<Wrapper<String> as Entity>::NAME, "Wrapper");
}

#[test]
fn value_object_name_is_the_type_name() {
    assert_eq!(Money::NAME, "Money");
    assert_eq!(Currency::NAME, "Currency");
}

// --- DomainService -----------------------------------------------------------

pub struct PricingEvent;
pub struct PricingError;

#[derive(Debug, Default, DomainService)]
#[service(event = PricingEvent, error = PricingError)]
pub struct PricingService;

// Arguments in the other order.
#[derive(Debug, Default, DomainService)]
#[service(error = PricingError, event = PricingEvent)]
pub struct ReversedService;

fn assert_service_types<S: DomainService<Event = E, Error = F>, E, F>() {}

#[test]
fn domain_service_name_and_types() {
    assert_eq!(PricingService::NAME, "PricingService");
    assert_service_types::<PricingService, PricingEvent, PricingError>();
}

#[test]
fn domain_service_arguments_can_come_in_any_order() {
    assert_service_types::<ReversedService, PricingEvent, PricingError>();
}

// --- Snapshot, Filter and Sorter ---------------------------------------------

#[derive(Debug, Default)]
pub struct Order;

pub struct OrderEvent;
pub struct OrderError;

croisiere::domain::make_aggregate!(
    #[emits = OrderEvent]
    #[fails = OrderError]
    Order
);

#[derive(Hash, PartialEq, Eq, Filter)]
pub enum OrderFilter {
    ById(u32),
}

#[derive(Sorter)]
pub enum OrderSorter {
    ById,
}

fn assert_filter<F: Filter>() {}
fn assert_sorter<S: Sorter>() {}

fn assert_snapshot_types<S, A, F, O>()
where
    S: Snapshot<Aggregate = A, Filter = F, Sorter = O>,
    A: croisiere::domain::Aggregate,
    F: Filter,
    O: Sorter,
{
}

/// Defines a snapshot of `OrderAggregate` with conversions both ways.
macro_rules! order_snapshot {
    ($(#[$attr:meta])* $name:ident) => {
        #[derive(Debug, Clone, Snapshot)]
        $(#[$attr])*
        pub struct $name;

        impl From<OrderAggregate> for $name {
            fn from(_: OrderAggregate) -> Self {
                Self
            }
        }

        impl From<$name> for OrderAggregate {
            fn from(_: $name) -> Self {
                Self::default()
            }
        }
    };
}

order_snapshot!(
    #[snapshot(aggregate = OrderAggregate, filter = OrderFilter, sorter = OrderSorter)]
    FullSnapshot
);
order_snapshot!(#[snapshot(aggregate = OrderAggregate)] BareSnapshot);
order_snapshot!(#[snapshot(aggregate = OrderAggregate, sorter = OrderSorter)] SortedSnapshot);
order_snapshot!(#[snapshot(sorter = OrderSorter, aggregate = OrderAggregate, filter = OrderFilter)] ReorderedSnapshot);

#[test]
fn filter_and_sorter_derives() {
    assert_filter::<OrderFilter>();
    assert_sorter::<OrderSorter>();
    assert_filter::<EmptyFilter>();
    assert_sorter::<EmptySorter>();
}

#[test]
fn snapshot_name_is_the_type_name() {
    assert_eq!(FullSnapshot::NAME, "FullSnapshot");
}

#[test]
fn snapshot_types_are_the_given_ones() {
    assert_snapshot_types::<FullSnapshot, OrderAggregate, OrderFilter, OrderSorter>();
    assert_snapshot_types::<ReorderedSnapshot, OrderAggregate, OrderFilter, OrderSorter>();
}

#[test]
fn snapshot_filter_and_sorter_default_to_empty() {
    assert_snapshot_types::<BareSnapshot, OrderAggregate, EmptyFilter, EmptySorter>();
    assert_snapshot_types::<SortedSnapshot, OrderAggregate, EmptyFilter, OrderSorter>();
}
