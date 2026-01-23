use std::io::Read;
use std::path::Path;

use crate::asset_server::connect_to_asset_server;
use crate::assets::TypedValue;
use crate::base_serde::Deserialize;
use crate::{AssetError, AssetMeta, AssetType, Type};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Comparison {
    Equals,
    NotEquals,
    LessThan,
    GreaterThan,
    LessEquals,
    GreaterEquals,
}

impl Deserialize for Comparison {
    fn deserialize(input: &mut impl Read) -> Result<Comparison, AssetError> {
        let opcode = u8::deserialize(input)?;
        match opcode {
            0 => Ok(Comparison::Equals),
            1 => Ok(Comparison::NotEquals),
            2 => Ok(Comparison::LessThan),
            3 => Ok(Comparison::GreaterThan),
            4 => Ok(Comparison::LessEquals),
            5 => Ok(Comparison::GreaterEquals),
            _ => panic!("Invalid comparison {}", opcode),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Op {
    PushOffset {
        node: String,
        property: String,
    },
    Set {
        value: TypedValue,
    },
    SetString {
        value: String,
    },
    Modify {
        delta: TypedValue,
    },
    ModifyF32 {
        delta: f32,
    },
    ModifyF64 {
        delta: f64,
    },
    BranchIfTrue {
        target: usize,
    },
    BranchIfFalse {
        target: usize,
    },
    BranchUInt {
        comparison: Comparison,
        value: TypedValue,
        target: usize,
    },
    BranchSInt {
        comparison: Comparison,
        value: TypedValue,
        target: usize,
    },
    BranchF32 {
        comparison: Comparison,
        value: f32,
        target: usize,
    },
    BranchF64 {
        comparison: Comparison,
        value: f64,
        target: usize,
    },
    Jump {
        target: usize,
    },
    Call {
        target: usize,
    },
    Return,
    Wait {
        frames: u16,
    },
    RunCustom {
        id: String,
    },
    BranchCustom {
        id: String,
        target: usize,
    },
}

impl Deserialize for Op {
    fn deserialize(input: &mut impl Read) -> Result<Op, AssetError> {
        let opcode = u8::deserialize(input)?;
        match opcode {
            0 => {
                let node = String::deserialize(input)?;
                let property = String::deserialize(input)?;
                Ok(Op::PushOffset { node, property })
            }
            1 => {
                let type_ = Type::read(input)?;
                let value = TypedValue::read(input, &type_)?;
                Ok(Op::Set { value })
            }
            2 => {
                let value = String::deserialize(input)?;
                Ok(Op::SetString { value })
            }
            3 => {
                let type_ = Type::read(input)?;
                let delta = TypedValue::read(input, &type_)?;
                Ok(Op::Modify { delta })
            }
            4 => {
                let delta = f32::deserialize(input)?;
                Ok(Op::ModifyF32 { delta })
            }
            5 => {
                let delta = f64::deserialize(input)?;
                Ok(Op::ModifyF64 { delta })
            }
            6 => {
                let target = u32::deserialize(input)? as usize;
                Ok(Op::BranchIfTrue { target })
            }
            7 => {
                let target = u32::deserialize(input)? as usize;
                Ok(Op::BranchIfFalse { target })
            }
            8 => {
                let comparison = Comparison::deserialize(input)?;
                let type_ = Type::read(input)?;
                let value = TypedValue::read(input, &type_)?;
                let target = u32::deserialize(input)? as usize;
                Ok(Op::BranchUInt {
                    comparison,
                    value,
                    target,
                })
            }
            9 => {
                let comparison = Comparison::deserialize(input)?;
                let type_ = Type::read(input)?;
                let value = TypedValue::read(input, &type_)?;
                let target = u32::deserialize(input)? as usize;
                Ok(Op::BranchSInt {
                    comparison,
                    value,
                    target,
                })
            }
            10 => {
                let comparison = Comparison::deserialize(input)?;
                let value = f32::deserialize(input)?;
                let target = u32::deserialize(input)? as usize;
                Ok(Op::BranchF32 {
                    comparison,
                    value,
                    target,
                })
            }
            11 => {
                let comparison = Comparison::deserialize(input)?;
                let value = f64::deserialize(input)?;
                let target = u32::deserialize(input)? as usize;
                Ok(Op::BranchF64 {
                    comparison,
                    value,
                    target,
                })
            }
            12 => {
                let target = u32::deserialize(input)? as usize;
                Ok(Op::Jump { target })
            }
            13 => {
                let target = u32::deserialize(input)? as usize;
                Ok(Op::Call { target })
            }
            14 => Ok(Op::Return),
            15 => {
                let frames = u16::deserialize(input)?;
                Ok(Op::Wait { frames })
            }
            16 => {
                let id = String::deserialize(input)?;
                Ok(Op::RunCustom { id })
            }
            17 => {
                let id = String::deserialize(input)?;
                let target = u32::deserialize(input)? as usize;
                Ok(Op::BranchCustom { id, target })
            }
            _ => panic!("Invalid operation {}", opcode),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Sequence {
    pub meta: AssetMeta,
    pub node: String,
    pub script: Vec<Op>,
}

impl Deserialize for Sequence {
    fn deserialize(input: &mut impl Read) -> Result<Sequence, AssetError> {
        let meta = AssetMeta::read(input)?;
        let node = String::deserialize(input)?;
        let script_len = u32::deserialize(input)? as usize;
        let mut script = Vec::with_capacity(script_len);
        for _ in 0..script_len {
            script.push(Op::deserialize(input)?);
        }
        Ok(Sequence { meta, node, script })
    }
}

pub fn load_sequence(project_path: &Path, name: &str) -> Result<Sequence, AssetError> {
    let mut connection = connect_to_asset_server()?;
    connection.send_load_asset_request(project_path, AssetType::Sequence, name)?;

    let mut status = [0u8; 1];
    connection.read_exact(&mut status)?;
    if status[0] == 0 {
        Ok(Sequence::deserialize(&mut connection)?)
    } else {
        Err(AssetError::read(&mut connection))
    }
}

pub fn load_all_sequences(project_path: &Path) -> Result<Vec<Sequence>, AssetError> {
    let mut connection = connect_to_asset_server()?;
    connection.send_all_assets_request(project_path, AssetType::Sequence)?;

    let mut status = [0u8; 1];
    connection.read_exact(&mut status)?;
    if status[0] == 0 {
        let len = u32::deserialize(&mut connection)? as usize;
        let mut out = Vec::with_capacity(len);
        for _ in 0..len {
            out.push(Sequence::deserialize(&mut connection)?);
        }
        Ok(out)
    } else {
        Err(AssetError::read(&mut connection))
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{load_sequence, Comparison, Op, Sequence};
    use crate::assets::TypedValue;

    #[test]
    fn test_load_sequence() {
        let project_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("./tests/test-project")
            .canonicalize()
            .unwrap();
        let sequence = load_sequence(&project_dir.join("project.rkt"), "sequence1").unwrap();
        assert_eq!(
            sequence,
            Sequence {
                meta: sequence.meta.clone(),
                node: "node2".to_string(),
                script: vec![
                    Op::PushOffset {
                        node: "node2".to_owned(),
                        property: "prop-u16".to_owned()
                    },
                    Op::Set {
                        value: TypedValue::U16(5)
                    },
                    Op::PushOffset {
                        node: "node2".to_owned(),
                        property: "prop-string".to_owned()
                    },
                    Op::SetString {
                        value: "hello".to_owned()
                    },
                    Op::PushOffset {
                        node: "node2".to_owned(),
                        property: "prop-u16".to_owned()
                    },
                    Op::Modify {
                        delta: TypedValue::U16(10)
                    },
                    Op::PushOffset {
                        node: "node2".to_owned(),
                        property: "prop-f32".to_owned()
                    },
                    Op::ModifyF32 { delta: 1.0 },
                    Op::PushOffset {
                        node: "node2".to_owned(),
                        property: "prop-f64".to_owned()
                    },
                    Op::ModifyF64 { delta: -1.0 },
                    Op::PushOffset {
                        node: "node2".to_owned(),
                        property: "prop-bool".to_owned()
                    },
                    Op::BranchIfTrue { target: 0 },
                    Op::PushOffset {
                        node: "node2".to_owned(),
                        property: "prop-bool".to_owned()
                    },
                    Op::BranchIfFalse { target: 0 },
                    Op::PushOffset {
                        node: "node2".to_owned(),
                        property: "prop-u16".to_owned()
                    },
                    Op::BranchUInt {
                        comparison: Comparison::LessThan,
                        value: TypedValue::U16(10),
                        target: 0
                    },
                    Op::PushOffset {
                        node: "node2".to_owned(),
                        property: "prop-i16".to_owned()
                    },
                    Op::BranchSInt {
                        comparison: Comparison::GreaterThan,
                        value: TypedValue::I16(10),
                        target: 0
                    },
                    Op::PushOffset {
                        node: "node2".to_owned(),
                        property: "prop-f32".to_owned()
                    },
                    Op::BranchF32 {
                        comparison: Comparison::Equals,
                        value: 10.0,
                        target: 0
                    },
                    Op::PushOffset {
                        node: "node2".to_owned(),
                        property: "prop-f64".to_owned()
                    },
                    Op::BranchF64 {
                        comparison: Comparison::NotEquals,
                        value: 10.0,
                        target: 0
                    },
                    Op::Jump { target: 0 },
                    Op::Call { target: 28 },
                    Op::Wait { frames: 1 },
                    Op::RunCustom {
                        id: "custom-fn".to_owned()
                    },
                    Op::BranchCustom {
                        id: "custom-cond".to_owned(),
                        target: 0
                    },
                    Op::Return,
                    Op::Return
                ]
            }
        )
    }
}
