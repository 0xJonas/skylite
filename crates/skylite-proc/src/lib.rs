use std::{path::PathBuf, str::FromStr};

use generate::actors::generate_actor_definition;
use generate::scenes::generate_scene_definition;
use generate::util::get_macro_item;
use parse::actors::Actor;
use parse::scenes::SceneStub;
use parse::util::{change_case, IdentCase};
use quote::{format_ident, quote};
use parse::{guile::SCM, project::SkyliteProjectStub};
use parse::scheme_util::form_to_string;
use proc_macro2::{TokenStream, TokenTree};
use parse::project::SkyliteProject;
use syn::{File, LitStr};
use syn::{parse::Parser, parse2, punctuated::Punctuated, Item, Token};

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

fn parse_project_file(tokens: &TokenStream) -> Result<PathBuf, SkyliteProcError> {
    let path_raw = parse2::<LitStr>(tokens.clone())
        .map(|lit| lit.value())
        .map_err(|err| SkyliteProcError::SyntaxError(format!("Illegal arguments to project_file!: {}", err)))?;

    let base_dir = PathBuf::from_str(&std::env::var("CARGO_MANIFEST_DIR").unwrap()).unwrap();
    let relative_path = PathBuf::from_str(&path_raw)
        .map_err(|err| SkyliteProcError::SyntaxError(format!("Invalid project path: {}", err)))?;
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

fn skylite_project_imp_fallible(body_raw: TokenStream) -> Result<TokenStream, SkyliteProcError> {
    let items = parse2::<File>(body_raw)
        .map_err(|err| SkyliteProcError::SyntaxError(err.to_string()))?
        .items;

    let project_file_mac = get_macro_item("skylite_proc::project_file", &items)?
        .ok_or(SkyliteProcError::DataError(format!("Missing required macro skylite_proc::project_file!")))?;
    let path = parse_project_file(project_file_mac)?;

    let target_type_mac = get_macro_item("skylite_proc::target_type", &items)?
        .ok_or(SkyliteProcError::DataError(format!("Missing required macro skylite_proc::target_type!")))?;
    // Verify that the content of target_type is actually a valid path.
    parse2::<syn::Path>(target_type_mac.clone())
        .map_err(|err| SkyliteProcError::SyntaxError(err.to_string()))?;

    let project_stub = SkyliteProjectStub::from_file(&path)?;
    let project = SkyliteProject::from_stub(project_stub)?;

    let module_name = format_ident!("{}", change_case(&project.name, IdentCase::LowerSnakeCase));
    let project_items = project.generate(&target_type_mac, &items)?;

    let crate_root_check = get_crate_root_check();

    let out = quote! {
        #crate_root_check

        #(#items)
        *

        mod #module_name {
            use super::*;

            #(#project_items)
            *
        }

        pub use #module_name::*;
    };

    #[cfg(debug_assertions)]
    {
        process_debug_output(&out, &items)?;
    }

    Ok(out)
}

fn extract_asset_file(definition_file: &TokenStream) -> Result<(SkyliteProjectStub, String), SkyliteProcError> {
    let args = Parser::parse2(Punctuated::<LitStr, Token![,]>::parse_separated_nonempty, definition_file.clone())
        .map_err(|err| SkyliteProcError::SyntaxError(format!("Failed to parse definition_file! macro: {}. Expected (\"project-path\", \"asset-name\")", err.to_string())))?;

    if args.len() != 2 {
        return Err(SkyliteProcError::SyntaxError(format!("Wrong number of arguments to definition_file!, expected (\"project-path\", \"asset-name\")")));
    }

    let relative_path = PathBuf::try_from(args[0].value())
        .map_err(|_| SkyliteProcError::DataError(format!("Not a valid project path: {}", args[0].value())))?;

    let base_dir = PathBuf::from_str(&std::env::var("CARGO_MANIFEST_DIR").unwrap()).unwrap();
    let stub = SkyliteProjectStub::from_file(&base_dir.join(relative_path))?;

    return Ok((stub, args[1].value()));
}

#[cfg(debug_assertions)]
fn process_debug_output(out: &TokenStream, items: &[Item]) -> Result<(), SkyliteProcError> {
    let tokens = match get_macro_item("skylite_proc::debug_output", &items)? {
        Some(m) => m,
        None => return Ok(())
    };

    let path = match tokens.clone().into_iter().next() {
        Some(TokenTree::Literal(lit)) => {
            let path_str = lit.to_string();
            PathBuf::try_from(&path_str[1 .. path_str.len() - 1])
                    .map_err(|e| SkyliteProcError::SyntaxError(format!("{}", e.to_string())))?
        },
        _ => return Err(SkyliteProcError::SyntaxError(format!("Wrong argument for debug_output!, expected string literal")))
    };

    let base_dir = PathBuf::from_str(&std::env::var("CARGO_MANIFEST_DIR").unwrap()).unwrap();
    std::fs::write(base_dir.join(path), out.to_string()).unwrap();

    Ok(())
}

fn actor_definition_fallible(body_raw: TokenStream) -> Result<TokenStream, SkyliteProcError> {
    let items = parse2::<File>(body_raw.clone())
        .map_err(|err| SkyliteProcError::SyntaxError(err.to_string()))?
        .items;

    let args = get_macro_item("skylite_proc::asset_file", &items)?
        .ok_or(SkyliteProcError::DataError(format!("Missing required macro asset_file!")))?;
    let (project_stub, name) = extract_asset_file(args)?;

    let (id, path) = project_stub.assets.actors.find_asset(&name)?;
    let actor = Actor::from_file(&path)?;

    let out = generate_actor_definition(&actor, id, &project_stub.name, &items, &body_raw)?;

    #[cfg(debug_assertions)]
    process_debug_output(&out, &items)?;

    Ok(out)
}

fn scene_definition_fallible(body_raw: TokenStream) -> Result<TokenStream, SkyliteProcError> {
    let items = parse2::<File>(body_raw.clone())
        .map_err(|err| SkyliteProcError::SyntaxError(err.to_string()))?
        .items;

    let mac = get_macro_item("skylite_proc::asset_file", &items)?
        .ok_or(SkyliteProcError::DataError(format!("Missing required macro asset_file!")))?;
    let (project_stub, name) = extract_asset_file(mac)?;

    let (id, path) = project_stub.assets.scenes.find_asset(&name)?;
    let scene = SceneStub::from_file(&path)?;

    let out = generate_scene_definition(&scene, id as u32, &items, &project_stub.name, &body_raw)?;

    #[cfg(debug_assertions)]
    process_debug_output(&out, &items)?;

    Ok(out)
}

fn skylite_project_impl(body_raw: TokenStream) -> TokenStream {
    match skylite_project_imp_fallible(body_raw) {
        Ok(t) => t,
        Err(err) => err.into()
    }
}

fn actor_definition_impl(body_raw: TokenStream) -> TokenStream {
    match actor_definition_fallible(body_raw) {
        Ok(stream) => stream,
        Err(err) => err.into()
    }
}

fn scene_definition_impl(body_raw: TokenStream) -> TokenStream {
    match scene_definition_fallible(body_raw) {
        Ok(stream) => stream,
        Err(err) => err.into()
    }
}

#[proc_macro]
pub fn skylite_project(body: proc_macro::TokenStream) -> proc_macro::TokenStream {
    skylite_project_impl(body.into()).into()
}

#[doc = include_str!("../../../docs/actor_definition.md")]
#[proc_macro]
pub fn actor_definition(body: proc_macro::TokenStream) -> proc_macro::TokenStream {
    actor_definition_impl(body.into()).into()
}

#[doc = include_str!("../../../docs/scene_definition.md")]
#[proc_macro]
pub fn scene_definition(body: proc_macro::TokenStream) -> proc_macro::TokenStream {
    scene_definition_impl(body.into()).into()
}

#[proc_macro]
pub fn project_file(_body: proc_macro::TokenStream) -> proc_macro::TokenStream { proc_macro::TokenStream::new() }

#[proc_macro]
pub fn target_type(_body: proc_macro::TokenStream) -> proc_macro::TokenStream { proc_macro::TokenStream::new() }

/// Marks a function to be called to initialize an instance of `SkyliteProject` or `Scene.`
///
/// **This macro must always be used with an absolute path: `#[skylite_proc::init]`.**
#[proc_macro_attribute]
pub fn init(_args: proc_macro::TokenStream, body: proc_macro::TokenStream) -> proc_macro::TokenStream { body }

/// Marks a function to be called at the beginning of an update.
///
/// **This macro must always be used with an absolute path: `#[skylite_proc::pre_update]`.**
#[proc_macro_attribute]
pub fn pre_update(_args: proc_macro::TokenStream, body: proc_macro::TokenStream) -> proc_macro::TokenStream { body }

/// Marks a function to be called at the end of an update.
///
/// **This macro must always be used with an absolute path: `#[skylite_proc::post_update]`.**
#[proc_macro_attribute]
pub fn post_update(_args: proc_macro::TokenStream, body: proc_macro::TokenStream) -> proc_macro::TokenStream { body }

/// Marks a function to be called at the beginning of rendering.
///
/// **This macro must always be used with an absolute path: `#[skylite_proc::pre_render]`.**
#[proc_macro_attribute]
pub fn pre_render(_args: proc_macro::TokenStream, body: proc_macro::TokenStream) -> proc_macro::TokenStream { body }

/// Marks a function to be called for rendering something. Used for actors, because actors do not
/// have any intrinsic properties that are rendered automatically.
///
/// **This macro must always be used with an absolute path: `#[skylite_proc::render]`.**
#[proc_macro_attribute]
pub fn render(_args: proc_macro::TokenStream, body: proc_macro::TokenStream) -> proc_macro::TokenStream { body }

/// Marks a function to be called at the beginning of rendering.
///
/// **This macro must always be used with an absolute path: `#[skylite_proc::post_render]`.**
#[proc_macro_attribute]
pub fn post_render(_args: proc_macro::TokenStream, body: proc_macro::TokenStream) -> proc_macro::TokenStream { body }

/// Marks a function to be used to construct an actor's or scene's properties from the parameters defined in the asset file
/// (see `properties!`).
///
/// **This macro must always be used with an absolute path: `#[skylite_proc::create_properties]`.**
#[proc_macro_attribute]
pub fn create_properties(_args: proc_macro::TokenStream, body: proc_macro::TokenStream) -> proc_macro::TokenStream { body }

/// Marks an action for an actor.
///
/// The name of the corresponding action in the actor asset file should be given like this:
///
/// ```rust
/// #[skylite_proc::action("some_action")]
/// fn some_action(actor: &mut Actor, project: &mut Project, args...) { ... }
/// ```
#[proc_macro_attribute]
pub fn action(_args: proc_macro::TokenStream, body: proc_macro::TokenStream) -> proc_macro::TokenStream { body }

/// Sets the backing asset file for an `actor_definition` or `scene_definition`.
///
/// **This macro must always be used with an absolute path: `skylite_proc::asset_file!`.**
///
/// The definition file consists of the path to the project root file and the name of the asset.
/// The name of the asset does *not* include the file extension. The specific file will be searched
/// within the files for the respective asset defined in the project definition, i.e. actors files for
/// `actor_definition!`, scenes for `scene_definition!`, etc.
///
/// ## Example
/// ```rust
/// actor_definition! {
///     // Uses the asset information from the actor asset 'some_actor'.
///     skylite_proc::asset_file!("./path/project.scm", "some_actor");
/// }
/// ```
#[proc_macro]
pub fn asset_file(_body: proc_macro::TokenStream) -> proc_macro::TokenStream { proc_macro::TokenStream::new() }

/// Defines properties for a scene or actor. These properties are converted into a separate type and can
/// be accessed through the `properties`-member on actors or scenes.
///
/// **This macro must always be used with an absolute path: `skylite_proc::properties!`.**
///
/// ## Example
///
/// ```rust
/// actor_definition! {
///     skylite_proc::properties! {
///         val1: u8,
///         val2: u8
///     }
/// }
/// ```
#[proc_macro]
pub fn properties(_body: proc_macro::TokenStream) -> proc_macro::TokenStream { proc_macro::TokenStream::new() }

#[cfg(debug_assertions)]
#[proc_macro]
pub fn debug_output(_body: proc_macro::TokenStream) -> proc_macro::TokenStream { proc_macro::TokenStream::new() }
