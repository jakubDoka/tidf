use proc_macro::{TokenStream, Literal};
use quote::ToTokens;
use syn::{parenthesized, parse::Parse, spanned::Spanned, token, DeriveInput, Ident, LitInt};

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

    let result = match &input.data {
        syn::Data::Struct(data) => {
            let bound_checks = data.fields.iter().map(|field| {
                let ty = &field.ty;
                quote::quote_spanned! {ty.span()=>
                    const _: Option<BitwiseBoundCheck<#ty>> = None;
                }
            });
            let ser_body = data.fields.iter().enumerate().map(|(i, field)| {
                let ident = field
                    .ident
                    .clone()
                    .map(|i| i.to_token_stream())
                    .unwrap_or_else(|| syn::Index::from(i).to_token_stream());

                quote::quote! {
                    self.#ident.encode(buffer);
                }
            });
            let de_body = data.fields.iter().enumerate().map(|(i, field)| {
                let ident = field
                    .ident
                    .clone()
                    .map(|i| i.to_token_stream())
                    .unwrap_or_else(|| syn::Index::from(i).to_token_stream());
                quote::quote! {
                    self.#ident.decode(cursor, buffer)?;
                }
            });

            quote::quote! {
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
            }
        }
        syn::Data::Enum(data) => {
            let enc_code = data.variants.iter().enumerate().map(|(i, v)| {
                let ident = &v.ident;

                const U8MAX: usize = u8::MAX as usize;
                const U16MAX: usize = u16::MAX as usize;
                const U32MAX: usize = u32::MAX as usize;


                let i = LitInt::from(match data.variants.len() {
                    0..=U8MAX => {
                        Literal::u8_suffixed(i as u8)
                    }
                    0..=U16MAX => {
                        Literal::u16_suffixed(i as u16)
                    }
                    0..=U32MAX => {
                        Literal::u32_suffixed(i as u32)
                    }
                    _ => {
                        Literal::u64_suffixed(i as u64)
                    }
                });

                let encodes = v.fields.iter().enumerate().map(|(i, f)| {
                    let ident = f
                        .ident
                        .clone()
                        .unwrap_or_else(|| quote::format_ident!("f{}", i));
                    quote::quote! {
                        #ident.encode(buffer);
                    }
                });

                if v.fields.iter().any(|f| f.ident.is_some()) {
                    let names = v.fields.iter().map(|f| &f.ident);
                    quote::quote! {
                        Self::#ident { #(#names),* } => {
                            #i.encode(buffer);
                            #(#encodes)*
                        }
                    }
                } else {
                    if v.fields.len() == 0 {
                        quote::quote! {
                            Self::#ident => {
                                #i.encode(buffer);
                            }
                        }
                    } else {
                        let names = v
                            .fields
                            .iter()
                            .enumerate()
                            .map(|(i, _)| quote::format_ident!("f{}", i));
                        quote::quote! {
                            Self::#ident(#(#names),*) => {
                                #i.encode(buffer);
                                #(#encodes)*
                            }
                        }
                    }
                }
            });

            let dec_code = data.variants.iter().enumerate().map(|(i, v)| {
                let ident = &v.ident;

                let decodes = v.fields.iter().enumerate().map(|(i, f)| {
                    let ident = f
                        .ident
                        .clone()
                        .unwrap_or_else(|| quote::format_ident!("f{}", i));
                    let datatype = &f.ty;
                    quote::quote! {
                        let mut #ident = <#datatype>::default();
                        #ident.decode(cursor, buffer)?;
                    }
                });

                if v.fields.iter().any(|f| f.ident.is_some()) {
                    let names = v.fields.iter().map(|f| &f.ident);
                    quote::quote! {
                        #i => {
                            #(#decodes)*
                            *self = Self::#ident { #(#names),* };
                        }
                    }
                } else {
                    if v.fields.len() == 0 {
                        quote::quote! {
                            #i => {
                                *self = Self::#ident;
                            }
                        }
                    } else {
                        let names = v
                            .fields
                            .iter()
                            .enumerate()
                            .map(|(i, _)| quote::format_ident!("f{}", i));
                        quote::quote! {
                            #i => {
                                #(#decodes)*
                                *self = Self::#ident(#(#names),*);
                            }
                        }
                    }
                }
            });

            quote::quote! {
                impl Bitwise for #name {
                    fn encode(&self, buffer: &mut Vec<u8>) {
                        match self {
                            #(#enc_code)*
                        }
                    }

                    fn decode(&mut self, cursor: &mut usize, buffer: &[u8]) -> Option<()> {
                        let mut id: usize = 0;
                        id.decode(cursor, buffer)?;
                        match id {
                            #(#dec_code)*
                            _ => return None,
                        }

                        Some(())
                    }
                }
            }
        }
        syn::Data::Union(_) => panic!("union is not supported"),
    };

    TokenStream::from(result)
}
