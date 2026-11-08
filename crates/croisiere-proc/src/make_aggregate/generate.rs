//! Code generation for `make_aggregate!`.

use std::collections::HashSet;

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{ext::IdentExt, Ident, Type};

use super::parse::{NodeDef, TypeDef};

struct WrapperDef {
    /// Generated wrapper name, e.g. `AAggregateCD` for field `c` then `d`.
    name: Ident,
    /// The original domain type held in `root`, or `None` for a `rootless` node.
    original: Option<TokenStream>,
    /// Wrapper fields: `(field name, field type)`.
    fields: Vec<(Ident, TokenStream)>,
    /// Set only on the top-level `<root>Aggregate` wrapper, which gets the
    /// `Aggregate` impl. Nested wrappers are only views inside it.
    aggregate: Option<AggregateTypes>,
}

/// Associated types of the `Aggregate` impl.
struct AggregateTypes {
    event: Type,
    error: Type,
}

#[derive(Default)]
pub struct Generator {
    wrappers: Vec<WrapperDef>,
    seen: HashSet<String>,
}

impl Generator {
    pub fn generate(mut self, roots: Vec<NodeDef>) -> TokenStream {
        for root in &roots {
            // `#[alias]` gives the aggregate its name; otherwise it is
            // `<root>Aggregate`.
            let wrapper = match &root.alias {
                Some(alias) => alias.clone(),
                None => format_ident!("{}Aggregate", root.name),
            };
            let prefix = wrapper.to_string();

            let mut fields = Vec::new();
            for field in &root.fields {
                // Children of the root start with an empty path: their wrapper
                // names are `<aggregate name><field name>`.
                let ty = self.resolve(&field.ty, &field.name, field.rootless, &prefix, &[]);
                fields.push((field.name.clone(), ty));
            }

            let types = AggregateTypes {
                event: root.event.clone(),
                error: root.error.clone(),
            };
            let root_name = &root.name;
            self.push(wrapper, Some(quote!(#root_name)), fields, Some(types));
        }
        self.expand()
    }

    /// Resolves a field type to the emitted field type of the wrapper, pushing
    /// every needed child wrapper into the output.
    ///
    /// `prefix` is the aggregate's name (e.g. `CounterAggregate`), which every
    /// nested wrapper name starts with. `field` is the name of the field being
    /// resolved and `ancestors` the chain of field names from the root to the
    /// enclosing field, excluding `field`. Wrapper names come from field names
    /// so that the same type can be used in several places.
    /// `rootless` is the `#[rootless]` flag of the field being resolved; it
    /// applies to the named type inside any `Vec`/`HashSet`.
    fn resolve(
        &mut self,
        ty: &TypeDef,
        field: &Ident,
        rootless: bool,
        prefix: &str,
        ancestors: &[String],
    ) -> TokenStream {
        match ty {
            TypeDef::VecOf(inner) => {
                let inner = self.resolve(inner, field, rootless, prefix, ancestors);
                quote!(::std::vec::Vec<#inner>)
            }
            TypeDef::SetOf(inner) => {
                let inner = self.resolve(inner, field, rootless, prefix, ancestors);
                quote!(::std::collections::HashSet<#inner>)
            }
            TypeDef::OptionOf(inner) => {
                let inner = self.resolve(inner, field, rootless, prefix, ancestors);
                quote!(::std::option::Option<#inner>)
            }
            TypeDef::Named { name, body } => {
                let mut path = ancestors.to_vec();
                path.push(pascal_case(field));
                let wrapper = format_ident!("{}{}", prefix, path.concat());

                let mut fields = Vec::new();
                if let Some(children) = body {
                    for child in children {
                        let child_ty =
                            self.resolve(&child.ty, &child.name, child.rootless, prefix, &path);
                        fields.push((child.name.clone(), child_ty));
                    }
                }
                let original = (!rootless).then(|| quote!(#name));
                self.push(wrapper.clone(), original, fields, None);
                quote!(#wrapper)
            }
            TypeDef::Generic(ty) => {
                let mut path = ancestors.to_vec();
                path.push(pascal_case(field));
                let wrapper = format_ident!("{}{}", prefix, path.concat());

                let original = (!rootless).then(|| quote!(#ty));
                self.push(wrapper.clone(), original, Vec::new(), None);
                quote!(#wrapper)
            }
        }
    }

    fn push(
        &mut self,
        name: Ident,
        original: Option<TokenStream>,
        fields: Vec<(Ident, TokenStream)>,
        aggregate: Option<AggregateTypes>,
    ) {
        // Identical paths resolve to the same wrapper; only emit it once.
        if self.seen.insert(name.to_string()) {
            self.wrappers.push(WrapperDef {
                name,
                original,
                fields,
                aggregate,
            });
        }
    }

    fn expand(self) -> TokenStream {
        let mut expanded = TokenStream::new();
        for wrapper in &self.wrappers {
            let name = &wrapper.name;
            let root = wrapper
                .original
                .as_ref()
                .map(|original| quote!(pub root: #original,));
            let fields = wrapper
                .fields
                .iter()
                .map(|(name, ty)| quote!(pub #name: #ty,));
            expanded.extend(quote! {
                #[derive(Debug, Default)]
                pub struct #name {
                    #root
                    #(#fields)*
                }
            });
            if let Some(AggregateTypes { event, error }) = &wrapper.aggregate {
                let name_str = name.to_string();
                expanded.extend(quote! {
                    impl ::croisiere::domain::Aggregate for #name {
                        const NAME: &'static str = #name_str;
                        type Error = #error;
                        type Event = #event;
                    }
                });
            }
        }
        expanded
    }
}

/// `field_name` as `FieldName`, used as a segment of a wrapper name.
fn pascal_case(field: &Ident) -> String {
    field
        .unraw()
        .to_string()
        .split('_')
        .filter(|segment| !segment.is_empty())
        .map(|segment| {
            let mut chars = segment.chars();
            chars
                .next()
                .into_iter()
                .flat_map(char::to_uppercase)
                .chain(chars)
                .collect::<String>()
        })
        .collect()
}
