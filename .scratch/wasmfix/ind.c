int sq(int x){return x*x;}
int main(){ int (*f)(int)=sq; return f(7); }
