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

/// An instantiation of a Node, containing arguments for a Node's parameters.
#[derive(PartialEq, Debug)]
pub(crate) struct NodeInstance {
    name: String,
    args: Vec<TypedValue>,
}

/// A partially parsed Node asset.
/// This type only includes the node's name and parameters, which is enough for
/// most use cases. Since this is called from a proc-macro, parsing only part of a
/// Node asset can reduce compilation time.
pub(crate) struct NodeStub {
    name: String,
    parameters: Vec<Variable>,
    properties: Option<SCM>,
    static_nodes: Option<SCM>,
    dynamic_nodes: Option<SCM>,
}

impl NodeStub {
    pub(crate) fn from_scheme(def: SCM, name: &str) -> Result<NodeStub, SkyliteProcError> {
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

    pub(crate) fn from_file(path: &Path) -> Result<NodeStub, SkyliteProcError> {
        // Since we are not actually accessing anything from this signature from C,
        // we can get away with ignoring the missing C representations.
        #[allow(improper_ctypes_definitions)]
        extern "C" fn from_file_guile(path: &Path) -> Result<NodeStub, SkyliteProcError> {
            let definition_raw = read_to_string(path).map_err(|e| {
                SkyliteProcError::OtherError(format!("Error reading project definition: {}", e))
            })?;
            let definition = unsafe { eval_str(&definition_raw)? };
            let name = &path.file_stem().unwrap().to_string_lossy();
            NodeStub::from_scheme(definition, &name)
        }

        with_guile(from_file_guile, path)
    }
}

fn parse_node_instance(
    instance: SCM,
    assets: &[PathBuf],
    stub_cache: &mut HashMap<String, NodeStub>,
) -> Result<NodeInstance, SkyliteProcError> {
    unsafe {
        let (instance_name, args_raw) = if scm_is_false(scm_pair_p(instance)) {
            return Err(SkyliteProcError::DataError(format!(
                "Expected node instance, found {}",
                form_to_string(instance)
            )));
        } else {
            (parse_symbol(scm_car(instance))?, scm_cdr(instance))
        };

        let stub = if let Some(s) = stub_cache.get(&instance_name) {
            s
        } else {
            let file = assets
                .iter()
                .find(|a| a.file_stem().unwrap().to_str().unwrap() == instance_name)
                .ok_or(SkyliteProcError::DataError(format!(
                    "Node {} not found.",
                    instance_name
                )))?;
            let stub = NodeStub::from_file(file)?;
            stub_cache.entry(instance_name.clone()).or_insert(stub)
        };

        Ok(NodeInstance {
            name: instance_name,
            args: parse_argument_list(args_raw, &stub.parameters)?,
        })
    }
}

fn parse_node_instance_list(
    instances_raw: SCM,
    assets: &[PathBuf],
    stub_cache: &mut HashMap<String, NodeStub>,
) -> Result<Vec<NodeInstance>, SkyliteProcError> {
    unsafe {
        iter_list(instances_raw)?
            .map(|item| parse_node_instance(item, assets, stub_cache))
            .collect::<Result<Vec<NodeInstance>, SkyliteProcError>>()
    }
}

/// Fully parsed Node asset.
#[derive(PartialEq, Debug)]
pub(crate) struct Node {
    name: String,
    parameters: Vec<Variable>,
    properties: Vec<Variable>,
    static_nodes: HashMap<String, NodeInstance>,
    dynamic_nodes: Vec<NodeInstance>,
}

impl Node {
    pub(crate) fn from_stub(
        stub: NodeStub,
        node_assets: &AssetGroup,
    ) -> Result<Node, SkyliteProcError> {
        let properties = unsafe {
            if let Some(properties_scm) = stub.properties {
                iter_list(properties_scm)?
                    .map(|p| parse_variable_definition(p))
                    .collect::<Result<Vec<Variable>, SkyliteProcError>>()?
            } else {
                vec![]
            }
        };

        let asset_files: Vec<PathBuf> = node_assets
            .into_iter()
            .collect::<Result<Vec<PathBuf>, GlobError>>()
            .map_err(|err| SkyliteProcError::OtherError(format!("IO Error: {}", err)))?;
        let mut stub_cache = HashMap::new();

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
                        let instance =
                            parse_node_instance(scm_cdr(item), &asset_files, &mut stub_cache)?;
                        Ok((name, instance))
                    })
                    .collect::<Result<HashMap<String, NodeInstance>, SkyliteProcError>>()?
            }
        } else {
            HashMap::new()
        };

        let dynamic_nodes = if let Some(dynamic_nodes_scm) = stub.dynamic_nodes {
            unsafe {
                iter_list(dynamic_nodes_scm)?
                    .map(|item| parse_node_instance(item, &asset_files, &mut stub_cache))
                    .collect::<Result<Vec<NodeInstance>, SkyliteProcError>>()?
            }
        } else {
            vec![]
        };

        Ok(Node {
            name: stub.name,
            parameters: stub.parameters,
            properties,
            static_nodes,
            dynamic_nodes,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, env, path::PathBuf, str::FromStr};

    use crate::{
        parse::{
            nodes::{Node, NodeInstance},
            project::AssetGroup,
            values::{Type, TypedValue, Variable},
        },
        SkyliteProjectStub,
    };

    use super::NodeStub;

    #[test]
    fn test_parse_node() {
        let project_dir = PathBuf::from_str(env!("CARGO_MANIFEST_DIR"))
            .unwrap()
            .join("../skylite-core/tests/test-project-1")
            .canonicalize()
            .unwrap();

        let stub = NodeStub::from_file(&project_dir.join("nodes/basic-node-1.scm")).unwrap();

        assert_eq!(stub.name, "basic-node-1");
        assert_eq!(
            stub.parameters,
            vec![Variable {
                name: "id".to_owned(),
                typename: Type::String,
                documentation: None,
                default: None
            }]
        );

        let project_stub = SkyliteProjectStub::from_file(&project_dir.join("project.scm")).unwrap();
        let node = Node::from_stub(stub, &project_stub.assets.nodes).unwrap();
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
                static_nodes: HashMap::from([
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
                            name: "basic-node-2".to_owned(),
                            args: vec![TypedValue::String("sub2".to_owned())]
                        }
                    )
                ]),
                dynamic_nodes: vec![NodeInstance {
                    name: "basic-node-2".to_owned(),
                    args: vec![TypedValue::String("dynamic1".to_owned())]
                }]
            }
        )
    }
}
