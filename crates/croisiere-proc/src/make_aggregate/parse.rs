//! Parsing of the `make_aggregate!` DSL.

use proc_macro2::TokenStream;
use quote::{format_ident, quote, ToTokens};
use syn::{
    braced,
    parse::{Parse, ParseStream},
    punctuated::Punctuated,
    Attribute, Ident, Meta, Path, Token, Type,
};

pub struct AggregateInput {
    pub roots: Vec<NodeDef>,
}

impl Parse for AggregateInput {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut roots = Vec::new();
        while !input.is_empty() {
            let attrs = input.call(Attribute::parse_outer)?;
            let name: Ident = input.parse()?;
            let fields = parse_block(input)?;
            let root = RootAttrs::from_attrs(&attrs, &name)?;
            roots.push(NodeDef {
                name,
                alias: root.alias,
                fields,
                event: root.event,
                error: root.error,
            });
            // Trailing commas are optional.
            if !input.is_empty() {
                input.parse::<Option<Token![,]>>()?;
            }
        }
        Ok(Self { roots })
    }
}

pub struct NodeDef {
    pub name: Ident,
    /// Full name of the generated aggregate, when set with `#[alias]`.
    pub alias: Option<Ident>,
    pub fields: Vec<FieldDef>,
    /// `Aggregate::Event`, from `#[emits = EventType]`.
    pub event: Type,
    /// `Aggregate::Error`, from `#[fails = ErrorType]`.
    pub error: Type,
}

pub struct FieldDef {
    pub name: Ident,
    /// Written as `#[rootless]`: the wrapper has no `root` field.
    pub rootless: bool,
    /// Written as `#[collection]`: the field is filled from an iterator.
    pub collection: bool,
    /// Traits written as `#[derive(...)]`, derived on the field's wrapper and
    /// the wrappers inside it.
    pub derives: Vec<Path>,
    pub ty: TypeDef,
}

pub enum TypeDef {
    /// A type path, optionally followed by a nested block.
    Named {
        path: TokenStream,
        body: Option<Vec<FieldDef>>,
    },
    /// A type path with one generic argument, e.g. `Vec<Line>` or
    /// `Option<Note>`. The path is kept as written.
    Container {
        path: TokenStream,
        inner: Box<TypeDef>,
    },
}

/// The attributes allowed before a root.
struct RootAttrs {
    alias: Option<Ident>,
    event: Type,
    error: Type,
}

impl RootAttrs {
    fn from_attrs(attrs: &[Attribute], root: &Ident) -> syn::Result<Self> {
        let mut alias = None;
        let mut event = None;
        let mut error = None;
        for attr in attrs {
            if attr.path().is_ident("alias") {
                set_once(&mut alias, attr, parse_alias(attr)?)?;
            } else if attr.path().is_ident("emits") {
                set_once(&mut event, attr, parse_value::<Type>(attr)?)?;
            } else if attr.path().is_ident("fails") {
                set_once(&mut error, attr, parse_value::<Type>(attr)?)?;
            } else {
                return Err(syn::Error::new_spanned(
                    attr,
                    "expected `#[alias = Name]`, `#[alias(exact = Name)]`, `#[emits = EventType]` or `#[fails = ErrorType]`",
                ));
            }
        }
        let missing = |what: &str| {
            syn::Error::new(
                root.span(),
                format!("missing `#[{what} = ...]` on the aggregate root `{root}`"),
            )
        };
        Ok(Self {
            alias,
            event: event.ok_or_else(|| missing("emits"))?,
            error: error.ok_or_else(|| missing("fails"))?,
        })
    }
}

/// Parses `#[alias = Name]`, which names the aggregate `NameAggregate`, or
/// `#[alias(exact = Name)]`, which names it `Name`.
fn parse_alias(attr: &Attribute) -> syn::Result<Ident> {
    match &attr.meta {
        Meta::NameValue(_) => {
            let name: Ident = parse_value(attr)?;
            Ok(format_ident!("{}Aggregate", name))
        }
        Meta::List(_) => attr.parse_args_with(|input: ParseStream| {
            let key: Ident = input.parse()?;
            if key != "exact" {
                return Err(syn::Error::new_spanned(&key, "expected `exact`"));
            }
            input.parse::<Token![=]>()?;
            input.parse::<Ident>()
        }),
        Meta::Path(_) => Err(syn::Error::new_spanned(
            attr,
            "expected `#[alias = Name]` or `#[alias(exact = Name)]`",
        )),
    }
}

/// Stores `value` in `slot`, rejecting a second occurrence of the same attribute.
fn set_once<T>(slot: &mut Option<T>, attr: &Attribute, value: T) -> syn::Result<()> {
    if slot.is_some() {
        return Err(syn::Error::new_spanned(attr, "duplicate attribute"));
    }
    *slot = Some(value);
    Ok(())
}

/// Parses the value of a `#[name = value]` attribute as `T`.
fn parse_value<T: Parse>(attr: &Attribute) -> syn::Result<T> {
    let Meta::NameValue(name_value) = &attr.meta else {
        return Err(syn::Error::new_spanned(attr, "expected `#[name = value]`"));
    };
    syn::parse2(name_value.value.to_token_stream())
}

/// The flags written before a field.
#[derive(Default)]
struct FieldFlags {
    rootless: bool,
    collection: bool,
    derives: Vec<Path>,
}

/// Parses the attributes allowed before a field: `#[rootless]`,
/// `#[collection]` (each at most once) and `#[derive(...)]`.
fn parse_field_attrs(attrs: &[Attribute]) -> syn::Result<FieldFlags> {
    let mut flags = FieldFlags::default();
    let mut has_derive = false;
    for attr in attrs {
        if attr.path().is_ident("derive") {
            if has_derive {
                return Err(syn::Error::new_spanned(attr, "duplicate attribute"));
            }
            has_derive = true;
            let traits = attr.parse_args_with(Punctuated::<Path, Token![,]>::parse_terminated)?;
            flags.derives = traits.into_iter().collect();
            continue;
        }
        let is_flag = matches!(attr.meta, Meta::Path(_));
        let slot = if attr.path().is_ident("rootless") && is_flag {
            &mut flags.rootless
        } else if attr.path().is_ident("collection") && is_flag {
            &mut flags.collection
        } else {
            return Err(syn::Error::new_spanned(
                attr,
                "expected `#[rootless]`, `#[collection]` or `#[derive(...)]` on a field",
            ));
        };
        if *slot {
            return Err(syn::Error::new_spanned(attr, "duplicate attribute"));
        }
        *slot = true;
    }
    Ok(flags)
}

/// Parses the contents of a block: `#[attr]* ident: Type (comma?), ...`.
fn parse_block(input: ParseStream) -> syn::Result<Vec<FieldDef>> {
    if !input.peek(syn::token::Brace) {
        return Ok(Vec::new());
    }
    let content;
    braced!(content in input);
    let mut fields = Vec::new();
    while !content.is_empty() {
        let attrs = content.call(Attribute::parse_outer)?;
        let flags = parse_field_attrs(&attrs)?;
        let name: Ident = content.parse()?;
        content.parse::<Token![:]>()?;
        let ty = TypeDef::parse(&content)?;
        fields.push(FieldDef {
            name,
            rootless: flags.rootless,
            collection: flags.collection,
            derives: flags.derives,
            ty,
        });
        // Trailing commas are optional.
        if !content.is_empty() {
            content.parse::<Option<Token![,]>>()?;
        }
    }
    Ok(fields)
}

impl TypeDef {
    /// Parses `Path`, `Path { fields }` or `Path<Inner>`. Any type with one
    /// generic argument is a container, whatever its name.
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let segments = Punctuated::<Ident, Token![::]>::parse_separated_nonempty(input)?;
        let path = quote!(#segments);

        if input.peek(Token![<]) {
            input.parse::<Token![<]>()?;
            let inner = Self::parse(input)?;
            input.parse::<Token![>]>()?;
            return Ok(Self::Container {
                path,
                inner: Box::new(inner),
            });
        }

        let body = if input.peek(syn::token::Brace) {
            Some(parse_block(input)?)
        } else {
            None
        };
        Ok(Self::Named { path, body })
    }
}
