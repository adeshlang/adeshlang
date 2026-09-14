#ifndef ADESH_EXPORTS_H
#define ADESH_EXPORTS_H

#include <stdint.h>
#include <stdbool.h>
#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Exported Functions */

int64_t hash_simple(int64_t value);

int64_t counter_increment(int64_t current, int64_t step);

int64_t compute_intensive(int64_t n);

int64_t process_batch(int64_t start, int64_t count);

int64_t validate_range(int64_t value, int64_t min, int64_t max);

#ifdef __cplusplus
}
#endif

#endif /* ADESH_EXPORTS_H */
