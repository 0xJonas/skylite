use proc_macro2::{Literal, TokenStream};
use quote::{format_ident, quote, ToTokens};
use syn::{Item, ItemFn, Meta};

use super::project::project_ident;
use crate::generate::nodes::node_type_name;
use crate::parse::util::{change_case, IdentCase};
use crate::parse::values::{Type, TypedValue, Variable};

/// Returns the function item annotated with the given `attribute` from the list
/// of `items`.
///
/// The attribute must be of the form `#[attribute-name]`.
pub(crate) fn get_annotated_function<'a>(items: &'a [Item], attribute: &str) -> Option<&'a ItemFn> {
    let meta = syn::parse_str::<Meta>(attribute).unwrap();
    items
        .iter()
        // Find item with matching attribute
        .find(|item| {
            if let Item::Fn(fun) = item {
                fun.attrs.iter().any(|attr| attr.meta == meta)
            } else {
                false
            }
        })
        // Unpack generic item as function item
        .map(|item| {
            if let Item::Fn(fun) = item {
                fun
            } else {
                panic!("Expected function item")
            }
        })
}

/// Generates a `TokenStream` of the form `var1: type1, var2: type2:, ...` from
/// a list of `Variables`. Can be used for parameter lists and struct members.
pub(crate) fn generate_field_list(params: &[Variable], prefix: TokenStream) -> TokenStream {
    let param_names = params
        .iter()
        .map(|p| format_ident!("{}", change_case(&p.name, IdentCase::LowerSnakeCase)));
    let param_types = params.iter().map(|p| skylite_type_to_rust(&p.typename));
    quote! {
        #(#prefix #param_names: #param_types),*
    }
}

/// Converts a `Type` to the corresponding owned Rust type.
pub(crate) fn skylite_type_to_rust(t: &Type) -> TokenStream {
    match t {
        Type::U8 => quote!(u8),
        Type::U16 => quote!(u16),
        Type::U32 => quote!(u32),
        Type::U64 => quote!(u64),
        Type::I8 => quote!(i8),
        Type::I16 => quote!(i16),
        Type::I32 => quote!(i32),
        Type::I64 => quote!(i64),
        Type::F32 => quote!(f32),
        Type::F64 => quote!(f64),
        Type::Bool => quote!(bool),
        Type::String => quote!(String),
        Type::Tuple(member_types) => {
            let member_types_tokens = member_types.iter().map(skylite_type_to_rust);
            quote!((#(#member_types_tokens),*))
        }
        Type::Vec(item_type) => {
            let item_type_tokens = skylite_type_to_rust(&item_type);
            quote!(Vec<#item_type_tokens>)
        }
        Type::NodeList => quote!(::skylite_core::nodes::NodeList),
        Type::Node(name) => node_type_name(name.as_str()).to_token_stream(),
    }
}

/// Generates a list of statements of the form `let <name> =
/// <type>::deserialize(decoder);`. Can be used as a building block for decode
/// functions.
pub(crate) fn generate_deserialize_statements(args: &[Variable]) -> TokenStream {
    let statements = args.iter().map(|v| {
        let t = skylite_type_to_rust(&v.typename);
        let ident = format_ident!("{}", change_case(&v.name, IdentCase::LowerSnakeCase));
        quote!(let #ident = #t::deserialize(decoder);)
    });
    quote!(#(#statements)*)
}

/// Generates a comma-separated list of identifiers from the list of
/// `Variables`.
pub(crate) fn generate_argument_list(args: &[Variable]) -> TokenStream {
    let idents = args
        .iter()
        .map(|v| format_ident!("{}", change_case(&v.name, IdentCase::LowerSnakeCase)));
    quote!(#(#idents),*)
}

pub(crate) fn typed_value_to_rust(val: &TypedValue, project_name: &str) -> TokenStream {
    match val {
        TypedValue::U8(v) => Literal::u8_suffixed(*v).into_token_stream(),
        TypedValue::U16(v) => Literal::u16_suffixed(*v).into_token_stream(),
        TypedValue::U32(v) => Literal::u32_suffixed(*v).into_token_stream(),
        TypedValue::U64(v) => Literal::u64_suffixed(*v).into_token_stream(),
        TypedValue::I8(v) => Literal::i8_suffixed(*v).into_token_stream(),
        TypedValue::I16(v) => Literal::i16_suffixed(*v).into_token_stream(),
        TypedValue::I32(v) => Literal::i32_suffixed(*v).into_token_stream(),
        TypedValue::I64(v) => Literal::i64_suffixed(*v).into_token_stream(),
        TypedValue::F32(v) => Literal::f32_suffixed(*v).into_token_stream(),
        TypedValue::F64(v) => Literal::f64_suffixed(*v).into_token_stream(),
        TypedValue::Bool(v) => {
            if *v {
                quote!(true)
            } else {
                quote!(false)
            }
        }
        TypedValue::String(v) => {
            let lit = Literal::string(v);
            quote!(String::from(#lit))
        }
        TypedValue::Tuple(vec) => {
            let members = vec
                .iter()
                .map(|item| typed_value_to_rust(item, project_name));
            quote!((#(#members),*))
        }
        TypedValue::Vec(vec) => {
            let members = vec
                .iter()
                .map(|item| typed_value_to_rust(item, project_name));
            quote!(vec![#(#members),*])
        }
        TypedValue::Node(instance) => {
            let args = instance
                .args
                .iter()
                .map(|item| typed_value_to_rust(item, project_name));
            let name = node_type_name(&instance.name);
            quote!(#name::new(#(#args),*))
        }
        TypedValue::NodeList(id) => {
            let project_ident = project_ident(project_name);
            quote!(#project_ident::_private_decode_node_list(#id))
        }
    }
}

pub(crate) fn validate_type(skylite_type: &Type, rust_type: &syn::Type) -> bool {
    match skylite_type {
        Type::U8 => matches!(rust_type, syn::Type::Path(p) if p.path.is_ident("u8")),
        Type::U16 => matches!(rust_type, syn::Type::Path(p) if p.path.is_ident("u16")),
        Type::U32 => matches!(rust_type, syn::Type::Path(p) if p.path.is_ident("u32")),
        Type::U64 => matches!(rust_type, syn::Type::Path(p) if p.path.is_ident("u64")),
        Type::I8 => matches!(rust_type, syn::Type::Path(p) if p.path.is_ident("i8")),
        Type::I16 => matches!(rust_type, syn::Type::Path(p) if p.path.is_ident("i16")),
        Type::I32 => matches!(rust_type, syn::Type::Path(p) if p.path.is_ident("i32")),
        Type::I64 => matches!(rust_type, syn::Type::Path(p) if p.path.is_ident("i64")),
        Type::F32 => matches!(rust_type, syn::Type::Path(p) if p.path.is_ident("f32")),
        Type::F64 => matches!(rust_type, syn::Type::Path(p) if p.path.is_ident("f64")),
        Type::Bool => matches!(rust_type, syn::Type::Path(p) if p.path.is_ident("bool")),

        // These are technically not 100% correct, since String and Node could be written as their
        // fully-qualified names, or their names could be changed in the `use`-statement.
        // TODO: Improve somehow
        Type::String => matches!(rust_type, syn::Type::Path(p) if p.path.is_ident("String")),
        Type::Node(name) => {
            matches!(rust_type, syn::Type::Path(p) if p.path.is_ident(&change_case(name, IdentCase::UpperCamelCase)))
        }
        _ => unimplemented!(),
    }
}
