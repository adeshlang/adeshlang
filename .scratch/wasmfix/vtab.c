int sq(int x) { return x * x; }
int cu(int x) { return x * x * x; }
static int (*const t[2])(int) = {sq, cu};
__attribute__((export_name("call"))) int call(int i, int v) { return t[i](v); }
