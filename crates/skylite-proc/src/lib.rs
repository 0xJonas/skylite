use std::{path::PathBuf, str::FromStr};

use generate::actors::generate_actor_definition;
use generate::scenes::generate_scene_definition;
use generate::util::get_macro_item;
use parse::actors::Actor;
use parse::scenes::SceneStub;
use quote::{quote, ToTokens};
use parse::{guile::SCM, project::SkyliteProjectStub};
use parse::scheme_util::form_to_string;
use proc_macro2::{TokenStream, TokenTree};
use parse::project::SkyliteProject;
use syn::{Block, File, LitStr, Macro, Stmt};
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

fn extract_asset_file(definition_file: &Macro) -> Result<(SkyliteProjectStub, String), SkyliteProcError> {
    let args = Parser::parse2(Punctuated::<LitStr, Token![,]>::parse_separated_nonempty, definition_file.tokens.clone())
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
    let mac = match get_macro_item("skylite_proc::debug_output", &items)? {
        Some(m) => m,
        None => return Ok(())
    };

    let path = match mac.tokens.clone().into_iter().next() {
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

    let mac = get_macro_item("skylite_proc::asset_file", &items)?
        .ok_or(SkyliteProcError::DataError(format!("Missing required macro asset_file!")))?;
    let (project_stub, name) = extract_asset_file(mac)?;

    let (id, path) = project_stub.assets.actors.find_asset(&name)?;
    let actor = Actor::from_file(&path)?;

    let out = generate_actor_definition(&actor, id as u32, &project_stub.name, &items, &body_raw)?;

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

#[proc_macro_attribute]
pub fn skylite_project(args: proc_macro::TokenStream, body: proc_macro::TokenStream) -> proc_macro::TokenStream {
    skylite_project_impl(args.into(), body.into()).into()
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
