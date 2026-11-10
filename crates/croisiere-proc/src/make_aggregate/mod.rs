//! `make_aggregate!(...)`.
//!
//! - `parse`: turns the DSL into a tree of [`parse::NodeDef`]s.
//! - `generate`: walks that tree and emits one wrapper struct per node.

mod generate;
mod parse;

use proc_macro2::TokenStream;

pub use parse::AggregateInput;

pub fn expand(input: AggregateInput) -> syn::Result<TokenStream> {
    generate::Generator::default().generate(input.roots)
}

#[cfg(test)]
mod tests {
    use quote::quote;

    use super::*;

    fn expand_tokens(tokens: TokenStream) -> syn::Result<TokenStream> {
        expand(syn::parse2(tokens)?)
    }

    fn error_of(tokens: TokenStream) -> String {
        expand_tokens(tokens)
            .expect_err("expected an error")
            .to_string()
    }

    #[test]
    fn distinct_wrapper_names_expand() {
        expand_tokens(quote!(
            #[emits = E]
            #[fails = F]
            A {
                foo: X { bar: Y },
                baz: Z
            }
        ))
        .unwrap();
    }

    #[test]
    fn fields_whose_paths_concatenate_to_the_same_name_are_rejected() {
        assert_eq!(
            error_of(quote!(
                #[emits = E]
                #[fails = F]
                A {
                    foo_bar: X,
                    foo: Y { bar: Z }
                }
            )),
            "`A.foo.bar` and `A.foo_bar` would both generate the wrapper \
             `AAggregateFooBar`; rename one of the fields"
        );
    }

    #[test]
    fn aggregate_named_like_another_wrapper_is_rejected() {
        assert_eq!(
            error_of(quote!(
                #[emits = E] #[fails = F]
                A { b: B }

                #[alias(exact = AAggregateB)]
                #[emits = E] #[fails = F]
                C
            )),
            "`C` and `A.b` would both generate the wrapper `AAggregateB`; \
             rename one of them or use `#[alias]`"
        );
    }

    #[test]
    fn roots_with_the_same_name_are_rejected() {
        assert_eq!(
            error_of(quote!(
                #[emits = E] #[fails = F] A
                #[emits = E] #[fails = F] A
            )),
            "`A` is declared twice"
        );
    }

    #[test]
    fn duplicate_fields_are_rejected() {
        assert_eq!(
            error_of(quote!(
                #[emits = E]
                #[fails = F]
                A { b: B, b: B }
            )),
            "`A.b` is declared twice"
        );
    }
}
