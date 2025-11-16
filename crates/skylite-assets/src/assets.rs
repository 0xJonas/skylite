use std::ffi::OsString;
use std::io::Read;
#[cfg(target_family = "unix")]
use std::path::Path;
use std::path::PathBuf;

use crate::base_serde::Deserialize;

#[cfg(target_family = "unix")]
pub(crate) fn path_to_native(path: &Path) -> Vec<u8> {
    use std::os::unix::ffi::OsStrExt;
    path.as_os_str().as_bytes().to_vec()
}

#[cfg(target_family = "unix")]
pub(crate) fn native_to_path(bytes: Vec<u8>) -> PathBuf {
    use std::os::unix::ffi::OsStringExt;
    PathBuf::from(OsString::from_vec(bytes))
}

#[cfg(target_family = "windows")]
pub(crate) fn path_to_native(path: &Path) -> Vec<u8> {
    use std::os::windows::ffi::OsStrExt;
    path.as_os_str()
        .encode_wide()
        .map(|c| c.to_ne_bytes())
        .flatten()
        .collect()
}

#[cfg(target_family = "windows")]
pub(crate) fn native_to_path(bytes: Vec<u8>) -> PathBuf {
    use std::os::windows::ffi::OsStringExt;
    let wide: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|chunk| u16::from_ne_bytes([chunk[0], chunk[1]]))
        .collect();
    PathBuf::from(OsString::from_wide(&wide))
}

#[cfg(not(any(target_family = "unix", target_family = "windows")))]
compile_error!("This platform is currently not supported.");

#[derive(Debug)]
pub enum AssetError {
    /// An exception was raised within Racket.
    RacketException {
        project_root: Option<PathBuf>,
        asset_file: Option<PathBuf>,
        asset: Option<String>,
        message: String,
    },

    /// IO-Error
    IOError(std::io::Error),
}

impl AssetError {
    pub(crate) fn read(input: &mut impl Read) -> AssetError {
        let project_root_bytes = match Vec::<u8>::deserialize(input) {
            Ok(bytes) => bytes,
            Err(err) => return err,
        };
        let asset_file_bytes = match Vec::<u8>::deserialize(input) {
            Ok(bytes) => bytes,
            Err(err) => return err,
        };
        let asset = match String::deserialize(input) {
            Ok(s) => s,
            Err(err) => return err,
        };
        let message = match String::deserialize(input) {
            Ok(s) => s,
            Err(err) => return err,
        };

        AssetError::RacketException {
            project_root: if project_root_bytes.len() > 0 {
                Some(native_to_path(project_root_bytes))
            } else {
                None
            },
            asset_file: if asset_file_bytes.len() > 0 {
                Some(native_to_path(asset_file_bytes))
            } else {
                None
            },
            asset: if asset.len() > 0 { Some(asset) } else { None },
            message,
        }
    }
}

impl std::fmt::Display for AssetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RacketException {
                project_root,
                asset_file,
                asset,
                message,
            } => {
                if let Some(file) = asset_file.as_ref().or(project_root.as_ref()) {
                    write!(f, "{}", file.to_string_lossy())?;
                    if asset.is_some() {
                        write!(f, ", ")?;
                    } else {
                        write!(f, ": ")?;
                    }
                }
                if let Some(a) = asset {
                    write!(f, "{a}: ")?;
                }
                write!(f, "Error processing asset: {message}")
            }
            Self::IOError(err) => write!(f, "IO Error: {err}"),
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
            t @ _ => panic!("Unknown asset type {t}. Reader desynced?"),
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
            let path_bytes = Vec::<u8>::deserialize(input)?;
            tracked_paths.push(native_to_path(path_bytes));
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
            t @ _ => panic!("Unknown variable type {t}. Reader desynced?"),
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
            Type::Project => todo!(),
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
        }
    }
}
