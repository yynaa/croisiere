//! Proc macros for the `croisiere` crate.
//!
//! Each macro lives in its own module; this file only holds the entry points
//! (which the compiler requires to be at the crate root) and their docs.
//!
//! - [`make_aggregate!`](macro@make_aggregate) — `make_aggregate/`
//! - [`#[aggregate_functions]`](macro@aggregate_functions) — `aggregate_functions.rs`
//! - [`#[derive(ValueObject)]`](derive@ValueObject) — `value_object.rs`
//! - [`#[derive(Entity)]`](derive@Entity) — `entity.rs`
//!
//! `named.rs` holds the expansion shared by the two derives.

mod aggregate_functions;
mod entity;
mod make_aggregate;
mod named;
mod value_object;

use proc_macro::TokenStream;
use syn::parse_macro_input;

/// Wraps user-defined domain types (entities / value objects) into aggregate
/// view structs.
///
/// # Example
///
/// ```ignore
/// make_aggregate!(
///     A {
///         b: B,
///         c: C {
///             d: D,
///         },
///     },
/// );
/// ```
///
/// generates (naming scheme: `<root>Aggregate<path>`, see below to override):
///
/// ```ignore
/// pub struct AAggregate {
///     pub root: A,
///     pub b: AAggregateB,
///     pub c: AAggregateC,
/// }
///
/// pub struct AAggregateB {
///     pub root: B,
/// }
///
/// pub struct AAggregateC {
///     pub root: C,
///     pub d: AAggregateCD,
/// }
///
/// pub struct AAggregateCD {
///     pub root: D,
/// }
///
/// impl croisiere::domain::Aggregate for AAggregate {
///     const NAME: &'static str = "AAggregate";
/// }
/// ```
///
/// Only the top-level wrapper implements `Aggregate`; nested wrappers are
/// views inside it.
///
/// # Custom aggregate name
///
/// `Root as Name` names the aggregate `Name` instead of `<root>Aggregate`.
/// Nested wrappers then use it as their prefix, and `NAME` follows it:
///
/// ```ignore
/// make_aggregate!(Counter as Tally { data: CounterData });
/// // pub struct Tally { pub root: Counter, pub data: TallyCounterData }
/// // pub struct TallyCounterData { pub root: CounterData }
/// // impl Aggregate for Tally { const NAME: &'static str = "Tally"; }
/// ```
///
/// Every comma is optional. Field types may also be `Vec<X>` or `HashSet<X>`,
/// in which case the generated field type wraps the child aggregate
/// accordingly.
#[proc_macro]
pub fn make_aggregate(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as make_aggregate::AggregateInput);
    make_aggregate::expand(input).into()
}

/// Wraps the impl block in a snake-cased module derived from the self type,
/// giving it a convenient scope for aggregate-related helpers.
///
/// # Example
///
/// ```ignore
/// #[aggregate_functions(CounterAggregateEvent, CounterAggregateError)]
/// impl CounterAggregate {
///     fn events(&self) -> DomainResult {
///         Ok(Vec::new())
///     }
/// }
/// ```
///
/// becomes:
///
/// ```ignore
/// mod counter_aggregate {
///     use super::CounterAggregate;
///     use super::*;
///
///     type DomainResult = Result<Vec<CounterAggregateEvent>, CounterAggregateError>;
///
///     impl CounterAggregate {
///         fn events(&self) -> DomainResult {
///             Ok(Vec::new())
///         }
///     }
/// }
/// ```
#[proc_macro_attribute]
pub fn aggregate_functions(attr: TokenStream, item: TokenStream) -> TokenStream {
    let args = parse_macro_input!(attr as aggregate_functions::FunctionArgs);
    let item = parse_macro_input!(item as syn::ItemImpl);
    aggregate_functions::expand(args, item)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// Implements `croisiere::domain::ValueObject` for the annotated type, setting
/// `NAME` to the type's identifier.
///
/// The supertraits (`Debug`, `Clone`, `PartialEq`, `Eq`, `Default`, `Hash`)
/// still have to be derived alongside it.
///
/// # Example
///
/// ```ignore
/// #[derive(Debug, Default, Clone, PartialEq, Eq, Hash, ValueObject)]
/// pub struct CounterData {
///     pub counter: u64,
/// }
///
/// assert_eq!(CounterData::NAME, "CounterData");
/// ```
#[proc_macro_derive(ValueObject)]
pub fn derive_value_object(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as syn::DeriveInput);
    value_object::expand(input).into()
}

/// Implements `croisiere::domain::Entity` for the annotated type, setting
/// `NAME` to the type's identifier.
///
/// The supertraits (`Debug`, `Default`) still have to be derived alongside it.
///
/// # Example
///
/// ```ignore
/// #[derive(Debug, Default, Entity)]
/// pub struct Counter {
///     pub name: String,
/// }
///
/// assert_eq!(Counter::NAME, "Counter");
/// ```
#[proc_macro_derive(Entity)]
pub fn derive_entity(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as syn::DeriveInput);
    entity::expand(input).into()
}
