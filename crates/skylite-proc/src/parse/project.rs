use std::fs::read_to_string;
use std::path::{Path, PathBuf, MAIN_SEPARATOR_STR};

use glob::{GlobError, Pattern};

use super::actors::Actor;
use super::nodes::{Node, NodeInstance};
use super::scenes::{Scene, SceneInstance};
use super::values::{parse_type, parse_typed_value, TypedValue};
use crate::parse::guile::{scm_is_false, scm_list_p, SCM};
use crate::parse::scheme_util::CXROp::{CAR, CDR};
use crate::parse::scheme_util::{
    assq_str, cxr, eval_str, iter_list, parse_string, parse_symbol, with_guile,
};
use crate::SkyliteProcError;

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
    globs: Vec<String>,
}

impl AssetGroup {
    fn from_scheme(list: SCM, base_dir: &Path) -> Result<AssetGroup, SkyliteProcError> {
        let mut globs: Vec<String> = Vec::new();
        unsafe {
            for g in iter_list(list)? {
                let glob = normalize_glob(&parse_string(g)?, base_dir);
                Pattern::new(&glob).map_err(|err| {
                    SkyliteProcError::DataError(format!("Error parsing glob: {}", err))
                })?;
                globs.push(glob);
            }
        }
        Ok(AssetGroup { globs })
    }

    /// Returns a unique id and the file path for a given asset name. The name
    /// of an asset is the last component of its filename without the file
    /// extension.
    ///
    /// This method will return an error if the name does not exist, or
    /// is ambiguous among the assets matched by the `AssetGroup`.
    ///
    /// The ids can be used to reference a particular asset in the encoded data
    /// of other assets.
    pub(crate) fn find_asset(&self, name: &str) -> Result<(usize, PathBuf), SkyliteProcError> {
        let mut out: Option<(usize, PathBuf)> = None;
        for (idx, entry_res) in self.into_iter().enumerate() {
            let entry = match entry_res {
                Ok(e) => e,
                Err(err) => return Err(SkyliteProcError::OtherError(format!("IO Error: {}", err))),
            };

            if entry.file_stem().unwrap().to_str().unwrap() != name {
                continue;
            }

            if let Some((_, prev_entry)) = out {
                return Err(SkyliteProcError::DataError(format!(
                    "Name {} is ambiguous; both {:?} and {:?} match",
                    name, prev_entry, entry
                )));
            }

            out = Some((idx, entry));
        }

        if let Some(id_and_path) = out {
            Ok(id_and_path)
        } else {
            Err(SkyliteProcError::DataError(format!(
                "Name not found: {}",
                name
            )))
        }
    }
}

pub(crate) struct AssetIterator<'base> {
    current_iter: glob::Paths,
    glob_idx: usize,
    asset_group: &'base AssetGroup,
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
            asset_group: self,
        }
    }
}

/// Container for `AssetGroups` for all asset types used by Skylite.
#[derive(PartialEq, Debug)]
pub(crate) struct AssetGroups {
    pub nodes: AssetGroup,
    pub actors: AssetGroup,
    pub scenes: AssetGroup,
    pub plays: AssetGroup,
    pub graphics: AssetGroup,
    pub sprites: AssetGroup,
    pub tilesets: AssetGroup,
    pub maps: AssetGroup,
}

impl AssetGroups {
    fn from_scheme(alist: SCM, base_dir: &Path) -> Result<AssetGroups, SkyliteProcError> {
        unsafe {
            if scm_is_false(scm_list_p(alist)) {
                return Err(SkyliteProcError::DataError(format!(
                    "Asset directories must be defined as an associative list."
                )));
            }
            let mut out = create_default_asset_groups(base_dir);

            if let Some(expr) = assq_str("nodes", alist)? {
                out.nodes = AssetGroup::from_scheme(expr, base_dir)?;
            }
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
        globs: vec![normalize_glob(pattern, base_dir)],
    }
}

fn create_default_asset_groups(base_dir: &Path) -> AssetGroups {
    AssetGroups {
        nodes: asset_group_from_single("./nodes/*.scm", base_dir),
        actors: asset_group_from_single("./actors/*.scm", base_dir),
        scenes: asset_group_from_single("./scenes/*.scm", base_dir),
        plays: asset_group_from_single("./plays/*.scm", base_dir),
        graphics: asset_group_from_single("./graphics/*.scm", base_dir),
        sprites: asset_group_from_single("./sprites/*.scm", base_dir),
        tilesets: asset_group_from_single("./tilesets/*.scm", base_dir),
        maps: asset_group_from_single("./maps/*.scm", base_dir),
    }
}

#[derive(PartialEq, Debug)]
pub(crate) struct SaveItem {
    name: String,
    data: TypedValue,
}

impl SaveItem {
    fn from_scheme(definition: SCM) -> Result<SaveItem, SkyliteProcError> {
        unsafe {
            let typename = parse_type(cxr(definition, &[CDR, CAR])?)?;
            Ok(SaveItem {
                name: parse_symbol(cxr(definition, &[CAR])?)?,
                data: parse_typed_value(&typename, cxr(definition, &[CDR, CDR, CAR])?)?,
            })
        }
    }
}

// Early form of `SkyliteProject`, where the assets are not yet
// resolved and parsed. Used for contexts where the full representation
// of the project is not required, e.g. node_definition`.
#[derive(PartialEq, Debug)]
pub(crate) struct SkyliteProjectStub {
    pub name: String,
    pub assets: AssetGroups,
    pub root_node: NodeInstance,
    pub save_data: Vec<SaveItem>,
    pub tile_types: Vec<String>,
}

impl SkyliteProjectStub {
    fn from_scheme(
        definition: SCM,
        project_root: &Path,
    ) -> Result<SkyliteProjectStub, SkyliteProcError> {
        unsafe {
            let name = parse_symbol(assq_str("name", definition)?.ok_or(
                SkyliteProcError::DataError("Missing required field 'name'".to_owned()),
            )?)?;

            let assets = if let Some(alist) = assq_str("assets", definition)? {
                AssetGroups::from_scheme(alist, &project_root)?
            } else {
                create_default_asset_groups(&project_root)
            };

            let root_node = {
                let instance_def = assq_str("root-node", definition)?.ok_or(
                    SkyliteProcError::DataError(format!("Missing required field 'root-node'")),
                )?;
                NodeInstance::from_scheme(instance_def, &assets.nodes)?
            };

            let save_data = if let Some(list) = assq_str("save-data", definition)? {
                iter_list(list)?
                    .map(SaveItem::from_scheme)
                    .collect::<Result<Vec<SaveItem>, SkyliteProcError>>()?
            } else {
                Vec::new()
            };

            let tile_types = if let Some(list) = assq_str("tile-types", definition)? {
                iter_list(list)?
                    .map(|t| parse_symbol(t))
                    .collect::<Result<Vec<String>, SkyliteProcError>>()?
            } else {
                Vec::new()
            };

            if tile_types.len() == 0 {
                return Err(SkyliteProcError::DataError(
                    "At least one tile-type must be defined.".to_owned(),
                ));
            }

            Ok(SkyliteProjectStub {
                name,
                assets,
                root_node,
                save_data,
                tile_types,
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
            let resolved_path = path.canonicalize().map_err(|e| {
                SkyliteProcError::OtherError(format!("Error resolving project path: {}", e))
            })?;
            let definition_raw = read_to_string(path).map_err(|e| {
                SkyliteProcError::OtherError(format!("Error reading project definition: {}", e))
            })?;
            let definition = unsafe { eval_str(&definition_raw)? };

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
    pub nodes: Vec<Node>,
    pub actors: Vec<Actor>,
    pub scenes: Vec<Scene>,
    pub root_node: NodeInstance,
    pub _save_data: Vec<SaveItem>,
    pub tile_types: Vec<String>,
}

impl SkyliteProject {
    pub(crate) fn from_stub(stub: SkyliteProjectStub) -> Result<SkyliteProject, SkyliteProcError> {
        let nodes = Node::from_asset_group_all(&stub.assets.nodes)?;

        let actors = stub
            .assets
            .actors
            .into_iter()
            .map(|path_res| {
                let path = path_res.map_err(|err| {
                    SkyliteProcError::OtherError(format!("GlobError: {}", err.to_string()))
                })?;
                Actor::from_file(path.as_path())
            })
            .collect::<Result<Vec<Actor>, SkyliteProcError>>()?;

        let scenes = stub
            .assets
            .scenes
            .into_iter()
            .map(|path_res| {
                let path = path_res.map_err(|err| {
                    SkyliteProcError::OtherError(format!("GlobError: {}", err.to_string()))
                })?;
                Scene::from_file(path.as_path(), &actors)
            })
            .collect::<Result<Vec<Scene>, SkyliteProcError>>()?;

        Ok(SkyliteProject {
            name: stub.name,
            nodes,
            actors,
            scenes,
            root_node: stub.root_node,
            _save_data: stub.save_data,
            tile_types: stub.tile_types,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::fs::{create_dir, remove_dir_all, File};
    use std::path::PathBuf;
    use std::str::FromStr;

    use super::SkyliteProjectStub;
    use crate::parse::nodes::NodeInstance;
    use crate::parse::project::{
        asset_group_from_single, normalize_glob, AssetGroup, AssetGroups, SaveItem,
    };
    use crate::parse::values::TypedValue;

    #[test]
    fn test_project_parsing() {
        let project_root = PathBuf::from_str("../skylite-core/tests/test-project-1/")
            .unwrap()
            .canonicalize()
            .unwrap();
        let project = SkyliteProjectStub::from_file(&project_root.join("project.scm")).unwrap();
        assert_eq!(
            project,
            SkyliteProjectStub {
                name: "TestProject1".to_owned(),
                assets: AssetGroups {
                    nodes: asset_group_from_single("./nodes/*.scm", &project_root),
                    actors: asset_group_from_single("./actors/*.scm", &project_root),
                    scenes: asset_group_from_single("./scenes/*.scm", &project_root),
                    plays: asset_group_from_single("./plays/*.scm", &project_root),
                    graphics: asset_group_from_single("./graphics/*.scm", &project_root),
                    sprites: asset_group_from_single("./sprites/*.scm", &project_root),
                    tilesets: asset_group_from_single("./tilesets/*.scm", &project_root),
                    maps: asset_group_from_single("./maps/*.scm", &project_root)
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
                root_node: NodeInstance {
                    name: "basic-node-1".to_owned(),
                    args: vec![TypedValue::String("node1".to_owned()),]
                },
                tile_types: vec![
                    "solid".to_owned(),
                    "non-solid".to_owned(),
                    "semi-solid".to_owned()
                ]
            }
        );
    }

    #[test]
    fn test_calc_id_for_asset() {
        let test_dir_name = format!(
            "skylite_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs()
        );
        let test_dir = std::env::temp_dir().join(test_dir_name);
        let sub_dir = test_dir.join("sub");
        create_dir(&test_dir).unwrap();
        create_dir(&sub_dir).unwrap();
        drop(File::create(test_dir.clone().join("test_1.scm")).unwrap());
        drop(File::create(test_dir.clone().join("test_2.scm")).unwrap());
        drop(File::create(test_dir.clone().join("asset.scm")).unwrap());
        drop(File::create(sub_dir.clone().join("asset.scm")).unwrap());

        let asset_group1 = asset_group_from_single("test_?.scm", &test_dir);

        assert_eq!(asset_group1.find_asset("test_1").unwrap().0, 0);
        assert_eq!(asset_group1.find_asset("test_2").unwrap().0, 1);

        // Test name not matched by glob
        assert!(asset_group1.find_asset("asset").is_err());

        // Test ambiguous name
        let asset_group2 = asset_group_from_single("**/asset.scm", &test_dir);
        assert!(asset_group2.find_asset("asset").is_err());

        remove_dir_all(test_dir).unwrap();
    }
}
