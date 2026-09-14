ADESH=../../target/debug/adeshlang.exe
CC=gcc
CFLAGS=-Wall -O2
PTHREAD_FLAGS=-pthread

.PHONY: all clean tests math string crypto advanced concurrent

all: tests

tests: math string crypto advanced concurrent

math: math_lib/math_lib.o math_lib/math_lib.h math_lib/test_math.exe
	./math_lib/test_math.exe

math_lib/math_lib.o math_lib/math_lib.h: math_lib/math_lib.adesh
	$(ADESH) compile-aot math_lib/math_lib.adesh math_lib/math_lib.o -c --emit-header math_lib/math_lib.h

math_lib/test_math.exe: math_lib/test_math.c math_lib/math_lib.o math_lib/math_lib.h
	$(CC) $(CFLAGS) math_lib/test_math.c math_lib/math_lib.o -o math_lib/test_math.exe

string: string_utils/string_utils.o string_utils/string_utils.h string_utils/test_string.exe
	./string_utils/test_string.exe

string_utils/string_utils.o string_utils/string_utils.h: string_utils/string_utils.adesh
	$(ADESH) compile-aot string_utils/string_utils.adesh string_utils/string_utils.o -c --emit-header string_utils/string_utils.h

string_utils/test_string.exe: string_utils/test_string.c string_utils/string_utils.o string_utils/string_utils.h
	$(CC) $(CFLAGS) string_utils/test_string.c string_utils/string_utils.o -o string_utils/test_string.exe

crypto: crypto_utils/crypto_utils.o crypto_utils/crypto_utils.h crypto_utils/test_crypto.exe
	./crypto_utils/test_crypto.exe

crypto_utils/crypto_utils.o crypto_utils/crypto_utils.h: crypto_utils/crypto_utils.adesh
	$(ADESH) compile-aot crypto_utils/crypto_utils.adesh crypto_utils/crypto_utils.o -c --emit-header crypto_utils/crypto_utils.h

crypto_utils/test_crypto.exe: crypto_utils/test_crypto.c crypto_utils/crypto_utils.o crypto_utils/crypto_utils.h
	$(CC) $(CFLAGS) crypto_utils/test_crypto.c crypto_utils/crypto_utils.o -o crypto_utils/test_crypto.exe

advanced: advanced_math/advanced_math.o advanced_math/advanced_math.h advanced_math/test_advanced_math.exe
	./advanced_math/test_advanced_math.exe

advanced_math/advanced_math.o advanced_math/advanced_math.h: advanced_math/advanced_math.adesh
	$(ADESH) compile-aot advanced_math/advanced_math.adesh advanced_math/advanced_math.o -c --emit-header advanced_math/advanced_math.h

advanced_math/test_advanced_math.exe: advanced_math/test_advanced_math.c advanced_math/advanced_math.o advanced_math/advanced_math.h
	$(CC) $(CFLAGS) advanced_math/test_advanced_math.c advanced_math/advanced_math.o -o advanced_math/test_advanced_math.exe

concurrent: concurrent_demo/concurrent_demo.o concurrent_demo/concurrent_demo.h concurrent_demo/test_concurrent.exe
	@echo "Note: Thread safety is ensured by Arc<Mutex<FfiRegistry>> in AdeshLang runtime"
	./concurrent_demo/test_concurrent.exe

concurrent_demo/concurrent_demo.o concurrent_demo/concurrent_demo.h: concurrent_demo/concurrent_demo.adesh
	$(ADESH) compile-aot concurrent_demo/concurrent_demo.adesh concurrent_demo/concurrent_demo.o -c --emit-header concurrent_demo/concurrent_demo.h

concurrent_demo/test_concurrent.exe: concurrent_demo/test_concurrent.c concurrent_demo/concurrent_demo.o concurrent_demo/concurrent_demo.h
	$(CC) $(CFLAGS) $(PTHREAD_FLAGS) concurrent_demo/test_concurrent.c concurrent_demo/concurrent_demo.o -o concurrent_demo/test_concurrent.exe

clean:
	rm -f math_lib/*.o math_lib/*.h math_lib/*.exe
	rm -f string_utils/*.o string_utils/*.h string_utils/*.exe
	rm -f crypto_utils/*.o crypto_utils/*.h crypto_utils/*.exe
	rm -f advanced_math/*.o advanced_math/*.h advanced_math/*.exe
	rm -f concurrent_demo/*.o concurrent_demo/*.h concurrent_demo/*.exe
