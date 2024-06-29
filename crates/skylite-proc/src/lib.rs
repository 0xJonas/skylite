use std::{os::raw::c_void, ptr::null_mut, sync::{Mutex, MutexGuard}};

use guile::{scm_with_guile, SCM};

mod guile;
mod parse_util;
mod project;

extern crate glob;

#[derive(Debug, Clone)]
enum SkyliteProcError {
    GuileException(SCM),
    DataError(String)
}

static guile_init_lock: Mutex<()> = Mutex::new(());

struct CallInfo<'a, P, R> {
    func: extern "C" fn(&P) -> R,
    params: P,
    res: Option<R>,
    guard: Option<MutexGuard<'a, ()>>
}

/// Runs code with access to Guile.
fn with_guile<P, R>(func: extern "C" fn(&P) -> R, params: P) -> R {
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

    unsafe extern "C" fn wrapper<P, R>(user_data: *mut c_void) -> *mut c_void {
        let call = user_data as *mut CallInfo<P, R>;
        // Unlocks guile_init_lock
        drop((*call).guard.take().unwrap());

        // Guile might do a nonlocal return on this line, causing res to not be set.
        // TODO: catch_unwind
        (*call).res = Some(((*call).func)(&(*call).params));
        null_mut()
    }

    // Lock during Guile initialization, because something in there is not thread-safe.
    let guard = guile_init_lock.lock().unwrap();
    let call = CallInfo { func, params, res: None, guard: Some(guard) };
    unsafe {
        scm_with_guile(Some(wrapper::<P, R>), &call as *const CallInfo<P, R> as *mut c_void);
        match call.res {
            Some(v) => v,
            None => panic!("Nonlocal exit from Guile mode!")
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{guile::{scm_car, scm_from_int16}, with_guile};


    extern "C" fn guile_bad(_: &()) -> () {
        unsafe {
            let _ = scm_car(scm_from_int16(0));
        }
    }

    #[test]
    #[should_panic(expected = "Nonlocal exit from Guile mode!")]
    fn test_exception() {
        with_guile(guile_bad, ());
    }
}
