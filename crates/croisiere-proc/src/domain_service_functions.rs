//! `#[domain_service_functions]`.

use proc_macro2::TokenStream;
use quote::quote;
use syn::ItemImpl;

use crate::functions_module::wrap_in_module;

/// `attr` is the attribute's argument list, which must be empty.
pub fn expand(attr: TokenStream, item: ItemImpl) -> syn::Result<TokenStream> {
    if !attr.is_empty() {
        return Err(syn::Error::new_spanned(
            attr,
            "domain_service_functions takes no arguments",
        ));
    }

    wrap_in_module("domain_service_functions", &item, |_| quote!())
}
