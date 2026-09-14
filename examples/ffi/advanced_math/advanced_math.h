#ifndef ADESH_EXPORTS_H
#define ADESH_EXPORTS_H

#include <stdint.h>
#include <stdbool.h>
#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Exported Functions */

int64_t gcd(int64_t a, int64_t b);

int64_t lcm(int64_t a, int64_t b);

int64_t is_prime(int64_t n);

int64_t nth_fibonacci(int64_t n);

int64_t sum_range(int64_t start, int64_t end);

int64_t modular_pow(int64_t base, int64_t exp, int64_t modulo);

double sqrt_approx(double x);

double circle_area(double radius);

double circle_circumference(double radius);

#ifdef __cplusplus
}
#endif

#endif /* ADESH_EXPORTS_H */
