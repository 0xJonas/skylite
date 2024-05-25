#include "chibi/eval.h"

#include <stdbool.h>

bool (sexp_booleanp)(sexp obj) { return sexp_booleanp(obj); }
bool (sexp_fixnump)(sexp obj) { return sexp_fixnump(obj); }
bool (sexp_flonump)(sexp obj) { return sexp_flonump(obj); }
bool (sexp_bignump)(sexp obj) { return sexp_bignump(obj); }
bool (sexp_integerp)(sexp obj) { return sexp_integerp(obj); }
bool (sexp_numberp)(sexp obj) { return sexp_numberp(obj); }
bool (sexp_charp)(sexp obj) { return sexp_charp(obj); }
bool (sexp_stringp)(sexp obj) { return sexp_stringp(obj); }
bool (sexp_string_cursorp)(sexp obj) { return sexp_string_cursorp(obj); }
bool (sexp_bytesp)(sexp obj) { return sexp_bytesp(obj); }
bool (sexp_symbolp)(sexp obj) { return sexp_symbolp(obj); }
bool (sexp_nullp)(sexp obj) { return sexp_nullp(obj); }
bool (sexp_pairp)(sexp obj) { return sexp_pairp(obj); }
bool (sexp_vectorp)(sexp obj) { return sexp_vectorp(obj); }
bool (sexp_iportp)(sexp obj) { return sexp_iportp(obj); }
bool (sexp_oportp)(sexp obj) { return sexp_oportp(obj); }
bool (sexp_portp)(sexp obj) { return sexp_portp(obj); }
bool (sexp_procedurep)(sexp obj) { return sexp_procedurep(obj); }
bool (sexp_opcodep)(sexp obj) { return sexp_opcodep(obj); }
bool (sexp_applicablep)(sexp obj) { return sexp_applicablep(obj); }
bool (sexp_typep)(sexp obj) { return sexp_typep(obj); }
bool (sexp_exceptionp)(sexp obj) { return sexp_exceptionp(obj); }
bool (sexp_contextp)(sexp obj) { return sexp_contextp(obj); }
bool (sexp_envp)(sexp obj) { return sexp_envp(obj); }
bool (sexp_corep)(sexp obj) { return sexp_corep(obj); }
bool (sexp_macrop)(sexp obj) { return sexp_macrop(obj); }
bool (sexp_synclop)(sexp obj) { return sexp_synclop(obj); }
bool (sexp_bytecodep)(sexp obj) { return sexp_bytecodep(obj); }
bool (sexp_cpointerp)(sexp obj) { return sexp_cpointerp(obj); }

char *(sexp_string_data)(sexp x) { return sexp_string_data(x); }
sexp_uint_t (sexp_string_size)(sexp x) { return sexp_string_size(x); }
sexp_uint_t (sexp_string_length)(sexp x) { return sexp_string_length(x); }

sexp (sexp_string_ref)(sexp ctx, sexp s, sexp i) { return sexp_string_ref(ctx, s, i); }
sexp (sexp_string_set)(sexp ctx, sexp s, sexp i, sexp ch) { return sexp_string_set(ctx, s, i, ch); }
sexp (sexp_string_cursor_ref)(sexp ctx, sexp s, sexp i) { return sexp_string_cursor_ref(ctx, s, i); }
void (sexp_string_cursor_set)(sexp ctx, sexp s, sexp i, sexp ch) { sexp_string_cursor_set(ctx, s, i, ch); }
sexp (sexp_string_cursor_next)(sexp s, sexp i) { return sexp_string_cursor_next(s, i); }
sexp (sexp_string_cursor_prev)(sexp s, sexp i) { return sexp_string_cursor_prev(s, i); }
sexp (sexp_substring)(sexp ctx, sexp s, sexp i, sexp j) { return sexp_substring(ctx, s, i, j); }
sexp (sexp_substring_cursor)(sexp ctx, sexp s, sexp i, sexp j) { return sexp_substring_cursor(ctx, s, i, j); }

sexp (sexp_make_boolean)(bool n) { return sexp_make_boolean(n); }
bool (sexp_unbox_boolean)(sexp obj) { return sexp_unbox_boolean(obj); }
sexp (sexp_make_fixnum)(sexp_sint_t n) { return sexp_make_fixnum(n); }
sexp_sint_t (sexp_unbox_fixnum)(sexp obj) { return sexp_unbox_fixnum(obj); }
sexp (sexp_make_character)(char n) { return sexp_make_character(n); }
char (sexp_unbox_character)(sexp obj) { return sexp_unbox_character(obj); }
sexp (sexp_make_string_cursor)(int n) { return sexp_make_string_cursor(n); }
int (sexp_unbox_string_cursor)(sexp obj) { return sexp_unbox_string_cursor(obj); }
sexp (sexp_car)(sexp pair) { return sexp_car(pair); }
sexp (sexp_cdr)(sexp pair) { return sexp_cdr(pair); }
sexp (sexp_ratio_numerator)(sexp q) { return sexp_ratio_numerator(q); }
sexp (sexp_ratio_denominator)(sexp q) { return sexp_ratio_denominator(q); }
sexp (sexp_complex_real)(sexp z) { return sexp_complex_real(z); }
sexp (sexp_complex_imag)(sexp z) { return sexp_complex_imag(z); }
sexp_uint_t (sexp_bytes_length)(sexp bv) { return sexp_bytes_length(bv); }
char *(sexp_bytes_data)(sexp bv) { return sexp_bytes_data(bv); }
sexp (sexp_bytes_ref)(sexp bv, sexp i) { return sexp_bytes_ref(bv, i); }
sexp (sexp_bytes_set)(sexp bv, sexp i, sexp obj) {
    sexp_bytes_set(bv, i, obj);
    return SEXP_VOID;
}
sexp_uint_t (sexp_vector_length)(sexp vec) { return sexp_vector_length(vec); }
sexp (sexp_vector_ref)(sexp vec, sexp i) { return sexp_vector_ref(vec, i); }
sexp (sexp_vector_set)(sexp vec, sexp i, sexp obj) {
    sexp_vector_set(vec, i, obj);
    return SEXP_VOID;
}

sexp (sexp_cons)(sexp ctx, sexp obj1, sexp obj2) { return sexp_cons(ctx, obj1, obj2); }
sexp (sexp_list1)(sexp ctx, sexp obj) { return sexp_list1(ctx, obj); }
sexp (sexp_make_string)(sexp ctx, sexp len, sexp ch) { return sexp_make_string(ctx, len, ch); }
sexp (sexp_make_bytes)(sexp ctx, sexp len, sexp i) { return sexp_make_bytes(ctx, len, i); }
sexp (sexp_make_vector)(sexp ctx, sexp len, sexp obj) { return sexp_make_vector(ctx, len, obj); }

sexp (sexp_read)(sexp ctx, sexp in) { return sexp_read(ctx, in); }
sexp (sexp_write)(sexp ctx, sexp out, sexp obj) { return sexp_write(ctx, out, obj); }
int (sexp_write_string)(sexp ctx, const char *str, sexp out) { return sexp_write_string(ctx, str, out); }
int (sexp_newline)(sexp ctx, sexp out) { return sexp_newline(ctx, out); }
sexp (sexp_print_exception)(sexp ctx, sexp exn, sexp out) { return sexp_print_exception(ctx, exn, out); }
sexp (sexp_current_input_port)(sexp ctx) { return sexp_current_input_port(ctx); }
sexp (sexp_current_output_port)(sexp ctx) { return sexp_current_output_port(ctx); }
sexp (sexp_current_error_port)(sexp ctx) { return sexp_current_error_port(ctx); }
int (sexp_debug)(sexp ctx, char* msg, sexp obj) { return sexp_debug(ctx, msg, obj); }
sexp (sexp_open_input_string)(sexp ctx, sexp str) { return sexp_open_input_string(ctx, str); }
sexp (sexp_open_output_string)(sexp ctx) { return sexp_open_output_string(ctx); }
sexp (sexp_get_output_string)(sexp ctx, sexp port) { return sexp_get_output_string(ctx, port); }

sexp (sexp_equalp)(sexp ctx, sexp x, sexp y) { return sexp_equalp(ctx, x, y); }
sexp (sexp_length)(sexp ctx, sexp ls) { return sexp_length(ctx, ls); }
sexp (sexp_listp)(sexp ctx, sexp x) { return sexp_listp(ctx, x); }
sexp (sexp_memq)(sexp ctx, sexp x, sexp ls) { return sexp_memq(ctx, x, ls); }
sexp (sexp_assq)(sexp ctx, sexp x, sexp ls) { return sexp_assq(ctx, x, ls); }
sexp (sexp_reverse)(sexp ctx, sexp ls) { return sexp_reverse(ctx, ls); }
sexp (sexp_nreverse)(sexp ctx, sexp ls) { return sexp_nreverse(ctx, ls); }
sexp (sexp_append2)(sexp ctx, sexp a, sexp b) { return sexp_append2(ctx, a, b); }
sexp (sexp_copy_list)(sexp ctx, sexp ls) { return sexp_copy_list(ctx, ls); }
sexp (sexp_list_to_vector)(sexp ctx, sexp ls) { return sexp_list_to_vector(ctx, ls); }
sexp (sexp_symbol_to_string)(sexp ctx, sexp sym) { return sexp_symbol_to_string(ctx, sym); }
sexp (sexp_string_to_symbol)(sexp ctx, sexp str) { return sexp_string_to_symbol(ctx, str); }
sexp (sexp_string_to_number)(sexp ctx, sexp str, sexp b) { return sexp_string_to_number(ctx, str,b ); }

sexp (sexp_define_foreign)(sexp ctx, sexp env, const char *name, int num_args, sexp_proc1 func) { return sexp_define_foreign(ctx, env, name, num_args, func); }
sexp (sexp_define_foreign_opt)(sexp ctx, sexp env, const char *name, int num_args, sexp_proc1 func, sexp dflt) { return sexp_define_foreign_opt(ctx, env, name, num_args, func, dflt); }
sexp (sexp_define_foreign_param)(sexp ctx, sexp env, const char *name, int num_args, sexp_proc1 func, const char *param) { return sexp_define_foreign_param(ctx, env, name, num_args, func, param); }
sexp (sexp_register_simple_type)(sexp ctx, sexp name, sexp parent, sexp slots) { return sexp_register_simple_type(ctx, name, parent, slots); }
sexp (sexp_register_c_type)(sexp ctx, sexp name, sexp finalizer) { return sexp_register_c_type(ctx, name, finalizer); }
