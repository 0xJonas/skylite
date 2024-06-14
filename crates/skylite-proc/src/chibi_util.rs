use crate::chibi_scheme;
use crate::chibi_scheme::sexp;
use std::slice;

pub unsafe fn form_to_string(ctx: &ChibiContext, obj: sexp) -> String {
    let sexp_str = ctx.make_var(chibi_scheme::sexp_write_to_string(ctx.c, obj));
    let bytes = slice::from_raw_parts(
        chibi_scheme::sexp_string_data(*sexp_str.get()) as *const u8,
        chibi_scheme::sexp_string_size(*sexp_str.get()) as usize
    );
    std::str::from_utf8(bytes).unwrap().to_owned()
}

pub fn wrap_result(obj: sexp) -> Result<sexp, sexp> {
    if obj == std::ptr::null_mut() {
        return Err(obj);
    }
    let is_exception = unsafe { chibi_scheme::sexp_exceptionp(obj) };
    if is_exception {
        Err(obj)
    } else {
        Ok(obj)
    }
}

pub struct ChibiContext {
    pub c: chibi_scheme::sexp
}

pub struct ChibiVar<'ctx> {
    var: std::pin::Pin<Box<sexp>>,
    gc_preserver: std::pin::Pin<Box<chibi_scheme::sexp_gc_var_t>>,
    context: &'ctx ChibiContext
}

impl ChibiContext {
    pub fn new() -> Result<ChibiContext, sexp> {
        unsafe {
            // SAFETY: May allocate global data, but there is no way to avoid that. Even when called multiple times, this will only have an effect once.
            chibi_scheme::sexp_scheme_init();
            // SAFETY: Only allocates on success.
            let c = wrap_result(chibi_scheme::sexp_make_eval_context(std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut(), 0, 0))?;
            // SAFETY: The new context is destroyed before exiting with an error.
            let sexp_seven = chibi_scheme::sexp_make_fixnum(7);
            wrap_result(chibi_scheme::sexp_load_standard_env(c, std::ptr::null_mut(), sexp_seven))
                .or_else(|_| Err(chibi_scheme::sexp_destroy_context(c)))?;
            Ok(ChibiContext {
                c
            })
        }
    }

    pub fn make_var(&self, val: sexp) -> ChibiVar {
        let var = Box::pin(val);
        let gc_var = var.as_ref().get_ref() as *const sexp as *mut sexp;

        let gc_preserver = Box::pin(chibi_scheme::sexp_gc_var_t {
            var: gc_var,
            next: unsafe { (*self.c).value.context.saves }
        });

        unsafe {
            (*self.c).value.context.saves = gc_preserver.as_ref().get_ref() as *const chibi_scheme::sexp_gc_var_t as *mut chibi_scheme::sexp_gc_var_t;
        }

        ChibiVar {
            var,
            gc_preserver,
            context: self
        }
    }
}

impl Drop for ChibiContext {
    fn drop(&mut self) {
        unsafe {
            chibi_scheme::sexp_destroy_context(self.c);
        }
    }
}

impl<'ctx> ChibiVar<'ctx> {
    pub fn set(&mut self, val: sexp) {
        self.var.set(val);
    }

    pub fn get(&self) -> &sexp {
        self.var.as_ref().get_ref()
    }
}

impl<'ctx> Drop for ChibiVar<'ctx> {
    fn drop(&mut self) {
        unsafe {
            let mut head = &mut (*self.context.c).value.context.saves;
            let self_addr = self.gc_preserver.as_ref().get_ref() as *const chibi_scheme::sexp_gc_var_t as *mut chibi_scheme::sexp_gc_var_t;
            while *head != self_addr && !(**head).next.is_null() {
                head = &mut (**head).next;
            }
            *head = (**head).next;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::ptr::addr_of_mut;

    use crate::{chibi_scheme, chibi_util::wrap_result};

    use super::ChibiContext;

    unsafe fn test_chibi_var_impl() -> Result<(), chibi_scheme::sexp> {
        chibi_scheme::sexp_scheme_init();
        let ctx = ChibiContext::new()?;
        let var1 = ctx.make_var(chibi_scheme::sexp_cons(ctx.c, chibi_scheme::sexp_make_fixnum(5), chibi_scheme::sexp_make_fixnum(10)));
        let var2 = ctx.make_var(chibi_scheme::sexp_cons(ctx.c, chibi_scheme::sexp_make_fixnum(15), chibi_scheme::sexp_make_fixnum(20)));
        let mut sum_freed = 0;
        // Clear out any intermediate objects
        wrap_result(chibi_scheme::sexp_gc(ctx.c, addr_of_mut!(sum_freed)))?;

        wrap_result(chibi_scheme::sexp_gc(ctx.c, addr_of_mut!(sum_freed)))?;
        assert_eq!(sum_freed, 0);
        drop(var1);
        wrap_result(chibi_scheme::sexp_gc(ctx.c, addr_of_mut!(sum_freed)))?;
        assert_ne!(sum_freed, 0);
        drop(var2);
        wrap_result(chibi_scheme::sexp_gc(ctx.c, addr_of_mut!(sum_freed)))?;
        assert_ne!(sum_freed, 0);

        Ok(())
    }

    #[test]
    fn test_chibi_var() {
        unsafe {
            let res = test_chibi_var_impl();
            assert!(res.is_ok());
        }
    }
}