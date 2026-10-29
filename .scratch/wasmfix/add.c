extern int twice(int x);
__attribute__((export_name("add"))) int add(int a, int b) { return twice(a) + b; }
