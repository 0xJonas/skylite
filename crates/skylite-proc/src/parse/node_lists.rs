use std::collections::HashMap;
use std::fs::read_to_string;
use std::path::Path;

use super::guile::SCM;
use super::nodes::NodeInstance;
use super::scheme_util::iter_list;
use crate::parse::scheme_util::{eval_str, with_guile};
use crate::{Node, SkyliteProcError};

pub(crate) struct NodeList(Vec<NodeInstance>);

impl NodeList {
    pub(crate) fn from_scheme(
        def: SCM,
        nodes: &HashMap<String, Node>,
    ) -> Result<NodeList, SkyliteProcError> {
        let instances = unsafe {
            iter_list(def)?
                .map(|item| NodeInstance::from_scheme(item, nodes))
                .collect::<Result<Vec<NodeInstance>, SkyliteProcError>>()?
        };
        Ok(NodeList(instances))
    }

    pub(crate) fn from_file(
        path: &Path,
        nodes: &HashMap<String, Node>,
    ) -> Result<NodeList, SkyliteProcError> {
        // Since we are not actually accessing anything from this signature from C,
        // we can get away with ignoring the missing C representations.
        #[allow(improper_ctypes_definitions)]
        extern "C" fn from_file_with_guile(
            args: &(&Path, &HashMap<String, Node>),
        ) -> Result<NodeList, SkyliteProcError> {
            let (path, nodes) = *args;
            let definition_raw = read_to_string(path).map_err(|e| {
                SkyliteProcError::OtherError(format!("Error reading project definition: {}", e))
            })?;

            unsafe {
                let definition = eval_str(&definition_raw)?;
                NodeList::from_scheme(definition, nodes)
            }
        }

        with_guile(from_file_with_guile, &(path, nodes))
    }
}
