use proc_macro2::TokenStream;
use quote::{format_ident, quote, ToTokens};
use syn::parse::Parser;
use syn::punctuated::Punctuated;
use syn::{Expr, ExprClosure, Pat, Token};

use crate::SkyliteProcError;

fn check_closure_args(closure: &ExprClosure) -> Result<(), SkyliteProcError> {
    if closure.inputs.len() == 0 {
        return Err(syntax_err!("System must take at least one parameter"));
    }

    if closure.inputs.len() > 8 {
        return Err(syntax_err!(
            "Too many parameters for system, max 8 are allowed"
        ));
    }

    let mut types = Vec::new();
    for i in closure.inputs.iter() {
        match i {
            Pat::Type(ty) => {
                if types.contains(ty) {
                    return Err(syntax_err!(
                        "Duplicate node type in system: {}",
                        ty.to_token_stream()
                    ));
                }
                types.push(ty.clone());
            }
            _ => {
                return Err(syntax_err!(
                    "Parameters to a system must always have an explicit type annotation"
                ));
            }
        }
    }

    Ok(())
}

fn system_fallible(args: TokenStream) -> Result<TokenStream, SkyliteProcError> {
    let args = Parser::parse2(
        Punctuated::<Expr, Token![,]>::parse_separated_nonempty,
        args.clone(),
    )
    .map_err(|err| syntax_err!("Failed to parse arguments: {err}"))?;
    if args.len() != 2 {
        return Err(syntax_err!("system takes exactly two arguments."));
    }
    let receiver = &args[0];
    let closure = match &args[1] {
        Expr::Closure(c) => c,
        _ => {
            return Err(syntax_err!("Second argument to system must be a closure."));
        }
    };

    check_closure_args(closure)?;

    let system_fn = format_ident!("system{}", closure.inputs.len());

    Ok(quote!(::skylite_core::nodes::_private::#system_fn(#receiver, #closure)))
}

pub(crate) fn system_impl(args: TokenStream) -> TokenStream {
    match system_fallible(args) {
        Ok(stream) => stream,
        Err(err) => err.into(),
    }
}
