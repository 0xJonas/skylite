use std::collections::HashMap;
use std::fs::read_to_string;
use std::path::{Path, PathBuf};

use glob::GlobError;

use super::guile::{scm_car, scm_cdr, SCM};
use super::project::AssetGroup;
use super::scheme_util::{iter_list, parse_symbol};
use super::values::{parse_argument_list, parse_variable_definition, TypedValue, Variable};
use crate::parse::guile::{scm_is_false, scm_is_null, scm_pair_p};
use crate::parse::scheme_util::{assq_str, eval_str, form_to_string, with_guile};
use crate::SkyliteProcError;

/// A partially parsed NodeInstance.
/// To resolve a NodeInstanceStub to a NodeInstance, a NodeStub is needed
/// to supply the necessary parameter type information.
struct NodeInstanceStub {
    name: String,
    args_raw: SCM,
}

impl NodeInstanceStub {
    fn from_scheme(def: SCM) -> Result<NodeInstanceStub, SkyliteProcError> {
        unsafe {
            if scm_is_false(scm_pair_p(def)) {
                return Err(SkyliteProcError::DataError(format!(
                    "Expected node instance, found {}",
                    form_to_string(def)
                )));
            } else {
                Ok(NodeInstanceStub {
                    name: parse_symbol(scm_car(def))?,
                    args_raw: scm_cdr(def),
                })
            }
        }
    }
}

/// An instantiation of a Node, containing arguments for a Node's parameters.
#[derive(PartialEq, Debug)]
pub(crate) struct NodeInstance {
    pub name: String,
    pub args: Vec<TypedValue>,
}

impl NodeInstance {
    /// Build a NodeInstance from a NodeInstanceStub.
    /// The NodeStub required to process the arguments to the instance is
    /// looked up from a cache, or loaded once if it is not found.
    fn from_stub_cached(
        instance_stub: NodeInstanceStub,
        asset_files: &[PathBuf],
        stub_cache: &mut HashMap<String, NodeStub>,
    ) -> Result<NodeInstance, SkyliteProcError> {
        let node_stub = if let Some(s) = stub_cache.get(&instance_stub.name) {
            // NodeStub found in cache
            s
        } else {
            // NodeStub was not found, create it and add it to the cache.
            let file = asset_files
                .iter()
                .find(|a| a.file_stem().unwrap().to_str().unwrap() == instance_stub.name)
                .ok_or(SkyliteProcError::DataError(format!(
                    "Node {} not found.",
                    instance_stub.name
                )))?;
            let stub = NodeStub::from_file_guile(file)?;
            stub_cache.entry(instance_stub.name.clone()).or_insert(stub)
        };

        Ok(NodeInstance {
            name: instance_stub.name,
            args: unsafe { parse_argument_list(instance_stub.args_raw, &node_stub.parameters)? },
        })
    }

    /// Build a NodeInstance from a NodeInstanceStub.
    /// The required NodeStub must be available in the `node_stubs` map.
    fn from_stub(
        instance_stub: NodeInstanceStub,
        node_stubs: &HashMap<String, NodeStub>,
    ) -> Result<NodeInstance, SkyliteProcError> {
        let node_stub = if let Some(s) = node_stubs.get(&instance_stub.name) {
            s
        } else {
            return Err(SkyliteProcError::DataError(format!(
                "Node not found: {}",
                instance_stub.name
            )));
        };

        Ok(NodeInstance {
            name: instance_stub.name,
            args: unsafe { parse_argument_list(instance_stub.args_raw, &node_stub.parameters)? },
        })
    }

    pub fn from_scheme(
        definition: SCM,
        assets: &AssetGroup,
    ) -> Result<NodeInstance, SkyliteProcError> {
        let stub = NodeInstanceStub::from_scheme(definition)?;
        let (_, node_file) = assets.find_asset(&stub.name)?;
        let node = NodeStub::from_file_guile(&node_file)?;
        let cache: HashMap<String, NodeStub> = [(stub.name.clone(), node)].into_iter().collect();
        NodeInstance::from_stub(stub, &cache)
    }
}

/// A partially parsed Node asset.
struct NodeStub {
    name: String,
    parameters: Vec<Variable>,
    properties: Option<SCM>,
    static_nodes: Option<SCM>,
    dynamic_nodes: Option<SCM>,
}

impl NodeStub {
    fn from_scheme(def: SCM, name: &str) -> Result<NodeStub, SkyliteProcError> {
        unsafe {
            if scm_is_false(scm_pair_p(def)) && !scm_is_null(def) {
                return Err(SkyliteProcError::DataError(format!(
                    "Expected list for node, got {}",
                    form_to_string(def)
                )));
            }

            let maybe_parameters = assq_str("parameters", def)?;

            let parameters = if let Some(parameters_scm) = maybe_parameters {
                iter_list(parameters_scm)?
                    .map(|p| parse_variable_definition(p))
                    .collect::<Result<Vec<Variable>, SkyliteProcError>>()?
            } else {
                vec![]
            };

            let maybe_properties = assq_str("properties", def)?;
            let maybe_static_nodes = assq_str("static-nodes", def)?;
            let maybe_dynamic_nodes = assq_str("dynamic-nodes", def)?;

            Ok(NodeStub {
                name: name.to_owned(),
                parameters,
                properties: maybe_properties,
                static_nodes: maybe_static_nodes,
                dynamic_nodes: maybe_dynamic_nodes,
            })
        }
    }

    fn from_file_guile(path: &Path) -> Result<NodeStub, SkyliteProcError> {
        let definition_raw = read_to_string(path).map_err(|e| {
            SkyliteProcError::OtherError(format!("Error reading project definition: {}", e))
        })?;
        let definition = unsafe { eval_str(&definition_raw)? };
        let name = &path.file_stem().unwrap().to_string_lossy();
        NodeStub::from_scheme(definition, &name)
    }
}

/// Fully parsed Node asset.
#[derive(PartialEq, Debug)]
pub(crate) struct Node {
    pub name: String,
    pub parameters: Vec<Variable>,
    pub properties: Vec<Variable>,
    pub static_nodes: Vec<(String, NodeInstance)>,
    pub dynamic_nodes: Vec<NodeInstance>,
}

impl Node {
    fn from_stub<F: FnMut(NodeInstanceStub) -> Result<NodeInstance, SkyliteProcError>>(
        stub: &NodeStub,
        mut resolve_instance_fn: F,
    ) -> Result<Node, SkyliteProcError> {
        let properties = if let Some(properties_scm) = stub.properties {
            unsafe {
                iter_list(properties_scm)?
                    .map(|p| parse_variable_definition(p))
                    .collect::<Result<Vec<Variable>, SkyliteProcError>>()?
            }
        } else {
            vec![]
        };

        let static_nodes = if let Some(static_nodes_scm) = stub.static_nodes {
            unsafe {
                iter_list(static_nodes_scm)?
                    .map(|item| {
                        if scm_is_false(scm_pair_p(item)) {
                            return Err(SkyliteProcError::DataError(format!(
                                "Expected (name . instance) pair for static node, got {}",
                                form_to_string(item)
                            )));
                        }

                        let name = parse_symbol(scm_car(item))?;
                        let stub = NodeInstanceStub::from_scheme(scm_cdr(item))?;
                        Ok((name, resolve_instance_fn(stub)?))
                    })
                    .collect::<Result<Vec<(String, NodeInstance)>, SkyliteProcError>>()?
            }
        } else {
            Vec::new()
        };

        let dynamic_nodes = if let Some(dynamic_nodes_scm) = stub.dynamic_nodes {
            unsafe {
                iter_list(dynamic_nodes_scm)?
                    .map(|item| {
                        let stub = NodeInstanceStub::from_scheme(item)?;
                        resolve_instance_fn(stub)
                    })
                    .collect::<Result<Vec<NodeInstance>, SkyliteProcError>>()?
            }
        } else {
            vec![]
        };

        Ok(Node {
            name: stub.name.clone(),
            parameters: stub.parameters.clone(),
            properties,
            static_nodes,
            dynamic_nodes,
        })
    }

    /// Creates a single Node from an asset file.
    pub(crate) fn from_file_single(
        path: &Path,
        node_assets: &AssetGroup,
    ) -> Result<Node, SkyliteProcError> {
        // Since we are not actually accessing anything from this signature from C,
        // we can get away with ignoring the missing C representations.
        #[allow(improper_ctypes_definitions)]
        extern "C" fn from_file_single_guile(
            params: &(&Path, &AssetGroup),
        ) -> Result<Node, SkyliteProcError> {
            let (path, node_assets) = *params;
            let stub = NodeStub::from_file_guile(path)?;
            let asset_files: Vec<PathBuf> = node_assets
                .into_iter()
                .collect::<Result<Vec<PathBuf>, GlobError>>()
                .map_err(|err| SkyliteProcError::OtherError(format!("IO Error: {}", err)))?;
            let mut stub_cache = HashMap::new();

            Node::from_stub(&stub, |instance_stub| {
                NodeInstance::from_stub_cached(instance_stub, &asset_files, &mut stub_cache)
            })
        }

        with_guile(from_file_single_guile, &(path, node_assets))
    }

    /// Creates Nodes for each asset file in the `AssetGroup`.
    /// If all nodes of a group should be loaded, this is more efficient
    /// than calling `from_file_single` for each asset file.
    pub(crate) fn from_asset_group_all(
        node_assets: &AssetGroup,
    ) -> Result<Vec<Node>, SkyliteProcError> {
        // Since we are not actually accessing anything from this signature from C,
        // we can get away with ignoring the missing C representations.
        #[allow(improper_ctypes_definitions)]
        extern "C" fn from_asset_group_all_guile(
            node_assets: &AssetGroup,
        ) -> Result<Vec<Node>, SkyliteProcError> {
            let stubs = node_assets
                .into_iter()
                .map(|path_res| {
                    let path = path_res.map_err(|err| {
                        SkyliteProcError::OtherError(format!("IO Error: {}", err))
                    })?;
                    let name = path.file_stem().unwrap().to_str().unwrap().to_owned();
                    let stub = NodeStub::from_file_guile(&path)?;
                    Ok((name, stub))
                })
                .collect::<Result<HashMap<String, NodeStub>, SkyliteProcError>>()?;

            stubs
                .values()
                .map(|stub| {
                    Node::from_stub(stub, |instance_stub| {
                        NodeInstance::from_stub(instance_stub, &stubs)
                    })
                })
                .collect::<Result<Vec<Node>, SkyliteProcError>>()
        }

        with_guile(from_asset_group_all_guile, node_assets)
    }
}

#[cfg(test)]
mod tests {
    use std::env;
    use std::path::PathBuf;
    use std::str::FromStr;

    use crate::parse::nodes::{Node, NodeInstance};
    use crate::parse::values::{Type, TypedValue, Variable};
    use crate::SkyliteProjectStub;

    #[test]
    fn test_parse_node() {
        let project_dir = PathBuf::from_str(env!("CARGO_MANIFEST_DIR"))
            .unwrap()
            .join("../skylite-core/tests/test-project-1")
            .canonicalize()
            .unwrap();

        let project_stub = SkyliteProjectStub::from_file(&project_dir.join("project.scm")).unwrap();
        let node = Node::from_file_single(
            &project_dir.join("nodes/basic-node-1.scm"),
            &project_stub.assets.nodes,
        )
        .unwrap();
        assert_eq!(
            node,
            Node {
                name: "basic-node-1".to_owned(),
                parameters: vec![Variable {
                    name: "id".to_owned(),
                    typename: Type::String,
                    documentation: None,
                    default: None
                }],
                properties: vec![Variable {
                    name: "id".to_owned(),
                    typename: Type::String,
                    documentation: None,
                    default: None
                }],
                static_nodes: vec![
                    (
                        "sub1".to_owned(),
                        NodeInstance {
                            name: "basic-node-2".to_owned(),
                            args: vec![TypedValue::String("sub1".to_owned())]
                        }
                    ),
                    (
                        "sub2".to_owned(),
                        NodeInstance {
                            name: "z-order-node".to_owned(),
                            args: vec![TypedValue::String("sub2".to_owned()), TypedValue::I16(2)]
                        }
                    )
                ],
                dynamic_nodes: vec![
                    NodeInstance {
                        name: "basic-node-2".to_owned(),
                        args: vec![TypedValue::String("dynamic1".to_owned())]
                    },
                    NodeInstance {
                        name: "z-order-node".to_owned(),
                        args: vec![
                            TypedValue::String("dynamic2".to_owned()),
                            TypedValue::I16(-1)
                        ]
                    }
                ]
            }
        )
    }
}
