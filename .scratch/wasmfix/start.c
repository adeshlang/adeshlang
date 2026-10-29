extern int greet(void);
__attribute__((export_name("_start"))) void _start(void) { (void)greet(); }
