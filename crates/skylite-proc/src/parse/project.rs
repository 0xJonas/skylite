use std::fs::read_to_string;
use std::path::{Path, PathBuf, MAIN_SEPARATOR_STR};

use crate::parse::guile::{scm_is_false, scm_list_p, SCM};
use crate::parse::scheme_util::{
    CXROp::{CAR, CDR},
    {assq_str, parse_string, parse_symbol, cxr, eval_str, iter_list, with_guile}
};
use crate::parse::util::{change_case, IdentCase};
use crate::SkyliteProcError;
use glob::{GlobError, Pattern};

use super::actors::Actor;
use super::scenes::{Scene, SceneInstance};
use super::values::{parse_type, parse_typed_value, TypedValue};


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
                let glob = normalize_glob(&parse_string(g)?, base_dir);
                Pattern::new(&glob).map_err(|err| SkyliteProcError::DataError(format!("Error parsing glob: {}", err)))?;
                globs.push(glob);
            }
        }
        Ok(AssetGroup { globs })
    }

    /// Returns a unique id and the file path for a given asset name. The name of an asset is the
    /// last component of its filename without the file extension. The name is also
    /// normalized to UpperCamelCase. For example, the name of the asset at
    /// `./tilesets/town_1.scm` would be `Town1`.
    ///
    /// This method will return an error if the name does not exist, or
    /// is ambiguous among the assets matched by the `AssetGroup`.
    ///
    /// The ids can be used to reference a particular asset in the encoded data
    /// of other assets.
    pub(crate) fn find_asset(&self, name: &str) -> Result<(usize, PathBuf), SkyliteProcError> {
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

        if let Some(id_and_path) = out {
            Ok(id_and_path)
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
pub(crate) struct AssetGroups {
    pub actors: AssetGroup,
    pub scenes: AssetGroup,
    pub plays: AssetGroup,
    pub graphics: AssetGroup,
    pub sprites: AssetGroup,
    pub tilesets: AssetGroup,
    pub maps: AssetGroup
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
pub(crate) struct SaveItem {
    name: String,
    data: TypedValue
}

impl SaveItem {
    fn from_scheme(definition: SCM) -> Result<SaveItem, SkyliteProcError> {
        unsafe {
            let typename = parse_type(cxr(definition, &[CDR, CAR])?)?;
            Ok(SaveItem {
                name: parse_symbol(cxr(definition, &[CAR])?)?,
                data: parse_typed_value(
                    &typename,
                    cxr(definition, &[CDR, CDR, CAR])?
                )?
            })
        }
    }
}

// Early form of `SkyliteProject`, where the assets are not yet
// resolved and parsed. Used for contexts where the full representation
// of the project is not required, e.g. actor_definition and `scene_definition`.
#[derive(PartialEq, Debug)]
pub(crate) struct SkyliteProjectStub {
    pub name: String,
    pub assets: AssetGroups,
    pub save_data: Vec<SaveItem>,
    pub initial_scene: SceneInstance,
    pub tile_types: Vec<String>
}

impl SkyliteProjectStub {
    fn from_scheme(definition: SCM, project_root: &Path) -> Result<SkyliteProjectStub, SkyliteProcError> {
        unsafe {
            let name = parse_symbol(
                assq_str("name", definition)?.ok_or(SkyliteProcError::DataError("Missing required field 'name'".to_owned()))?
            )?;

            let assets = if let Some(alist) = assq_str("assets", definition)? {
                AssetGroups::from_scheme(alist, &project_root)?
            } else {
                create_default_asset_groups(&project_root)
            };

            let save_data = if let Some(list) = assq_str("save-data", definition)? {
                iter_list(list)?
                    .map(SaveItem::from_scheme)
                    .collect::<Result<Vec<SaveItem>, SkyliteProcError>>()?
            } else {
                Vec::new()
            };

            let initial_scene = {
                let instance_def = assq_str("initial-scene", definition)?.ok_or(SkyliteProcError::DataError(format!("Missing required field 'initial-scene'")))?;
                SceneInstance::from_scheme(instance_def, &assets.scenes)?
            };


            let tile_types = if let Some(list) = assq_str("tile-types", definition)? {
                iter_list(list)?
                    .map(|t| parse_symbol(t))
                    .collect::<Result<Vec<String>, SkyliteProcError>>()?
            } else {
                Vec::new()
            };

            if tile_types.len() == 0 {
                return Err(SkyliteProcError::DataError("At least one tile-type must be defined.".to_owned()))
            }

            Ok(SkyliteProjectStub {
                name,
                assets,
                save_data,
                initial_scene,
                tile_types
            })
        }
    }

    /// Loads a project from a project definition file.
    ///
    /// The file at the given `Path` will be evaluated as a Scheme file, and the
    /// resulting form will be parsed into an instance of `SkyliteProjectStub`.
    pub(crate) fn from_file(path: &Path) -> Result<SkyliteProjectStub, SkyliteProcError> {
        // Since we are not actually accessing anything from this signature from C,
        // we can get away with ignoring the missing C representations.
        #[allow(improper_ctypes_definitions)]
        extern "C" fn from_file_guile(path: &Path) -> Result<SkyliteProjectStub, SkyliteProcError> {
            let resolved_path = path.canonicalize().map_err(|e| SkyliteProcError::OtherError(format!("Error resolving project path: {}", e)))?;
            let definition_raw = read_to_string(path).map_err(|e| SkyliteProcError::OtherError(format!("Error reading project definition: {}", e)))?;
            let definition = unsafe {
                eval_str(&definition_raw)?
            };

            let project_root = resolved_path.parent().unwrap();
            SkyliteProjectStub::from_scheme(definition, project_root)
        }
        with_guile(from_file_guile, path)
    }
}

/// Main type for managing the asset files and code generation
/// of a Skylite project.
pub(crate) struct SkyliteProject {
    pub name: String,
    pub actors: Vec<Actor>,
    pub scenes: Vec<Scene>,
    pub _save_data: Vec<SaveItem>,
    pub initial_scene: SceneInstance,
    pub tile_types: Vec<String>
}

impl SkyliteProject {
    pub(crate) fn from_stub(stub: SkyliteProjectStub) -> Result<SkyliteProject, SkyliteProcError> {
        let actors = stub.assets.actors.into_iter()
            .map(|path_res| {
                let path = path_res.map_err(|err| SkyliteProcError::OtherError(format!("GlobError: {}", err.to_string())))?;
                Actor::from_file(path.as_path())
            })
            .collect::<Result<Vec<Actor>, SkyliteProcError>>()?;

        let scenes = stub.assets.scenes.into_iter()
            .map(|path_res| {
                let path = path_res.map_err(|err| SkyliteProcError::OtherError(format!("GlobError: {}", err.to_string())))?;
                Scene::from_file(path.as_path(), &actors)
            })
            .collect::<Result<Vec<Scene>, SkyliteProcError>>()?;

        Ok(SkyliteProject {
            name: stub.name,
            actors,
            scenes,
            _save_data: stub.save_data,
            initial_scene: stub.initial_scene,
            tile_types: stub.tile_types
        })
    }
}

#[cfg(test)]
mod tests {
    use std::{fs::{create_dir, remove_dir_all, File}, path::PathBuf, str::FromStr};

    use crate::parse::{project::{asset_group_from_single, normalize_glob, AssetGroup, AssetGroups, SaveItem}, scenes::SceneInstance, scheme_util::{eval_str, with_guile}, values::TypedValue};

    use super::SkyliteProjectStub;

    extern "C" fn test_project_parsing_impl(_: &()) {
        unsafe {
            let definition = eval_str(r#"
                '((name . TestProject)
                  (assets .
                    ((actors . ("./test1/*.scm" "./test2/*.scm"))
                     (maps . ("./test3/*.scm"))))

                    (save-data .
                      ((flag1 bool #f)
                      (val2 u8 5)))

                    (initial-scene . (basic_scene_1 "test"))
                    (tile-types . (solid semi-solid non-solid)))"#).unwrap();

            // Use a path to the test project to resolve the initial-scene
            let project_root = PathBuf::from_str("../skylite-core/tests/test-project-1/").unwrap();
            let project = SkyliteProjectStub::from_scheme(definition, &project_root).unwrap();
            assert_eq!(project, SkyliteProjectStub {
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
                initial_scene: SceneInstance {
                    name: "BasicScene1".to_owned(),
                    args: vec![
                        TypedValue::String("test".to_owned()),
                    ]
                },
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
        assert_eq!(asset_group1.find_asset("test_1").unwrap().0, 0);
        assert_eq!(asset_group1.find_asset("TEST_1").unwrap().0, 0);
        assert_eq!(asset_group1.find_asset("test-2").unwrap().0, 1);

        // Test name not matched by glob
        assert!(asset_group1.find_asset("asset").is_err());

        // Test ambiguous name
        let asset_group2 = asset_group_from_single("**/asset.scm", &test_dir);
        assert!(asset_group2.find_asset("asset").is_err());

        remove_dir_all(test_dir).unwrap();
    }
}
