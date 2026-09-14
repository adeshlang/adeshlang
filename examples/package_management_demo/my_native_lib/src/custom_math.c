#include "custom_math.h"

#include <math.h>

double custom_multiply(double a, double b) {
    return a * b;
}

int custom_factorial(int n) {
    if (n <= 1) {
        return 1;
    }
    return n * custom_factorial(n - 1);
}

double custom_hypotenuse(double a, double b) {
    return sqrt(a * a + b * b);
}
