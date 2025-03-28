use std::collections::HashMap;

use super::nodes::NodeInstance;
use super::scheme_util::iter_list;
use crate::assets::{AssetMetaData, Assets};
use crate::parse::scheme_util::with_guile;
use crate::{Node, SkyliteProcError};

pub(crate) struct NodeList {
    pub meta: AssetMetaData,
    pub content: Vec<NodeInstance>,
}

impl NodeList {
    pub(crate) fn from_meta_guile(
        meta: AssetMetaData,
        nodes: &HashMap<String, Node>,
        assets: &Assets,
    ) -> Result<NodeList, SkyliteProcError> {
        let def = meta.source.load_with_guile()?;
        let content = unsafe {
            iter_list(def)?
                .map(|item| NodeInstance::from_scheme(item, nodes, assets))
                .collect::<Result<Vec<NodeInstance>, SkyliteProcError>>()?
        };
        Ok(NodeList { meta, content })
    }

    pub(crate) fn from_meta(
        meta: &AssetMetaData,
        nodes: &HashMap<String, Node>,
        assets: &Assets,
    ) -> Result<NodeList, SkyliteProcError> {
        // Since we are not actually accessing anything from this signature from C,
        // we can get away with ignoring the missing C representations.
        #[allow(improper_ctypes_definitions)]
        extern "C" fn from_meta_c(
            args: &(&AssetMetaData, &HashMap<String, Node>, &Assets),
        ) -> Result<NodeList, SkyliteProcError> {
            let (meta, nodes, assets) = *args;
            NodeList::from_meta_guile(meta.clone(), nodes, assets)
        }

        with_guile(from_meta_c, &(meta, nodes, assets))
    }
}
