#ifndef ADESH_EXPORTS_H
#define ADESH_EXPORTS_H

#include <stdint.h>
#include <stdbool.h>
#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Exported Functions */

int64_t char_to_lower(int64_t ch);

int64_t sum_of_digits(int64_t n);

int64_t is_even(int64_t n);

int64_t is_odd(int64_t n);

#ifdef __cplusplus
}
#endif

#endif /* ADESH_EXPORTS_H */
