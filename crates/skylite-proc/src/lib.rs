use std::{path::PathBuf, str::FromStr};

use quote::{quote, ToTokens};
use parse::{guile::SCM, project::SkyliteProjectStub};
use parse::scheme_util::form_to_string;
use proc_macro2::TokenStream;
use parse::project::SkyliteProject;
use syn::{parse::Parser, parse2, punctuated::Punctuated, Expr, ExprLit, ExprPath, Item, ItemMod, Lit, Path as SynPath, Token};

mod parse;
mod generate;

#[derive(Debug, Clone)]
enum SkyliteProcError {
    GuileException(SCM),
    DataError(String),
    SyntaxError(String),
    OtherError(String)
}

impl std::fmt::Display for SkyliteProcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::GuileException(scm) => write!(f, "Scheme Exception: {}", form_to_string(*scm)),
            Self::DataError(str) => write!(f, "Data Error: {}", str),
            Self::SyntaxError(str) => write!(f, "Syntax Error: {}", str),
            Self::OtherError(str) => write!(f, "Error: {}", str)
        }
    }
}

impl Into<TokenStream> for SkyliteProcError {
    fn into(self) -> TokenStream {
        let msg = self.to_string();
        quote! {
            std::compile_error!(#msg);
        }
    }
}

struct SkyliteProjectArgs {
    path: PathBuf,
    target: SynPath,
    #[cfg(debug_assertions)]
    debug_out: Option<PathBuf>
}

fn parse_skylite_project_args(args_raw: TokenStream) -> Result<SkyliteProjectArgs, SkyliteProcError> {
    let parser = Punctuated::<Expr, Token![,]>::parse_terminated;
    let mut iter = parser.parse2(args_raw)
        .map_err(|err| SkyliteProcError::SyntaxError(err.to_string()))?
        .into_iter();

    // Extract path to project root file
    let path_str = match iter.next() {
        Some(
            Expr::Lit(ExprLit {
                lit: Lit::Str(lit),
                ..
            }
        )) => lit.value(),
        _ => return Err(SkyliteProcError::SyntaxError("Expected project path".to_owned()))
    };
    let base_dir = PathBuf::from_str(&std::env::var("CARGO_MANIFEST_DIR").unwrap()).unwrap();
    let relative_path = match PathBuf::from_str(&path_str) {
        Ok(p) => p,
        Err(e) => return Err(SkyliteProcError::SyntaxError("Project path invalid".to_owned() + &e.to_string()))
    };

    // Extract type of the Skylite target
    let target = match iter.next() {
        Some(
            Expr::Path(ExprPath {
                path,
                ..
            })
        ) => path,
        _ => return Err(SkyliteProcError::SyntaxError("Expected target type".to_owned()))
    };

    #[cfg(debug_assertions)]
    {
        let debug_out = match iter.next() {
            Some(
                Expr::Lit(ExprLit {
                    lit: Lit::Str(lit),
                    ..
                }
            )) => {
                let debug_out_str = lit.value();
                let relative_path = match PathBuf::from_str(&debug_out_str) {
                    Ok(p) => p,
                    Err(e) => return Err(SkyliteProcError::SyntaxError("Debug path invalid".to_owned() + &e.to_string()))
                };
                Some(base_dir.join(relative_path))
            },
            Some(_) => return Err(SkyliteProcError::SyntaxError("Invalid value for debug_out".to_owned())),
            None => None
        };
        Ok(SkyliteProjectArgs { path: base_dir.join(relative_path), target: target.clone(), debug_out })
    }

    #[cfg(not(debug_assertions))]
    {
        Ok(SkyliteProjectArgs { path: base_dir.join(relative_path), target: target.clone() })
    }
}

fn get_default_imports() -> Item {
    Item::Verbatim(
        quote! {
            use skylite_core::SkyliteTarget;
            use skylite_core;
        }
    )
}

fn skylite_project_impl(args_raw: TokenStream, body_raw: TokenStream) -> TokenStream {
    let mut body_parsed: ItemMod = match parse2(body_raw) {
        Ok(ast) => ast,
        Err(err) => return SkyliteProcError::SyntaxError(err.to_string()).into()
    };

    let args = match parse_skylite_project_args(args_raw) {
        Ok(args) => args,
        Err(err) => return err.into()
    };

    let project_stub = match SkyliteProjectStub::from_file(&args.path) {
        Ok(stub) => stub,
        Err(err) => return err.into()
    };
    let project = match SkyliteProject::from_stub(project_stub) {
        Ok(project) => project,
        Err(err) => return err.into()
    };

    let mut items = match project.generate(&args.target.into_token_stream(), &body_parsed) {
        Ok(items) => items,
        Err(err) => return err.into()
    };

    body_parsed.content.as_mut().unwrap().1.insert(0, get_default_imports());
    body_parsed.content.as_mut().unwrap().1.append(&mut items);

    let out = body_parsed.into_token_stream();

    #[cfg(debug_assertions)]
    {
        if let Some(debug_out) = args.debug_out {
            std::fs::write(debug_out, out.to_string()).unwrap();
        }
    }

    out
}

#[proc_macro_attribute]
pub fn skylite_project(args: proc_macro::TokenStream, body: proc_macro::TokenStream) -> proc_macro::TokenStream {
    skylite_project_impl(args.into(), body.into()).into()
}

#[proc_macro_attribute]
pub fn init(_args: proc_macro::TokenStream, body: proc_macro::TokenStream) -> proc_macro::TokenStream { body }

#[proc_macro_attribute]
pub fn pre_update(_args: proc_macro::TokenStream, body: proc_macro::TokenStream) -> proc_macro::TokenStream { body }

#[proc_macro_attribute]
pub fn post_update(_args: proc_macro::TokenStream, body: proc_macro::TokenStream) -> proc_macro::TokenStream { body }

#[proc_macro_attribute]
pub fn pre_render(_args: proc_macro::TokenStream, body: proc_macro::TokenStream) -> proc_macro::TokenStream { body }

#[proc_macro_attribute]
pub fn post_render(_args: proc_macro::TokenStream, body: proc_macro::TokenStream) -> proc_macro::TokenStream { body }
