//! `#[aggregate_functions]`.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{Ident, ItemImpl, Type};

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

    let self_name = self_type_name(&item)?;
    let mod_name = format_ident!("{}", to_snake_case(&self_name.to_string()));
    let aggregate = quote!(<#self_name as ::croisiere::domain::Aggregate>);

    Ok(quote! {
        mod #mod_name {
            use super::#self_name;
            use super::*;

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

            #item
        }
    })
}

/// The last path segment of the impl's self type, e.g. `CounterAggregate`.
fn self_type_name(item: &ItemImpl) -> syn::Result<Ident> {
    match &*item.self_ty {
        Type::Path(path) if path.qself.is_none() => path
            .path
            .segments
            .last()
            .map(|segment| segment.ident.clone())
            .ok_or_else(|| {
                syn::Error::new_spanned(
                    &item.self_ty,
                    "aggregate_functions expects a non-empty impl path",
                )
            }),
        _ => Err(syn::Error::new_spanned(
            &item.self_ty,
            "aggregate_functions only supports plain path types",
        )),
    }
}

/// `CounterAggregate` -> `counter_aggregate`.
fn to_snake_case(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 4);
    for (index, ch) in name.chars().enumerate() {
        if ch.is_ascii_uppercase() {
            if index > 0 {
                out.push('_');
            }
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}
