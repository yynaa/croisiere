//! Shared expansion for derives of traits that only carry a `NAME` constant
//! (`ValueObject`, `Entity`).

use proc_macro2::TokenStream;
use quote::quote;
use syn::DeriveInput;

/// Emits `impl <trait_path> for <type> { const NAME = "<type>"; }`.
pub fn impl_named_trait(input: &DeriveInput, trait_path: TokenStream) -> TokenStream {
    let name = &input.ident;
    let name_str = name.to_string();
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    quote! {
        impl #impl_generics #trait_path for #name #ty_generics #where_clause {
            const NAME: &'static str = #name_str;
        }
    }
}
