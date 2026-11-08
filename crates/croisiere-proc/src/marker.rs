//! Shared expansion for derives of empty marker traits (`Filter`, `Sorter`).

use proc_macro2::TokenStream;
use quote::quote;
use syn::DeriveInput;

/// Emits `impl <trait_path> for <type> {}`.
pub fn impl_marker_trait(input: &DeriveInput, trait_path: TokenStream) -> TokenStream {
    let name = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    quote! {
        impl #impl_generics #trait_path for #name #ty_generics #where_clause {}
    }
}
