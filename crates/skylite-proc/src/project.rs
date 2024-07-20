use std::fs::read_to_string;
use std::path::Path;

use crate::guile::{scm_is_false, scm_list_p, SCM};
use crate::scheme_util::CXROp::{CAR, CDR};
use crate::util::{change_case, IdentCase};
use crate::SkyliteProcError;
use glob::Pattern;
use proc_macro2::{Ident, Span, TokenStream};
use quote::{format_ident, quote};
use syn::Item;
use crate::scheme_util::{assq_str, conv_string, conv_symbol, cxr, eval_str, iter_list, parse_typed_value, with_guile, TypedValue};

#[derive(PartialEq, Debug)]
struct AssetDirectories {
    actors: Vec<Pattern>,
    scenes: Vec<Pattern>,
    graphics: Vec<Pattern>,
    sprites: Vec<Pattern>,
    tilesets: Vec<Pattern>,
    maps: Vec<Pattern>
}

fn parse_glob_list(list: SCM) -> Result<Vec<Pattern>, SkyliteProcError> {
    let mut out: Vec<Pattern> = Vec::new();
    unsafe {
        for g in iter_list(list)? {
            let glob_raw = conv_string(g)?;
            out.push(Pattern::new(&glob_raw)
                .or(Err(SkyliteProcError::DataError(format!("Not a valid glob: {}", glob_raw))))?);
        }
    }
    Ok(out)
}

impl AssetDirectories {
    fn from_scheme(alist: SCM) -> Result<AssetDirectories, SkyliteProcError> {
        unsafe {
            if scm_is_false(scm_list_p(alist)) {
                return Err(SkyliteProcError::DataError(format!("Asset directories must be defined as an associative list.")));
            }
            let mut out = Self::default();

            if let Some(expr) = assq_str("actors", alist)? {
                out.actors = parse_glob_list(expr)?;
            }
            if let Some(expr) = assq_str("scenes", alist)? {
                out.scenes = parse_glob_list(expr)?;
            }
            if let Some(expr) = assq_str("graphics", alist)? {
                out.graphics = parse_glob_list(expr)?;
            }
            if let Some(expr) = assq_str("sprites", alist)? {
                out.sprites = parse_glob_list(expr)?;
            }
            if let Some(expr) = assq_str("tilesets", alist)? {
                out.tilesets = parse_glob_list(expr)?;
            }
            if let Some(expr) = assq_str("maps", alist)? {
                out.maps = parse_glob_list(expr)?;
            }

            Ok(out)
        }
    }
}

impl Default for AssetDirectories {
    fn default() -> Self {
        Self {
            actors: vec![Pattern::new("./actors/*.scm").unwrap()],
            scenes: vec![Pattern::new("./scenes/*.scm").unwrap()],
            graphics: vec![Pattern::new("./graphics/*.scm").unwrap()],
            sprites: vec![Pattern::new("./sprites/*.scm").unwrap()],
            tilesets: vec![Pattern::new("./tilesets/*.scm").unwrap()],
            maps: vec![Pattern::new("./maps/*.scm").unwrap()]
        }
    }
}

#[derive(PartialEq, Debug)]
struct SaveItem {
    name: String,
    data: TypedValue
}

impl SaveItem {
    fn from_scheme(definition: SCM) -> Result<SaveItem, SkyliteProcError> {
        unsafe {
            Ok(SaveItem {
                name: conv_symbol(cxr(definition, &[CAR])?)?,
                data: parse_typed_value(
                    cxr(definition, &[CDR, CAR])?,
                    cxr(definition, &[CDR, CDR, CAR])?
                )?
            })
        }
    }
}

#[derive(PartialEq, Debug)]
pub(crate) struct SkyliteProject {
    name: String,
    assets: AssetDirectories,
    save_data: Vec<SaveItem>,
    tile_types: Vec<String>
}

impl SkyliteProject {
    fn from_scheme(definition: SCM) -> Result<SkyliteProject, SkyliteProcError> {
        unsafe {
            let name = conv_symbol(
                assq_str("name", definition)?.ok_or(SkyliteProcError::DataError("Missing required field 'name'".to_owned()))?
            )?;

            let assets = if let Some(alist) = assq_str("assets", definition)? {
                AssetDirectories::from_scheme(alist)?
            } else {
                AssetDirectories::default()
            };

            let mut save_data = Vec::new();
            if let Some(list) = assq_str("save-data", definition)? {
                for item in iter_list(list)? {
                    save_data.push(SaveItem::from_scheme(item)?)
                }
            }

            let mut tile_types = Vec::new();
            if let Some(list) = assq_str("tile-types", definition)? {
                for item in iter_list(list)? {
                    tile_types.push(conv_symbol(item)?)
                }
            }

            if tile_types.len() == 0 {
                return Err(SkyliteProcError::DataError("At least one tile-type must be defined.".to_owned()))
            }

            Ok(SkyliteProject {
                name,
                assets,
                save_data,
                tile_types
            })
        }
    }

    pub(crate) fn from_file(path: &Path) -> Result<SkyliteProject, SkyliteProcError> {
        // Since we are not actually accessing anything from this signature from C,
        // we can get away with ignoring the missing C representations.
        #[allow(improper_ctypes_definitions)]
        extern "C" fn from_file_guile(path: &Path) -> Result<SkyliteProject, SkyliteProcError> {
            let definition_raw = read_to_string(path).map_err(|e| SkyliteProcError::OtherError(e.to_string()))?;
            let definition = unsafe {
                eval_str(&definition_raw)?
            };
            SkyliteProject::from_scheme(definition)
        }
        with_guile(from_file_guile, path)
    }

    fn tile_type_name(&self) -> Ident {
        format_ident!("{}Tiles", change_case(&self.name, IdentCase::UpperCamelCase))
    }

    fn generate_tile_type_enum(&self) -> TokenStream {
        let tile_type_name = self.tile_type_name();
        let tile_types = self.tile_types.iter()
            .map(|tt| Ident::new(&change_case(tt, IdentCase::UpperCamelCase), Span::call_site()));
        quote! {
            #[derive(Clone, Copy)]
            pub enum #tile_type_name {
                #(#tile_types),*
            }
        }
    }

    fn generate_project_type(&self, target_type: &TokenStream) -> TokenStream {
        let project_name = Ident::new(&change_case(&self.name, IdentCase::UpperCamelCase), Span::call_site());
        quote! {
            pub struct #project_name {
                target: #target_type,
                // TODO
            }
        }
    }

    fn generate_project_implementation(&self, target_type: &TokenStream) -> Result<TokenStream, SkyliteProcError> {
        let project_name = Ident::new(&change_case(&self.name, IdentCase::UpperCamelCase), Span::call_site());
        let tile_type_name = self.tile_type_name();
        Ok(quote! {
            impl SkyliteProject for #project_name {
                type TileType = #tile_type_name;
                type Target = #target_type;
            }
        })
    }

    pub(crate) fn generate(&self, target_type: &TokenStream) -> Result<Vec<Item>, SkyliteProcError> {
        Ok(vec![
            Item::Verbatim(self.generate_tile_type_enum()),
            Item::Verbatim(self.generate_project_type(&target_type)),
            Item::Verbatim(self.generate_project_implementation(&target_type)?)
        ])
    }
}

#[cfg(test)]
mod tests {
    use glob::Pattern;

    use crate::{project::{AssetDirectories, SaveItem}, scheme_util::{eval_str, with_guile, TypedValue}};

    use super::SkyliteProject;

    extern "C" fn test_project_parsing_impl(_: &()) {
        unsafe {
            let definition = eval_str(
                r#"'((name TestProject)
                     (assets ((actors ("./test1/*.scm" "./test2/*.scm"))
                              (maps ("./test3/*.scm"))))
                     (save-data
                       ((flag1 bool #f)
                        (val2 u8 5)))
                     (tile-types (solid semi-solid non-solid)))"#).unwrap();
            let project = SkyliteProject::from_scheme(definition).unwrap();
            assert_eq!(project, SkyliteProject {
                name: "TestProject".to_owned(),
                assets: AssetDirectories {
                    actors: vec![Pattern::new("./test1/*.scm").unwrap(), Pattern::new("./test2/*.scm").unwrap()],
                    scenes: vec![Pattern::new("./scenes/*.scm").unwrap()],
                    graphics: vec![Pattern::new("./graphics/*.scm").unwrap()],
                    sprites: vec![Pattern::new("./sprites/*.scm").unwrap()],
                    tilesets: vec![Pattern::new("./tilesets/*.scm").unwrap()],
                    maps: vec![Pattern::new("./test3/*.scm").unwrap()]
                },
                save_data: vec![
                    SaveItem {
                        name: "flag1".to_owned(),
                        data: TypedValue::Bool(false)
                    },
                    SaveItem {
                        name: "val2".to_owned(),
                        data: TypedValue::U8(5)
                    }
                ],
                tile_types: vec!["solid".to_owned(), "semi-solid".to_owned(), "non-solid".to_owned()]
            });
        }
    }

    #[test]
    fn test_project_parsing() {
        with_guile(test_project_parsing_impl, &());
    }
}
