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
            let args = parse_argument_list(args_raw, &node.parameters.clone(), assets)?;

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
                    .map(|p| parse_variable_definition(p, assets))
                    .collect::<Result<Vec<Variable>, SkyliteProcError>>()?
            } else {
                vec![]
            };

            let maybe_properties = assq_str("properties", def)?;
            let properties = if let Some(properties_scm) = maybe_properties {
                iter_list(properties_scm)?
                    .map(|p| parse_variable_definition(p, assets))
                    .collect::<Result<Vec<Variable>, SkyliteProcError>>()?
            } else {
                vec![]
            };

            Ok(Node {
                meta,
                parameters,
                properties,
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
            params: (AssetMetaData, &mut Assets),
        ) -> Result<Node, SkyliteProcError> {
            let (meta, assets) = params;

            Node::from_meta_with_guile(meta, assets)
        }

        with_guile(from_meta_inner, (meta, assets))
    }
}

#[cfg(test)]
mod tests {
    use crate::assets::tests::create_tmp_fs;
    use crate::assets::Assets;
    use crate::parse::nodes::Node;
    use crate::parse::values::{Type, Variable};

    #[test]
    fn test_parse_node() {
        let tmp_fs = create_tmp_fs(&[
            (
                "nodes/test-node-1.scm",
                r#"
                '((parameters . ((id string)))
                   (properties . ((id string)
                                  (sub1 (node test-node-2)))))
                "#,
            ),
            (
                "nodes/test-node-2.scm",
                r#"
                '((parameters . ((id string))))
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
                properties: vec![
                    Variable {
                        name: "id".to_owned(),
                        typename: Type::String,
                        documentation: None,
                        default: None
                    },
                    Variable {
                        name: "sub1".to_owned(),
                        typename: Type::Node("test-node-2".to_owned()),
                        documentation: None,
                        default: None
                    }
                ]
            }
        )
    }
}
