//! Parsing of the `make_aggregate!` DSL.

use syn::{
    braced,
    parse::{Parse, ParseStream},
    Ident, Token,
};

pub struct AggregateInput {
    pub roots: Vec<NodeDef>,
}

impl Parse for AggregateInput {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut roots = Vec::new();
        while !input.is_empty() {
            let name: Ident = input.parse()?;
            // Optional custom aggregate name: `Root as CustomName { ... }`.
            let alias = if input.peek(Token![as]) {
                input.parse::<Token![as]>()?;
                Some(input.parse::<Ident>()?)
            } else {
                None
            };
            let fields = parse_block(input)?;
            roots.push(NodeDef {
                name,
                alias,
                fields,
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
    /// Custom name for the generated aggregate (`Root as Alias`).
    pub alias: Option<Ident>,
    pub fields: Vec<FieldDef>,
}

pub struct FieldDef {
    pub name: Ident,
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
