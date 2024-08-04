use std::fs::read_to_string;
use std::path::{Path, PathBuf, MAIN_SEPARATOR_STR};

use crate::guile::{scm_is_false, scm_list_p, SCM};
use crate::scheme_util::CXROp::{CAR, CDR};
use crate::util::{change_case, IdentCase};
use crate::SkyliteProcError;
use glob::{GlobError, Pattern};
use proc_macro2::{Ident, Span, TokenStream};
use quote::{format_ident, quote};
use syn::Item;
use crate::scheme_util::{assq_str, conv_string, conv_symbol, cxr, eval_str, iter_list, parse_typed_value, with_guile, TypedValue};

fn normalize_glob(glob: &str, base_dir: &Path) -> String {
    if Path::new(&glob).is_relative() {
        base_dir.to_str().unwrap().to_owned() + MAIN_SEPARATOR_STR + &glob
    } else {
        glob.to_owned()
    }
}

/// A collection of similar assets, e.g. tilesets or maps.
///
/// An `AssetGroup` is represented by a set of globs for matching
/// the files containing the assets. If a glob is relative,
/// it is resolved relative to the directory containing the
/// project definition file.
#[derive(Debug, PartialEq)]
pub(crate) struct AssetGroup {
    globs: Vec<String>
}

impl AssetGroup {
    fn from_scheme(list: SCM, base_dir: &Path) -> Result<AssetGroup, SkyliteProcError> {
        let mut globs: Vec<String> = Vec::new();
        unsafe {
            for g in iter_list(list)? {
                let glob = normalize_glob(&conv_string(g)?, base_dir);
                Pattern::new(&glob).map_err(|err| SkyliteProcError::DataError(format!("Error parsing glob: {}", err)))?;
                globs.push(glob);
            }
        }
        Ok(AssetGroup { globs })
    }

    /// Returns a unique id for a given asset name. The name of an asset is the
    /// last component of its filename without the file extension. The name is also
    /// normalized to UpperCamelCase. For example, the name of the asset at
    /// `./tilesets/town_1.scm` would be `Town1`.
    ///
    /// This method will return an error if the name does not exist, or
    /// is ambiguous among the assets matched by the `AssetGroup`.
    ///
    /// The ids can be used to reference a particular asset in the encoded data
    /// of other assets.
    pub(crate) fn calc_id_for_asset(&self, name: &str) -> Result<usize, SkyliteProcError> {
        let name_camel_case = change_case(name, IdentCase::UpperCamelCase);

        let mut out: Option<(usize, PathBuf)> = None;
        for (idx, entry_res) in self.into_iter().enumerate() {
            let entry = match entry_res {
                Ok(e) => e,
                Err(err) => return Err(SkyliteProcError::OtherError(format!("IO Error: {}", err)))
            };

            if change_case(entry.file_stem().unwrap().to_str().unwrap(), IdentCase::UpperCamelCase) != name_camel_case {
                continue;
            }

            if let Some((_, prev_entry)) = out {
                return Err(SkyliteProcError::DataError(format!("Name {} is ambiguous; both {:?} and {:?} match", name, prev_entry, entry)));
            }

            out = Some((idx, entry));
        }

        if let Some((idx, _)) = out {
            Ok(idx)
        } else {
            Err(SkyliteProcError::DataError(format!("Name not found: {}", name)))
        }
    }
}

pub(crate) struct AssetIterator<'base> {
    current_iter: glob::Paths,
    glob_idx: usize,
    asset_group: &'base AssetGroup
}

impl<'base> Iterator for AssetIterator<'base> {
    type Item = Result<PathBuf, GlobError>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(res) = self.current_iter.next() {
            Some(res)
        } else if self.glob_idx < self.asset_group.globs.len() - 1 {
            self.glob_idx += 1;
            self.current_iter = glob::glob(&self.asset_group.globs[self.glob_idx]).unwrap();
            self.current_iter.next()
        } else {
            None
        }
    }
}

impl<'base> IntoIterator for &'base AssetGroup {
    type Item = Result<PathBuf, GlobError>;

    type IntoIter = AssetIterator<'base>;

    fn into_iter(self) -> Self::IntoIter {
        AssetIterator {
            current_iter: glob::glob(&self.globs[0]).unwrap(),
            glob_idx: 0,
            asset_group: self
        }
    }
}

/// Container for `AssetGroups` for all asset types used by Skylite.
#[derive(PartialEq, Debug)]
struct AssetGroups {
    actors: AssetGroup,
    scenes: AssetGroup,
    plays: AssetGroup,
    graphics: AssetGroup,
    sprites: AssetGroup,
    tilesets: AssetGroup,
    maps: AssetGroup
}

impl AssetGroups {
    fn from_scheme(alist: SCM, base_dir: &Path) -> Result<AssetGroups, SkyliteProcError> {
        unsafe {
            if scm_is_false(scm_list_p(alist)) {
                return Err(SkyliteProcError::DataError(format!("Asset directories must be defined as an associative list.")));
            }
            let mut out = create_default_asset_groups(base_dir);

            if let Some(expr) = assq_str("actors", alist)? {
                out.actors = AssetGroup::from_scheme(expr, base_dir)?;
            }
            if let Some(expr) = assq_str("scenes", alist)? {
                out.scenes = AssetGroup::from_scheme(expr, base_dir)?;
            }
            if let Some(expr) = assq_str("plays", alist)? {
                out.plays = AssetGroup::from_scheme(expr, base_dir)?;
            }
            if let Some(expr) = assq_str("graphics", alist)? {
                out.graphics = AssetGroup::from_scheme(expr, base_dir)?;
            }
            if let Some(expr) = assq_str("sprites", alist)? {
                out.sprites = AssetGroup::from_scheme(expr, base_dir)?;
            }
            if let Some(expr) = assq_str("tilesets", alist)? {
                out.tilesets = AssetGroup::from_scheme(expr, base_dir)?;
            }
            if let Some(expr) = assq_str("maps", alist)? {
                out.maps = AssetGroup::from_scheme(expr, base_dir)?;
            }

            Ok(out)
        }
    }
}

fn asset_group_from_single(pattern: &str, base_dir: &Path) -> AssetGroup {
    AssetGroup {
        globs: vec![normalize_glob(pattern, base_dir)]
    }
}

fn create_default_asset_groups(base_dir: &Path) -> AssetGroups {
    AssetGroups {
        actors: asset_group_from_single("./actors/*.scm", base_dir),
        scenes: asset_group_from_single("./scenes/*.scm", base_dir),
        plays: asset_group_from_single("./plays/*.scm", base_dir),
        graphics: asset_group_from_single("./graphics/*.scm", base_dir),
        sprites: asset_group_from_single("./sprites/*.scm", base_dir),
        tilesets: asset_group_from_single("./tilesets/*.scm", base_dir),
        maps: asset_group_from_single("./maps/*.scm", base_dir)
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

/// Main type for managing the asset files and code generation
/// of a Skylite project.
#[derive(PartialEq, Debug)]
pub(crate) struct SkyliteProject {
    name: String,
    assets: AssetGroups,
    save_data: Vec<SaveItem>,
    tile_types: Vec<String>
}

impl SkyliteProject {
    fn from_scheme(definition: SCM, project_root: &Path) -> Result<SkyliteProject, SkyliteProcError> {
        unsafe {
            let name = conv_symbol(
                assq_str("name", definition)?.ok_or(SkyliteProcError::DataError("Missing required field 'name'".to_owned()))?
            )?;

            let assets = if let Some(alist) = assq_str("assets", definition)? {
                AssetGroups::from_scheme(alist, &project_root)?
            } else {
                create_default_asset_groups(&project_root)
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

    /// Loads a project from a project definition file.
    ///
    /// The file at the given `Path` will be evaluated as a Scheme file, and the
    /// resulting form will be parsed into an instance of `SkyliteProject`.
    pub(crate) fn from_file(path: &Path) -> Result<SkyliteProject, SkyliteProcError> {
        // Since we are not actually accessing anything from this signature from C,
        // we can get away with ignoring the missing C representations.
        #[allow(improper_ctypes_definitions)]
        extern "C" fn from_file_guile(path: &Path) -> Result<SkyliteProject, SkyliteProcError> {
            let resolved_path = path.canonicalize().map_err(|e| SkyliteProcError::OtherError(format!("Error resolving project path: {}", e)))?;
            let definition_raw = read_to_string(path).map_err(|e| SkyliteProcError::OtherError(format!("Error reading project definition: {}", e)))?;
            let definition = unsafe {
                eval_str(&definition_raw)?
            };

            let project_root = resolved_path.parent().unwrap();
            SkyliteProject::from_scheme(definition, project_root)
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
    use std::{fs::{create_dir, remove_dir_all, File}, path::PathBuf};

    use crate::{project::{asset_group_from_single, normalize_glob, AssetGroup, AssetGroups, SaveItem}, scheme_util::{eval_str, with_guile, TypedValue}};

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
            let project_root = PathBuf::new();
            let project = SkyliteProject::from_scheme(definition, &project_root).unwrap();
            assert_eq!(project, SkyliteProject {
                name: "TestProject".to_owned(),
                assets: AssetGroups {
                    actors: AssetGroup {
                        globs: vec![
                            normalize_glob("./test1/*.scm", &project_root),
                            normalize_glob("./test2/*.scm", &project_root),
                        ]
                    },
                    scenes: asset_group_from_single("./scenes/*.scm", &project_root),
                    plays: asset_group_from_single("./plays/*.scm", &project_root),
                    graphics: asset_group_from_single("./graphics/*.scm", &project_root),
                    sprites: asset_group_from_single("./sprites/*.scm", &project_root),
                    tilesets: asset_group_from_single("./tilesets/*.scm", &project_root),
                    maps: asset_group_from_single("./test3/*.scm", &project_root)
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

    #[test]
    fn test_calc_id_for_asset() {
        let test_dir_name = format!("skylite_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs());
        let test_dir = std::env::temp_dir().join(test_dir_name);
        let sub_dir = test_dir.join("sub");
        create_dir(&test_dir).unwrap();
        create_dir(&sub_dir).unwrap();
        drop(File::create(test_dir.clone().join("test_1.scm")).unwrap());
        drop(File::create(test_dir.clone().join("test_2.scm")).unwrap());
        drop(File::create(test_dir.clone().join("asset.scm")).unwrap());
        drop(File::create(sub_dir.clone().join("asset.scm")).unwrap());

        let asset_group1 = asset_group_from_single("test_?.scm", &test_dir);

        // Test different casings
        assert_eq!(asset_group1.calc_id_for_asset("test_1").unwrap(), 0);
        assert_eq!(asset_group1.calc_id_for_asset("TEST_1").unwrap(), 0);
        assert_eq!(asset_group1.calc_id_for_asset("test-2").unwrap(), 1);

        // Test name not matched by glob
        assert!(asset_group1.calc_id_for_asset("asset").is_err());

        // Test ambiguous name
        let asset_group2 = asset_group_from_single("**/asset.scm", &test_dir);
        assert!(asset_group2.calc_id_for_asset("asset").is_err());

        remove_dir_all(test_dir).unwrap();
    }
}
