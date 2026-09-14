// Helper functions for Native JIT print builtin
#include <stdio.h>
#include <stdint.h>

// Print an integer (i64) with newline
void print_i64_nl(int64_t value) {
    printf("%lld\n", (long long)value);
    fflush(stdout);
}

// Print an integer (i64) without newline  
void print_i64(int64_t value) {
    printf("%lld", (long long)value);
    fflush(stdout);
}

// Print a float (f64) with newline
void print_f64_nl(double value) {
    printf("%.17g\n", value);
    fflush(stdout);
}

// Print a float (f64) without newline
void print_f64(double value) {
    printf("%.17g", value);
    fflush(stdout);
}

// Print a 32-bit float (f32) with newline
void print_f32_nl(float value) {
    printf("%.7g\n", value);
    fflush(stdout);
}

// Print a 32-bit float (f32) without newline
void print_f32(float value) {
    printf("%.7g", value);
    fflush(stdout);
}

// Print a string with newline (uses puts which adds newline)
void print_str_nl(const char* str) {
    puts(str);
    fflush(stdout);
}

// Print a string without newline
void print_str(const char* str) {
    printf("%s", str);
    fflush(stdout);
}

// Flush stdout
void print_flush() {
    fflush(stdout);
}

// Append a string to a file (path is UTF-8, null-terminated)
void file_write_str(const char* path, const char* str) {
    FILE* f = fopen(path, "ab");
    if (!f) return;
    fputs(str, f);
    fclose(f);
}

// Append an i64 to a file
void file_write_i64(const char* path, int64_t value) {
    FILE* f = fopen(path, "ab");
    if (!f) return;
    fprintf(f, "%lld", (long long)value);
    fclose(f);
}

// Append an f64 to a file
void file_write_f64(const char* path, double value) {
    FILE* f = fopen(path, "ab");
    if (!f) return;
    fprintf(f, "%.17g", value);
    fclose(f);
}
