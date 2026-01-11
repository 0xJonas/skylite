use std::io::Read;
use std::path::Path;

use crate::asset_server::connect_to_asset_server;
use crate::assets::{AssetError, AssetMeta, AssetType, Type, TypedValue};
use crate::base_serde::Deserialize;

#[derive(Debug, PartialEq)]
pub struct Variable {
    pub name: String,
    pub vtype: Type,
}

impl Variable {
    fn read(input: &mut impl Read) -> Result<Variable, AssetError> {
        let name = String::deserialize(input)?;
        let vtype = Type::read(input)?;
        Ok(Variable { name, vtype })
    }
}

#[derive(Debug, PartialEq)]
pub struct Node {
    pub meta: AssetMeta,
    pub parameters: Vec<Variable>,
    pub properties: Vec<Variable>,
}

impl Node {
    fn read(input: &mut impl Read) -> Result<Node, AssetError> {
        let meta = AssetMeta::read(input)?;
        let param_len = u32::deserialize(input)? as usize;
        let mut parameters = Vec::with_capacity(param_len);
        for _ in 0..param_len {
            parameters.push(Variable::read(input)?);
        }
        let prop_len = u32::deserialize(input)? as usize;
        let mut properties = Vec::with_capacity(prop_len);
        for _ in 0..prop_len {
            properties.push(Variable::read(input)?);
        }
        Ok(Node {
            meta,
            parameters,
            properties,
        })
    }
}

pub fn load_node(project_path: &Path, name: &str) -> Result<Node, AssetError> {
    let mut connection = connect_to_asset_server()?;
    connection.send_load_asset_request(project_path, AssetType::Node, name)?;

    let mut status = [0u8; 1];
    connection.read_exact(&mut status)?;
    if status[0] == 0 {
        Ok(Node::read(&mut connection)?)
    } else {
        Err(AssetError::read(&mut connection))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct NodeInstance {
    pub node: String,
    pub node_id: u32,
    pub args: Vec<TypedValue>,
}

impl NodeInstance {
    fn read(input: &mut impl Read) -> Result<NodeInstance, AssetError> {
        let node = String::deserialize(input)?;
        let node_id = u32::deserialize(input)?;
        let TypedValue::Node(args) = TypedValue::read(input, &Type::Node(node.clone()))? else {
            unreachable!()
        };

        Ok(NodeInstance { node, node_id, args })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct NodeList {
    pub meta: AssetMeta,
    pub nodes: Vec<NodeInstance>,
}

impl NodeList {
    fn read(input: &mut impl Read) -> Result<NodeList, AssetError> {
        let meta = AssetMeta::read(input)?;
        let len = u32::deserialize(input)? as usize;
        let mut nodes = Vec::with_capacity(len);
        for _ in 0..len {
            nodes.push(NodeInstance::read(input)?);
        }
        Ok(NodeList { meta, nodes })
    }
}

pub fn load_node_list(project_path: &Path, name: &str) -> Result<NodeList, AssetError> {
    let mut connection = connect_to_asset_server()?;
    connection.send_load_asset_request(project_path, AssetType::NodeList, name)?;

    let mut status = [0u8; 1];
    connection.read_exact(&mut status)?;
    if status[0] == 0 {
        Ok(NodeList::read(&mut connection)?)
    } else {
        Err(AssetError::read(&mut connection))
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{load_node, load_node_list, Node, NodeInstance, NodeList, Variable};
    use crate::assets::{Type, TypedValue};

    #[test]
    fn test_load_node() {
        let project_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("./tests/test-project")
            .canonicalize()
            .unwrap();
        let node = load_node(&project_dir.join("project.rkt"), "node1").unwrap();
        assert_eq!(
            node,
            Node {
                meta: crate::AssetMeta {
                    id: node.meta.id,
                    name: "node1".to_owned(),
                    asset_type: crate::AssetType::Node,
                    tracked_paths: vec![project_dir.join("assets/node1.rkt")]
                },
                parameters: vec![
                    Variable {
                        name: "param1".to_owned(),
                        vtype: Type::U8
                    },
                    Variable {
                        name: "param2".to_owned(),
                        vtype: Type::String
                    },
                ],
                properties: vec![
                    Variable {
                        name: "prop1".to_owned(),
                        vtype: Type::F32
                    },
                    Variable {
                        name: "prop2".to_owned(),
                        vtype: Type::Bool
                    },
                ],
            }
        )
    }

    #[test]
    fn test_load_node_list() {
        let project_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("./tests/test-project")
            .canonicalize()
            .unwrap();
        let node_list = load_node_list(&project_dir.join("project.rkt"), "node-list1").unwrap();
        assert_eq!(
            node_list,
            NodeList {
                meta: crate::AssetMeta {
                    id: node_list.meta.id,
                    name: "node-list1".to_owned(),
                    asset_type: crate::AssetType::NodeList,
                    tracked_paths: vec![project_dir.join("assets/node-list1.rkt")]
                },
                nodes: vec![
                    NodeInstance {
                        node: "node1".to_owned(),
                        node_id: 0,
                        args: vec![TypedValue::U8(1), TypedValue::String("test1".to_owned())],
                    },
                    NodeInstance {
                        node: "node1".to_owned(),
                        node_id: 0,
                        args: vec![TypedValue::U8(2), TypedValue::String("test2".to_owned())],
                    },
                ],
            }
        )
    }
}
