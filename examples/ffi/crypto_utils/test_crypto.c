#include <stdio.h>
#include <stdint.h>
#include "crypto_utils.h"

int main() {
    printf("=== Crypto Utilities Tests ===\n");
    printf("xor_values(15, 7) = %ld\n", xor_values(15, 7));
    printf("count_set_bits(15) = %ld\n", count_set_bits(15));
    printf("fibonacci(10) = %ld\n", fibonacci(10));
    printf("gcd(48, 18) = %ld\n\n", gcd(48, 18));
    return 0;
}
