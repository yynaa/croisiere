//! `#[derive(DomainService)]` with its `#[service(...)]` helper attribute.

use proc_macro2::TokenStream;
use quote::quote;
use syn::{DeriveInput, Type};

/// Types read from `#[service(event = E, error = R)]`.
struct ServiceArgs {
    event: Type,
    error: Type,
}

pub fn expand(input: DeriveInput) -> syn::Result<TokenStream> {
    let ServiceArgs { event, error } = parse_args(&input)?;

    let name = &input.ident;
    let name_str = name.to_string();
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    Ok(quote! {
        impl #impl_generics ::croisiere::domain::DomainService for #name #ty_generics #where_clause {
            const NAME: &'static str = #name_str;
            type Error = #error;
            type Event = #event;
        }
    })
}

fn parse_args(input: &DeriveInput) -> syn::Result<ServiceArgs> {
    let mut attrs = input
        .attrs
        .iter()
        .filter(|attr| attr.path().is_ident("service"));

    let Some(attr) = attrs.next() else {
        return Err(syn::Error::new_spanned(
            &input.ident,
            "missing `#[service(event = ..., error = ...)]` attribute",
        ));
    };
    if let Some(extra) = attrs.next() {
        return Err(syn::Error::new_spanned(
            extra,
            "only one `#[service(...)]` attribute is allowed",
        ));
    }

    let mut event = None;
    let mut error = None;
    attr.parse_nested_meta(|meta| {
        let slot = if meta.path.is_ident("event") {
            &mut event
        } else if meta.path.is_ident("error") {
            &mut error
        } else {
            return Err(meta.error("expected `event` or `error`"));
        };
        if slot.is_some() {
            return Err(meta.error("duplicate argument"));
        }
        *slot = Some(meta.value()?.parse::<Type>()?);
        Ok(())
    })?;

    let missing = |what: &str| {
        syn::Error::new_spanned(attr, format!("missing required `{what} = ...` argument"))
    };
    Ok(ServiceArgs {
        event: event.ok_or_else(|| missing("event"))?,
        error: error.ok_or_else(|| missing("error"))?,
    })
}
