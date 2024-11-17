use proc_macro2::{Literal, TokenStream};
use quote::{format_ident, quote, ToTokens};
use syn::{parse_str, Item, ItemFn, Macro, Meta, Path};

use crate::{parse::{util::{change_case, IdentCase}, values::{Type, TypedValue, Variable}}, SkyliteProcError};

/// Returns the function item annotated with the given `attribute` from the list of `items`.
///
/// The attribute must be of the form `#[attribute-name]`.
pub(crate) fn get_annotated_function<'a>(items: &'a [Item], attribute: &str) -> Option<&'a ItemFn> {
    let attribute_path = syn::parse_str::<Path>(attribute).unwrap();
    items.iter()
        // Find item with matching attribute
        .find(|item| if let Item::Fn(fun) = item {
            fun.attrs.iter()
                .any(|attr| if let Meta::Path(ref p) = attr.meta { *p == attribute_path } else { false })
        } else {
            false
        })
        // Unpack generic item as function item
        .map(|item| if let Item::Fn(fun) = item {
            fun
        } else {
            panic!("Expected function item")
        })
}

/// Returns a function macro invocation with the given `name` from the list of `items`.
///
/// If no invocation with the given `name` is found, `Ok(None)` is returned. If multiple
/// invocations are found, an `Err` is returned.
pub(crate) fn get_macro_item<'mac>(name: &str, items: &'mac [Item]) -> Result<Option<&'mac Macro>, SkyliteProcError> {
    let name_parsed = parse_str::<syn::Path>(name).unwrap();
    let mut definitions_iter = items.iter()
        .filter_map(|item| if let Item::Macro(m) = item {
            Some(m)
        } else {
            None
        })
        .filter(|m| m.mac.path == name_parsed);

    let out = match definitions_iter.next() {
        Some(def) => &def.mac,
        None => return Ok(None)
    };
    match definitions_iter.next() {
        None => Ok(Some(out)),
        Some(_) => Err(SkyliteProcError::SyntaxError(format!("Multiple macro invocations for {}!", name)))
    }
}

/// Converts a `Type` to the corresponding Rust type.
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
        },
        Type::Vec(item_type) => {
            let item_type_tokens = skylite_type_to_rust(&item_type);
            quote!(Vec<#item_type_tokens>)
        }
    }
}

/// Generates a `TokenStream` of the form `var1: type1, var2: type2:, ...` from a list of `Variables`.
pub(crate) fn generate_param_list(params: &[Variable]) -> TokenStream {
    let param_names = params.iter().map(|p| format_ident!("{}", change_case(&p.name, IdentCase::LowerSnakeCase)));
    let param_types = params.iter().map(|p| skylite_type_to_rust(&p.typename));
    quote! {
        #(#param_names: #param_types),*
    }
}

pub(crate) fn typed_value_to_rust(val: &TypedValue) -> TokenStream {
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
        TypedValue::Bool(v) => if *v { quote!(true) } else { quote!(false) }
        TypedValue::String(v) => {
            let lit = Literal::string(v);
            quote!(String::from(#lit))
        },
        TypedValue::Tuple(vec) => {
            let members = vec.iter().map(typed_value_to_rust);
            quote!((#(#members),*))
        },
        TypedValue::Vec(vec) => {
            let members = vec.iter().map(typed_value_to_rust);
            quote!(vec![#(#members),*])
        },
    }
}