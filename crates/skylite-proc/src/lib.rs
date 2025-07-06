use std::path::PathBuf;
use std::str::FromStr;

use generate::nodes::generate_node_definition;
use generate::sequences::generate_sequence_definition;
use parse::guile::SCM;
use parse::project::SkyliteProject;
use parse::scheme_util::form_to_string;
use parse::util::{change_case, IdentCase};
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

    let items = &module
        .content
        .as_ref()
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

    module
        .content
        .as_mut()
        .unwrap()
        .1
        .append(&mut project_items);

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
    let module = parse2::<ItemMod>(body_raw.clone())
        .map_err(|err| SkyliteProcError::SyntaxError(err.to_string()))?;

    // The actual content is moved out of the module, everything else is
    // retained. The content is passed through generate_node_definition, the
    // output of which becomes the module's new content.
    let mut module_out = ItemMod {
        content: None,
        ..module
    };
    let (brace, items) = module
        .content
        .ok_or(data_err!("Node definition module must have a body"))?;

    let (mut project, name) = extract_asset_file(&args_raw)?;
    let node = project.assets.load_node(&name)?;

    let tokens = generate_node_definition(&node, &project.name, items)?;
    module_out.content = Some((brace, vec![Item::Verbatim(tokens)]));

    Ok(module_out.into_token_stream())
}

fn sequence_definition_fallible(
    args_raw: TokenStream,
    body_raw: TokenStream,
) -> Result<TokenStream, SkyliteProcError> {
    let mut module = parse2::<ItemMod>(body_raw.clone())
        .map_err(|err| SkyliteProcError::SyntaxError(err.to_string()))?;
    let items = &module
        .content
        .as_ref()
        .ok_or(data_err!("Node definition module must have a body"))?
        .1;

    let (mut project, name) = extract_asset_file(&args_raw)?;
    let sequence = project.assets.load_sequence(&name)?;

    let tokens = Item::Verbatim(generate_sequence_definition(
        &sequence,
        &project.name,
        &items,
    )?);

    module.content.as_mut().unwrap().1.push(tokens);

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

#[proc_macro_attribute]
pub fn new(
    _args: proc_macro::TokenStream,
    body: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    body
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

#[proc_macro_attribute]
pub fn is_visible(
    _args: proc_macro::TokenStream,
    body: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    body
}
