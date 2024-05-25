#ifndef WRAPPER_H
#include <chibi/eval.h>

#include <stdbool.h>

/*
Chibi-Scheme uses a lot of macros, which bindgen cannot generate bindings for.
This wrapper is used to provide actual functions for a selection of the macro-based API.
*/

bool (sexp_booleanp)(sexp obj);
bool (sexp_fixnump)(sexp obj);
bool (sexp_flonump)(sexp obj);
bool (sexp_bignump)(sexp obj);
bool (sexp_integerp)(sexp obj);
bool (sexp_numberp)(sexp obj);
bool (sexp_charp)(sexp obj);
bool (sexp_stringp)(sexp obj);
bool (sexp_string_cursorp)(sexp obj);
bool (sexp_bytesp)(sexp obj);
bool (sexp_symbolp)(sexp obj);
bool (sexp_nullp)(sexp obj);
bool (sexp_pairp)(sexp obj);
bool (sexp_vectorp)(sexp obj);
bool (sexp_iportp)(sexp obj);
bool (sexp_oportp)(sexp obj);
bool (sexp_portp)(sexp obj);
bool (sexp_procedurep)(sexp obj);
bool (sexp_opcodep)(sexp obj);
bool (sexp_applicablep)(sexp obj);
bool (sexp_typep)(sexp obj);
bool (sexp_exceptionp)(sexp obj);
bool (sexp_contextp)(sexp obj);
bool (sexp_envp)(sexp obj);
bool (sexp_corep)(sexp obj);
bool (sexp_macrop)(sexp obj);
bool (sexp_synclop)(sexp obj);
bool (sexp_bytecodep)(sexp obj);
bool (sexp_cpointerp)(sexp obj);

char *(sexp_string_data)(sexp x);
sexp_uint_t (sexp_string_size)(sexp x);
sexp_uint_t (sexp_string_length)(sexp x);

sexp (sexp_string_ref)(sexp ctx, sexp s, sexp i);
sexp (sexp_string_set)(sexp ctx, sexp s, sexp i, sexp ch);
sexp (sexp_string_cursor_ref)(sexp ctx, sexp s, sexp i);
sexp (sexp_string_cursor_set)(sexp ctx, sexp s, sexp i, sexp ch);
sexp (sexp_string_cursor_next)(sexp s, sexp i);
sexp (sexp_string_cursor_prev)(sexp s, sexp i);
sexp (sexp_substring)(sexp ctx, sexp s, sexp i, sexp j);
sexp (sexp_substring_cursor)(sexp ctx, sexp s, sexp i, sexp j);

sexp (sexp_make_boolean)(bool n);
bool (sexp_unbox_boolean)(sexp obj);
sexp (sexp_make_fixnum)(sexp_sint_t n);
sexp_sint_t (sexp_unbox_fixnum)(sexp obj);
sexp (sexp_make_character)(char n);
char (sexp_unbox_character)(sexp obj);
sexp (sexp_make_string_cursor)(int n);
int (sexp_unbox_string_cursor)(sexp obj);
sexp (sexp_car)(sexp pair);
sexp (sexp_cdr)(sexp pair);
sexp (sexp_ratio_numerator)(sexp q);
sexp (sexp_ratio_denominator)(sexp q);
sexp (sexp_complex_real)(sexp z);
sexp (sexp_complex_imag)(sexp z);
sexp_uint_t (sexp_bytes_length)(sexp bv);
char *(sexp_bytes_data)(sexp bv);
sexp (sexp_bytes_ref)(sexp bv, sexp i);
sexp (sexp_bytes_set)(sexp bv, sexp i, sexp obj);
sexp_uint_t (sexp_vector_length)(sexp vec);
sexp (sexp_vector_ref)(sexp vec, sexp i);
sexp (sexp_vector_set)(sexp vec, sexp i, sexp obj);


sexp (sexp_cons)(sexp ctx, sexp obj1, sexp obj2);
sexp (sexp_list1)(sexp ctx, sexp obj);
sexp (sexp_make_string)(sexp ctx, sexp len, sexp ch);
sexp (sexp_make_bytes)(sexp ctx, sexp len, sexp i);
sexp (sexp_make_vector)(sexp ctx, sexp len, sexp obj);

sexp (sexp_read)(sexp ctx, sexp in);
sexp (sexp_write)(sexp ctx, sexp out, sexp obj);
int (sexp_write_string)(sexp ctx, const char *str, sexp out);
int (sexp_newline)(sexp ctx, sexp out);
sexp (sexp_print_exception)(sexp ctx, sexp exn, sexp out);
sexp (sexp_current_input_port)(sexp ctx);
sexp (sexp_current_output_port)(sexp ctx);
sexp (sexp_current_error_port)(sexp ctx);
int (sexp_debug)(sexp ctx, char* msg, sexp obj);
sexp (sexp_open_input_string)(sexp ctx, sexp str);
sexp (sexp_open_output_string)(sexp ctx);
sexp (sexp_get_output_string)(sexp ctx, sexp port);

sexp (sexp_equalp)(sexp ctx, sexp x, sexp y);
sexp (sexp_length)(sexp ctx, sexp ls);
sexp (sexp_listp)(sexp ctx, sexp x);
sexp (sexp_memq)(sexp ctx, sexp x, sexp ls);
sexp (sexp_assq)(sexp ctx, sexp x, sexp ls);
sexp (sexp_reverse)(sexp ctx, sexp ls);
sexp (sexp_nreverse)(sexp ctx, sexp ls);
sexp (sexp_append2)(sexp ctx, sexp a, sexp b);
sexp (sexp_copy_list)(sexp ctx, sexp ls);
sexp (sexp_list_to_vector)(sexp ctx, sexp ls);
sexp (sexp_symbol_to_string)(sexp ctx, sexp sym);
sexp (sexp_string_to_symbol)(sexp ctx, sexp str);
sexp (sexp_string_to_number)(sexp ctx, sexp str, sexp b);

sexp (sexp_define_foreign)(sexp ctx, sexp env, const char *name, int num_args, sexp_proc1 func);
sexp (sexp_define_foreign_opt)(sexp ctx, sexp env, const char *name, int num_args, sexp_proc1 func, sexp dflt);
sexp (sexp_define_foreign_param)(sexp ctx, sexp env, const char *name, int num_args, sexp_proc1 func, const char *param);
sexp (sexp_register_simple_type)(sexp ctx, sexp name, sexp parent, sexp slots);
sexp (sexp_register_c_type)(sexp ctx, sexp name, sexp finalizer);
#endif
