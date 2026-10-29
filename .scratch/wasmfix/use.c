extern const char tag[];
extern int counter;
__attribute__((export_name("peek"))) int peek(int i) { return tag[i]; }
__attribute__((export_name("bump"))) int bump(void) { return ++counter; }
