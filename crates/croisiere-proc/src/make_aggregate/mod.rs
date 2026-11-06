//! `make_aggregate!(...)`.
//!
//! - `parse`: turns the DSL into a tree of [`parse::NodeDef`]s.
//! - `generate`: walks that tree and emits one wrapper struct per node.

mod generate;
mod parse;

use proc_macro2::TokenStream;

pub use parse::AggregateInput;

pub fn expand(input: AggregateInput) -> TokenStream {
    generate::Generator::default().generate(input.roots)
}
