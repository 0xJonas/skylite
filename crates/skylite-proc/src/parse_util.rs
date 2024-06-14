use std::{ffi::CString, fmt::Display, ptr::null_mut};

use crate::{
    chibi_scheme::{sexp, sexp_assq, sexp_booleanp, sexp_c_string, sexp_car, sexp_cdr, sexp_eval_string, sexp_flonum_value, sexp_flonump, sexp_length, sexp_listp, sexp_nullp, sexp_numberp, sexp_pairp, sexp_string_data, sexp_string_size, sexp_string_to_symbol, sexp_stringp, sexp_symbol_to_string, sexp_symbolp, sexp_unbox_boolean, sexp_unbox_fixnum},
    chibi_util::{form_to_string, wrap_result, ChibiContext, ChibiVar},
    SkyliteProcError
};


pub(crate) fn catch_err(val: sexp) -> Result<sexp, SkyliteProcError> {
    wrap_result(val).map_err(|err| SkyliteProcError::ChibiException(err))
}

/// Returns the value associated with `key` in `alist`.
pub(crate) unsafe fn assq_str(ctx: &ChibiContext, key: &str, alist: sexp) -> Result<Option<sexp>, SkyliteProcError> {
    if !sexp_pairp(alist) {
        return Err(SkyliteProcError::DataError(format!("Not an alist: {}", form_to_string(ctx, alist))))
    }

    let key_cstr = CString::new(Into::<Vec<u8>>::into(key.as_bytes().to_owned())).unwrap();
    let sexp_str = ctx.make_var(
        catch_err(sexp_c_string(ctx.c, key_cstr.as_ptr(), key.len() as i64))?
    );
    let sexp_symbol = ctx.make_var(
        catch_err(sexp_string_to_symbol(ctx.c, *sexp_str.get()))?
    );
    let res = ctx.make_var(sexp_assq(ctx.c, *sexp_symbol.get(), alist));
    if sexp_booleanp(*res.get()) {
        Ok(None)
    } else if sexp_pairp(sexp_cdr(*res.get())){
        // For ('key val)
        Ok(Some(sexp_car(sexp_cdr(*res.get()))))
    } else {
        // For ('key . val)
        Ok(Some(sexp_cdr(*res.get())))
    }
}

/// Converts a Scheme fixnum to an an integer of type `T`.
pub(crate) unsafe fn conv_int<T>(ctx: &ChibiContext, obj: sexp) -> Result<T, SkyliteProcError>
where
    T: TryFrom<i64>,
    <T as TryFrom<i64>>::Error: Display
{
    if !sexp_numberp(obj) {
        return Err(SkyliteProcError::DataError(format!("Expected integer, found {}", form_to_string(ctx, obj))));
    }
    match T::try_from(sexp_unbox_fixnum(obj)) {
        Ok(val) => Ok(val),
        Err(err) => Err(SkyliteProcError::DataError(format!("{}", err)))
    }
}

/// Converts a Scheme flonum to an `f64`.
pub(crate) unsafe fn conv_f64(ctx: &ChibiContext, obj: sexp) -> Result<f64, SkyliteProcError>
{
    if !sexp_flonump(obj) {
        return Err(SkyliteProcError::DataError(format!("Expected floating point numer, found {}", form_to_string(ctx, obj))));
    }
    Ok(sexp_flonum_value(obj))
}

/// Converts a Scheme flonum to an `f32`.
pub(crate) unsafe fn conv_f32(ctx: &ChibiContext, obj: sexp) -> Result<f32, SkyliteProcError> {
    conv_f64(ctx, obj).map(|val| val as f32)
}

/// Converts a Scheme boolean to a Rust `bool`.
pub(crate) unsafe fn conv_bool(ctx: &ChibiContext, obj: sexp) -> Result<bool, SkyliteProcError> {
    if !sexp_booleanp(obj) {
        return Err(SkyliteProcError::DataError(format!("Expected boolean, found {}", form_to_string(ctx, obj))));
    }

    Ok(sexp_unbox_boolean(obj))
}

/// Converts a Scheme string to a Rust `String`.
pub(crate) unsafe fn conv_string(ctx: &ChibiContext, obj: sexp) -> Result<String, SkyliteProcError> {
    if !sexp_stringp(obj) {
        return Err(SkyliteProcError::DataError(format!("Expected string, found {}", form_to_string(ctx, obj))));
    }

    let bytes = std::slice::from_raw_parts(
        sexp_string_data(obj) as *const u8,
        sexp_string_size(obj) as usize
    );
    Ok(std::str::from_utf8(bytes).unwrap().to_owned())
}

/// Converts a Scheme symbol to a Rust `String`.
pub(crate) unsafe fn conv_symbol(ctx: &ChibiContext, obj: sexp) -> Result<String, SkyliteProcError> {
    if !sexp_symbolp(obj) {
        return Err(SkyliteProcError::DataError(format!("Expected symbol, found {}", form_to_string(ctx, obj))));
    }

    let string = ctx.make_var(catch_err(sexp_symbol_to_string(ctx.c, obj))?);
    Ok(conv_string(ctx, *string.get()).unwrap())
}

pub(crate) struct SchemeListIterator<'ctx> {
    list: ChibiVar<'ctx>,
    cursor: sexp
}

impl<'ctx> Iterator for SchemeListIterator<'ctx> {
    type Item = sexp;

    fn next(&mut self) -> Option<Self::Item> {
        unsafe {
            if sexp_nullp(self.cursor) {
                return None;
            }
            let out = sexp_car(self.cursor);
            self.cursor = sexp_cdr(self.cursor);
            Some(out)
        }
    }
}

/// Iterate over items in a scheme list.
///
/// Returns an `Err` if the input is not a list.
pub(crate) unsafe fn iter_list(ctx: &ChibiContext, list: sexp) -> Result<SchemeListIterator, SkyliteProcError> {
    if !sexp_unbox_boolean(catch_err(sexp_listp(ctx.c, list))?) {
        Err(SkyliteProcError::DataError(format!("Not a list: {}", form_to_string(ctx, list))))
    } else {
        Ok(SchemeListIterator { list: ctx.make_var(list), cursor: list })
    }
}

pub(crate) enum CXROp {
    CAR, CDR
}

use CXROp::*;

/// Performs a sequence of CAR/CDR operations.
pub(crate) unsafe fn cxr(ctx: &ChibiContext, pair: sexp, ops: &[CXROp]) -> Result<sexp, SkyliteProcError> {
    let mut cursor = pair;
    for op in ops {
        if !sexp_pairp(cursor) {
            return Err(SkyliteProcError::DataError(format!("Not a pair, cannot do car/cdr: {}", form_to_string(ctx, cursor))));
        }
        match op {
            CAR => cursor = sexp_car(cursor),
            CDR => cursor = sexp_cdr(cursor),
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
pub(crate) unsafe fn parse_typed_value(ctx: &ChibiContext, item_type: sexp, data: sexp) -> Result<TypedValue, SkyliteProcError> {
    if sexp_symbolp(item_type) {
        let type_name = conv_symbol(ctx, item_type)?;
        match &type_name[..] {
            "u8" => Ok(TypedValue::U8(conv_int(ctx, data)?)),
            "u16" => Ok(TypedValue::U16(conv_int(ctx, data)?)),
            "u32" => Ok(TypedValue::U32(conv_int(ctx, data)?)),
            "u64" => Ok(TypedValue::U64(conv_int(ctx, data)?)),
            "i8" => Ok(TypedValue::I8(conv_int(ctx, data)?)),
            "i16" => Ok(TypedValue::I16(conv_int(ctx, data)?)),
            "i32" => Ok(TypedValue::I32(conv_int(ctx, data)?)),
            "i64" => Ok(TypedValue::I64(conv_int(ctx, data)?)),
            "f32" => Ok(TypedValue::F32(conv_f32(ctx, data)?)),
            "f64" => Ok(TypedValue::F64(conv_f64(ctx, data)?)),
            "bool" => Ok(TypedValue::Bool(conv_bool(ctx, data)?)),
            "string" => Ok(TypedValue::String(conv_string(ctx, data)?)),
            _ => Err(SkyliteProcError::DataError(format!("Unknown data type: {}", type_name)))
        }
    } else if sexp_unbox_boolean(sexp_listp(ctx.c, item_type)) {
        let car = sexp_car(item_type);
        if sexp_symbolp(car) && conv_symbol(ctx, car)? == "vec" {
            parse_typed_value_vec(ctx, item_type, data)
        } else {
            parse_typed_value_tuple(ctx, item_type, data)
        }
    } else {
        Err(SkyliteProcError::DataError(format!("Unsupported item type: {}", form_to_string(ctx, item_type))))
    }
}

unsafe fn parse_typed_value_vec(ctx: &ChibiContext, vec_type: sexp, data: sexp) -> Result<TypedValue, SkyliteProcError> {
    let item_type = ctx.make_var(cxr(ctx, vec_type, &[CDR, CAR])?);
    let data_var = ctx.make_var(data);
    let mut out: Vec<TypedValue> = Vec::new();

    for item in iter_list(ctx, *data_var.get())? {
        out.push(parse_typed_value(ctx, *item_type.get(), item)?)
    }

    Ok(TypedValue::Vec(out))
}

unsafe fn parse_typed_value_tuple(ctx: &ChibiContext, types: sexp, values: sexp) -> Result<TypedValue, SkyliteProcError> {
    if sexp_unbox_fixnum(sexp_length(ctx.c, types)) != sexp_unbox_fixnum(sexp_length(ctx.c, values)) {
        return Err(SkyliteProcError::DataError(format!("Tuple definition has differing number of types and values.")));
    }

    let mut out: Vec<TypedValue> = Vec::new();
    for (t, v) in Iterator::zip(iter_list(ctx, types)?, iter_list(ctx, values)?) {
        out.push(parse_typed_value(ctx, t, v)?);
    }

    Ok(TypedValue::Tuple(out))
}

pub(crate) unsafe fn eval_str(ctx: &ChibiContext, expr: &str) -> Result<sexp, SkyliteProcError> {
    let c_expr = CString::new(expr).unwrap();
    catch_err(sexp_eval_string(ctx.c, c_expr.as_ptr(), expr.len() as i64, null_mut()))
}

#[cfg(test)]
mod tests {
    use crate::{chibi_scheme::{sexp_make_boolean, sexp_make_fixnum, sexp_make_flonum, sexp_unbox_fixnum}, chibi_util::ChibiContext, parse_util::{assq_str, eval_str}};

    use super::{parse_typed_value, TypedValue};

    #[test]
    fn test_assq_str() {
        unsafe {
            let ctx = ChibiContext::new().unwrap();
            let alist = eval_str(&ctx, "'((a 1) (b 2) (c 3) (d 4))").unwrap();

            match assq_str(&ctx, "c", alist) {
                Ok(Some(v)) => assert_eq!(sexp_unbox_fixnum(v), 3),
                _ => assert!(false)
            }

            assert!(assq_str(&ctx, "e", alist).unwrap().is_none());
            assert!(assq_str(&ctx, "c", sexp_make_fixnum(15)).is_err());
        }
    }

    #[test]
    fn test_typed_value() {
        unsafe {
            let ctx = ChibiContext::new().unwrap();

            let type_name = ctx.make_var(eval_str(&ctx, "'u8").unwrap());
            assert_eq!(parse_typed_value(&ctx, *type_name.get(), sexp_make_fixnum(5)).unwrap(), TypedValue::U8(5));
            assert!(parse_typed_value(&ctx, *type_name.get(), sexp_make_fixnum(300)).is_err());

            let type_name = ctx.make_var(eval_str(&ctx, "'f64").unwrap());
            let value = ctx.make_var(sexp_make_flonum(ctx.c, 1.0));
            assert_eq!(parse_typed_value(&ctx, *type_name.get(), *value.get()).unwrap(), TypedValue::F64(1.0));

            let type_name = ctx.make_var(eval_str(&ctx, "'string").unwrap());
            let value = ctx.make_var(eval_str(&ctx, "\"test123\"").unwrap());
            assert_eq!(parse_typed_value(&ctx, *type_name.get(), *value.get()).unwrap(), TypedValue::String("test123".to_owned()));

            let type_name = ctx.make_var(eval_str(&ctx, "'bool").unwrap());
            assert_eq!(parse_typed_value(&ctx, *type_name.get(), sexp_make_boolean(true)).unwrap(), TypedValue::Bool(true));

            let type_name = ctx.make_var(eval_str(&ctx, "'(u8 bool (u16 u16))").unwrap());
            let value = ctx.make_var(eval_str(&ctx, "'(1 #t (2 3))").unwrap());
            assert_eq!(
                parse_typed_value(&ctx, *type_name.get(), *value.get()).unwrap(),
                TypedValue::Tuple(vec![
                    TypedValue::U8(1),
                    TypedValue::Bool(true),
                    TypedValue::Tuple(vec![
                        TypedValue::U16(2),
                        TypedValue::U16(3),
                    ])
                ])
            );

            let type_name = ctx.make_var(eval_str(&ctx, "'(vec i16)").unwrap());
            let value = ctx.make_var(eval_str(&ctx, "'(0 5 10 15 20 25)").unwrap());
            assert_eq!(
                parse_typed_value(&ctx, *type_name.get(), *value.get()).unwrap(),
                TypedValue::Vec(vec![
                    TypedValue::I16(0), TypedValue::I16(5), TypedValue::I16(10), TypedValue::I16(15), TypedValue::I16(20), TypedValue::I16(25)
                ])
            );
        }
    }
}
