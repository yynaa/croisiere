//! What `#[aggregate_functions]` and `#[domain_service_functions]` generate:
//! a snake-cased module around the impl block, and for aggregates the result
//! aliases.

use croisiere::domain::{DomainService, aggregate_functions, domain_service_functions};

// --- #[aggregate_functions] --------------------------------------------------

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Cart {
    pub items: u32,
}

#[derive(Debug, PartialEq, Eq)]
pub enum CartEvent {
    Created,
    ItemAdded,
}

#[derive(Debug, PartialEq, Eq)]
pub enum CartError {
    Full,
}

croisiere::domain::make_aggregate!(
    #[alias = ShoppingCart]
    #[emits = CartEvent]
    #[fails = CartError]
    Cart
);

const MAX_ITEMS: u32 = 2;

#[aggregate_functions]
impl ShoppingCartAggregate {
    pub fn create() -> CreationResult {
        Ok((Self::default(), vec![CartEvent::Created]))
    }

    pub fn check_not_full(&self) -> CheckResult {
        if self.root.items >= MAX_ITEMS {
            return Err(CartError::Full);
        }
        Ok(())
    }

    pub fn add_item(&mut self) -> ActionResult {
        self.check_not_full()?;
        self.root.items += 1;
        Ok(vec![CartEvent::ItemAdded])
    }

    pub fn items(&self) -> AggregateResult<u32> {
        Ok(self.root.items)
    }
}

/// Compiles only if the module is named after the self type in snake case.
#[allow(unused_imports)]
use shopping_cart_aggregate as _;

#[test]
fn creation_result_is_the_aggregate_with_its_events() {
    let result: Result<(ShoppingCartAggregate, Vec<CartEvent>), CartError> =
        ShoppingCartAggregate::create();

    let (cart, events) = result.unwrap();
    assert_eq!(cart.root, Cart::default());
    assert_eq!(events, vec![CartEvent::Created]);
}

#[test]
fn action_result_is_the_events_or_the_error() {
    let mut cart = ShoppingCartAggregate::default();

    let result: Result<Vec<CartEvent>, CartError> = cart.add_item();
    assert_eq!(result, Ok(vec![CartEvent::ItemAdded]));

    cart.add_item().unwrap();
    assert_eq!(cart.add_item(), Err(CartError::Full));
    assert_eq!(cart.root.items, MAX_ITEMS);
}

#[test]
fn check_result_is_unit_or_the_error() {
    let cart = ShoppingCartAggregate::default();

    let result: Result<(), CartError> = cart.check_not_full();
    assert_eq!(result, Ok(()));
}

#[test]
fn aggregate_result_is_any_value_or_the_error() {
    let result: Result<u32, CartError> = ShoppingCartAggregate::default().items();
    assert_eq!(result, Ok(0));
}

/// The module sees the items of its parent through `use super::*`.
mod nested_scope {
    #[derive(Debug, Default)]
    pub struct Thing;

    pub struct ThingEvent;
    pub struct ThingError;

    croisiere::domain::make_aggregate!(
        #[emits = ThingEvent]
        #[fails = ThingError]
        Thing
    );

    fn helper() -> u32 {
        42
    }

    #[croisiere::domain::aggregate_functions]
    impl ThingAggregate {
        pub fn answer(&self) -> AggregateResult<u32> {
            Ok(helper())
        }
    }

    #[test]
    fn functions_can_use_private_items_of_the_parent_module() {
        assert_eq!(ThingAggregate::default().answer().ok(), Some(42));
    }
}

// --- #[domain_service_functions] ---------------------------------------------

#[derive(Debug, PartialEq, Eq)]
pub struct TransferEvent(u32);

#[derive(Debug, PartialEq, Eq)]
pub struct TransferError;

#[derive(Debug, Default, DomainService)]
#[service(event = TransferEvent, error = TransferError)]
pub struct MoneyTransferService;

#[domain_service_functions]
impl MoneyTransferService {
    pub fn transfer(
        &self,
        from: &mut ShoppingCartAggregate,
        to: &mut ShoppingCartAggregate,
    ) -> Result<Vec<TransferEvent>, TransferError> {
        if from.root.items == 0 {
            return Err(TransferError);
        }
        from.root.items -= 1;
        to.add_item().map_err(|_| TransferError)?;
        Ok(vec![TransferEvent(1)])
    }
}

/// Compiles only if the module is named after the self type in snake case.
#[allow(unused_imports)]
use money_transfer_service as _;

#[test]
fn domain_service_functions_keep_the_impl_usable() {
    let mut from = ShoppingCartAggregate::default();
    from.add_item().unwrap();
    let mut to = ShoppingCartAggregate::default();

    let service = MoneyTransferService;
    assert_eq!(service.transfer(&mut from, &mut to), Ok(vec![TransferEvent(1)]));
    assert_eq!((from.root.items, to.root.items), (0, 1));
    assert_eq!(service.transfer(&mut from, &mut to), Err(TransferError));
}
