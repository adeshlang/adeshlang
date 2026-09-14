#include <stdio.h>
#include <stdint.h>
#include "string_utils.h"

int main() {
    printf("=== String Utilities Tests ===\n");
    printf("char_to_lower('A') = %c\n", (char)char_to_lower('A'));
    printf("sum_of_digits(12345) = %ld\n", sum_of_digits(12345));
    printf("is_even(10) = %ld\n", is_even(10));
    printf("is_odd(7) = %ld\n\n", is_odd(7));
    return 0;
}
