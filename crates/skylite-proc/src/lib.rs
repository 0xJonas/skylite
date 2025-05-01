use std::path::PathBuf;
use std::str::FromStr;

use generate::nodes::generate_node_definition;
use generate::sequences::generate_sequence_definition;
use generate::util::get_macro_item;
use parse::guile::SCM;
use parse::nodes::Node;
use parse::project::{SkyliteProject, SkyliteProjectStub};
use parse::scheme_util::form_to_string;
use parse::sequences::SequenceStub;
use parse::util::{change_case, IdentCase};
use proc_macro2::{TokenStream, TokenTree};
use quote::{format_ident, quote};
use syn::parse::Parser;
use syn::punctuated::Punctuated;
use syn::{parse2, File, Item, LitStr, Token};

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

fn parse_project_file(tokens: &TokenStream) -> Result<PathBuf, SkyliteProcError> {
    let path_raw = parse2::<LitStr>(tokens.clone())
        .map(|lit| lit.value())
        .map_err(|err| syntax_err!("Illegal arguments to project_file!: {}", err))?;

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

fn skylite_project_impl_fallible(body_raw: TokenStream) -> Result<TokenStream, SkyliteProcError> {
    let items = parse2::<File>(body_raw)
        .map_err(|err| SkyliteProcError::SyntaxError(err.to_string()))?
        .items;

    let project_file_mac = get_macro_item("skylite_proc::project_file", &items)?.ok_or(
        data_err!("Missing required macro skylite_proc::project_file!"),
    )?;
    let path = parse_project_file(project_file_mac)?;

    let target_type_mac = get_macro_item("skylite_proc::target_type", &items)?.ok_or(data_err!(
        "Missing required macro skylite_proc::target_type!"
    ))?;
    // Verify that the content of target_type is actually a valid path.
    parse2::<syn::Path>(target_type_mac.clone())
        .map_err(|err| SkyliteProcError::SyntaxError(err.to_string()))?;

    let project_stub = SkyliteProjectStub::from_file(&path)?;
    let project = SkyliteProject::from_stub(project_stub)?;

    let module_name = format_ident!("{}", change_case(&project.name, IdentCase::LowerSnakeCase));
    let project_items = project.generate(&target_type_mac, &items)?;

    let crate_root_check = get_crate_root_check();
    let endianness_check = get_endianness_check();

    let out = quote! {
        #crate_root_check
        #endianness_check

        mod #module_name {
            pub mod generated {
                use ::skylite_core::prelude::*;
                use super::*;

                #(#project_items)
                *
            }

            use ::skylite_core::prelude::*;
            use generated::*;

            #(#items)
            *
        }

        pub use #module_name::generated::*;
    };

    #[cfg(debug_assertions)]
    {
        process_debug_output(&out, &items)?;
    }

    Ok(out)
}

fn extract_asset_file(
    asset_file: &TokenStream,
) -> Result<(SkyliteProjectStub, String), SkyliteProcError> {
    let args = Parser::parse2(
        Punctuated::<LitStr, Token![,]>::parse_separated_nonempty,
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

    let relative_path = PathBuf::try_from(args[0].value())
        .map_err(|_| data_err!("Not a valid project path: {}", args[0].value()))?;

    let base_dir = PathBuf::from_str(&std::env::var("CARGO_MANIFEST_DIR").unwrap()).unwrap();
    let stub = SkyliteProjectStub::from_file(&base_dir.join(relative_path))?;

    return Ok((stub, args[1].value()));
}

/// Implements the `skylite_proc::debug_output!` macro. This macro takes a
/// string literal representing a file as an argument and writes the generated
/// code to that file.
#[cfg(debug_assertions)]
fn process_debug_output(out: &TokenStream, items: &[Item]) -> Result<(), SkyliteProcError> {
    let tokens = match get_macro_item("skylite_proc::debug_output", &items)? {
        Some(m) => m,
        None => return Ok(()),
    };

    let path = match tokens.clone().into_iter().next() {
        Some(TokenTree::Literal(lit)) => {
            let path_str = lit.to_string();
            // Strip quotes from string literal
            PathBuf::try_from(&path_str[1..path_str.len() - 1])
                .map_err(|e| SkyliteProcError::SyntaxError(e.to_string()))?
        }
        _ => {
            return Err(syntax_err!(
                "Wrong argument for debug_output!, expected string literal"
            ))
        }
    };

    let base_dir = PathBuf::from_str(&std::env::var("CARGO_MANIFEST_DIR").unwrap()).unwrap();
    std::fs::write(base_dir.join(path), out.to_string()).unwrap();

    Ok(())
}

fn node_definition_fallible(body_raw: TokenStream) -> Result<TokenStream, SkyliteProcError> {
    let items = parse2::<File>(body_raw.clone())
        .map_err(|err| SkyliteProcError::SyntaxError(err.to_string()))?
        .items;

    let args = get_macro_item("skylite_proc::asset_file", &items)?
        .ok_or(data_err!("Missing required macro asset_file!"))?;
    let (project_stub, name) = extract_asset_file(args)?;
    let meta = project_stub
        .assets
        .nodes
        .get(&name)
        .ok_or(data_err!("Node not found: {name}"))?;

    let node = Node::from_meta(meta.clone(), &project_stub.assets)?;

    let out = generate_node_definition(&node, &project_stub.name, &items, &body_raw)?;

    #[cfg(debug_assertions)]
    process_debug_output(&out, &items)?;

    Ok(out)
}

fn sequence_definition_fallible(body_raw: TokenStream) -> Result<TokenStream, SkyliteProcError> {
    let items = parse2::<File>(body_raw.clone())
        .map_err(|err| SkyliteProcError::SyntaxError(err.to_string()))?
        .items;

    let args = get_macro_item("skylite_proc::asset_file", &items)?
        .ok_or(data_err!("Missing required macro asset_file!"))?;
    let (project_stub, name) = extract_asset_file(args)?;
    let meta = project_stub
        .assets
        .sequences
        .get(&name)
        .ok_or(data_err!("Sequence not found: {name}"))?;

    let sequence_stub = SequenceStub::from_meta(meta)?;

    let out = generate_sequence_definition(&sequence_stub, &project_stub.name, &items, &body_raw)?;

    #[cfg(debug_assertions)]
    process_debug_output(&out, &items)?;

    Ok(out)
}

fn skylite_project_impl(body_raw: TokenStream) -> TokenStream {
    match skylite_project_impl_fallible(body_raw) {
        Ok(t) => t,
        Err(err) => err.into(),
    }
}

fn node_definition_impl(body_raw: TokenStream) -> TokenStream {
    match node_definition_fallible(body_raw) {
        Ok(stream) => stream,
        Err(err) => err.into(),
    }
}

fn sequence_definition_impl(body_raw: TokenStream) -> TokenStream {
    match sequence_definition_fallible(body_raw) {
        Ok(stream) => stream,
        Err(err) => err.into(),
    }
}

#[proc_macro]
pub fn skylite_project(body: proc_macro::TokenStream) -> proc_macro::TokenStream {
    skylite_project_impl(body.into()).into()
}

#[proc_macro]
pub fn node_definition(body: proc_macro::TokenStream) -> proc_macro::TokenStream {
    node_definition_impl(body.into()).into()
}

#[proc_macro]
pub fn sequence_definition(body: proc_macro::TokenStream) -> proc_macro::TokenStream {
    sequence_definition_impl(body.into()).into()
}

#[proc_macro]
pub fn system(args: proc_macro::TokenStream) -> proc_macro::TokenStream {
    system_impl(args.into()).into()
}

#[proc_macro]
pub fn project_file(_body: proc_macro::TokenStream) -> proc_macro::TokenStream {
    proc_macro::TokenStream::new()
}

#[proc_macro]
pub fn target_type(_body: proc_macro::TokenStream) -> proc_macro::TokenStream {
    proc_macro::TokenStream::new()
}

/// Marks a function to initialize something. Used for `node_definition!` and
/// `skylite_project!`.
///
/// **This macro must always be used with an absolute path:
/// `#[skylite_proc::init]`.**
#[proc_macro_attribute]
pub fn init(
    _args: proc_macro::TokenStream,
    body: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    body
}

/// Marks a function to be called at the beginning of updating a Node,
/// before the child nodes are updated.
///
/// **This macro must always be used with an absolute path:
/// `#[skylite_proc::pre_update]`.**
#[proc_macro_attribute]
pub fn pre_update(
    _args: proc_macro::TokenStream,
    body: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    body
}

/// Marks a function to be called to update a Node. This is called after
/// the Node's children have been updated.
///
/// **This macro must always be used with an absolute path:
/// `#[skylite_proc::update]`.**
#[proc_macro_attribute]
pub fn update(
    _args: proc_macro::TokenStream,
    body: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    body
}

/// Alias for `skylite_proc::update`.
///
/// **This macro must always be used with an absolute path:
/// `#[skylite_proc::post_update]`.**
#[proc_macro_attribute]
pub fn post_update(
    _args: proc_macro::TokenStream,
    body: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    body
}

/// Marks a function to be called at the beginning of rendering.
///
/// **This macro must always be used with an absolute path:
/// `#[skylite_proc::pre_render]`.**
#[proc_macro_attribute]
pub fn pre_render(
    _args: proc_macro::TokenStream,
    body: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    body
}

/// Marks a function to be called for rendering something.
///
/// This macro should be used from inside a `node_definition!`.
///
/// **This macro must always be used with an absolute path:
/// `#[skylite_proc::render]`.**
#[proc_macro_attribute]
pub fn render(
    _args: proc_macro::TokenStream,
    body: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    body
}

/// Marks a function to be called at the beginning of rendering.
///
/// **This macro must always be used with an absolute path:
/// `#[skylite_proc::post_render]`.**
#[proc_macro_attribute]
pub fn post_render(
    _args: proc_macro::TokenStream,
    body: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    body
}

/// Marks a function to be used to construct an node's properties
/// from the parameters defined in the asset file (see `properties!`).
///
/// **This macro must always be used with an absolute path:
/// `#[skylite_proc::create_properties]`.**
#[proc_macro_attribute]
pub fn create_properties(
    _args: proc_macro::TokenStream,
    body: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    body
}

/// Sets the backing asset file for an `node_definition`.
///
/// **This macro must always be used with an absolute path:
/// `skylite_proc::asset_file!`.**
///
/// The definition file consists of the path to the project root file and the
/// name of the asset. The name of the asset does *not* include the file
/// extension. The specific file will be searched within the files for the
/// respective asset defined in the project definition, e.g. nodes for
/// `node_definition!`.
///
/// ## Example
/// ```ignore
/// node_definition! {
///     // Uses the asset information from the node asset 'some_node'.
///     skylite_proc::asset_file!("./path/project.scm", "some_node");
/// }
/// ```
#[proc_macro]
pub fn asset_file(_body: proc_macro::TokenStream) -> proc_macro::TokenStream {
    proc_macro::TokenStream::new()
}

/// Defines additional properties for a node. These properties are added to the
/// properties defined in the asset file. The properties should be defined as a
/// list of struct fields.
///
/// **This macro must always be used with an absolute path:
/// `skylite_proc::extra_properties!`.**
///
/// ## Example
///
/// ```ignore
/// node_definition! {
///     skylite_proc::extra_properties! {
///         pub val1: u8,
///         pub val2: u8
///     }
/// }
/// ```
#[proc_macro]
pub fn extra_properties(_body: proc_macro::TokenStream) -> proc_macro::TokenStream {
    proc_macro::TokenStream::new()
}

/// Marks a function that returns the z-order for a node. The marked function
/// must take an immutable reference to a node type.
///
/// **This macro must always be used with an absolute path:
/// `#[skylite_proc::z_order]`.**
#[proc_macro_attribute]
pub fn z_order(
    _args: proc_macro::TokenStream,
    body: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    body
}

#[cfg(debug_assertions)]
#[proc_macro]
pub fn debug_output(_body: proc_macro::TokenStream) -> proc_macro::TokenStream {
    proc_macro::TokenStream::new()
}
