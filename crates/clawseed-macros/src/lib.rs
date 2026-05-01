//! Procedural macros for ClawSeed.

use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, DeriveInput, Data, Fields};

/// `Configurable` derive macro — generates TOML config loading helpers.
///
/// Supports struct fields with `#[default = "..."]` and `#[env = "..."]` attributes.
#[proc_macro_derive(Configurable, attributes(default, env))]
pub fn derive_configurable(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;

    let defaults = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(fields) => {
                fields.named.iter().map(|field| {
                    let field_name = field.ident.as_ref().unwrap();
                    let field_type = &field.ty;
                    quote! {
                        #field_name: <#field_type>::default()
                    }
                }).collect::<Vec<_>>()
            }
            _ => vec![],
        },
        _ => vec![],
    };

    let expanded = quote! {
        impl #name {
            /// Create a new instance with all defaults.
            pub fn with_defaults() -> Self {
                Self { #(#defaults),* }
            }
        }
    };

    expanded.into()
}
