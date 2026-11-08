//! Parsing of the `make_aggregate!` DSL.

use quote::ToTokens;
use syn::{
    braced,
    parse::{Parse, ParseStream},
    Attribute, Ident, Meta, Token, Type,
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
    /// Custom name for the generated aggregate (`#[alias = Alias]`).
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
    pub ty: TypeDef,
}

pub enum TypeDef {
    /// A simple type name, optionally followed by a nested block.
    Named {
        name: Ident,
        body: Option<Vec<FieldDef>>,
    },
    VecOf(Box<TypeDef>),
    SetOf(Box<TypeDef>),
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
                set_once(&mut alias, attr, parse_value::<Ident>(attr)?)?;
            } else if attr.path().is_ident("emits") {
                set_once(&mut event, attr, parse_value::<Type>(attr)?)?;
            } else if attr.path().is_ident("fails") {
                set_once(&mut error, attr, parse_value::<Type>(attr)?)?;
            } else {
                return Err(syn::Error::new_spanned(
                    attr,
                    "expected `#[alias = Name]`, `#[emits = EventType]` or `#[fails = ErrorType]`",
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

/// Parses the attributes allowed before a field: only `#[rootless]`.
fn parse_field_attrs(attrs: &[Attribute]) -> syn::Result<bool> {
    let mut rootless = false;
    for attr in attrs {
        if !attr.path().is_ident("rootless") || !matches!(attr.meta, Meta::Path(_)) {
            return Err(syn::Error::new_spanned(
                attr,
                "expected `#[rootless]` on a field",
            ));
        }
        if rootless {
            return Err(syn::Error::new_spanned(attr, "duplicate attribute"));
        }
        rootless = true;
    }
    Ok(rootless)
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
        let rootless = parse_field_attrs(&attrs)?;
        let name: Ident = content.parse()?;
        content.parse::<Token![:]>()?;
        let ty = TypeDef::parse(&content)?;
        fields.push(FieldDef { name, rootless, ty });
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
            "Vec" => Ok(Self::VecOf(Box::new(Self::parse_generic_arg(input)?))),
            "HashSet" => Ok(Self::SetOf(Box::new(Self::parse_generic_arg(input)?))),
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

    /// Parses `<Inner>` after a container name.
    fn parse_generic_arg(input: ParseStream) -> syn::Result<Self> {
        input.parse::<Token![<]>()?;
        let inner = Self::parse(input)?;
        input.parse::<Token![>]>()?;
        Ok(inner)
    }
}
