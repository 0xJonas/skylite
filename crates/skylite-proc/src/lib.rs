use std::path::PathBuf;
use std::str::FromStr;

use generate::nodes::generate_node_definition;
use generate::remove_annotations_from_items;
use generate::sequences::generate_sequence_definition;
use parse::guile::SCM;
use parse::project::SkyliteProject;
use parse::scheme_util::form_to_string;
use proc_macro2::TokenStream;
use quote::{quote, ToTokens};
use syn::parse::Parser;
use syn::punctuated::Punctuated;
use syn::{parse2, Expr, ExprLit, Item, ItemMod, Token};

macro_rules! syntax_err {
    ($msg:literal $(,$args:expr)*) => {
        SkyliteProcError::SyntaxError(format!($msg, $($args),*))
    };
}

macro_rules! data_err {
    ($msg:literal $(,$args:expr)*) => {
        SkyliteProcError::DataError(format!($msg, $($args),*))
    };
}

mod assets;
mod ecs;
mod generate;
mod parse;

use ecs::system_impl;

#[derive(Debug, Clone)]
enum SkyliteProcError {
    GuileException(SCM),
    DataError(String),
    SyntaxError(String),
    OtherError(String),
}

impl std::fmt::Display for SkyliteProcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::GuileException(scm) => write!(f, "Scheme Exception: {}", form_to_string(*scm)),
            Self::DataError(str) => write!(f, "Data Error: {}", str),
            Self::SyntaxError(str) => write!(f, "Syntax Error: {}", str),
            Self::OtherError(str) => write!(f, "Error: {}", str),
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

fn string_from_expr(expr: &Expr, err: SkyliteProcError) -> Result<String, SkyliteProcError> {
    if let Expr::Lit(ExprLit {
        lit: syn::Lit::Str(s),
        ..
    }) = expr
    {
        Ok(s.value())
    } else {
        Err(err)
    }
}

fn parse_project_file(expr: &Expr) -> Result<PathBuf, SkyliteProcError> {
    let path_raw = string_from_expr(
        expr,
        syntax_err!("Expected a string literal for project path"),
    )?;

    let base_dir = PathBuf::from_str(&std::env::var("CARGO_MANIFEST_DIR").unwrap()).unwrap();
    let relative_path =
        PathBuf::from_str(&path_raw).map_err(|err| syntax_err!("Invalid project path: {err}"))?;
    Ok(base_dir.join(relative_path))
}

fn get_crate_root_check() -> TokenStream {
    quote! {
        const _: () = {
            let expected = env!("CARGO_CRATE_NAME").as_bytes();
            let actual = module_path!().as_bytes();

            // Complicated string compare, because the == operator for str
            // is not const, as well as various other functions that might
            // have been more appropriate here.
            let max = if expected.len() > actual.len() {
                expected.len()
            } else {
                actual.len()
            };
            let mut i = 0;
            while i < max {
                if i >= expected.len() || i >= actual.len() || expected[i] != actual[i] {
                    panic!("skylite_project! can only be called at the crate root.");
                }
                i += 1;
            }
        };
    }
}

// Matching endianness is required because various functions
// for decoding compressed data assume that they are stored in native
// endianness. Endianness is configured via a feature for skylite_proc. Because
// skylite_proc is a proc-macro crate, it does not have access to the final
// compilation target and thus does not know the correct endianness.

#[cfg(feature = "big-endian")]
fn get_endianness_check() -> TokenStream {
    quote! {
        #[cfg(not(target_endian = "big"))]
        const _: () = panic!(
            "Wrong endianness configured for skylite_proc. "
            "Remove feature \"big-endian\" from skylite_proc dependency or use a big-endian target."
        );
    }
}

#[cfg(not(feature = "big-endian"))]
fn get_endianness_check() -> TokenStream {
    quote! {
        #[cfg(target_endian = "big")]
        const _: () = panic!(
            "Wrong endianness configured for skylite_proc. "
            "Add feature \"big-endian\" to skylite_proc dependency or use a little-endian target."
        );
    }
}

fn skylite_project_impl_fallible(
    args_raw: TokenStream,
    body_raw: TokenStream,
) -> Result<TokenStream, SkyliteProcError> {
    let args = Punctuated::<Expr, Token![,]>::parse_separated_nonempty.parse2(args_raw).map_err(|err| {
        syntax_err!(
            "Failed to parse #[skylite_project(...)] arguments: {err}. Expected (\"project-path\", target_type)"
        )
    })?;

    if args.len() != 2 {
        return Err(syntax_err!(
            "Wrong number of arguments to #[skylite_project(...)], expected (\"project-path\", target_type)"
        ));
    }

    let path = parse_project_file(&args[0])?;

    let mut module = parse2::<ItemMod>(body_raw)
        .map_err(|err| SkyliteProcError::SyntaxError(err.to_string()))?;

    let items = &mut module
        .content
        .as_mut()
        .ok_or(data_err!("skylite_project! module must have a body"))?
        .1;

    // Verify that the content of target_type is actually a valid path.
    let target_type = match &args[1] {
        Expr::Path(path) => &path.path,
        _ => {
            return Err(syntax_err!("target_type must be a type."));
        }
    };

    let mut project = SkyliteProject::from_file(&path, true)?;

    let mut project_items = project.generate(target_type, &items)?;
    remove_annotations_from_items(items);
    items.append(&mut project_items);

    let crate_root_check = get_crate_root_check();
    let endianness_check = get_endianness_check();

    let out = quote! {
        #crate_root_check
        #endianness_check

        #module
    };

    Ok(out)
}

fn extract_asset_file(
    asset_file: &TokenStream,
) -> Result<(SkyliteProject, String), SkyliteProcError> {
    let args = Parser::parse2(
        Punctuated::<Expr, Token![,]>::parse_separated_nonempty,
        asset_file.clone(),
    )
    .map_err(|err| {
        syntax_err!(
            "Failed to parse asset_file! macro: {err}. Expected (\"project-path\", \"asset-name\")"
        )
    })?;

    if args.len() != 2 {
        return Err(syntax_err!(
            "Wrong number of arguments to asset_file!, expected (\"project-path\", \"asset-name\")"
        ));
    }

    let project_root = parse_project_file(&args[0])?;
    let stub = SkyliteProject::from_file(&project_root, false)?;

    let asset_name = string_from_expr(
        &args[1],
        syntax_err!("Expected a string literal for asset name"),
    )?;

    return Ok((stub, asset_name));
}

fn node_definition_fallible(
    args_raw: TokenStream,
    body_raw: TokenStream,
) -> Result<TokenStream, SkyliteProcError> {
    let mut module = parse2::<ItemMod>(body_raw.clone())
        .map_err(|err| SkyliteProcError::SyntaxError(err.to_string()))?;

    let items = &mut module
        .content
        .as_mut()
        .ok_or(data_err!("Node definition module must have a body"))?
        .1;

    let (mut project, name) = extract_asset_file(&args_raw)?;
    let node = project.assets.load_node(&name)?;

    let tokens = generate_node_definition(&node, &project.name, &items)?;
    remove_annotations_from_items(items);
    items.push(syn::Item::Verbatim(tokens));

    Ok(module.into_token_stream())
}

fn sequence_definition_fallible(
    args_raw: TokenStream,
    body_raw: TokenStream,
) -> Result<TokenStream, SkyliteProcError> {
    let mut module = parse2::<ItemMod>(body_raw.clone())
        .map_err(|err| SkyliteProcError::SyntaxError(err.to_string()))?;
    let items = &mut module
        .content
        .as_mut()
        .ok_or(data_err!("Node definition module must have a body"))?
        .1;

    let (mut project, name) = extract_asset_file(&args_raw)?;
    let sequence = project.assets.load_sequence(&name)?;

    let tokens = Item::Verbatim(generate_sequence_definition(
        &sequence,
        &project.name,
        &items,
    )?);
    remove_annotations_from_items(items);
    items.push(tokens);

    Ok(module.into_token_stream())
}

fn skylite_project_impl(args_raw: TokenStream, body_raw: TokenStream) -> TokenStream {
    match skylite_project_impl_fallible(args_raw, body_raw) {
        Ok(t) => t,
        Err(err) => err.into(),
    }
}

fn node_definition_impl(args_raw: TokenStream, body_raw: TokenStream) -> TokenStream {
    match node_definition_fallible(args_raw, body_raw) {
        Ok(stream) => stream,
        Err(err) => err.into(),
    }
}

fn sequence_definition_impl(args_raw: TokenStream, body_raw: TokenStream) -> TokenStream {
    match sequence_definition_fallible(args_raw, body_raw) {
        Ok(stream) => stream,
        Err(err) => err.into(),
    }
}

#[proc_macro_attribute]
pub fn skylite_project(
    args: proc_macro::TokenStream,
    body: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    skylite_project_impl(args.into(), body.into()).into()
}

#[proc_macro_attribute]
pub fn node_definition(
    args: proc_macro::TokenStream,
    body: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    node_definition_impl(args.into(), body.into()).into()
}

#[proc_macro_attribute]
pub fn sequence_definition(
    args: proc_macro::TokenStream,
    body: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    sequence_definition_impl(args.into(), body.into()).into()
}

#[proc_macro]
pub fn system(args: proc_macro::TokenStream) -> proc_macro::TokenStream {
    system_impl(args.into()).into()
}
