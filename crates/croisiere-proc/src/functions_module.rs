//! Shared expansion for the `*_functions` attributes (`aggregate_functions`,
//! `domain_service_functions`): wraps an impl block in a module named after its
//! self type.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{Ident, ItemImpl, Type};

/// Emits `mod <self_type_snake_case> { use super::*; <prelude> <item> }`.
///
/// `macro_name` is only used in error messages. `prelude` holds the items the
/// calling macro adds next to the impl block; it receives the self type's
/// name to build them.
pub fn wrap_in_module(
    macro_name: &str,
    item: &ItemImpl,
    prelude: impl FnOnce(&Ident) -> TokenStream,
) -> syn::Result<TokenStream> {
    let self_name = self_type_name(macro_name, item)?;
    let mod_name = format_ident!("{}", to_snake_case(&self_name.to_string()));
    let prelude = prelude(&self_name);

    Ok(quote! {
        mod #mod_name {
            use super::#self_name;
            use super::*;

            #prelude

            #item
        }
    })
}

/// The last path segment of the impl's self type, e.g. `CounterAggregate`.
fn self_type_name(macro_name: &str, item: &ItemImpl) -> syn::Result<Ident> {
    match &*item.self_ty {
        Type::Path(path) if path.qself.is_none() => path
            .path
            .segments
            .last()
            .map(|segment| segment.ident.clone())
            .ok_or_else(|| {
                syn::Error::new_spanned(
                    &item.self_ty,
                    format!("{macro_name} expects a non-empty impl path"),
                )
            }),
        _ => Err(syn::Error::new_spanned(
            &item.self_ty,
            format!("{macro_name} only supports plain path types"),
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
