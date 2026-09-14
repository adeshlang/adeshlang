#ifndef ADESH_EXPORTS_H
#define ADESH_EXPORTS_H

#include <stdint.h>
#include <stdbool.h>
#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Exported Functions */

int64_t add(int64_t a, int64_t b);

int64_t subtract(int64_t a, int64_t b);

int64_t multiply(int64_t a, int64_t b);

int64_t divide(int64_t a, int64_t b);

int64_t power(int64_t base, int64_t exp);

int64_t abs_int(int64_t x);

int64_t max(int64_t a, int64_t b);

int64_t min(int64_t a, int64_t b);

int64_t factorial(int64_t n);

double add_floats(double a, double b);

double multiply_floats(double a, double b);

#ifdef __cplusplus
}
#endif

#endif /* ADESH_EXPORTS_H */
