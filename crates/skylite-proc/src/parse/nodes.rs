use std::collections::HashMap;

use super::guile::{scm_car, scm_cdr, SCM};
use super::scheme_util::{iter_list, parse_symbol};
use super::values::{parse_argument_list, parse_variable_definition, TypedValue, Variable};
use crate::assets::{AssetMetaData, Assets};
use crate::parse::guile::{scm_is_false, scm_is_null, scm_pair_p};
use crate::parse::scheme_util::{assq_str, form_to_string, with_guile};
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
                return Err(data_err!(
                    "Expected node instance, found {}",
                    form_to_string(def)
                ));
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
    pub node_id: usize,
    pub name: String,
    pub args: Vec<TypedValue>,
}

impl NodeInstance {
    /// Build a NodeInstance from a NodeInstanceStub.
    /// The NodeStub required to process the arguments to the instance is
    /// looked up from a cache, or loaded once if it is not found.
    fn from_stub_cached(
        instance_stub: NodeInstanceStub,
        stub_cache: &mut HashMap<String, NodeStub>,
        assets: &Assets,
    ) -> Result<NodeInstance, SkyliteProcError> {
        let node_stub = if instance_stub.name.starts_with("skylite/") {
            todo!()
        } else if let Some(s) = stub_cache.get(&instance_stub.name) {
            // NodeStub found in cache
            s
        } else {
            // NodeStub was not found, create it and add it to the cache.
            let meta = assets
                .nodes
                .get(&instance_stub.name)
                .ok_or(data_err!(
                    "No node asset with name {} found.",
                    &instance_stub.name
                ))?
                .to_owned();
            let stub = NodeStub::from_meta_guile(meta, assets)?;
            stub_cache.entry(instance_stub.name.clone()).or_insert(stub)
        };

        Ok(NodeInstance {
            node_id: node_stub.meta.id,
            name: node_stub.meta.name.clone(),
            args: unsafe {
                parse_argument_list(instance_stub.args_raw, &node_stub.parameters, assets)?
            },
        })
    }

    /// Build a NodeInstance from a NodeInstanceStub.
    /// The required NodeStub must be available in the `node_stubs` map.
    fn from_stub(
        instance_stub: NodeInstanceStub,
        node_stubs: &HashMap<String, NodeStub>,
        assets: &Assets,
    ) -> Result<NodeInstance, SkyliteProcError> {
        let node_stub = if let Some(s) = node_stubs.get(&instance_stub.name) {
            s
        } else {
            return Err(data_err!("Node not found: {}", instance_stub.name));
        };

        Ok(NodeInstance {
            node_id: node_stub.meta.id,
            name: node_stub.meta.name.clone(),
            args: unsafe {
                parse_argument_list(instance_stub.args_raw, &node_stub.parameters, assets)?
            },
        })
    }

    pub fn from_scheme(
        definition: SCM,
        nodes: &HashMap<String, Node>,
        assets: &Assets,
    ) -> Result<NodeInstance, SkyliteProcError> {
        let instance_stub = NodeInstanceStub::from_scheme(definition)?;

        let node = if let Some(s) = nodes.get(&instance_stub.name) {
            s
        } else {
            return Err(data_err!("Node not found: {}", instance_stub.name));
        };

        Ok(NodeInstance {
            node_id: node.meta.id,
            name: node.meta.name.clone(),
            args: unsafe { parse_argument_list(instance_stub.args_raw, &node.parameters, assets)? },
        })
    }
}

/// A partially parsed Node asset.
struct NodeStub {
    meta: AssetMetaData,
    parameters: Vec<Variable>,
    properties: Option<SCM>,
    static_nodes: Option<SCM>,
    dynamic_nodes: Option<SCM>,
}

impl NodeStub {
    fn from_meta_guile(meta: AssetMetaData, assets: &Assets) -> Result<NodeStub, SkyliteProcError> {
        let def = meta.source.load_with_guile()?;
        unsafe {
            if scm_is_false(scm_pair_p(def)) && !scm_is_null(def) {
                return Err(data_err!(
                    "Expected list for node, got {}",
                    form_to_string(def)
                ));
            }

            let maybe_parameters = assq_str("parameters", def)?;

            let parameters = if let Some(parameters_scm) = maybe_parameters {
                iter_list(parameters_scm)?
                    .map(|p| parse_variable_definition(p, assets))
                    .collect::<Result<Vec<Variable>, SkyliteProcError>>()?
            } else {
                vec![]
            };

            let maybe_properties = assq_str("properties", def)?;
            let maybe_static_nodes = assq_str("static-nodes", def)?;
            let maybe_dynamic_nodes = assq_str("dynamic-nodes", def)?;

            Ok(NodeStub {
                meta,
                parameters,
                properties: maybe_properties,
                static_nodes: maybe_static_nodes,
                dynamic_nodes: maybe_dynamic_nodes,
            })
        }
    }
}

/// Fully parsed Node asset.
#[derive(PartialEq, Debug)]
pub(crate) struct Node {
    pub meta: AssetMetaData,
    pub parameters: Vec<Variable>,
    pub properties: Vec<Variable>,
    pub static_nodes: Vec<(String, NodeInstance)>,
    pub dynamic_nodes: Vec<NodeInstance>,
}

impl Node {
    fn from_stub<F: FnMut(NodeInstanceStub) -> Result<NodeInstance, SkyliteProcError>>(
        stub: &NodeStub,
        assets: &Assets,
        mut resolve_instance_fn: F,
    ) -> Result<Node, SkyliteProcError> {
        let properties = if let Some(properties_scm) = stub.properties {
            unsafe {
                iter_list(properties_scm)?
                    .map(|p| parse_variable_definition(p, assets))
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
                            return Err(data_err!(
                                "Expected (name . instance) pair for static node, got {}",
                                form_to_string(item)
                            ));
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
            meta: stub.meta.to_owned(),
            parameters: stub.parameters.clone(),
            properties,
            static_nodes,
            dynamic_nodes,
        })
    }

    /// Creates a single Node from an asset file.
    pub(crate) fn from_meta(
        meta: AssetMetaData,
        assets: &Assets,
    ) -> Result<Node, SkyliteProcError> {
        // Since we are not actually accessing anything from this signature from C,
        // we can get away with ignoring the missing C representations.
        #[allow(improper_ctypes_definitions)]
        extern "C" fn from_meta_guile(
            params: &(&AssetMetaData, &Assets),
        ) -> Result<Node, SkyliteProcError> {
            let (meta, assets) = *params;
            let stub = NodeStub::from_meta_guile(meta.to_owned(), assets)?;
            let mut stub_cache = HashMap::new();

            Node::from_stub(&stub, assets, |instance_stub| {
                NodeInstance::from_stub_cached(instance_stub, &mut stub_cache, assets)
            })
        }

        with_guile(from_meta_guile, &(&meta, assets))
    }

    /// Creates Nodes for each asset file in the `AssetGroup`.
    /// If all nodes of a group should be loaded, this is more efficient
    /// than calling `from_file_single` for each asset file.
    pub(crate) fn parse_all_nodes(
        assets: &Assets,
    ) -> Result<HashMap<String, Node>, SkyliteProcError> {
        // Since we are not actually accessing anything from this signature from C,
        // we can get away with ignoring the missing C representations.
        #[allow(improper_ctypes_definitions)]
        extern "C" fn from_asset_group_all_guile(
            assets: &Assets,
        ) -> Result<HashMap<String, Node>, SkyliteProcError> {
            let stubs = assets
                .nodes
                .iter()
                .map(|(name, meta)| {
                    let stub = NodeStub::from_meta_guile(meta.clone(), assets)?;
                    Ok((name.clone(), stub))
                })
                .collect::<Result<HashMap<String, NodeStub>, SkyliteProcError>>()?;

            stubs
                .iter()
                .map(|(name, stub)| {
                    let node = Node::from_stub(stub, assets, |instance_stub| {
                        NodeInstance::from_stub(instance_stub, &stubs, assets)
                    })?;
                    Ok((name.clone(), node))
                })
                .collect::<Result<HashMap<String, Node>, SkyliteProcError>>()
        }

        with_guile(from_asset_group_all_guile, assets)
    }
}

#[cfg(test)]
mod tests {
    use std::env;
    use std::path::PathBuf;
    use std::str::FromStr;

    use crate::assets::{AssetMetaData, AssetSource, AssetType};
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
        let meta = AssetMetaData {
            id: 0,
            atype: AssetType::Node,
            name: "basic-node-1".to_owned(),
            source: AssetSource::Path(project_dir.join("nodes/basic-node-1.scm")),
        };
        let node = Node::from_meta(meta.clone(), &project_stub.assets).unwrap();
        assert_eq!(
            node,
            Node {
                meta,
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
                            node_id: 1,
                            name: "basic-node-2".to_owned(),
                            args: vec![TypedValue::String("sub1".to_owned())]
                        }
                    ),
                    (
                        "sub2".to_owned(),
                        NodeInstance {
                            node_id: 2,
                            name: "z-order-node".to_owned(),
                            args: vec![TypedValue::String("sub2".to_owned()), TypedValue::I16(2)]
                        }
                    )
                ],
                dynamic_nodes: vec![
                    NodeInstance {
                        node_id: 1,
                        name: "basic-node-2".to_owned(),
                        args: vec![TypedValue::String("dynamic1".to_owned())]
                    },
                    NodeInstance {
                        node_id: 2,
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
