use std::collections::HashMap;
use std::fs::read_to_string;

use super::guile::SCM;
use super::nodes::NodeInstance;
use super::scheme_util::iter_list;
use crate::assets::{AssetMetaData, Assets};
use crate::parse::scheme_util::{eval_str, with_guile};
use crate::{Node, SkyliteProcError};

pub(crate) struct NodeList {
    pub meta: AssetMetaData,
    pub content: Vec<NodeInstance>,
}

impl NodeList {
    pub(crate) fn from_scheme(
        def: SCM,
        meta: AssetMetaData,
        nodes: &HashMap<String, Node>,
        assets: &Assets,
    ) -> Result<NodeList, SkyliteProcError> {
        let content = unsafe {
            iter_list(def)?
                .map(|item| NodeInstance::from_scheme(item, nodes, assets))
                .collect::<Result<Vec<NodeInstance>, SkyliteProcError>>()?
        };
        Ok(NodeList { meta, content })
    }

    pub(crate) fn from_file(
        meta: &AssetMetaData,
        nodes: &HashMap<String, Node>,
        assets: &Assets,
    ) -> Result<NodeList, SkyliteProcError> {
        // Since we are not actually accessing anything from this signature from C,
        // we can get away with ignoring the missing C representations.
        #[allow(improper_ctypes_definitions)]
        extern "C" fn from_file_with_guile(
            args: &(&AssetMetaData, &HashMap<String, Node>, &Assets),
        ) -> Result<NodeList, SkyliteProcError> {
            let (meta, nodes, assets) = *args;
            let definition_raw = read_to_string(&meta.path).map_err(|e| {
                SkyliteProcError::OtherError(format!("Error reading node list: {}", e))
            })?;

            unsafe {
                let definition = eval_str(&definition_raw)?;
                NodeList::from_scheme(definition, meta.clone(), nodes, assets)
            }
        }

        with_guile(from_file_with_guile, &(meta, nodes, assets))
    }
}
