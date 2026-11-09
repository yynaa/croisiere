//! `#[aggregate_functions]`.

use proc_macro2::TokenStream;
use quote::quote;
use syn::ItemImpl;

use crate::functions_module::wrap_in_module;

/// `attr` is the attribute's argument list, which must be empty: the event and
/// error types come from the self type's `Aggregate` impl.
pub fn expand(attr: TokenStream, item: ItemImpl) -> syn::Result<TokenStream> {
    if !attr.is_empty() {
        return Err(syn::Error::new_spanned(
            attr,
            "aggregate_functions takes no arguments; event and error types come from \
             `#[emits]`/`#[fails]` in make_aggregate!",
        ));
    }

    wrap_in_module("aggregate_functions", &item, |self_name| {
        let aggregate = quote!(<#self_name as ::croisiere::domain::Aggregate>);

        quote! {
            /// General result of an aggregate function: a `T`, or the
            /// aggregate's error.
            #[allow(dead_code)]
            type AggregateResult<T> = ::std::result::Result<T, #aggregate::Error>;

            /// Result of a check that produces nothing on success, or the
            /// error that made it fail.
            #[allow(dead_code)]
            type CheckResult = AggregateResult<()>;

            /// Result of an action on an existing aggregate: the events it
            /// emitted, or the error that prevented it.
            #[allow(dead_code)]
            type ActionResult = AggregateResult<::std::vec::Vec<#aggregate::Event>>;

            /// Result of creating an aggregate: the new aggregate with the
            /// events its creation emitted, or the error that prevented it.
            #[allow(dead_code)]
            type CreationResult =
                AggregateResult<(#self_name, ::std::vec::Vec<#aggregate::Event>)>;
        }
    })
}
