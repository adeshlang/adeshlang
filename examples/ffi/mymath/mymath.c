// mymath.c - Implementation for @cImport example

#include "mymath.h"
#include <stdio.h>
#include <string.h>
#include <stdlib.h>

int32_t add(int32_t a, int32_t b) {
    return a + b;
}

int32_t subtract(int32_t a, int32_t b) {
    return a - b;
}

int32_t multiply_int(int32_t a, int32_t b) {
    return a * b;
}

float multiply(float x, float y) {
    return x * y;
}

float multiply_float(float x, float y) {
    return x * y;
}

double divide_double(double a, double b) {
    if (b == 0.0) return 0.0;
    return a / b;
}

void print_hello(void) {
    printf("Hello from C library!\n");
}

size_t get_length(const char* str) {
    return strlen(str);
}

int32_t* create_array(int32_t size) {
    return (int32_t*)malloc(size * sizeof(int32_t));
}

void free_array(int32_t* arr) {
    free(arr);
}
