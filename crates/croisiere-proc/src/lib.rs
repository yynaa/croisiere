//! Proc macros for the `croisiere` crate.

use std::collections::HashSet;

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{
    braced,
    parse::{Parse, ParseStream},
    parse_macro_input,
    Ident, ItemImpl, Token, Type,
};

/// Wraps user-defined domain types (entities / value objects) into aggregate
/// view structs.
///
/// # Example
///
/// ```ignore
/// make_aggregate!(
///     A {
///         b: B,
///         c: C {
///             d: D,
///         },
///     },
/// );
/// ```
///
/// generates (naming scheme: `<root>Aggregate<path>`):
///
/// ```ignore
/// pub struct AAggregate {
///     pub root: A,
///     pub b: AAggregateB,
///     pub c: AAggregateC,
/// }
///
/// pub struct AAggregateB {
///     pub root: B,
/// }
///
/// pub struct AAggregateC {
///     pub root: C,
///     pub d: AAggregateCD,
/// }
///
/// pub struct AAggregateCD {
///     pub root: D,
/// }
/// ```
///
/// Every comma is optional. Field types may also be `Vec<X>` or `HashSet<X>`,
/// in which case the generated field type wraps the child aggregate
/// accordingly.
#[proc_macro]
pub fn make_aggregate(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as AggregateInput);
    Generator::default().generate(input.roots).into()
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

struct AggregateInput {
    roots: Vec<NodeDef>,
}

impl Parse for AggregateInput {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut roots = Vec::new();
        while !input.is_empty() {
            let name: Ident = input.parse()?;
            let fields = parse_block(input)?;
            roots.push(NodeDef { name, fields });
            // Trailing commas are optional.
            if !input.is_empty() {
                input.parse::<Option<Token![,]>>()?;
            }
        }
        Ok(Self { roots })
    }
}

struct NodeDef {
    name: Ident,
    fields: Vec<FieldDef>,
}

struct FieldDef {
    name: Ident,
    ty: TypeDef,
}

enum TypeDef {
    /// A simple type name, optionally followed by a nested block.
    Named {
        name: Ident,
        body: Option<Vec<FieldDef>>,
    },
    VecOf(Box<TypeDef>),
    SetOf(Box<TypeDef>),
}

/// Parses the contents of a block: `ident: Type (comma?), ...`.
fn parse_block(input: ParseStream) -> syn::Result<Vec<FieldDef>> {
    if !input.peek(syn::token::Brace) {
        return Ok(Vec::new());
    }
    let content;
    braced!(content in input);
    let mut fields = Vec::new();
    while !content.is_empty() {
        let name: Ident = content.parse()?;
        content.parse::<Token![:]>()?;
        let ty = TypeDef::parse(&content)?;
        fields.push(FieldDef { name, ty });
        // Trailing commas are optional.
        if !content.is_empty() {
            content.parse::<Option<Token![,]>>()?;
        }
    }
    Ok(fields)
}

impl TypeDef {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let name: Ident = input.parse()?;
        match name.to_string().as_str() {
            "Vec" => {
                input.parse::<Token![<]>()?;
                let inner = Self::parse(input)?;
                input.parse::<Token![>]>()?;
                Ok(Self::VecOf(Box::new(inner)))
            }
            "HashSet" => {
                input.parse::<Token![<]>()?;
                let inner = Self::parse(input)?;
                input.parse::<Token![>]>()?;
                Ok(Self::SetOf(Box::new(inner)))
            }
            _ => {
                let body = if input.peek(syn::token::Brace) {
                    Some(parse_block(input)?)
                } else {
                    None
                };
                Ok(Self::Named { name, body })
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Generation
// ---------------------------------------------------------------------------

struct WrapperDef {
    /// Generated wrapper name, e.g. `AAggregateCD`.
    name: Ident,
    /// The original domain type held in `root`.
    original: Ident,
    /// Wrapper fields: `(field name, field type)`.
    fields: Vec<(Ident, TokenStream2)>,
}

#[derive(Default)]
struct Generator {
    wrappers: Vec<WrapperDef>,
    seen: HashSet<String>,
}

impl Generator {
    fn generate(mut self, roots: Vec<NodeDef>) -> TokenStream2 {
        for root in &roots {
            let root_name = root.name.to_string();

            let mut fields = Vec::new();
            for field in &root.fields {
                // Children of the root start with an empty path: their wrapper
                // names are `<root>Aggregate<own name>`.
                let ty = self.resolve(&field.ty, &root_name, &[]);
                fields.push((field.name.clone(), ty));
            }

            let wrapper = format_ident!("{}Aggregate", root_name);
            self.push(wrapper, root.name.clone(), fields);
        }
        self.expand()
    }

    /// Resolves a field type to the emitted field type of the wrapper, pushing
    /// every needed child wrapper into the output.
    ///
    /// `ancestors` is the chain of type names from the root to the enclosing
    /// node, excluding the root's own name.
    fn resolve(&mut self, ty: &TypeDef, root: &str, ancestors: &[String]) -> TokenStream2 {
        match ty {
            TypeDef::VecOf(inner) => {
                let inner = self.resolve(inner, root, ancestors);
                quote!(::std::vec::Vec<#inner>)
            }
            TypeDef::SetOf(inner) => {
                let inner = self.resolve(inner, root, ancestors);
                quote!(::std::collections::HashSet<#inner>)
            }
            TypeDef::Named { name, body } => {
                let mut path = ancestors.to_vec();
                path.push(name.to_string());
                let wrapper = format_ident!("{}Aggregate{}", root, path.concat());

                let mut fields = Vec::new();
                if let Some(children) = body {
                    for field in children {
                        let child_ty = self.resolve(&field.ty, root, &path);
                        fields.push((field.name.clone(), child_ty));
                    }
                }
                self.push(wrapper.clone(), name.clone(), fields);
                quote!(#wrapper)
            }
        }
    }

    fn push(&mut self, name: Ident, original: Ident, fields: Vec<(Ident, TokenStream2)>) {
        // Identical paths resolve to the same wrapper; only emit it once.
        if self.seen.insert(name.to_string()) {
            self.wrappers.push(WrapperDef {
                name,
                original,
                fields,
            });
        }
    }

    fn expand(self) -> TokenStream2 {
        let mut expanded = TokenStream2::new();
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
        }
        expanded
    }
}

// ---------------------------------------------------------------------------
// aggregate_functions
// ---------------------------------------------------------------------------

/// Wraps the impl block in a snake-cased module derived from the self type,
/// giving it a convenient scope for aggregate-related helpers.
///
/// # Example
///
/// ```ignore
/// #[aggregate_functions(CounterAggregateEvent, CounterAggregateError)]
/// impl CounterAggregate {
///     fn events(&self) -> DomainResult {
///         Ok(Vec::new())
///     }
/// }
/// ```
///
/// becomes:
///
/// ```ignore
/// mod counter_aggregate {
///     use super::CounterAggregate;
///     use super::*;
///
///     type DomainResult = Result<Vec<CounterAggregateEvent>, CounterAggregateError>;
///
///     impl CounterAggregate {
///         fn events(&self) -> DomainResult {
///             Ok(Vec::new())
///         }
///     }
/// }
/// ```
#[proc_macro_attribute]
pub fn aggregate_functions(attr: TokenStream, item: TokenStream) -> TokenStream {
    let args = parse_macro_input!(attr as FunctionArgs);
    let item = parse_macro_input!(item as ItemImpl);

    let self_name = match &*item.self_ty {
        Type::Path(path) if path.qself.is_none() => match path.path.segments.last() {
            Some(segment) => segment.ident.clone(),
            None => {
                return syn::Error::new_spanned(
                    &item.self_ty,
                    "aggregate_functions expects a non-empty impl path",
                )
                .to_compile_error()
                .into()
            }
        },
        _ => {
            return syn::Error::new_spanned(
                &item.self_ty,
                "aggregate_functions only supports plain path types",
            )
            .to_compile_error()
            .into()
        }
    };

    let mod_name = format_ident!("{}", to_snake_case(&self_name.to_string()));
    let event = &args.event;
    let error = &args.error;

    quote! {
        mod #mod_name {
            use super::#self_name;
            use super::*;

            type DomainResult = ::std::result::Result<::std::vec::Vec<#event>, #error>;

            #item
        }
    }
    .into()
}

/// Attribute arguments: `(EventType, ErrorType)`.
struct FunctionArgs {
    event: Type,
    error: Type,
}

impl Parse for FunctionArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let event: Type = input.parse()?;
        input.parse::<Token![,]>()?;
        let error: Type = input.parse()?;
        Ok(Self { event, error })
    }
}

/// `CounterAggregate` -> `counter_aggregate`.
fn to_snake_case(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 4);
    for (index, ch) in name.chars().enumerate() {
        if ch.is_ascii_uppercase() {
            if index > 0 {
                out.push('_');
            }
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}
