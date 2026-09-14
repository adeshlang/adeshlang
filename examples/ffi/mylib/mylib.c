// mylib.c - Comprehensive C library for AdeshLang FFI examples
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
#include <math.h>
#include <time.h>

// ====== Basic Math Operations ======
int32_t add_int(int32_t a, int32_t b) {
    return a + b;
}

int32_t subtract_int(int32_t a, int32_t b) {
    return a - b;
}

int32_t multiply_int(int32_t a, int32_t b) {
    return a * b;
}

int32_t divide_int(int32_t a, int32_t b) {
    if (b == 0) return 0;
    return a / b;
}

float add_float(float a, float b) {
    return a + b;
}

double add_double(double a, double b) {
    return a + b;
}

// ====== Advanced Math ======
double power(double base, double exp) {
    return pow(base, exp);
}

double square_root(double x) {
    return sqrt(x);
}

double sine(double x) {
    return sin(x);
}

double cosine(double x) {
    return cos(x);
}

double tangent(double x) {
    return tan(x);
}

double logarithm(double x) {
    return log(x);
}

int32_t factorial(int32_t n) {
    if (n <= 1) return 1;
    return n * factorial(n - 1);
}

// ====== String Operations ======
size_t string_length(const char* str) {
    return strlen(str);
}

// Convert string to uppercase (returns new string - caller must free)
char* string_to_upper(const char* str) {
    size_t len = strlen(str);
    char* result = (char*)malloc(len + 1);
    if (!result) return NULL;
    
    for (size_t i = 0; i < len; i++) {
        if (str[i] >= 'a' && str[i] <= 'z') {
            result[i] = str[i] - 32;
        } else {
            result[i] = str[i];
        }
    }
    result[len] = '\0';
    return result;
}

// Convert string to lowercase (returns new string - caller must free)
char* string_to_lower(const char* str) {
    size_t len = strlen(str);
    char* result = (char*)malloc(len + 1);
    if (!result) return NULL;
    
    for (size_t i = 0; i < len; i++) {
        if (str[i] >= 'A' && str[i] <= 'Z') {
            result[i] = str[i] + 32;
        } else {
            result[i] = str[i];
        }
    }
    result[len] = '\0';
    return result;
}

int32_t string_compare(const char* s1, const char* s2) {
    return strcmp(s1, s2);
}

void string_reverse(char* str) {
    int len = strlen(str);
    for (int i = 0; i < len / 2; i++) {
        char temp = str[i];
        str[i] = str[len - 1 - i];
        str[len - 1 - i] = temp;
    }
}

// ====== Array Operations ======
int32_t array_sum(int32_t* arr, int32_t size) {
    int32_t sum = 0;
    for (int32_t i = 0; i < size; i++) {
        sum += arr[i];
    }
    return sum;
}

int32_t array_max(int32_t* arr, int32_t size) {
    if (size <= 0) return 0;
    int32_t max = arr[0];
    for (int32_t i = 1; i < size; i++) {
        if (arr[i] > max) max = arr[i];
    }
    return max;
}

int32_t array_min(int32_t* arr, int32_t size) {
    if (size <= 0) return 0;
    int32_t min = arr[0];
    for (int32_t i = 1; i < size; i++) {
        if (arr[i] < min) min = arr[i];
    }
    return min;
}

double array_average(int32_t* arr, int32_t size) {
    if (size <= 0) return 0.0;
    int32_t sum = array_sum(arr, size);
    return (double)sum / size;
}

void array_sort(int32_t* arr, int32_t size) {
    // Simple bubble sort
    for (int32_t i = 0; i < size - 1; i++) {
        for (int32_t j = 0; j < size - i - 1; j++) {
            if (arr[j] > arr[j + 1]) {
                int32_t temp = arr[j];
                arr[j] = arr[j + 1];
                arr[j + 1] = temp;
            }
        }
    }
}

// ====== Memory Operations ======
void* allocate_memory(size_t size) {
    return malloc(size);
}

void free_memory(void* ptr) {
    free(ptr);
}

void memory_copy(void* dest, const void* src, size_t n) {
    memcpy(dest, src, n);
}

void memory_set(void* ptr, int value, size_t n) {
    memset(ptr, value, n);
}

// ====== Random Numbers ======
void random_seed(uint32_t seed) {
    srand(seed);
}

int32_t random_int(int32_t min, int32_t max) {
    if (max <= min) return min;
    return min + (rand() % (max - min + 1));
}

double random_double(void) {
    return (double)rand() / RAND_MAX;
}

// ====== Time Operations ======
int64_t get_timestamp(void) {
    return (int64_t)time(NULL);
}

void print_timestamp(void) {
    time_t now = time(NULL);
    printf("Current timestamp: %ld\n", (long)now);
}

// ====== Printing Functions ======
void print_string(const char* str) {
    printf("%s", str);
}

void print_int(int32_t num) {
    printf("%d", num);
}

void print_float(float num) {
    printf("%f", num);
}

void print_double(double num) {
    printf("%f", num);
}

void print_newline(void) {
    printf("\n");
}

void print_message(const char* msg) {
    printf("[C Library] %s\n", msg);
}

// ====== Type Conversion ======
int32_t float_to_int(float f) {
    return (int32_t)f;
}

float int_to_float(int32_t i) {
    return (float)i;
}

double int_to_double(int32_t i) {
    return (double)i;
}

// ====== Boolean Operations ======
int32_t is_even(int32_t n) {
    return (n % 2) == 0;
}

int32_t is_odd(int32_t n) {
    return (n % 2) != 0;
}

int32_t is_prime(int32_t n) {
    if (n <= 1) return 0;
    if (n <= 3) return 1;
    if (n % 2 == 0 || n % 3 == 0) return 0;
    
    for (int32_t i = 5; i * i <= n; i += 6) {
        if (n % i == 0 || n % (i + 2) == 0)
            return 0;
    }
    return 1;
}

// ====== Struct Example ======
typedef struct {
    int32_t x;
    int32_t y;
} Point;

Point* create_point(int32_t x, int32_t y) {
    Point* p = (Point*)malloc(sizeof(Point));
    if (p) {
        p->x = x;
        p->y = y;
    }
    return p;
}

void free_point(Point* p) {
    free(p);
}

int32_t point_get_x(Point* p) {
    return p ? p->x : 0;
}

int32_t point_get_y(Point* p) {
    return p ? p->y : 0;
}

double point_distance(Point* p1, Point* p2) {
    if (!p1 || !p2) return 0.0;
    int32_t dx = p2->x - p1->x;
    int32_t dy = p2->y - p1->y;
    return sqrt(dx * dx + dy * dy);
}


// gcc -shared -o mylib.dll mylib.c