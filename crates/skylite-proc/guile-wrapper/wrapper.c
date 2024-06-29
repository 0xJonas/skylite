#include "wrapper.h"

#include <stdlib.h>

SCM scm_car_wrapper(SCM obj) {
    return scm_car(obj);
};

SCM scm_cdr_wrapper(SCM obj) {
    return scm_cdr(obj);
}

bool scm_is_true_wrapper(SCM obj) {
    return scm_is_true(obj);
}

bool scm_is_false_wrapper(SCM obj) {
    return scm_is_false(obj);
}

bool (scm_is_null)(SCM obj) {
    return scm_is_null(obj);
}

bool (scm_is_symbol)(SCM obj) {
    return scm_is_symbol(obj);
}

SCM (scm_from_bool)(bool obj) {
    return scm_from_bool(obj);
}

void wrapper_free(void *ptr) {
    free(ptr);
}
