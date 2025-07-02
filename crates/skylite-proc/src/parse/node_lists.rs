use super::nodes::NodeInstance;
use super::scheme_util::iter_list;
use crate::assets::{AssetMetaData, Assets};
use crate::parse::scheme_util::with_guile;
use crate::SkyliteProcError;

#[derive(Debug, Clone)]
pub(crate) struct NodeList {
    pub meta: AssetMetaData,
    pub content: Vec<NodeInstance>,
}

impl NodeList {
    pub(crate) fn from_meta_guile(
        meta: AssetMetaData,
        assets: &mut Assets,
    ) -> Result<NodeList, SkyliteProcError> {
        let def = meta.source.load_with_guile()?;
        let content = unsafe {
            iter_list(def)?
                .map(|item| NodeInstance::from_scheme_with_guile(item, assets))
                .collect::<Result<Vec<NodeInstance>, SkyliteProcError>>()?
        };
        Ok(NodeList { meta, content })
    }

    pub(crate) fn from_meta(
        meta: AssetMetaData,
        assets: &mut Assets,
    ) -> Result<NodeList, SkyliteProcError> {
        // Since we are not actually accessing anything from this signature from C,
        // we can get away with ignoring the missing C representations.
        #[allow(improper_ctypes_definitions)]
        extern "C" fn from_meta_inner(
            args: (AssetMetaData, &mut Assets),
        ) -> Result<NodeList, SkyliteProcError> {
            let (meta, assets) = args;
            NodeList::from_meta_guile(meta, assets)
        }

        with_guile(from_meta_inner, (meta, assets))
    }
}
