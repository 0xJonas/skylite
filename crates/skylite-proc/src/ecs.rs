use proc_macro2::TokenStream;
use quote::{quote, format_ident, ToTokens};
use syn::{parse::Parser, parse2, punctuated::Punctuated, Expr, ExprClosure, Item, ItemEnum, ItemStruct, ItemUnion, Pat, Token};

use crate::SkyliteProcError;

fn check_closure_args(closure: &ExprClosure) -> Result<(), SkyliteProcError> {
    if closure.inputs.len() == 0 {
        return Err(SkyliteProcError::SyntaxError("System must take at least one parameter".to_owned()));
    }

    if closure.inputs.len() > 8 {
        return Err(SkyliteProcError::SyntaxError("Too many parameters for system, max 8 are allowed".to_owned()));
    }

    let mut types = Vec::new();
    for i in closure.inputs.iter() {
        match i {
            Pat::Type(ty) => {
                if types.contains(ty) {
                    return Err(SkyliteProcError::SyntaxError(format!("Duplicate component type in system: {}", ty.to_token_stream())));
                }
                types.push(ty.clone());
            },
            _ => {
                return Err(SkyliteProcError::SyntaxError("Parameters to a system must always have an explicit type annotation".to_owned()));
            }
        }
    }

    Ok(())
}

fn system_fallible(args: TokenStream) -> Result<TokenStream, SkyliteProcError> {
    let args = Parser::parse2(Punctuated::<Expr, Token![,]>::parse_separated_nonempty, args.clone())
        .map_err(|err| SkyliteProcError::SyntaxError(format!("Failed to parse arguments: {}", err.to_string())))?;
    if args.len() != 2 {
        return Err(SkyliteProcError::SyntaxError("system takes exactly to arguments.".to_owned()));
    }
    let receiver = &args[0];
    let closure = match &args[1] {
        Expr::Closure(c) => c,
        _ => {
            return Err(SkyliteProcError::SyntaxError("Second argument to system must be a closure.".to_owned()));
        }
    };

    check_closure_args(closure)?;

    let system_fn = format_ident!("system{}", closure.inputs.len());

    Ok(quote!(::skylite_core::ecs::_private::#system_fn(#receiver, #closure)))
}

pub(crate) fn system_impl(args: TokenStream) -> TokenStream {
    match system_fallible(args) {
        Ok(stream) => stream,
        Err(err) => err.into()
    }
}

pub(crate) fn derive_component_impl(item: TokenStream) -> TokenStream {
    let (typename, typeparams) = match parse2::<Item>(item) {
        Ok(Item::Struct(ItemStruct { ident, generics ,..})) => (ident, generics),
        Ok(Item::Enum(ItemEnum { ident, generics ,..})) => (ident, generics),
        Ok(Item::Union(ItemUnion { ident, generics ,..})) => (ident, generics),
        _ => todo!()
    };

    quote! {
        impl #typeparams ::skylite_core::actors::TypeId  for #typename #typeparams {
            fn get_id() -> usize {
                <#typename as ::skylite_core::actors::TypeId>::get_id as usize
            }
        }

        impl #typeparams ::skylite_core::ecs::Component  for #typename #typeparams {}
    }
}
