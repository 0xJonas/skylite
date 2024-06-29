#ifndef WRAPPER_H

#include <libguile.h>
#include <stdbool.h>

SCM scm_car_wrapper(SCM obj);
SCM scm_cdr_wrapper(SCM obj);

bool scm_is_true_wrapper(SCM obj);
bool scm_is_false_wrapper(SCM obj);

bool (scm_is_null)(SCM obj);
bool (scm_is_symbol)(SCM obj);

SCM (scm_from_bool)(bool obj);

void wrapper_free(void *ptr);

#endif
