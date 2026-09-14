#ifndef ADESH_EXPORTS_H
#define ADESH_EXPORTS_H

#include <stdint.h>
#include <stdbool.h>
#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Exported Functions */

int64_t xor_values(int64_t a, int64_t b);

int64_t and_values(int64_t a, int64_t b);

int64_t or_values(int64_t a, int64_t b);

int64_t left_shift(int64_t value, int64_t bits);

int64_t right_shift(int64_t value, int64_t bits);

int64_t count_set_bits(int64_t n);

int64_t fibonacci(int64_t n);

int64_t gcd(int64_t a, int64_t b);

#ifdef __cplusplus
}
#endif

#endif /* ADESH_EXPORTS_H */
