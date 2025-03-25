use std::fs::read_to_string;
use std::path::Path;

use super::node_lists::NodeList;
use super::nodes::{Node, NodeInstance};
use super::values::{parse_type, parse_typed_value, TypedValue};
use crate::assets::Assets;
use crate::parse::guile::SCM;
use crate::parse::scheme_util::CXROp::{CAR, CDR};
use crate::parse::scheme_util::{assq_str, cxr, eval_str, iter_list, parse_symbol, with_guile};
use crate::SkyliteProcError;

#[derive(PartialEq, Debug)]
pub(crate) struct SaveItem {
    name: String,
    data: TypedValue,
}

impl SaveItem {
    fn from_scheme(definition: SCM, assets: &Assets) -> Result<SaveItem, SkyliteProcError> {
        unsafe {
            let typename = parse_type(cxr(definition, &[CDR, CAR])?)?;
            Ok(SaveItem {
                name: parse_symbol(cxr(definition, &[CAR])?)?,
                data: parse_typed_value(&typename, cxr(definition, &[CDR, CDR, CAR])?, assets)?,
            })
        }
    }
}

// Early form of `SkyliteProject`, where the assets are not yet
// resolved and parsed. Used for contexts where the full representation
// of the project is not required, e.g. node_definition`.
#[derive(Debug)]
pub(crate) struct SkyliteProjectStub {
    pub name: String,
    pub assets: Assets,
    pub root_node_def: SCM,
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

            let assets =
                Assets::from_scheme_with_guile(assq_str("assets", definition)?, project_root)?;

            let root_node_def = assq_str("root-node", definition)?.ok_or(
                SkyliteProcError::DataError(format!("Missing required field 'root-node'")),
            )?;

            let save_data = if let Some(list) = assq_str("save-data", definition)? {
                iter_list(list)?
                    .map(|item| SaveItem::from_scheme(item, &assets))
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
                root_node_def,
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

#[cfg(test)]
impl PartialEq for SkyliteProjectStub {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
            && self.assets == other.assets
            // && self.root_node_def == other.root_node_def // exclude SCM type
            && self.save_data == other.save_data
            && self.tile_types == other.tile_types
    }
}

/// Main type for managing the asset files and code generation
/// of a Skylite project.
pub(crate) struct SkyliteProject {
    pub name: String,
    pub nodes: Vec<Node>,
    pub node_lists: Vec<NodeList>,
    pub root_node: NodeInstance,
    pub _save_data: Vec<SaveItem>,
    pub tile_types: Vec<String>,
}

impl SkyliteProject {
    pub(crate) fn from_stub(stub: SkyliteProjectStub) -> Result<SkyliteProject, SkyliteProcError> {
        let nodes = Node::parse_all_nodes(&stub.assets)?;
        let node_lists = stub
            .assets
            .node_lists
            .values()
            .map(|meta| NodeList::from_file(meta, &nodes, &stub.assets))
            .collect::<Result<Vec<NodeList>, SkyliteProcError>>()?;

        let root_node = NodeInstance::from_scheme(stub.root_node_def, &nodes, &stub.assets)?;

        let nodes_vec: Vec<Node> = nodes.into_values().collect();

        Ok(SkyliteProject {
            name: stub.name,
            nodes: nodes_vec,
            node_lists,
            root_node: root_node,
            _save_data: stub.save_data,
            tile_types: stub.tile_types,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::ptr;

    use super::SkyliteProjectStub;
    use crate::assets::tests::create_tmp_fs;
    use crate::assets::{AssetMetaData, AssetType, Assets};
    use crate::parse::project::SaveItem;
    use crate::parse::values::TypedValue;

    #[test]
    fn test_project_parsing() {
        let tmp_fs = create_tmp_fs(&[
            (
                "project.scm",
                r#"
                '((name . TestProject1)
                  (save-data . ((flag1 bool #f) (val2 u8 5)))
                  (root-node . (basic-node-1))
                  (tile-types . (solid non-solid semi-solid)))"#,
            ),
            ("nodes/basic-node-1.scm", r#"'()"#),
            ("nodes/basic-node-2.scm", r#"'()"#),
        ])
        .unwrap();

        let project = SkyliteProjectStub::from_file(&tmp_fs.path().join("project.scm")).unwrap();
        assert_eq!(
            project,
            SkyliteProjectStub {
                name: "TestProject1".to_owned(),
                assets: Assets {
                    nodes: [
                        (
                            "basic-node-1".to_owned(),
                            AssetMetaData {
                                atype: AssetType::Node,
                                id: 0,
                                name: "basic-node-1".to_owned(),
                                path: tmp_fs.path().join("nodes/basic-node-1.scm")
                            }
                        ),
                        (
                            "basic-node-2".to_owned(),
                            AssetMetaData {
                                atype: AssetType::Node,
                                id: 1,
                                name: "basic-node-2".to_owned(),
                                path: tmp_fs.path().join("nodes/basic-node-2.scm")
                            }
                        )
                    ]
                    .into(),
                    node_lists: HashMap::new()
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
                root_node_def: ptr::null_mut(),
                tile_types: vec![
                    "solid".to_owned(),
                    "non-solid".to_owned(),
                    "semi-solid".to_owned()
                ]
            }
        );
    }
}
