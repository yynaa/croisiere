//! `#[aggregate_functions(Event, Error)]`.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{
    parse::{Parse, ParseStream},
    Ident, ItemImpl, Token, Type,
};

/// Attribute arguments: `(EventType, ErrorType)`.
pub struct FunctionArgs {
    event: Type,
    error: Type,
}

impl Parse for FunctionArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let event: Type = input.parse()?;
        input.parse::<Token![,]>()?;
        let error: Type = input.parse()?;
        Ok(Self { event, error })
    }
}

pub fn expand(args: FunctionArgs, item: ItemImpl) -> syn::Result<TokenStream> {
    let self_name = self_type_name(&item)?;
    let mod_name = format_ident!("{}", to_snake_case(&self_name.to_string()));
    let FunctionArgs { event, error } = args;

    Ok(quote! {
        mod #mod_name {
            use super::#self_name;
            use super::*;

            type DomainResult = ::std::result::Result<::std::vec::Vec<#event>, #error>;

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
