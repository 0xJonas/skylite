use std::fs::read_to_string;
use std::path::Path;

use super::nodes::NodeInstance;
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
    fn from_scheme(definition: SCM, assets: &mut Assets) -> Result<SaveItem, SkyliteProcError> {
        unsafe {
            let typename = parse_type(cxr(definition, &[CDR, CAR])?, &assets.index)?;
            Ok(SaveItem {
                name: parse_symbol(cxr(definition, &[CAR])?)?,
                data: parse_typed_value(&typename, cxr(definition, &[CDR, CDR, CAR])?, assets)?,
            })
        }
    }
}

/// Main type for managing the asset files and code generation
/// of a Skylite project.
#[derive(Debug)]
pub(crate) struct SkyliteProject {
    pub name: String,
    pub assets: Assets,
    pub root_node: Option<NodeInstance>,
    pub save_data: Vec<SaveItem>,
    pub tile_types: Vec<String>,
}

impl SkyliteProject {
    fn from_scheme(
        definition: SCM,
        project_root: &Path,
        parse_root_node: bool,
    ) -> Result<SkyliteProject, SkyliteProcError> {
        unsafe {
            let name = parse_symbol(
                assq_str("name", definition)?.ok_or(data_err!("Missing required field 'name'"))?,
            )?;

            let mut assets =
                Assets::from_scheme_with_guile(assq_str("assets", definition)?, project_root)?;

            let root_node_def = assq_str("root-node", definition)?
                .ok_or(data_err!("Missing required field 'root-node'"))?;
            let root_node = if parse_root_node {
                Some(NodeInstance::from_scheme_with_guile(
                    root_node_def,
                    &mut assets,
                )?)
            } else {
                None
            };

            let save_data = if let Some(list) = assq_str("save-data", definition)? {
                iter_list(list)?
                    .map(|item| SaveItem::from_scheme(item, &mut assets))
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
                return Err(data_err!("At least one tile-type must be defined."));
            }

            Ok(SkyliteProject {
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
    pub(crate) fn from_file(
        path: &Path,
        parse_root_node: bool,
    ) -> Result<SkyliteProject, SkyliteProcError> {
        // Since we are not actually accessing anything from this signature from C,
        // we can get away with ignoring the missing C representations.
        #[allow(improper_ctypes_definitions)]
        extern "C" fn from_file_guile(
            args: (&Path, bool),
        ) -> Result<SkyliteProject, SkyliteProcError> {
            let (path, parse_root_node) = args;
            let resolved_path = path.canonicalize().map_err(|e| {
                SkyliteProcError::OtherError(format!("Error resolving project path: {}", e))
            })?;
            let definition_raw = read_to_string(path).map_err(|e| {
                SkyliteProcError::OtherError(format!("Error reading project definition: {}", e))
            })?;
            let definition = unsafe { eval_str(&definition_raw)? };

            let project_root = resolved_path.parent().unwrap();
            SkyliteProject::from_scheme(definition, project_root, parse_root_node)
        }
        with_guile(from_file_guile, (path, parse_root_node))
    }
}

#[cfg(test)]
impl PartialEq for SkyliteProject {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
            && self.assets.index == other.assets.index
            && self.root_node == other.root_node
            && self.save_data == other.save_data
            && self.tile_types == other.tile_types
    }
}

#[cfg(test)]
mod tests {
    use super::SkyliteProject;
    use crate::assets::tests::create_tmp_fs;
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

        let project = SkyliteProject::from_file(&tmp_fs.path().join("project.scm"), false).unwrap();

        assert_eq!(project.name, "TestProject1");
        assert_eq!(
            project.save_data,
            vec![
                SaveItem {
                    name: "flag1".to_owned(),
                    data: TypedValue::Bool(false)
                },
                SaveItem {
                    name: "val2".to_owned(),
                    data: TypedValue::U8(5)
                }
            ]
        );
        assert_eq!(
            project.tile_types,
            vec![
                "solid".to_owned(),
                "non-solid".to_owned(),
                "semi-solid".to_owned()
            ]
        );
    }
}
