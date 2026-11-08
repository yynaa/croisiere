//! Proc macros for the `croisiere` crate.
//!
//! Each macro lives in its own module; this file only holds the entry points
//! (which the compiler requires to be at the crate root) and their docs.
//!
//! - [`make_aggregate!`](macro@make_aggregate) — `make_aggregate/`
//! - [`#[aggregate_functions]`](macro@aggregate_functions) — `aggregate_functions.rs`
//! - [`#[derive(ValueObject)]`](derive@ValueObject) — `value_object.rs`
//! - [`#[derive(Entity)]`](derive@Entity) — `entity.rs`
//! - [`#[derive(Filter)]`](derive@Filter) — `filter.rs`
//! - [`#[derive(Sorter)]`](derive@Sorter) — `sorter.rs`
//! - [`#[derive(Snapshot)]`](derive@Snapshot) — `snapshot.rs`
//!
//! `named.rs` holds the expansion shared by the `NAME` derives (`ValueObject`,
//! `Entity`); `marker.rs` the one shared by the marker derives (`Filter`,
//! `Sorter`).

mod aggregate_functions;
mod entity;
mod filter;
mod make_aggregate;
mod marker;
mod named;
mod snapshot;
mod sorter;
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
///     #[emits = AEvent]
///     #[fails = AError]
///     A {
///         b: B,
///         c: C {
///             d: D,
///         },
///     }
/// );
/// ```
///
/// generates (naming scheme: `<root>Aggregate<field path>`, with field names in
/// PascalCase; see below to override):
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
///     type Error = AError;
///     type Event = AEvent;
/// }
/// ```
///
/// Only the top-level wrapper implements `Aggregate`; nested wrappers are
/// views inside it.
///
/// # Events and errors
///
/// Every root must have `#[emits = EventType]` and `#[fails = ErrorType]`
/// attributes before it. They become `Aggregate::Event` and
/// `Aggregate::Error`, which `#[aggregate_functions]` uses for
/// `ActionResult` and `CreationResult`.
///
/// # Custom aggregate name
///
/// `#[alias = Name]` names the aggregate `NameAggregate` instead of
/// `<root>Aggregate`. Nested wrappers then use it as their prefix, and `NAME`
/// follows it. `#[alias(exact = Name)]` names the aggregate `Name` as written:
///
/// ```ignore
/// make_aggregate!(
///     #[alias = Tally]
///     #[emits = CounterEvent]
///     #[fails = CounterError]
///     Counter { data: CounterData }
/// );
/// // pub struct TallyAggregate { pub root: Counter, pub data: TallyAggregateData }
/// // pub struct TallyAggregateData { pub root: CounterData }
/// // impl Aggregate for TallyAggregate { const NAME: &'static str = "TallyAggregate"; ... }
/// ```
///
/// ```ignore
/// make_aggregate!(
///     #[alias(exact = Tally)]
///     #[emits = CounterEvent]
///     #[fails = CounterError]
///     Counter { data: CounterData }
/// );
/// // pub struct Tally { pub root: Counter, pub data: TallyData }
/// // pub struct TallyData { pub root: CounterData }
/// // impl Aggregate for Tally { const NAME: &'static str = "Tally"; ... }
/// ```
///
/// # Rootless nodes
///
/// `#[rootless]` on a field declares its nested wrapper without a `root`
/// field, for when there is no domain type to hold. Its fields are still
/// generated as usual:
///
/// ```ignore
/// make_aggregate!(
///     #[emits = AEvent]
///     #[fails = AError]
///     A { #[rootless] c: C { d: D } }
/// );
/// // pub struct AAggregate { pub root: A, pub c: AAggregateC }
/// // pub struct AAggregateC { pub d: AAggregateCD }
/// // pub struct AAggregateCD { pub root: D }
/// ```
///
/// Every comma is optional.
///
/// # Containers and collections
///
/// A field written as `Container<X>`, such as `Option<X>` or `Vec<X>`, holds
/// the child wrapper of `X` inside `Container`. Whether the field is a
/// single value or a collection is set with `#[collection]`:
///
/// ```ignore
/// make_aggregate!(
///     #[emits = AEvent]
///     #[fails = AError]
///     A {
///         #[collection] items: Vec<B { c: C }>,
///         maybe: Option<D>,
///     }
/// );
/// ```
///
/// - Without `#[collection]`, the field is a single value of type
///   `Container<X>`, set from `X` through `From`. `Option<X>` works this way.
/// - With `#[collection]`, the container must implement `FromIterator`
///   (`Vec`, `HashSet`, or any user type that does).
/// - `#[derive(Trait, ...)]` on a field derives those traits on its wrapper and
///   on the wrappers inside it, so the domain types must implement them too.
///   `Debug` and `Default` are always derived. A `HashSet` needs
///   `#[derive(PartialEq, Eq, Hash)]`; without it, elements need no extra
///   traits.
///
/// Containers can't be nested, and `#[rootless]` fields can't be collections.
///
/// # Builders
///
/// `<Aggregate>::builder()` returns a builder whose setters are named after
/// the fields. Anything not set keeps its default value, and `build()` never
/// fails:
///
/// - a field takes its domain value: `.data(data)`;
/// - a field with children also takes a closure that sets them:
///   `.c(c, |c| c.d(d))`;
/// - a `#[rootless]` field takes only that closure: `.meta(|m| m.code(code))`;
/// - a `#[collection]` field takes a closure that returns an iterator of
///   domain values, each converted to its wrapper with `Into`:
///   `.items(|| vec![b1, b2])`. The elements' own children keep their
///   defaults, so set them through field access afterwards.
///
/// An `Option<X>` field, as a single value, takes the value and, with children,
/// the closure: `.maybe(d)` or `.maybe(d, |d| d.e(e))`.
#[proc_macro]
pub fn make_aggregate(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as make_aggregate::AggregateInput);
    make_aggregate::expand(input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// Wraps the impl block in a snake-cased module derived from the self type,
/// giving it a convenient scope for aggregate-related helpers.
///
/// It defines two result aliases in that module, built from the event and
/// error types of the self type's `Aggregate` impl (set with `#[emits]`/`#[fails]`
/// in `make_aggregate!`), so the attribute takes no arguments:
///
/// - `ActionResult`: for actions on an existing aggregate, returning the
///   emitted events.
/// - `CreationResult`: for creating an aggregate, returning it with the
///   emitted events.
///
/// # Example
///
/// ```ignore
/// #[aggregate_functions]
/// impl CounterAggregate {
///     fn create() -> CreationResult {
///         Ok((Self::default(), Vec::new()))
///     }
///
///     fn events(&self) -> ActionResult {
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
///     type ActionResult = Result<
///         Vec<<CounterAggregate as Aggregate>::Event>,
///         <CounterAggregate as Aggregate>::Error,
///     >;
///
///     type CreationResult = Result<
///         (CounterAggregate, Vec<<CounterAggregate as Aggregate>::Event>),
///         <CounterAggregate as Aggregate>::Error,
///     >;
///
///     impl CounterAggregate {
///         fn create() -> CreationResult {
///             Ok((Self::default(), Vec::new()))
///         }
///
///         fn events(&self) -> ActionResult {
///             Ok(Vec::new())
///         }
///     }
/// }
/// ```
#[proc_macro_attribute]
pub fn aggregate_functions(attr: TokenStream, item: TokenStream) -> TokenStream {
    let item = parse_macro_input!(item as syn::ItemImpl);
    aggregate_functions::expand(attr.into(), item)
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

/// Implements the `croisiere::application::Filter` marker trait for
/// the annotated type.
///
/// The supertrait (`Hash`) still has to be derived alongside it.
///
/// # Example
///
/// ```ignore
/// #[derive(Hash, Filter)]
/// pub struct CounterFilter {
///     pub name: Option<String>,
/// }
/// ```
#[proc_macro_derive(Filter)]
pub fn derive_filter(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as syn::DeriveInput);
    filter::expand(input).into()
}

/// Implements the `croisiere::application::Sorter` marker trait for
/// the annotated type.
///
/// # Example
///
/// ```ignore
/// #[derive(Sorter)]
/// pub enum CounterSorter {
///     ByName,
///     ByCount,
/// }
/// ```
#[proc_macro_derive(Sorter)]
pub fn derive_sorter(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as syn::DeriveInput);
    sorter::expand(input).into()
}

/// Implements `croisiere::application::Snapshot` for the annotated
/// type, setting `NAME` to the type's identifier and the associated types from
/// the `#[snapshot(...)]` attribute.
///
/// - `aggregate = Type` (required): the `Aggregate` the snapshot is taken of.
/// - `filter = Type` (optional): defaults to `EmptyFilter`.
/// - `sorter = Type` (optional): defaults to `EmptySorter`.
///
/// The supertraits (`Debug`, `From<Aggregate>`, `TryInto<Aggregate>`) still
/// have to be provided by hand.
///
/// # Example
///
/// ```ignore
/// #[derive(Debug, Snapshot)]
/// #[snapshot(aggregate = CounterAggregate, filter = CounterFilter)]
/// pub struct CounterSnapshot {
///     pub name: String,
///     pub counter: u64,
/// }
///
/// // impl Snapshot for CounterSnapshot {
/// //     const NAME: &'static str = "CounterSnapshot";
/// //     type Aggregate = CounterAggregate;
/// //     type Filter = CounterFilter;
/// //     type Sorter = EmptySorter;
/// // }
/// ```
#[proc_macro_derive(Snapshot, attributes(snapshot))]
pub fn derive_snapshot(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as syn::DeriveInput);
    snapshot::expand(input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}
