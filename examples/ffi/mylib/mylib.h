// mylib.h - Header file for comprehensive C library
#ifndef MYLIB_H
#define MYLIB_H

#include <stdint.h>
#include <stddef.h>

// ====== Basic Math Operations ======
int32_t add_int(int32_t a, int32_t b);
int32_t subtract_int(int32_t a, int32_t b);
int32_t multiply_int(int32_t a, int32_t b);
int32_t divide_int(int32_t a, int32_t b);
float add_float(float a, float b);
double add_double(double a, double b);

// ====== Advanced Math ======
double power(double base, double exp);
double square_root(double x);
double sine(double x);
double cosine(double x);
double tangent(double x);
double logarithm(double x);
int32_t factorial(int32_t n);

// ====== String Operations ======
size_t string_length(const char* str);
char* string_to_upper(const char* str);
char* string_to_lower(const char* str);
int32_t string_compare(const char* s1, const char* s2);
void string_reverse(char* str);

// ====== Array Operations ======
int32_t array_sum(int32_t* arr, int32_t size);
int32_t array_max(int32_t* arr, int32_t size);
int32_t array_min(int32_t* arr, int32_t size);
double array_average(int32_t* arr, int32_t size);
void array_sort(int32_t* arr, int32_t size);

// ====== Memory Operations ======
void* allocate_memory(size_t size);
void free_memory(void* ptr);
void memory_copy(void* dest, const void* src, size_t n);
void memory_set(void* ptr, int value, size_t n);

// ====== Random Numbers ======
void random_seed(uint32_t seed);
int32_t random_int(int32_t min, int32_t max);
double random_double(void);

// ====== Time Operations ======
int64_t get_timestamp(void);
void print_timestamp(void);

// ====== Printing Functions ======
void print_string(const char* str);
void print_int(int32_t num);
void print_float(float num);
void print_double(double num);
void print_newline(void);
void print_message(const char* msg);

// ====== Type Conversion ======
int32_t float_to_int(float f);
float int_to_float(int32_t i);
double int_to_double(int32_t i);

// ====== Boolean Operations ======
int32_t is_even(int32_t n);
int32_t is_odd(int32_t n);
int32_t is_prime(int32_t n);

// ====== Struct Example ======
typedef struct Point Point;

Point* create_point(int32_t x, int32_t y);
void free_point(Point* p);
int32_t point_get_x(Point* p);
int32_t point_get_y(Point* p);
double point_distance(Point* p1, Point* p2);

#endif // MYLIB_H
