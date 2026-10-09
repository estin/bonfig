//! The `#[bonfig::settings]` attribute. Use it through the `bonfig` crate,
//! because the generated code refers to `::bonfig`.

use proc_macro::TokenStream;
use proc_macro2::TokenTree;
use quote::quote;
use syn::{Attribute, Data, DeriveInput, Fields, Meta, Type, parse_macro_input, parse_quote};

/// Makes a struct a bonfig configuration struct. See the `bonfig` docs.
#[proc_macro_attribute]
pub fn settings(args: TokenStream, item: TokenStream) -> TokenStream {
    let mut input = parse_macro_input!(item as DeriveInput);
    if !args.is_empty() {
        return error(&input, "#[bonfig::settings] takes no arguments");
    }
    let Data::Struct(data) = &mut input.data else {
        return error(&input, "#[bonfig::settings] supports only structs");
    };
    let Fields::Named(fields) = &mut data.fields else {
        return error(
            &input,
            "#[bonfig::settings] supports only structs with named fields",
        );
    };
    for field in &mut fields.named {
        if !is_option(&field.ty) && !has_builder_value(&field.attrs) {
            field.attrs.push(parse_quote!(#[builder(default)]));
        }
    }

    let attrs = std::mem::take(&mut input.attrs);
    let name = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();
    quote! {
        // The derive comes first: helper attributes must follow it.
        #[derive(::bonfig::serde::Serialize, ::bonfig::serde::Deserialize, ::bonfig::bon::Builder)]
        #[serde(default, crate = "::bonfig::serde")]
        #[builder(crate = ::bonfig::bon, on(String, into))]
        #(#attrs)*
        #input

        impl #impl_generics ::core::default::Default for #name #ty_generics #where_clause {
            fn default() -> Self {
                Self::builder().build()
            }
        }
    }
    .into()
}

fn error(input: &DeriveInput, message: &str) -> TokenStream {
    syn::Error::new_spanned(&input.ident, message)
        .to_compile_error()
        .into()
}

/// bon makes `Option` fields optional by itself.
fn is_option(ty: &Type) -> bool {
    matches!(ty, Type::Path(path)
        if path.qself.is_none()
            && path.path.segments.last().is_some_and(|segment| segment.ident == "Option"))
}

/// True when a `#[builder(...)]` attribute already gives the field its value.
fn has_builder_value(attrs: &[Attribute]) -> bool {
    attrs
        .iter()
        .filter(|attr| attr.path().is_ident("builder"))
        .any(|attr| {
            let Meta::List(list) = &attr.meta else {
                return false;
            };
            list.tokens.clone().into_iter().any(|token| {
                matches!(token, TokenTree::Ident(ident)
                    if ident == "default" || ident == "skip" || ident == "field")
            })
        })
}
