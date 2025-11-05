use std::io::Read;
use std::path::PathBuf;

use crate::base_serde::Deserialize;

macro_rules! format_err {
    ($msg:literal $(,$args:expr)*) => {
        AssetError::FormatError(format!($msg, $($args),*))
    };
}

macro_rules! data_err {
    ($msg:literal $(,$args:expr)*) => {
        AssetError::DataError(format!($msg, $($args),*))
    };
}

#[derive(Debug)]
pub enum AssetError {
    /// An exception was raised within Racket.
    RacketException(String),

    /// The data format of an asset is incorrect.
    FormatError(String),

    /// The data for an asset is inconsistent.
    DataError(String),

    /// IO-Error
    IOError(std::io::Error),

    /// Something else went wrong.
    OtherError(String),
}

impl std::fmt::Display for AssetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RacketException(str) => write!(f, "Racket Exception: {}", str),
            Self::FormatError(str) => write!(f, "Format Error: {}", str),
            Self::DataError(str) => write!(f, "Data Error: {}", str),
            Self::IOError(err) => write!(f, "IO Error: {}", err),
            Self::OtherError(str) => write!(f, "Error: {}", str),
        }
    }
}

impl From<std::io::Error> for AssetError {
    fn from(err: std::io::Error) -> Self {
        AssetError::IOError(err)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AssetType {
    Project,
    Node,
    NodeList,
    Sequence,
}

impl AssetType {
    fn read(input: &mut impl Read) -> Result<AssetType, AssetError> {
        let asset_type_byte = u8::deserialize(input)?;
        match asset_type_byte {
            0 => Ok(AssetType::Project),
            1 => Ok(AssetType::Node),
            2 => Ok(AssetType::NodeList),
            3 => Ok(AssetType::Sequence),
            _ => Err(AssetError::OtherError("Unknown asset type".to_owned())),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct AssetMeta {
    pub id: u32,
    pub name: String,
    pub asset_type: AssetType,
    pub tracked_paths: Vec<PathBuf>,
}

impl AssetMeta {
    pub(crate) fn read(input: &mut impl Read) -> Result<AssetMeta, AssetError> {
        let id = u32::deserialize(input)?;
        let name = String::deserialize(input)?;
        let asset_type = AssetType::read(input)?;
        let tracked_paths_len = u32::deserialize(input)? as usize;
        let mut tracked_paths = Vec::with_capacity(tracked_paths_len);
        for _ in 0..tracked_paths_len {
            let path_str = String::deserialize(input)?;
            tracked_paths.push(PathBuf::from(path_str));
        }
        Ok(AssetMeta {
            id,
            name,
            asset_type,
            tracked_paths,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    U8,
    U16,
    U32,
    U64,
    I8,
    I16,
    I32,
    I64,
    F32,
    F64,
    Bool,
    String,
    Vec(Box<Type>),
    Tuple(Vec<Type>),
    Project,
    Node(String),
    NodeList,
    Sequence,
}

impl Type {
    pub(crate) fn read(input: &mut impl Read) -> Result<Type, AssetError> {
        match u8::deserialize(input)? {
            0 => Ok(Type::U8),
            1 => Ok(Type::U16),
            2 => Ok(Type::U32),
            3 => Ok(Type::U64),
            4 => Ok(Type::I8),
            5 => Ok(Type::I16),
            6 => Ok(Type::I32),
            7 => Ok(Type::I64),
            8 => Ok(Type::F32),
            9 => Ok(Type::F64),
            10 => Ok(Type::Bool),
            11 => Ok(Type::String),
            12 => {
                let item_type = Type::read(input)?;
                Ok(Type::Vec(Box::new(item_type)))
            }
            13 => {
                let len = u32::deserialize(input)?;
                let mut item_types = Vec::with_capacity(len as usize);
                for _ in 0..len {
                    item_types.push(Type::read(input)?);
                }
                Ok(Type::Tuple(item_types))
            }
            14 => Ok(Type::Project),
            15 => {
                let name = String::deserialize(input)?;
                Ok(Type::Node(name))
            }
            16 => Ok(Type::NodeList),
            17 => Ok(Type::Sequence),
            _ => Err(AssetError::OtherError(
                "Unknown type. Decoder desynced?".to_owned(),
            )),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct NodeArgs {
    pub args: Vec<TypedValue>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TypedValue {
    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
    I8(i8),
    I16(i16),
    I32(i32),
    I64(i64),
    F32(f32),
    F64(f64),
    Bool(bool),
    String(String),
    Vec(Vec<TypedValue>),
    Tuple(Vec<TypedValue>),
    // Project,
    Node(NodeArgs),
    NodeList(String),
    Sequence(String),
}

impl TypedValue {
    pub(crate) fn read(input: &mut impl Read, type_: &Type) -> Result<TypedValue, AssetError> {
        match type_ {
            Type::U8 => Ok(TypedValue::U8(u8::deserialize(input)?)),
            Type::U16 => Ok(TypedValue::U16(u16::deserialize(input)?)),
            Type::U32 => Ok(TypedValue::U32(u32::deserialize(input)?)),
            Type::U64 => Ok(TypedValue::U64(u64::deserialize(input)?)),
            Type::I8 => Ok(TypedValue::I8(i8::deserialize(input)?)),
            Type::I16 => Ok(TypedValue::I16(i16::deserialize(input)?)),
            Type::I32 => Ok(TypedValue::I32(i32::deserialize(input)?)),
            Type::I64 => Ok(TypedValue::I64(i64::deserialize(input)?)),
            Type::F32 => Ok(TypedValue::F32(f32::deserialize(input)?)),
            Type::F64 => Ok(TypedValue::F64(f64::deserialize(input)?)),
            Type::Bool => Ok(TypedValue::Bool(bool::deserialize(input)?)),
            Type::String => Ok(TypedValue::String(String::deserialize(input)?)),
            Type::Vec(item_type) => {
                let len = u32::deserialize(input)? as usize;
                let mut vec = Vec::with_capacity(len);
                for _ in 0..len {
                    vec.push(TypedValue::read(input, item_type)?);
                }
                Ok(TypedValue::Vec(vec))
            }
            Type::Tuple(item_types) => {
                let mut items = Vec::with_capacity(item_types.len());
                for item_type in item_types {
                    items.push(TypedValue::read(input, item_type)?);
                }
                Ok(TypedValue::Tuple(items))
            }
            Type::Node(_) => {
                let args_len = u32::deserialize(input)? as usize;
                let mut args = Vec::with_capacity(args_len);
                for _ in 0..args_len {
                    let t = Type::read(input)?;
                    args.push(TypedValue::read(input, &t)?);
                }
                Ok(TypedValue::Node(NodeArgs { args }))
            }
            Type::NodeList => {
                let name = String::deserialize(input)?;
                Ok(TypedValue::NodeList(name))
            }
            Type::Sequence => {
                let name = String::deserialize(input)?;
                Ok(TypedValue::Sequence(name))
            }
            _ => Err(AssetError::OtherError(
                "Unsupported type for reading typed value".to_owned(),
            )),
        }
    }
}
