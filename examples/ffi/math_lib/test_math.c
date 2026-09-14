#include <stdio.h>
#include <stdint.h>
#include "math_lib.h"

int main() {
    printf("=== Math Library Tests ===\n");
    printf("add(10, 20) = %ld\n", add(10, 20));
    printf("multiply(7, 6) = %ld\n", multiply(7, 6));
    printf("power(2, 10) = %ld\n", power(2, 10));
    printf("factorial(5) = %ld\n", factorial(5));
    printf("add_floats(3.14, 2.86) = %.2f\n", add_floats(3.14, 2.86));
    printf("multiply_floats(2.5, 4.0) = %.2f\n\n", multiply_floats(2.5, 4.0));
    return 0;
}
