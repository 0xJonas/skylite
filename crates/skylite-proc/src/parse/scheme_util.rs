use std::{ffi::{c_void, CStr, CString}, fmt::Display, ptr::null_mut, sync::{Mutex, MutexGuard}};

use crate::{parse::guile::{scm_assq, scm_c_eval_string, scm_cadr, scm_car, scm_cdr, scm_from_utf8_symbol, scm_is_bool, scm_is_false, scm_is_integer, scm_is_null, scm_is_real, scm_is_symbol, scm_is_true, scm_list_p, scm_object_to_string, scm_pair_p, scm_string_p, scm_symbol_to_string, scm_to_bool, scm_to_double, scm_to_int64, scm_to_utf8_stringn, scm_with_guile, wrapper_free, SCM}, SkyliteProcError};

static GUILE_INIT_LOCK: Mutex<()> = Mutex::new(());

struct CallInfo<'a, P: ?Sized, R> {
    func: extern "C" fn(&P) -> R,
    params: &'a P,
    res: Option<R>,
    guard: Option<MutexGuard<'static, ()>>
}

/// Runs code with access to Guile.
pub(crate) fn with_guile<P: ?Sized, R>(func: extern "C" fn(&P) -> R, params: &P) -> R {
    // This function has to jump a few hoops to deal with some of
    // libguile's shenanigans:
    // - Any Guile function can do a nonlocal exit (via longjmp), which is
    //   crazy unsafe. A nonlocal exit is detected by executing the actual
    //   user code inside a wrapper function, which either sets a result
    //   variable (CallInfo::res) if the code ran successfully, or does not
    //   set a result, if a nonlocal exit back up to the enclosing scm_with_guile
    //   call was done. In the latter case, we simply panic, because we just leaked
    //   a bunch of memory by subverting Rust's borrow checker with a longjmp.
    //   User code must ensure that this never happens.
    // - Guile initialization is not actually thread-safe (at least the
    //   initialization of any thread after the first one isn't). So the code
    //   before calling scm_with_guile until actually running the user code
    //   must happen while holding a lock.

    unsafe extern "C" fn wrapper<P: ?Sized, R>(user_data: *mut c_void) -> *mut c_void {
        let call = user_data as *mut CallInfo<P, R>;
        // Unlocks guile_init_lock
        drop((*call).guard.take().unwrap());

        // Guile might do a nonlocal return on this line, causing res to not be set.
        // TODO: catch_unwind
        (*call).res = Some(((*call).func)(&(*call).params));
        null_mut()
    }

    // Lock during Guile initialization, because something in there is not thread-safe.
    let guard = GUILE_INIT_LOCK.lock().unwrap();
    let call = CallInfo { func, params, res: None, guard: Some(guard) };
    unsafe {
        scm_with_guile(Some(wrapper::<P, R>), &call as *const CallInfo<P, R> as *mut c_void);
        match call.res {
            Some(v) => v,
            None => panic!("Nonlocal exit from Guile mode!")
        }
    }
}

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
    } else {
        // For ('key . val)
        Ok(Some(scm_cdr(res)))
    }
}

/// Converts a Scheme fixnum to an an integer of type `T`.
pub(crate) unsafe fn parse_int<T>(obj: SCM) -> Result<T, SkyliteProcError>
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
pub(crate) unsafe fn parse_f64(obj: SCM) -> Result<f64, SkyliteProcError>
{
    if scm_is_real(obj) == 0 {
        return Err(SkyliteProcError::DataError(format!("Expected floating point numer, found {}", form_to_string(obj))));
    }
    Ok(scm_to_double(obj))
}

/// Converts a Scheme flonum to an `f32`.
pub(crate) unsafe fn parse_f32(obj: SCM) -> Result<f32, SkyliteProcError> {
    parse_f64(obj).map(|val| val as f32)
}

/// Converts a Scheme boolean to a Rust `bool`.
pub(crate) unsafe fn parse_bool(obj: SCM) -> Result<bool, SkyliteProcError> {
    if scm_is_bool(obj) == 0{
        return Err(SkyliteProcError::DataError(format!("Expected boolean, found {}", form_to_string(obj))));
    }

    Ok(scm_is_true(obj))
}

/// Converts a Scheme string to a Rust `String`.
pub(crate) unsafe fn parse_string(obj: SCM) -> Result<String, SkyliteProcError> {
    if scm_is_false(scm_string_p(obj)) {
        return Err(SkyliteProcError::DataError(format!("Expected string, found {}", form_to_string(obj))));
    }

    let raw_string = scm_to_utf8_stringn(obj, null_mut());
    let out = CStr::from_ptr(raw_string).to_str().unwrap().to_owned();
    wrapper_free(raw_string as *mut c_void);
    Ok(out)
}

/// Converts a Scheme symbol to a Rust `String`.
pub(crate) unsafe fn parse_symbol(obj: SCM) -> Result<String, SkyliteProcError> {
    if !scm_is_symbol(obj) {
        return Err(SkyliteProcError::DataError(format!("Expected symbol, found {}", form_to_string(obj))));
    }

    Ok(parse_string(scm_symbol_to_string(obj)).unwrap())
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

pub(crate) unsafe fn eval_str(expr: &str) -> Result<SCM, SkyliteProcError> {
    let safe_expr = format!("\
        (with-exception-handler
          (lambda (exc) `(err . ,exc))
          (lambda () `(ok . ,{}))
          #:unwind? #t)", expr);
    let c_expr = CString::new(safe_expr).unwrap();
    let res = scm_c_eval_string(c_expr.as_ptr());
    if parse_symbol(scm_car(res))? == "err" {
        Err(SkyliteProcError::GuileException(scm_cdr(res)))
    } else {
        Ok(scm_cdr(res))
    }
}

#[cfg(test)]
mod tests {
    use crate::parse::{guile::{scm_car, scm_from_int16, scm_from_int32, scm_to_int32}, scheme_util::{assq_str, eval_str}};

    use super::with_guile;

    extern "C" fn guile_bad(_: &()) -> () {
        unsafe {
            let _ = scm_car(scm_from_int16(0));
        }
    }

    #[test]
    #[should_panic(expected = "Nonlocal exit from Guile mode!")]
    fn test_exception() {
        with_guile(guile_bad, &());
    }

    extern "C" fn test_assq_str_impl(_: &()) {
        unsafe {
            let alist = eval_str("'((a . 1) (b . 2) (c . 3) (d . 4))").unwrap();

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
        with_guile(test_assq_str_impl, &());
    }
}
