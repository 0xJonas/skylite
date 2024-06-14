use chibi_scheme::sexp;

mod chibi_scheme;
mod chibi_util;
mod parse_util;
mod project;

extern crate glob;

#[derive(Debug, Clone)]
enum SkyliteProcError {
    ChibiException(sexp),
    DataError(String)
}
