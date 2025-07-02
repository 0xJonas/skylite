use super::guile::{scm_car, scm_cdr, SCM};
use super::scheme_util::{iter_list, parse_symbol};
use super::values::{parse_argument_list, parse_variable_definition, TypedValue, Variable};
use crate::assets::{AssetMetaData, Assets};
use crate::parse::guile::{scm_is_false, scm_is_null, scm_pair_p};
use crate::parse::scheme_util::{assq_str, form_to_string, with_guile};
use crate::SkyliteProcError;

/// An instantiation of a Node, containing arguments for a Node's parameters.
#[derive(PartialEq, Debug, Clone)]
pub(crate) struct NodeInstance {
    pub node_id: usize,
    pub name: String,
    pub args: Vec<TypedValue>,
}

impl NodeInstance {
    pub fn from_scheme_with_guile(
        def: SCM,
        assets: &mut Assets,
    ) -> Result<NodeInstance, SkyliteProcError> {
        unsafe {
            if scm_is_false(scm_pair_p(def)) {
                return Err(data_err!(
                    "Expected node instance, found {}",
                    form_to_string(def)
                ));
            }

            let name = parse_symbol(scm_car(def))?;
            let args_raw = scm_cdr(def);

            let node = assets.load_node(&name)?;
            let node_id = node.meta.id;
            let name = node.meta.name.clone();
            let args = parse_argument_list(args_raw, &node.parameters.clone(), &assets.index)?;

            Ok(NodeInstance {
                node_id,
                name,
                args,
            })
        }
    }
}

/// Fully parsed Node asset.
#[derive(PartialEq, Debug, Clone)]
pub(crate) struct Node {
    pub meta: AssetMetaData,
    pub parameters: Vec<Variable>,
    pub properties: Vec<Variable>,
    pub static_nodes: Vec<(String, NodeInstance)>,
    pub dynamic_nodes: Vec<NodeInstance>,
}

impl Node {
    fn from_meta_with_guile(
        meta: AssetMetaData,
        assets: &mut Assets,
    ) -> Result<Node, SkyliteProcError> {
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
                    .map(|p| parse_variable_definition(p, &assets.index))
                    .collect::<Result<Vec<Variable>, SkyliteProcError>>()?
            } else {
                vec![]
            };

            let maybe_properties = assq_str("properties", def)?;
            let properties = if let Some(properties_scm) = maybe_properties {
                iter_list(properties_scm)?
                    .map(|p| parse_variable_definition(p, &assets.index))
                    .collect::<Result<Vec<Variable>, SkyliteProcError>>()?
            } else {
                vec![]
            };

            let maybe_static_nodes = assq_str("static-nodes", def)?;
            let static_nodes = if let Some(static_nodes_scm) = maybe_static_nodes {
                iter_list(static_nodes_scm)?
                    .map(|item| {
                        if scm_is_false(scm_pair_p(item)) {
                            return Err(data_err!(
                                "Expected (name . instance) pair for static node, got {}",
                                form_to_string(item)
                            ));
                        }

                        let name = parse_symbol(scm_car(item))?;
                        let instance = NodeInstance::from_scheme_with_guile(scm_cdr(item), assets)?;
                        Ok((name, instance))
                    })
                    .collect::<Result<Vec<(String, NodeInstance)>, SkyliteProcError>>()?
            } else {
                Vec::new()
            };

            let maybe_dynamic_nodes = assq_str("dynamic-nodes", def)?;
            let dynamic_nodes = if let Some(dynamic_nodes_scm) = maybe_dynamic_nodes {
                iter_list(dynamic_nodes_scm)?
                    .map(|item| Ok(NodeInstance::from_scheme_with_guile(item, assets)?))
                    .collect::<Result<Vec<NodeInstance>, SkyliteProcError>>()?
            } else {
                vec![]
            };

            Ok(Node {
                meta,
                parameters,
                properties,
                static_nodes,
                dynamic_nodes,
            })
        }
    }

    /// Creates a single Node from an asset file.
    pub(crate) fn from_meta(
        meta: AssetMetaData,
        assets: &mut Assets,
    ) -> Result<Node, SkyliteProcError> {
        // Since we are not actually accessing anything from this signature from C,
        // we can get away with ignoring the missing C representations.
        #[allow(improper_ctypes_definitions)]
        extern "C" fn from_meta_inner(
            params: (&AssetMetaData, &mut Assets),
        ) -> Result<Node, SkyliteProcError> {
            let (meta, assets) = params;

            Node::from_meta_with_guile(meta.clone(), assets)
        }

        with_guile(from_meta_inner, (&meta, assets))
    }
}

#[cfg(test)]
mod tests {
    use crate::assets::tests::create_tmp_fs;
    use crate::assets::Assets;
    use crate::parse::nodes::{Node, NodeInstance};
    use crate::parse::values::{Type, TypedValue, Variable};

    #[test]
    fn test_parse_node() {
        let tmp_fs = create_tmp_fs(&[
            (
                "nodes/test-node-1.scm",
                r#"
                '((parameters . ((id string)))
                   (properties . ((id string)))
                   (static-nodes .
                     ((sub1 . (basic-node-2 "sub1"))
                      (sub2 . (z-order-node "sub2" 2))))
                   (dynamic-nodes .
                     ((basic-node-2 "dynamic1")
                      (z-order-node "dynamic2" -1))))
                "#,
            ),
            (
                "nodes/basic-node-2.scm",
                r#"
                '((parameters . ((id string))))
                "#,
            ),
            (
                "nodes/z-order-node.scm",
                r#"
                '((parameters . ((id string) (z-order i16))))
                "#,
            ),
        ])
        .unwrap();
        let mut assets = Assets::from_scheme_with_guile(None, tmp_fs.path()).unwrap();
        let node = assets.load_node("test-node-1").unwrap();
        assert_eq!(
            node,
            &Node {
                meta: node.meta.clone(),
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
                            node_id: 0,
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
                        node_id: 0,
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
