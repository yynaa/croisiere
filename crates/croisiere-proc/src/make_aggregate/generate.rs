//! Code generation for `make_aggregate!`.

use std::collections::HashMap;

use proc_macro2::TokenStream;
use quote::{format_ident, quote, ToTokens};
use syn::{ext::IdentExt, Ident, Path, Type};

use super::parse::{FieldDef, NodeDef, TypeDef};

/// A field of a wrapper, as its parent sees it.
struct ChildDef {
    name: Ident,
    /// Container written around the child, e.g. `Option` or `Vec`.
    container: Option<TokenStream>,
    /// Written as `#[collection]`: the setter takes an iterator.
    collection: bool,
    /// Wrapper of the child.
    wrapper: Ident,
    /// Domain type held in the child's `root`, or `None` for a `rootless` node.
    root: Option<TokenStream>,
    /// Whether the child has fields of its own.
    has_children: bool,
}

struct WrapperDef {
    /// Generated wrapper name, e.g. `AAggregateCD` for field `c` then `d`.
    name: Ident,
    /// Domain type held in `root`, or `None` for a `rootless` node.
    root: Option<TokenStream>,
    children: Vec<ChildDef>,
    /// Traits derived on the wrapper, besides `Debug` and `Default`.
    derives: Vec<Path>,
    /// Set only on the top-level `<root>Aggregate` wrapper, which gets the
    /// `Aggregate` impl and `builder()`. Nested wrappers are only views inside
    /// it.
    aggregate: Option<AggregateTypes>,
    /// Whether the wrapper gets a builder; off for every wrapper of an
    /// aggregate written with `#[no_builder]`.
    builder: bool,
}

/// Associated types of the `Aggregate` impl.
struct AggregateTypes {
    event: Type,
    error: Type,
}

#[derive(Default)]
pub struct Generator {
    wrappers: Vec<WrapperDef>,
    /// Name of every wrapper generated so far, with the path of the node it
    /// was generated for (e.g. `A.c.d`), to report name collisions.
    seen: HashMap<String, String>,
}

impl Generator {
    pub fn generate(mut self, roots: Vec<NodeDef>) -> syn::Result<TokenStream> {
        for root in &roots {
            // `#[alias]` gives the aggregate its name; otherwise it is
            // `<root>Aggregate`.
            let wrapper = match &root.alias {
                Some(alias) => alias.clone(),
                None => format_ident!("{}Aggregate", root.name),
            };
            let prefix = wrapper.to_string();
            let node_path = root.name.to_string();

            // Children of the root start with an empty path: their wrapper
            // names are `<aggregate name><field name>`.
            let mut children = Vec::new();
            for field in &root.fields {
                children.push(self.resolve_child(
                    field,
                    &prefix,
                    &[],
                    &node_path,
                    &[],
                    root.builder,
                )?);
            }

            let root_name = &root.name;
            let types = AggregateTypes {
                event: root.event.clone(),
                error: root.error.clone(),
            };
            self.push(
                WrapperDef {
                    name: wrapper,
                    root: Some(quote!(#root_name)),
                    children,
                    derives: Vec::new(),
                    aggregate: Some(types),
                    builder: root.builder,
                },
                root.alias.as_ref().unwrap_or(&root.name),
                node_path,
            )?;
        }
        Ok(self.expand())
    }

    /// Resolves a field to its child, pushing the child's wrapper and those of
    /// its own fields into the output.
    ///
    /// `prefix` is the aggregate's name (e.g. `CounterAggregate`), which every
    /// nested wrapper name starts with. `ancestors` is the chain of field names
    /// from the root to the field's parent, in PascalCase. `parent_path` is the
    /// parent's path as written (e.g. `A.c`), for error messages. `derives` are
    /// the traits the parent derives, which its fields must derive too.
    /// `builder` is whether the
    /// aggregate generates builders.
    fn resolve_child(
        &mut self,
        field: &FieldDef,
        prefix: &str,
        ancestors: &[String],
        parent_path: &str,
        derives: &[Path],
        builder: bool,
    ) -> syn::Result<ChildDef> {
        let (container, inner) = match &field.ty {
            TypeDef::Container { path, inner } => (Some(path.clone()), inner.as_ref()),
            other => (None, other),
        };
        if matches!(inner, TypeDef::Container { .. }) {
            return Err(syn::Error::new_spanned(
                &field.name,
                "nested containers are not supported",
            ));
        }
        if field.collection && container.is_none() {
            return Err(syn::Error::new_spanned(
                &field.name,
                "`#[collection]` needs a container type, such as `Vec<...>`",
            ));
        }
        if field.collection && field.rootless {
            return Err(syn::Error::new_spanned(
                &field.name,
                "a `#[rootless]` field cannot be a collection",
            ));
        }
        let TypeDef::Named {
            path: root_path,
            body,
        } = inner
        else {
            unreachable!("containers are unwrapped above")
        };

        let derives = merge_derives(derives, &field.derives);
        let mut path = ancestors.to_vec();
        path.push(pascal_case(&field.name));
        let wrapper = format_ident!("{}{}", prefix, path.concat());
        let node_path = format!("{parent_path}.{}", field.name.unraw());

        let mut children = Vec::new();
        for child in body.iter().flatten() {
            children.push(self.resolve_child(child, prefix, &path, &node_path, &derives, builder)?);
        }
        let root = (!field.rootless).then(|| root_path.clone());
        let has_children = !children.is_empty();
        self.push(
            WrapperDef {
                name: wrapper.clone(),
                root: root.clone(),
                children,
                derives,
                aggregate: None,
                builder,
            },
            &field.name,
            node_path,
        )?;

        Ok(ChildDef {
            name: field.name.clone(),
            container,
            collection: field.collection,
            wrapper,
            root,
            has_children,
        })
    }

    /// Adds a wrapper to the output, generated for the node at `node_path`
    /// and written at `span`.
    ///
    /// Two nodes can't share a wrapper name: different paths can concatenate
    /// to the same name (`foo_bar` and `foo { bar }` both give `FooBar`), and
    /// keeping only one of them would silently give the other the wrong root
    /// type or derives.
    fn push(&mut self, wrapper: WrapperDef, span: &Ident, node_path: String) -> syn::Result<()> {
        let name = wrapper.name.to_string();
        if let Some(previous) = self.seen.get(&name) {
            let message = if *previous == node_path {
                format!("`{node_path}` is declared twice")
            } else {
                // Root paths have no `.`. `#[alias]` renames a whole aggregate,
                // so it only helps when one of the two is an aggregate.
                let hint = if node_path.contains('.') && previous.contains('.') {
                    "rename one of the fields"
                } else {
                    "rename one of them or use `#[alias]`"
                };
                format!(
                    "`{node_path}` and `{previous}` would both generate the wrapper \
                     `{name}`; {hint}"
                )
            };
            return Err(syn::Error::new_spanned(span, message));
        }
        self.seen.insert(name, node_path);
        self.wrappers.push(wrapper);
        Ok(())
    }

    fn expand(self) -> TokenStream {
        let mut expanded = TokenStream::new();
        for wrapper in &self.wrappers {
            expanded.extend(wrapper.expand());
        }
        expanded
    }
}

impl WrapperDef {
    fn expand(&self) -> TokenStream {
        let name = &self.name;
        let derives = &self.derives;
        let root_field = self.root.as_ref().map(|root| quote!(pub root: #root,));
        let struct_fields: Vec<TokenStream> = self
            .children
            .iter()
            .map(|child| {
                let field = &child.name;
                let ty = child.field_type();
                quote!(pub #field: #ty,)
            })
            .collect();
        let mut expanded = quote! {
            #[derive(Debug, Default)]
            #[derive(#(#derives),*)]
            pub struct #name {
                #root_field
                #(#struct_fields)*
            }
        };

        if let (None, Some(root)) = (&self.aggregate, &self.root) {
            let rest = if self.children.is_empty() {
                quote!()
            } else {
                quote!(..Default::default())
            };
            expanded.extend(quote! {
                impl ::core::convert::From<#root> for #name {
                    fn from(root: #root) -> Self {
                        Self { root, #rest }
                    }
                }
            });
        }

        if let Some(AggregateTypes { event, error }) = &self.aggregate {
            let name_str = name.to_string();
            expanded.extend(quote! {
                impl ::croisiere::domain::Aggregate for #name {
                    const NAME: &'static str = #name_str;
                    type Error = #error;
                    type Event = #event;
                }
            });
        }

        if self.builder {
            expanded.extend(self.expand_builder());
        }

        expanded
    }

    /// The wrapper's builder, and `builder()` on the aggregate.
    fn expand_builder(&self) -> TokenStream {
        let name = &self.name;
        let builder = builder_name(name);

        let builder_root = self.root.as_ref().map(|root| quote!(root: #root,));
        let builder_fields: Vec<TokenStream> = self
            .children
            .iter()
            .map(|child| {
                let field = &child.name;
                let ty = child.field_type();
                quote!(#field: #ty,)
            })
            .collect();

        let mut expanded = quote! {
            #[derive(Default)]
            pub struct #builder {
                #builder_root
                #(#builder_fields)*
            }
        };

        let root_setter = match (&self.aggregate, &self.root) {
            (Some(_), Some(root)) => Some(quote! {
                pub fn root(mut self, root: #root) -> Self {
                    self.root = root;
                    self
                }
            }),
            _ => None,
        };
        let setters: Vec<TokenStream> = self
            .children
            .iter()
            .filter_map(|child| child.setter())
            .collect();
        let build_root = self.root.as_ref().map(|_| quote!(root: self.root,));
        let build_fields: Vec<TokenStream> = self
            .children
            .iter()
            .map(|child| {
                let field = &child.name;
                quote!(#field: self.#field,)
            })
            .collect();
        expanded.extend(quote! {
            impl #builder {
                #root_setter
                #(#setters)*

                pub fn build(self) -> #name {
                    #name {
                        #build_root
                        #(#build_fields)*
                    }
                }
            }
        });

        if self.aggregate.is_some() {
            expanded.extend(quote! {
                impl #name {
                    pub fn builder() -> #builder {
                        #builder::default()
                    }
                }
            });
        }

        expanded
    }
}

impl ChildDef {
    /// The type of the field in the wrapper and in its builder.
    fn field_type(&self) -> TokenStream {
        let wrapper = &self.wrapper;
        match &self.container {
            Some(container) => quote!(#container<#wrapper>),
            None => quote!(#wrapper),
        }
    }

    /// The builder setter for the field, or `None` when there is nothing to
    /// set (a `rootless` node without fields).
    fn setter(&self) -> Option<TokenStream> {
        let field = &self.name;
        let wrapper = &self.wrapper;
        let builder = builder_name(wrapper);

        if self.collection {
            return Some(quote! {
                pub fn #field<I, T>(mut self, items: impl FnOnce() -> I) -> Self
                where
                    I: IntoIterator<Item = T>,
                    T: Into<#wrapper>,
                {
                    self.#field = items().into_iter().map(Into::into).collect();
                    self
                }
            });
        }

        // The value of the field before any container is applied.
        let assign = |value: TokenStream| match self.container {
            Some(_) => quote!(self.#field = (#value).into()),
            None => quote!(self.#field = #value),
        };

        match (&self.root, self.has_children) {
            (Some(root), false) => {
                let stmt = assign(quote!(<#wrapper>::from(root)));
                Some(quote! {
                    pub fn #field(mut self, root: #root) -> Self {
                        #stmt;
                        self
                    }
                })
            }
            (Some(root), true) => {
                let stmt = assign(quote!(
                    children(#builder { root, ..Default::default() }).build()
                ));
                Some(quote! {
                    pub fn #field(
                        mut self,
                        root: #root,
                        children: impl FnOnce(#builder) -> #builder,
                    ) -> Self {
                        #stmt;
                        self
                    }
                })
            }
            (None, true) => {
                let stmt = assign(quote!(children(#builder::default()).build()));
                Some(quote! {
                    pub fn #field(mut self, children: impl FnOnce(#builder) -> #builder) -> Self {
                        #stmt;
                        self
                    }
                })
            }
            (None, false) => None,
        }
    }
}

fn builder_name(wrapper: &Ident) -> Ident {
    format_ident!("{}Builder", wrapper)
}

/// The traits a field's wrapper derives: those of its parent, then its own,
/// without repeats.
fn merge_derives(inherited: &[Path], own: &[Path]) -> Vec<Path> {
    let mut merged: Vec<Path> = Vec::new();
    for path in inherited.iter().chain(own) {
        let written = path.to_token_stream().to_string();
        if !merged
            .iter()
            .any(|known| known.to_token_stream().to_string() == written)
        {
            merged.push(path.clone());
        }
    }
    merged
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
