use proc_macro::TokenStream;
use syn::{parenthesized, parse::Parse, token, DeriveInput, Ident, spanned::Spanned};

struct ParserAttr {
    _paren: token::Paren,
    ident: Ident,
}

impl Parse for ParserAttr {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let content;
        Ok(Self {
            _paren: parenthesized!(content in input),
            ident: content.parse()?,
        })
    }
}

#[proc_macro_derive(Meta, attributes(meta_parser, meta_required))]
pub fn meta_derive(input: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(input as DeriveInput);
    let attribute = input
        .attrs
        .iter()
        .find(|attr| {
            attr.path.segments.len() == 1
                && attr
                    .path
                    .segments
                    .first()
                    .unwrap()
                    .ident
                    .to_string()
                    .as_str()
                    == "meta_parser"
        })
        .expect("The meta_parser top attribute is missing.")
        .tokens
        .clone()
        .into();
    let parser = syn::parse_macro_input!(attribute as ParserAttr).ident;

    let name = &input.ident;

    let body = match &input.data {
        syn::Data::Struct(data) => {
            data.fields.iter().map(|field| {

                let ident = &field.ident;
                if field.attrs.iter().any(|attr|
                    attr.path.segments.len() == 1 &&
                    attr.path.segments.first().unwrap().ident.to_string().as_str() == "meta_required"
                ) {
                    quote::quote! {
                        let field = util::meta_data::extract_field(&mut map, stringify!(#ident))
                            .ok_or_else(|| format!("missing required field: {}", stringify!(#ident)))?;
                        self.deserialize_into(state, field)
                            .map_err(|err| format!("inside {}: {}", stringify!(#ident), err))?;
                    }
                } else {
                    quote::quote! {
                        if let Some(field) = util::meta_data::extract_field(&mut map, stringify!(#ident)) {
                            self.deserialize_into(state, field)
                                .map_err(|err| format!("inside {}: {}", stringify!(#ident), err))?;
                        }
                    }
                }
            })
        },
        syn::Data::Enum(_) => panic!("enum is not supported yet"),
        syn::Data::Union(_) => panic!("union is not supported"),
    };

    let result = quote::quote! {
        impl util::meta_data::Deserialize<#parser> for #name {
            fn deserialize_into(&mut self, state: &mut #parser, node: util::meta_data::Yaml) -> Result<(), String> {
                match node {
                    util::meta_data::Yaml::Mapping(mut map) => {
                        #(#body)*
                        Ok(())
                    }
                    _ => Err(format!("expected mapping, got {:?}", node)),
                }
            }
        }
    };

    TokenStream::from(result)
}

#[proc_macro_derive(Bitwise)]
pub fn bitwise_derive(input: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(input as DeriveInput);

    let name = &input.ident;

    let (ser_body, de_body, bound_checks) = match &input.data {
        syn::Data::Struct(data) => {
            let bound_checks = data.fields.iter().map(|field| {
                let ty = &field.ty;
                quote::quote_spanned! {ty.span()=> 
                    const _: Option<BitwiseBoundCheck<#ty>> = None;
                }
            });
            let ser_body = data.fields.iter().map(|field| {
                let ident = &field.ident;
                
                quote::quote! {
                    self.#ident.encode(buffer);
                }
            });
            let de_body = data.fields.iter().map(|field| {
                let ident = &field.ident;
                quote::quote! {
                    self.#ident.decode(cursor, buffer)?;
                }
            });
            (ser_body, de_body, bound_checks)
        }
        syn::Data::Enum(_) => panic!("enum is not supported yet"),
        syn::Data::Union(_) => panic!("union is not supported"),
    };

    let result = quote::quote! {
        #(#bound_checks)*
        impl Bitwise for #name {
            fn encode(&self, buffer: &mut Vec<u8>) {
                #(#ser_body)*
            }

            fn decode(&mut self, cursor: &mut usize, buffer: &[u8]) -> Option<()> {
                #(#de_body)*

                Some(())
            }
        }
    };

    TokenStream::from(result)
}
