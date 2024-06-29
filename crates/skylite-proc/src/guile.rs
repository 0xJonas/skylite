#![allow(unused)]
#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
include!(concat!(env!("OUT_DIR"), "/guile.rs"));

extern "C" {
    #[link_name = "scm_car_wrapper"]
    pub fn scm_car(obj: SCM) -> SCM;

    #[link_name = "scm_cdr_wrapper"]
    pub fn scm_cdr(obj: SCM) -> SCM;

    #[link_name = "scm_is_true_wrapper"]
    pub fn scm_is_true(obj: SCM) -> bool;

    #[link_name = "scm_is_false_wrapper"]
    pub fn scm_is_false(obj: SCM) -> bool;
}
