//! `#[derive(Filter)]`.

use proc_macro2::TokenStream;
use quote::quote;
use syn::DeriveInput;

use crate::marker::impl_marker_trait;

pub fn expand(input: DeriveInput) -> TokenStream {
    impl_marker_trait(&input, quote!(::croisiere::application::Filter))
}
