//! `#[derive(ValueObject)]`.

use proc_macro2::TokenStream;
use quote::quote;
use syn::DeriveInput;

use crate::named::impl_named_trait;

pub fn expand(input: DeriveInput) -> TokenStream {
    impl_named_trait(&input, quote!(::croisiere::domain::ValueObject))
}
