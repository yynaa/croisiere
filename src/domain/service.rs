use std::fmt::Debug;

/// Domain service, as defined by DDD.
///
/// It holds domain logic that does not belong to a single aggregate, such as
/// an operation spanning several of them. It is stateless: it works on the
/// aggregates and values it is given.
///
/// Implement it with `#[derive(DomainService)]`, alongside the derives for its
/// supertraits, setting its event and error types with the `#[service(...)]`
/// attribute:
///
/// ```ignore
/// #[derive(Debug, Default, DomainService)]
/// #[service(event = TransferEvent, error = TransferError)]
/// pub struct TransferService;
/// ```
///
/// Its functions can be grouped in an impl block marked with
/// [`#[domain_service_functions]`](domain_service_functions).
pub trait DomainService: Debug + Default {
    /// Name of the service type, set to the type's identifier by the derive.
    const NAME: &'static str;
    /// Error returned by the service's functions, set with
    /// `#[service(error = ...)]`.
    type Error;
    /// Events emitted by the service's functions, set with
    /// `#[service(event = ...)]`.
    type Event;
}

pub use croisiere_proc::DomainService;
pub use croisiere_proc::domain_service_functions;
