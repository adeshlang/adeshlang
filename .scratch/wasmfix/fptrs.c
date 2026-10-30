int sq(int x) { return x * x; }
int cu(int x) { return x * x * x; }
int (*pick(int n))(int) { return n ? cu : sq; }
__attribute__((export_name("apply"))) int apply(int n, int v) { return pick(n)(v); }
