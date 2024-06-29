use std::{ffi::{c_void, CStr, CString}, fmt::Display, ptr::null_mut};

use crate::{
    guile::{scm_assq, scm_c_eval_string, scm_cadr, scm_car, scm_cdr, scm_from_utf8_symbol, scm_is_bool, scm_is_false, scm_is_integer, scm_is_null, scm_is_real, scm_is_symbol, scm_is_true, scm_length, scm_list_p, scm_object_to_string, scm_pair_p, scm_string_p, scm_symbol_to_string, scm_to_bool, scm_to_double, scm_to_int64, scm_to_utf8_stringn, wrapper_free, SCM}, SkyliteProcError
};


pub fn form_to_string(obj: SCM) -> String {
    unsafe {
        let scm_string = scm_object_to_string(obj, scm_c_eval_string(CStr::from_bytes_with_nul(b"write\0").unwrap().as_ptr()));
        let raw_string = scm_to_utf8_stringn(scm_string, null_mut());
        let out = CStr::from_ptr(raw_string).to_str().unwrap().to_owned();
        wrapper_free(raw_string as *mut c_void);
        out
    }
}

/// Returns the value associated with `key` in `alist`.
pub(crate) unsafe fn assq_str(key: &str, alist: SCM) -> Result<Option<SCM>, SkyliteProcError> {
    if scm_is_false(scm_pair_p(alist)) {
        return Err(SkyliteProcError::DataError(format!("Not an alist: {}", form_to_string(alist))))
    }

    let key_cstr = CString::new(Into::<Vec<u8>>::into(key.as_bytes().to_owned())).unwrap();
    let res = scm_assq(scm_from_utf8_symbol(key_cstr.as_ptr()), alist);
    if scm_is_bool(res) != 0 {
        Ok(None)
    } else if scm_to_bool(scm_pair_p(scm_cdr(res))) != 0 {
        // For ('key val)
        Ok(Some(scm_cadr(res)))
    } else {
        // For ('key . val)
        Ok(Some(scm_cdr(res)))
    }
}

/// Converts a Scheme fixnum to an an integer of type `T`.
pub(crate) unsafe fn conv_int<T>(obj: SCM) -> Result<T, SkyliteProcError>
where
    T: TryFrom<i64>,
    <T as TryFrom<i64>>::Error: Display
{
    if scm_is_integer(obj) == 0{
        return Err(SkyliteProcError::DataError(format!("Expected integer, found {}", form_to_string(obj))));
    }
    match T::try_from(scm_to_int64(obj)) {
        Ok(val) => Ok(val),
        Err(err) => Err(SkyliteProcError::DataError(format!("{}", err)))
    }
}

/// Converts a Scheme flonum to an `f64`.
pub(crate) unsafe fn conv_f64(obj: SCM) -> Result<f64, SkyliteProcError>
{
    if scm_is_real(obj) == 0 {
        return Err(SkyliteProcError::DataError(format!("Expected floating point numer, found {}", form_to_string(obj))));
    }
    Ok(scm_to_double(obj))
}

/// Converts a Scheme flonum to an `f32`.
pub(crate) unsafe fn conv_f32(obj: SCM) -> Result<f32, SkyliteProcError> {
    conv_f64(obj).map(|val| val as f32)
}

/// Converts a Scheme boolean to a Rust `bool`.
pub(crate) unsafe fn conv_bool(obj: SCM) -> Result<bool, SkyliteProcError> {
    if scm_is_bool(obj) == 0{
        return Err(SkyliteProcError::DataError(format!("Expected boolean, found {}", form_to_string(obj))));
    }

    Ok(scm_is_true(obj))
}

/// Converts a Scheme string to a Rust `String`.
pub(crate) unsafe fn conv_string(obj: SCM) -> Result<String, SkyliteProcError> {
    if scm_is_false(scm_string_p(obj)) {
        return Err(SkyliteProcError::DataError(format!("Expected string, found {}", form_to_string(obj))));
    }

    let raw_string = scm_to_utf8_stringn(obj, null_mut());
    let out = CStr::from_ptr(raw_string).to_str().unwrap().to_owned();
    wrapper_free(raw_string as *mut c_void);
    Ok(out)
}

/// Converts a Scheme symbol to a Rust `String`.
pub(crate) unsafe fn conv_symbol(obj: SCM) -> Result<String, SkyliteProcError> {
    if !scm_is_symbol(obj) {
        return Err(SkyliteProcError::DataError(format!("Expected symbol, found {}", form_to_string(obj))));
    }

    Ok(conv_string(scm_symbol_to_string(obj)).unwrap())
}

pub(crate) struct SchemeListIterator {
    cursor: SCM
}

impl Iterator for SchemeListIterator {
    type Item = SCM;

    fn next(&mut self) -> Option<Self::Item> {
        unsafe {
            if scm_is_null(self.cursor) {
                return None;
            }
            let out = scm_car(self.cursor);
            self.cursor = scm_cdr(self.cursor);
            Some(out)
        }
    }
}

/// Iterate over items in a scheme list.
///
/// Returns an `Err` if the input is not a list.
pub(crate) unsafe fn iter_list(list: SCM) -> Result<SchemeListIterator, SkyliteProcError> {
    if scm_is_false(scm_list_p(list)) {
        Err(SkyliteProcError::DataError(format!("Not a list: {}", form_to_string(list))))
    } else {
        Ok(SchemeListIterator { cursor: list })
    }
}

pub(crate) enum CXROp {
    CAR, CDR
}

use CXROp::*;

/// Performs a sequence of CAR/CDR operations.
pub(crate) unsafe fn cxr(pair: SCM, ops: &[CXROp]) -> Result<SCM, SkyliteProcError> {
    let mut cursor = pair;
    for op in ops {
        if scm_to_bool(scm_pair_p(cursor)) == 0 {
            return Err(SkyliteProcError::DataError(format!("Not a pair, cannot do car/cdr: {}", form_to_string(cursor))));
        }
        match op {
            CAR => cursor = scm_car(cursor),
            CDR => cursor = scm_cdr(cursor),
        }
    }
    Ok(cursor)
}

/// A data item combined with a type.
#[derive(PartialEq, Debug)]
pub enum TypedValue {
    U8(u8), U16(u16), U32(u32), U64(u64),
    I8(i8), I16(i16), I32(i32), I64(i64),
    F32(f32), F64(f64),
    Bool(bool),
    String(String),
    Tuple(Vec<TypedValue>),
    Vec(Vec<TypedValue>)
}

/// Constructs a `TypedValue` given a Scheme form for the type and a form for the value.
///
/// `item_type` must be one of the following symbols for primitive types:
/// - `u8`, `u16`, `u32`, `u64`
/// - `i8`, `i16`, `i32`, `i64`
/// - `f32`, `f64`
/// - `bool`
/// - `string`
///
/// For primitive types, `data` must be a value which is convertible to the given type.
///
/// In addition, `item_type` can use the following forms to construct aggregate types:
/// - `(<type1> <type2> ... )` to construct a tuple of the given types. `data` must
///   be a tuple of the same shape.
/// - `(vec <type>)` to construct a vector of the given types. `data` must be a list of items
///   of the given type.
pub(crate) unsafe fn parse_typed_value(item_type: SCM, data: SCM) -> Result<TypedValue, SkyliteProcError> {
    if scm_is_symbol(item_type) {
        let type_name = conv_symbol(item_type)?;
        match &type_name[..] {
            "u8" => Ok(TypedValue::U8(conv_int(data)?)),
            "u16" => Ok(TypedValue::U16(conv_int(data)?)),
            "u32" => Ok(TypedValue::U32(conv_int(data)?)),
            "u64" => Ok(TypedValue::U64(conv_int(data)?)),
            "i8" => Ok(TypedValue::I8(conv_int(data)?)),
            "i16" => Ok(TypedValue::I16(conv_int(data)?)),
            "i32" => Ok(TypedValue::I32(conv_int(data)?)),
            "i64" => Ok(TypedValue::I64(conv_int(data)?)),
            "f32" => Ok(TypedValue::F32(conv_f32(data)?)),
            "f64" => Ok(TypedValue::F64(conv_f64(data)?)),
            "bool" => Ok(TypedValue::Bool(conv_bool(data)?)),
            "string" => Ok(TypedValue::String(conv_string(data)?)),
            _ => Err(SkyliteProcError::DataError(format!("Unknown data type: {}", type_name)))
        }
    } else if scm_is_true(scm_list_p(item_type)) {
        let car = scm_car(item_type);
        if scm_is_symbol(car) && conv_symbol(car)? == "vec" {
            parse_typed_value_vec(item_type, data)
        } else {
            parse_typed_value_tuple(item_type, data)
        }
    } else {
        Err(SkyliteProcError::DataError(format!("Unsupported item type: {}", form_to_string(item_type))))
    }
}

unsafe fn parse_typed_value_vec(vec_type: SCM, data: SCM) -> Result<TypedValue, SkyliteProcError> {
    let item_type = cxr(vec_type, &[CDR, CAR])?;
    let mut out: Vec<TypedValue> = Vec::new();

    for item in iter_list(data)? {
        out.push(parse_typed_value(item_type, item)?)
    }

    Ok(TypedValue::Vec(out))
}

unsafe fn parse_typed_value_tuple(types: SCM, values: SCM) -> Result<TypedValue, SkyliteProcError> {
    if scm_to_int64(scm_length(types)) != scm_to_int64(scm_length(values)) {
        return Err(SkyliteProcError::DataError(format!("Tuple definition has differing number of types and values.")));
    }

    let mut out: Vec<TypedValue> = Vec::new();
    for (t, v) in Iterator::zip(iter_list(types)?, iter_list(values)?) {
        out.push(parse_typed_value(t, v)?);
    }

    Ok(TypedValue::Tuple(out))
}

pub(crate) unsafe fn eval_str(expr: &str) -> Result<SCM, SkyliteProcError> {
    let safe_expr = format!("\
        (with-exception-handler
          (lambda (exc) `(err . ,exc))
          (lambda () `(ok . ,{}))
          #:unwind? #t)", expr);
    let c_expr = CString::new(safe_expr).unwrap();
    let res = scm_c_eval_string(c_expr.as_ptr());
    if conv_symbol(scm_car(res))? == "err" {
        println!("{}", form_to_string(scm_cdr(res)));
        Err(SkyliteProcError::GuileException(scm_cdr(res)))
    } else {
        Ok(scm_cdr(res))
    }
}

#[cfg(test)]
mod tests {
    use crate::{guile::{scm_from_bool, scm_from_double, scm_from_int32, scm_to_int32}, parse_util::{assq_str, eval_str}, with_guile};

    use super::{parse_typed_value, TypedValue};

    extern "C" fn test_assq_str_impl(_: &()) {
        unsafe {
            let alist = eval_str("'((a 1) (b 2) (c 3) (d 4))").unwrap();

            match assq_str("c", alist) {
                Ok(Some(v)) => assert_eq!(scm_to_int32(v), 3),
                res @ _ => {
                    println!("{:?}", res);
                    assert!(false)
                }
            }

            assert!(assq_str("e", alist).unwrap().is_none());
            assert!(assq_str("c", scm_from_int32(15)).is_err());
        }
    }

    #[test]
    fn test_assq_str() {
        with_guile(test_assq_str_impl, ());
    }

    extern "C" fn test_typed_value_impl(_: &()) {
        unsafe {
            let type_name = eval_str("'u8").unwrap();
            assert_eq!(parse_typed_value(type_name, scm_from_int32(5)).unwrap(), TypedValue::U8(5));
            assert!(parse_typed_value(type_name, scm_from_int32(300)).is_err());

            let type_name = eval_str("'f64").unwrap();
            let value = scm_from_double(1.0);
            assert_eq!(parse_typed_value(type_name, value).unwrap(), TypedValue::F64(1.0));

            let type_name = eval_str("'string").unwrap();
            let value = eval_str("\"test123\"").unwrap();
            assert_eq!(parse_typed_value(type_name, value).unwrap(), TypedValue::String("test123".to_owned()));

            let type_name = eval_str("'bool").unwrap();
            assert_eq!(parse_typed_value(type_name, scm_from_bool(true)).unwrap(), TypedValue::Bool(true));

            let type_name = eval_str("'(u8 bool (u16 u16))").unwrap();
            let value = eval_str("'(1 #t (2 3))").unwrap();
            assert_eq!(
                parse_typed_value(type_name, value).unwrap(),
                TypedValue::Tuple(vec![
                    TypedValue::U8(1),
                    TypedValue::Bool(true),
                    TypedValue::Tuple(vec![
                        TypedValue::U16(2),
                        TypedValue::U16(3),
                    ])
                ])
            );

            let type_name = eval_str("'(vec i16)").unwrap();
            let value = eval_str("'(0 5 10 15 20 25)").unwrap();
            assert_eq!(
                parse_typed_value(type_name, value).unwrap(),
                TypedValue::Vec(vec![
                    TypedValue::I16(0), TypedValue::I16(5), TypedValue::I16(10), TypedValue::I16(15), TypedValue::I16(20), TypedValue::I16(25)
                ])
            );
        }
    }

    #[test]
    fn test_typed_value() {
        with_guile(test_typed_value_impl, ());
    }
}
