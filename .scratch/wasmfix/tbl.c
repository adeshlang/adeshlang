#include <stdint.h>
static int sq(int x){return x*x;}
static int cu(int x){return x*x*x;}
int run(int(*f)(int),int n){return f(n);}
int table(){ int (*t[2])(int)={sq,cu}; return t[1](5); }
