//! Code generation for `make_aggregate!`.

use std::collections::HashSet;

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::Ident;

use super::parse::{NodeDef, TypeDef};

struct WrapperDef {
    /// Generated wrapper name, e.g. `AAggregateCD`.
    name: Ident,
    /// The original domain type held in `root`.
    original: Ident,
    /// Wrapper fields: `(field name, field type)`.
    fields: Vec<(Ident, TokenStream)>,
    /// Whether this is the top-level `<root>Aggregate` wrapper, which gets the
    /// `Aggregate` impl. Nested wrappers are only views inside it.
    is_root: bool,
}

#[derive(Default)]
pub struct Generator {
    wrappers: Vec<WrapperDef>,
    seen: HashSet<String>,
}

impl Generator {
    pub fn generate(mut self, roots: Vec<NodeDef>) -> TokenStream {
        for root in &roots {
            // `Root as Alias` names the aggregate `Alias`; otherwise it is
            // `<root>Aggregate`.
            let wrapper = match &root.alias {
                Some(alias) => alias.clone(),
                None => format_ident!("{}Aggregate", root.name),
            };
            let prefix = wrapper.to_string();

            let mut fields = Vec::new();
            for field in &root.fields {
                // Children of the root start with an empty path: their wrapper
                // names are `<aggregate name><own name>`.
                let ty = self.resolve(&field.ty, &prefix, &[]);
                fields.push((field.name.clone(), ty));
            }

            self.push(wrapper, root.name.clone(), fields, true);
        }
        self.expand()
    }

    /// Resolves a field type to the emitted field type of the wrapper, pushing
    /// every needed child wrapper into the output.
    ///
    /// `prefix` is the aggregate's name (e.g. `CounterAggregate`), which every
    /// nested wrapper name starts with. `ancestors` is the chain of type names
    /// from the root to the enclosing node, excluding the root's own name.
    fn resolve(&mut self, ty: &TypeDef, prefix: &str, ancestors: &[String]) -> TokenStream {
        match ty {
            TypeDef::VecOf(inner) => {
                let inner = self.resolve(inner, prefix, ancestors);
                quote!(::std::vec::Vec<#inner>)
            }
            TypeDef::SetOf(inner) => {
                let inner = self.resolve(inner, prefix, ancestors);
                quote!(::std::collections::HashSet<#inner>)
            }
            TypeDef::Named { name, body } => {
                let mut path = ancestors.to_vec();
                path.push(name.to_string());
                let wrapper = format_ident!("{}{}", prefix, path.concat());

                let mut fields = Vec::new();
                if let Some(children) = body {
                    for field in children {
                        let child_ty = self.resolve(&field.ty, prefix, &path);
                        fields.push((field.name.clone(), child_ty));
                    }
                }
                self.push(wrapper.clone(), name.clone(), fields, false);
                quote!(#wrapper)
            }
        }
    }

    fn push(
        &mut self,
        name: Ident,
        original: Ident,
        fields: Vec<(Ident, TokenStream)>,
        is_root: bool,
    ) {
        // Identical paths resolve to the same wrapper; only emit it once.
        if self.seen.insert(name.to_string()) {
            self.wrappers.push(WrapperDef {
                name,
                original,
                fields,
                is_root,
            });
        }
    }

    fn expand(self) -> TokenStream {
        let mut expanded = TokenStream::new();
        for wrapper in &self.wrappers {
            let name = &wrapper.name;
            let original = &wrapper.original;
            let fields = wrapper
                .fields
                .iter()
                .map(|(name, ty)| quote!(pub #name: #ty,));
            expanded.extend(quote! {
                #[derive(Debug, Default)]
                pub struct #name {
                    pub root: #original,
                    #(#fields)*
                }
            });
            if wrapper.is_root {
                let name_str = name.to_string();
                expanded.extend(quote! {
                    impl ::croisiere::domain::Aggregate for #name {
                        const NAME: &'static str = #name_str;
                    }
                });
            }
        }
        expanded
    }
}
