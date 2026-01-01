use std::io::Read;
use std::path::Path;

use crate::asset_server::connect_to_asset_server;
use crate::base_serde::Deserialize;
use crate::{list_assets_conn, AssetError, AssetMeta, AssetType};

#[derive(Debug, Clone, PartialEq)]
pub struct Project {
    pub meta: AssetMeta,
    pub name: String,
}

impl Deserialize for Project {
    fn deserialize(input: &mut impl Read) -> Result<Self, AssetError>
    where
        Self: Sized,
    {
        let meta = AssetMeta::read(input)?;
        let name = String::deserialize(input)?;
        Ok(Project { meta, name })
    }
}

pub fn load_project(project_path: &Path) -> Result<Project, AssetError> {
    let mut connection = connect_to_asset_server()?;
    let project_assets = list_assets_conn(project_path, AssetType::Project, &mut connection)?;
    assert_eq!(project_assets.len(), 1);

    connection.send_load_asset_request(
        project_path,
        AssetType::Project,
        &project_assets[0].name,
    )?;

    let mut status = [0u8; 1];
    connection.read_exact(&mut status)?;
    if status[0] == 0 {
        Ok(Project::deserialize(&mut connection)?)
    } else {
        Err(AssetError::read(&mut connection))
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{load_project, Project};

    #[test]
    fn test_load_project_asset() {
        let project_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("./tests/test-project")
            .canonicalize()
            .unwrap();
        let project = load_project(&project_dir.join("project.rkt")).unwrap();

        assert_eq!(
            project,
            Project {
                meta: project.meta.clone(),
                name: "Test1".to_owned()
            }
        );
    }
}
