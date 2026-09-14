// mymath.h - Sample C header for @cImport example

#ifndef MYMATH_H
#define MYMATH_H

#include <stdint.h>

// Basic arithmetic
int32_t add(int32_t a, int32_t b);
int32_t subtract(int32_t a, int32_t b);
int32_t multiply_int(int32_t a, int32_t b);

// Floating point
float multiply(float x, float y);
float multiply_float(float x, float y);
double divide_double(double a, double b);

// String functions
void print_hello(void);
size_t get_length(const char* str);

// Pointer examples
int32_t* create_array(int32_t size);
void free_array(int32_t* arr);

#endif // MYMATH_H
