use super::guile::{
    scm_car, scm_cdr, scm_is_false, scm_is_null, scm_is_symbol, scm_is_true, scm_length,
    scm_list_p, scm_pair_p, scm_to_int64, SCM,
};
use super::scheme_util::CXROp::*;
use super::scheme_util::{
    cxr, form_to_string, iter_list, parse_bool, parse_f32, parse_f64, parse_int, parse_string,
    parse_symbol,
};
use crate::assets::Assets;
use crate::SkyliteProcError;

/// Type of a Skylite variable or parameter.
#[derive(PartialEq, Debug, Clone)]
pub(crate) enum Type {
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
    Tuple(Vec<Type>),
    Vec(Box<Type>),
    NodeList,
}

/// Converts a type name from Scheme to an instance of `Type`.
///
/// `item_type` must be one of the following symbols for primitive types:
/// - `u8`, `u16`, `u32`, `u64`
/// - `i8`, `i16`, `i32`, `i64`
/// - `f32`, `f64`
/// - `bool`
/// - `string`
///
/// In addition, `item_type` can use the following forms to construct aggregate
/// types:
/// - `(<type1> <type2> ... )`: A tuple of the given types.
/// - `(vec <type>)`: A vector of the given types.
pub(crate) unsafe fn parse_type(typename: SCM) -> Result<Type, SkyliteProcError> {
    if scm_is_symbol(typename) {
        let type_name = parse_symbol(typename)?;
        match &type_name[..] {
            "u8" => Ok(Type::U8),
            "u16" => Ok(Type::U16),
            "u32" => Ok(Type::U32),
            "u64" => Ok(Type::U64),
            "i8" => Ok(Type::I8),
            "i16" => Ok(Type::I16),
            "i32" => Ok(Type::I32),
            "i64" => Ok(Type::I64),
            "f32" => Ok(Type::F32),
            "f64" => Ok(Type::F64),
            "bool" => Ok(Type::Bool),
            "string" => Ok(Type::String),
            "node-list" => Ok(Type::NodeList),
            _ => Err(SkyliteProcError::DataError(format!(
                "Unknown data type: {}",
                type_name
            ))),
        }
    } else if scm_is_true(scm_list_p(typename)) {
        let car = scm_car(typename);
        if scm_is_symbol(car) && parse_symbol(car)? == "vec" {
            let item_type = cxr(typename, &[CDR, CAR])?;
            Ok(Type::Vec(Box::new(parse_type(item_type)?)))
        } else {
            iter_list(typename)
                .unwrap()
                .map(|t| parse_type(t))
                .collect::<Result<Vec<Type>, SkyliteProcError>>()
                .map(|ok| Type::Tuple(ok))
        }
    } else {
        Err(SkyliteProcError::DataError(format!(
            "Unsupported type: {}",
            form_to_string(typename)
        )))
    }
}

/// A data item combined with a type.
#[derive(PartialEq, Debug, Clone)]
pub(crate) enum TypedValue {
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
    Tuple(Vec<TypedValue>),
    Vec(Vec<TypedValue>),
    NodeList(usize),
}

/// Constructs a `TypedValue` given a type and a Scheme form for the value.
pub(crate) unsafe fn parse_typed_value(
    typename: &Type,
    data: SCM,
    assets: &Assets,
) -> Result<TypedValue, SkyliteProcError> {
    match typename {
        Type::U8 => Ok(TypedValue::U8(parse_int(data)?)),
        Type::U16 => Ok(TypedValue::U16(parse_int(data)?)),
        Type::U32 => Ok(TypedValue::U32(parse_int(data)?)),
        Type::U64 => Ok(TypedValue::U64(parse_int(data)?)),
        Type::I8 => Ok(TypedValue::I8(parse_int(data)?)),
        Type::I16 => Ok(TypedValue::I16(parse_int(data)?)),
        Type::I32 => Ok(TypedValue::I32(parse_int(data)?)),
        Type::I64 => Ok(TypedValue::I64(parse_int(data)?)),
        Type::F32 => Ok(TypedValue::F32(parse_f32(data)?)),
        Type::F64 => Ok(TypedValue::F64(parse_f64(data)?)),
        Type::Bool => Ok(TypedValue::Bool(parse_bool(data)?)),
        Type::String => Ok(TypedValue::String(parse_string(data)?)),

        Type::Vec(item_type) => iter_list(data)?
            .map(|e| parse_typed_value(&item_type, e, assets))
            .collect::<Result<Vec<TypedValue>, SkyliteProcError>>()
            .map(|ok| TypedValue::Vec(ok)),

        Type::Tuple(types) => parse_typed_value_tuple(types, data, assets),

        Type::NodeList => {
            let name = parse_symbol(data)?;
            let meta = assets
                .node_lists
                .get(&name)
                .ok_or(SkyliteProcError::DataError(format!(
                    "Node list not found: {}",
                    name
                )))?;
            Ok(TypedValue::NodeList(meta.id))
        }
    }
}

unsafe fn parse_typed_value_tuple(
    types: &[Type],
    values: SCM,
    assets: &Assets,
) -> Result<TypedValue, SkyliteProcError> {
    if types.len() as i64 != scm_to_int64(scm_length(values)) {
        return Err(SkyliteProcError::DataError(format!(
            "Tuple definition has differing number of types and values."
        )));
    }

    Iterator::zip(types.iter(), iter_list(values)?)
        .map(|(t, v)| parse_typed_value(t, v, assets))
        .collect::<Result<Vec<TypedValue>, SkyliteProcError>>()
        .map(|ok| TypedValue::Tuple(ok))
}

#[derive(PartialEq, Debug, Clone)]
pub(crate) struct Variable {
    pub name: String,
    pub typename: Type,
    pub documentation: Option<String>,
    pub default: Option<TypedValue>,
}

pub(crate) unsafe fn parse_variable_definition(
    def: SCM,
    assets: &Assets,
) -> Result<Variable, SkyliteProcError> {
    if scm_is_false(scm_list_p(def)) {
        return Err(SkyliteProcError::DataError(format!(
            "Expected variable definition, found {}",
            form_to_string(def)
        )));
    }

    let mut current_pair = def;
    if scm_is_null(current_pair) {
        return Err(SkyliteProcError::DataError(format!(
            "Expected variable name"
        )));
    }
    let name = parse_symbol(scm_car(current_pair))?;

    current_pair = scm_cdr(current_pair);
    if scm_is_null(current_pair) {
        return Err(SkyliteProcError::DataError(format!(
            "Expected variable type"
        )));
    }
    let typename = parse_type(scm_car(current_pair))?;

    current_pair = scm_cdr(current_pair);
    let documentation = if scm_is_null(current_pair) {
        return Ok(Variable {
            name,
            typename,
            documentation: None,
            default: None,
        });
    } else {
        Some(parse_string(scm_car(current_pair))?)
    };

    current_pair = scm_cdr(current_pair);
    let default = if scm_is_null(current_pair) {
        return Ok(Variable {
            name,
            typename,
            documentation,
            default: None,
        });
    } else {
        Some(parse_typed_value(&typename, scm_car(current_pair), assets)?)
    };

    Ok(Variable {
        name,
        typename,
        documentation,
        default,
    })
}

pub(crate) unsafe fn parse_argument_list(
    args_raw: SCM,
    parameters: &[Variable],
    assets: &Assets,
) -> Result<Vec<TypedValue>, SkyliteProcError> {
    // Pad with empty values. If there are any empty values left after the argument
    // list has been parsed, replace with the corresponding default values. If
    // there is no default value, raise an error.
    let mut args: Vec<Option<TypedValue>> = vec![None; parameters.len()];
    let mut next_arg = 0;
    for arg_raw in iter_list(args_raw)? {
        let (arg_idx, param, value) =
            if scm_is_true(scm_pair_p(arg_raw)) && scm_is_symbol(scm_car(arg_raw)) {
                // Named argument
                let arg_name = parse_symbol(scm_car(arg_raw)).unwrap();
                let (idx, p) = parameters
                    .iter()
                    .enumerate()
                    .find(|(_, param)| param.name == arg_name)
                    .ok_or(SkyliteProcError::DataError(format!(
                        "No parameter {} found",
                        arg_name
                    )))?;

                (idx, p, scm_cdr(arg_raw))
            } else {
                // Positional argument
                if next_arg >= parameters.len() {
                    return Err(SkyliteProcError::DataError(format!("Too many arguments")));
                } else {
                    (next_arg, &parameters[next_arg], arg_raw)
                }
            };
        next_arg = arg_idx + 1;

        args[arg_idx] = Some(parse_typed_value(&param.typename, value, assets)?);
    }

    let mut out = Vec::with_capacity(parameters.len());
    for (i, maybe_arg) in args.into_iter().enumerate() {
        let val = match maybe_arg {
            Some(arg) => arg,
            None => {
                if let Some(def) = parameters[i].default.clone() {
                    def
                } else {
                    return Err(SkyliteProcError::DataError(format!(
                        "Missing argument for parameter {}",
                        parameters[i].name
                    )));
                }
            }
        };
        out.push(val);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::parse_argument_list;
    use crate::assets::Assets;
    use crate::parse::guile::{scm_from_bool, scm_from_double, scm_from_int32};
    use crate::parse::scheme_util::{eval_str, with_guile};
    use crate::parse::values::{
        parse_type, parse_typed_value, parse_variable_definition, Type, TypedValue, Variable,
    };

    fn empty_assets() -> Assets {
        Assets {
            nodes: HashMap::new(),
            node_lists: HashMap::new(),
            sequences: HashMap::new(),
        }
    }

    extern "C" fn test_typed_value_impl(_: &()) {
        let assets = empty_assets();
        unsafe {
            let type_name = parse_type(eval_str("'u8").unwrap()).unwrap();
            assert_eq!(
                parse_typed_value(&type_name, scm_from_int32(5), &assets).unwrap(),
                TypedValue::U8(5)
            );
            assert!(parse_typed_value(&type_name, scm_from_int32(300), &assets).is_err());

            let type_name = parse_type(eval_str("'f64").unwrap()).unwrap();
            let value = scm_from_double(1.0);
            assert_eq!(
                parse_typed_value(&type_name, value, &assets).unwrap(),
                TypedValue::F64(1.0)
            );

            let type_name = parse_type(eval_str("'string").unwrap()).unwrap();
            let value = eval_str("\"test123\"").unwrap();
            assert_eq!(
                parse_typed_value(&type_name, value, &assets).unwrap(),
                TypedValue::String("test123".to_owned())
            );

            let type_name = parse_type(eval_str("'bool").unwrap()).unwrap();
            assert_eq!(
                parse_typed_value(&type_name, scm_from_bool(true), &assets).unwrap(),
                TypedValue::Bool(true)
            );

            let type_name = parse_type(eval_str("'(u8 bool (u16 u16))").unwrap()).unwrap();
            let value = eval_str("'(1 #t (2 3))").unwrap();
            assert_eq!(
                parse_typed_value(&type_name, value, &assets).unwrap(),
                TypedValue::Tuple(vec![
                    TypedValue::U8(1),
                    TypedValue::Bool(true),
                    TypedValue::Tuple(vec![TypedValue::U16(2), TypedValue::U16(3),])
                ])
            );

            let type_name = parse_type(eval_str("'(vec i16)").unwrap()).unwrap();
            let value = eval_str("'(0 5 10 15 20 25)").unwrap();
            assert_eq!(
                parse_typed_value(&type_name, value, &assets).unwrap(),
                TypedValue::Vec(vec![
                    TypedValue::I16(0),
                    TypedValue::I16(5),
                    TypedValue::I16(10),
                    TypedValue::I16(15),
                    TypedValue::I16(20),
                    TypedValue::I16(25)
                ])
            );
        }
    }

    #[test]
    fn test_typed_value() {
        with_guile(test_typed_value_impl, &());
    }

    extern "C" fn test_variable_impl(_: &()) {
        let assets = empty_assets();
        unsafe {
            let form = eval_str("'(test1 u8)").unwrap();
            assert_eq!(
                parse_variable_definition(form, &assets).unwrap(),
                Variable {
                    name: String::from("test1"),
                    typename: Type::U8,
                    documentation: None,
                    default: None
                }
            );

            let form = eval_str("'(test2 i32 \"Something\")").unwrap();
            assert_eq!(
                parse_variable_definition(form, &assets).unwrap(),
                Variable {
                    name: String::from("test2"),
                    typename: Type::I32,
                    documentation: Some(String::from("Something")),
                    default: None
                }
            );

            let form = eval_str("'(test3 (vec u8) \"Something else\" (0 1 2 3))").unwrap();
            assert_eq!(
                parse_variable_definition(form, &assets).unwrap(),
                Variable {
                    name: String::from("test3"),
                    typename: Type::Vec(Box::new(Type::U8)),
                    documentation: Some(String::from("Something else")),
                    default: Some(TypedValue::Vec(vec![
                        TypedValue::U8(0),
                        TypedValue::U8(1),
                        TypedValue::U8(2),
                        TypedValue::U8(3),
                    ]))
                }
            );
        }
    }

    #[test]
    fn test_variable() {
        with_guile(test_variable_impl, &());
    }

    extern "C" fn test_argument_list_impl(_: &()) {
        let parameters = &[
            Variable {
                name: "a".to_owned(),
                typename: Type::U8,
                documentation: None,
                default: None,
            },
            Variable {
                name: "b".to_owned(),
                typename: Type::U8,
                documentation: None,
                default: Some(TypedValue::U8(5)),
            },
            Variable {
                name: "c".to_owned(),
                typename: Type::U8,
                documentation: None,
                default: Some(TypedValue::U8(10)),
            },
        ];
        let assets = empty_assets();

        unsafe {
            let args_raw = eval_str("'(1 2 3)").unwrap();
            let args = parse_argument_list(args_raw, parameters, &assets).unwrap();
            assert_eq!(
                args,
                vec![TypedValue::U8(1), TypedValue::U8(2), TypedValue::U8(3)]
            );

            let args_raw = eval_str("'(1)").unwrap();
            let args = parse_argument_list(args_raw, parameters, &assets).unwrap();
            assert_eq!(
                args,
                vec![TypedValue::U8(1), TypedValue::U8(5), TypedValue::U8(10)]
            );

            let args_raw = eval_str("'((c . 3) (a . 1) (b . 2))").unwrap();
            let args = parse_argument_list(args_raw, parameters, &assets).unwrap();
            assert_eq!(
                args,
                vec![TypedValue::U8(1), TypedValue::U8(2), TypedValue::U8(3)]
            );

            let args_raw = eval_str("'((c . 3))").unwrap();
            assert!(parse_argument_list(args_raw, parameters, &assets).is_err());

            let args_raw = eval_str("'(1 2 3 4)").unwrap();
            assert!(parse_argument_list(args_raw, parameters, &assets).is_err());
        }
    }

    #[test]
    fn test_argument_list() {
        with_guile(test_argument_list_impl, &())
    }
}
