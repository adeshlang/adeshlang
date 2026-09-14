#include "advanced_math.h"
#include <stdio.h>
#include <assert.h>

int main() {
    printf("=== Advanced Math Library Tests ===\n\n");
    
    // GCD tests
    printf("GCD Tests:\n");
    printf("  gcd(48, 18) = %ld\n", gcd(48, 18));
    assert(gcd(48, 18) == 6);
    printf("  gcd(100, 50) = %ld\n", gcd(100, 50));
    assert(gcd(100, 50) == 50);
    
    // LCM tests
    printf("\nLCM Tests:\n");
    printf("  lcm(12, 18) = %ld\n", lcm(12, 18));
    assert(lcm(12, 18) == 36);
    printf("  lcm(21, 6) = %ld\n", lcm(21, 6));
    assert(lcm(21, 6) == 42);
    
    // Prime tests
    printf("\nPrime Tests:\n");
    printf("  is_prime(17) = %ld (1=prime)\n", is_prime(17));
    assert(is_prime(17) == 1);
    printf("  is_prime(20) = %ld (0=composite)\n", is_prime(20));
    assert(is_prime(20) == 0);
    printf("  is_prime(97) = %ld (1=prime)\n", is_prime(97));
    assert(is_prime(97) == 1);
    
    // Fibonacci tests
    printf("\nFibonacci Tests:\n");
    printf("  nth_fibonacci(10) = %ld\n", nth_fibonacci(10));
    assert(nth_fibonacci(10) == 55);
    printf("  nth_fibonacci(15) = %ld\n", nth_fibonacci(15));
    assert(nth_fibonacci(15) == 610);
    
    // Sum range tests
    printf("\nSum Range Tests:\n");
    printf("  sum_range(1, 10) = %ld\n", sum_range(1, 10));
    assert(sum_range(1, 10) == 55);
    printf("  sum_range(1, 100) = %ld\n", sum_range(1, 100));
    assert(sum_range(1, 100) == 5050);
    
    // Modular exponentiation
    printf("\nModular Exponentiation:\n");
    printf("  modular_pow(2, 10, 1000) = %ld\n", modular_pow(2, 10, 1000));
    assert(modular_pow(2, 10, 1000) == 24);
    printf("  modular_pow(3, 5, 13) = %ld\n", modular_pow(3, 5, 13));
    assert(modular_pow(3, 5, 13) == 9);
    
    // Floating point tests
    printf("\nFloating Point Tests:\n");
    printf("  sqrt_approx(16.0) = %.6f\n", sqrt_approx(16.0));
    printf("  sqrt_approx(2.0) = %.6f\n", sqrt_approx(2.0));
    printf("  circle_area(5.0) = %.6f\n", circle_area(5.0));
    printf("  circle_circumference(5.0) = %.6f\n", circle_circumference(5.0));
    
    printf("\n✅ All tests passed!\n");
    return 0;
}
