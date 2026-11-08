//! `#[derive(Snapshot)]` with its `#[snapshot(...)]` helper attribute.

use proc_macro2::TokenStream;
use quote::{quote, ToTokens};
use syn::{DeriveInput, Type};

/// Types read from `#[snapshot(aggregate = A, filter = F, sorter = S)]`.
struct SnapshotArgs {
    aggregate: Type,
    filter: Option<Type>,
    sorter: Option<Type>,
}

pub fn expand(input: DeriveInput) -> syn::Result<TokenStream> {
    let args = parse_args(&input)?;

    let name = &input.ident;
    let name_str = name.to_string();
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    let aggregate = &args.aggregate;
    let filter = args
        .filter
        .map(|ty| ty.into_token_stream())
        .unwrap_or_else(|| quote!(::croisiere::application::EmptyFilter));
    let sorter = args
        .sorter
        .map(|ty| ty.into_token_stream())
        .unwrap_or_else(|| quote!(::croisiere::application::EmptySorter));

    Ok(quote! {
        impl #impl_generics ::croisiere::application::Snapshot for #name #ty_generics #where_clause {
            const NAME: &'static str = #name_str;
            type Aggregate = #aggregate;
            type Filter = #filter;
            type Sorter = #sorter;
        }
    })
}

fn parse_args(input: &DeriveInput) -> syn::Result<SnapshotArgs> {
    let mut attrs = input
        .attrs
        .iter()
        .filter(|attr| attr.path().is_ident("snapshot"));

    let Some(attr) = attrs.next() else {
        return Err(syn::Error::new_spanned(
            &input.ident,
            "missing `#[snapshot(aggregate = ...)]` attribute",
        ));
    };
    if let Some(extra) = attrs.next() {
        return Err(syn::Error::new_spanned(
            extra,
            "only one `#[snapshot(...)]` attribute is allowed",
        ));
    }

    let mut aggregate = None;
    let mut filter = None;
    let mut sorter = None;
    attr.parse_nested_meta(|meta| {
        let slot = if meta.path.is_ident("aggregate") {
            &mut aggregate
        } else if meta.path.is_ident("filter") {
            &mut filter
        } else if meta.path.is_ident("sorter") {
            &mut sorter
        } else {
            return Err(meta.error("expected `aggregate`, `filter` or `sorter`"));
        };
        if slot.is_some() {
            return Err(meta.error("duplicate argument"));
        }
        *slot = Some(meta.value()?.parse::<Type>()?);
        Ok(())
    })?;

    let aggregate = aggregate.ok_or_else(|| {
        syn::Error::new_spanned(attr, "missing required `aggregate = ...` argument")
    })?;

    Ok(SnapshotArgs {
        aggregate,
        filter,
        sorter,
    })
}
