# Domain: Services

A domain service holds business logic that does not belong to a single
[aggregate](./aggregates.md). A rule involving two accounts, for example, may belong in a transfer
service rather than in either account alone.

Domain services are stateless: they work on the aggregates and values supplied to them. The
application loads that data, calls the service and coordinates saving the results. Repository
access, transactions and event publication remain application and infrastructure responsibilities.

In `croisiere`, domain services are described by the [`DomainService`] trait.

## Declaring a domain service

Derive [`DomainService`] alongside its supertraits, and specify the event and error types:

```rust,ignore
use croisiere::domain::DomainService;

#[derive(Debug, Default, DomainService)]
#[service(event = AccountEvent, error = AccountError)]
pub struct TransferService;
```

Both `event` and `error` are required. They become `DomainService::Event` and
`DomainService::Error`. A service can reuse aggregate event and error types or define its own.
Include any new domain event and error types in the public `DomainEvent` and `DomainError` enums.

The derive implements only `DomainService`. You must also implement `Debug` and `Default`.
Its `NAME` constant is the type's identifier, without generic parameters:

```rust,ignore
assert_eq!(TransferService::NAME, "TransferService");
```

## Service functions

You can mark an inherent impl block with [`#[domain_service_functions]`][service-functions-macro]:

```rust,ignore
use croisiere::domain::domain_service_functions;

#[domain_service_functions]
impl TransferService {
    pub fn check_distinct_accounts(
        &self,
        from: &AccountAggregate,
        to: &AccountAggregate,
    ) -> Result<(), AccountError> {
        if from.root.id == to.root.id {
            return Err(AccountError::SameAccount);
        }
        Ok(())
    }
}
```

Here, `AccountError::SameAccount` represents the domain rule that a transfer requires distinct
accounts. The service checks a relationship between two aggregates without accessing storage or
changing either account.

The attribute wraps the impl in a module named after its type in snake case: `TransferService`
produces `transfer_service`. The module imports its parent's items, so the impl can still refer to
types declared alongside the service. Methods called from outside that module must be `pub`.

Unlike [`#[aggregate_functions]`](./aggregates.md#aggregate-functions), this attribute generates
no result aliases. Write return types explicitly, such as `Result<(), AccountError>` for a check
or `Result<Vec<AccountEvent>, AccountError>` for an operation returning events. It takes no arguments
and does not generate validation, persistence or transaction handling.

## Keeping responsibilities clear

> [!TIP]
> **Use a domain service for a business rule that has no single aggregate owner.** Keep rules
> about one aggregate in that aggregate, and rules about individual values in value objects.
> Loading snapshots, calling domain operations and committing changes are application work;
> coordinating those steps alone does not call for a domain service.

A service that changes several aggregates should use their operations to preserve their individual
rules. Validate conditions that could reject the overall operation before making changes where
possible. Returning `Err` does not automatically undo mutations already made to the aggregates.
The application must also arrange the appropriate persistence transaction; the domain service
macro supplies neither in-memory rollback nor a database transaction.

[`DomainService`]: https://docs.rs/croisiere/latest/croisiere/domain/trait.DomainService.html
[service-functions-macro]: https://docs.rs/croisiere/latest/croisiere/domain/attr.domain_service_functions.html
