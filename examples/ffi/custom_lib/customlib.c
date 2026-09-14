// Minimal custom library for the custom_lib demo:
// implements add_numbers and print_message, as declared in custom_lib.adesh.
#include <stdio.h>
#include <stdint.h>

int32_t add_numbers(int32_t a, int32_t b) {
    return a + b;
}

void print_message(const char* msg) {
    printf("%s", msg);
    fflush(stdout);
}
