#include <stddef.h>
#include <stdint.h>

#if defined(_WIN32)
#include <windows.h>

static size_t c_string_length(const char *text) {
    size_t length = 0;
    if (text != NULL) {
        while (text[length] != '\0') {
            ++length;
        }
    }
    return length;
}

static int next_utf8(
    const unsigned char *bytes,
    size_t length,
    size_t *cursor,
    uint32_t *codepoint
) {
    const size_t index = *cursor;
    if (index >= length) {
        return 0;
    }

    const unsigned char first = bytes[index];
    if (first < 0x80) {
        *codepoint = first;
        *cursor = index + 1;
        return 1;
    }

    size_t width;
    uint32_t value;
    if (first >= 0xC2 && first <= 0xDF) {
        width = 2;
        value = first & 0x1F;
    } else if (first >= 0xE0 && first <= 0xEF) {
        width = 3;
        value = first & 0x0F;
    } else if (first >= 0xF0 && first <= 0xF4) {
        width = 4;
        value = first & 0x07;
    } else {
        return 0;
    }

    if (length - index < width) {
        return 0;
    }
    for (size_t offset = 1; offset < width; ++offset) {
        const unsigned char continuation = bytes[index + offset];
        if ((continuation & 0xC0) != 0x80) {
            return 0;
        }
        value = (value << 6) | (continuation & 0x3F);
    }

    if ((width == 3 && value < 0x800) ||
        (width == 4 && value < 0x10000) ||
        (value >= 0xD800 && value <= 0xDFFF) ||
        value > 0x10FFFF) {
        return 0;
    }

    *codepoint = value;
    *cursor = index + width;
    return 1;
}

static int valid_utf8(const unsigned char *bytes, size_t length) {
    size_t cursor = 0;
    uint32_t codepoint;
    while (cursor < length) {
        if (!next_utf8(bytes, length, &cursor, &codepoint)) {
            return 0;
        }
    }
    return 1;
}

static int write_file_all(HANDLE handle, const unsigned char *bytes, size_t length) {
    while (length > 0) {
        const DWORD chunk = length > 0xFFFFFFFFu ? 0xFFFFFFFFu : (DWORD)length;
        DWORD written = 0;
        if (!WriteFile(handle, bytes, chunk, &written, NULL) || written == 0) {
            return 0;
        }
        bytes += written;
        length -= written;
    }
    return 1;
}

static int write_console_utf8(HANDLE handle, const unsigned char *bytes, size_t length) {
    size_t cursor = 0;
    while (cursor < length) {
        WCHAR units[256];
        size_t count = 0;
        while (cursor < length && count < 256) {
            uint32_t codepoint;
            const size_t codepoint_start = cursor;
            if (!next_utf8(bytes, length, &cursor, &codepoint)) {
                return 0;
            }
            if (codepoint <= 0xFFFF) {
                units[count++] = (WCHAR)codepoint;
            } else {
                if (count == 255) {
                    // Retry this codepoint in the next chunk so its UTF-16
                    // surrogate pair is never split.
                    cursor = codepoint_start;
                    break;
                }
                codepoint -= 0x10000;
                units[count++] = (WCHAR)(0xD800 + (codepoint >> 10));
                units[count++] = (WCHAR)(0xDC00 + (codepoint & 0x3FF));
            }
        }

        DWORD written = 0;
        if (count == 0 ||
            !WriteConsoleW(handle, units, (DWORD)count, &written, NULL) ||
            written != count) {
            return 0;
        }
    }
    return 1;
}

uint64_t adesh_native_print_cstr(const char *text, int64_t newline) {
    const size_t length = c_string_length(text);
    const unsigned char *bytes = (const unsigned char *)text;
    HANDLE handle = GetStdHandle(STD_OUTPUT_HANDLE);
    DWORD mode = 0;

    if (handle != NULL && handle != INVALID_HANDLE_VALUE &&
        GetConsoleMode(handle, &mode)) {
        if (valid_utf8(bytes, length)) {
            write_console_utf8(handle, bytes, length);
        } else {
            // Invalid UTF-8 retains the raw-byte behavior used for redirected
            // output rather than producing a partial UTF-16 conversion.
            write_file_all(handle, bytes, length);
        }
        if (newline != 0) {
            const WCHAR line_feed = L'\n';
            DWORD written = 0;
            WriteConsoleW(handle, &line_feed, 1, &written, NULL);
        }
    } else if (handle != NULL && handle != INVALID_HANDLE_VALUE) {
        write_file_all(handle, bytes, length);
        if (newline != 0) {
            static const unsigned char line_feed = '\n';
            write_file_all(handle, &line_feed, 1);
        }
    }
    return 0;
}

__declspec(noreturn) void adesh_native_abort_str(const char *message) {
    adesh_native_print_cstr(message, 1);
    ExitProcess(101);
}

#else
#if defined(__i386__) && defined(__linux__)
#include <unistd.h>

static ssize_t i686_write(int fd, const void *bytes, size_t length) {
    long result;
    __asm__ volatile(
        "int $0x80"
        : "=a"(result)
        : "0"(4), "b"(fd), "c"(bytes), "d"(length)
        : "memory"
    );
    return (ssize_t)result;
}

__attribute__((noreturn)) static void i686_exit(int status) {
    __asm__ volatile(
        "int $0x80"
        :
        : "a"(1), "b"(status)
        : "memory"
    );
    __builtin_unreachable();
}
#else
#include <unistd.h>
#endif

static size_t c_string_length(const char *text) {
    size_t length = 0;
    if (text != NULL) {
        const volatile char *cursor = text;
        while (*cursor != '\0') {
            ++length;
            ++cursor;
        }
    }
    return length;
}

static void write_all(const char *bytes, size_t length) {
    while (length > 0) {
#if defined(__i386__) && defined(__linux__)
        const ssize_t written = i686_write(STDOUT_FILENO, bytes, length);
#else
        const ssize_t written = write(STDOUT_FILENO, bytes, length);
#endif
        if (written <= 0) {
            return;
        }
        bytes += written;
        length -= (size_t)written;
    }
}

uint64_t adesh_native_print_cstr(const char *text, int64_t newline) {
    write_all(text, c_string_length(text));
    if (newline != 0) {
        // Materialize the LF on the stack. A RIP-relative load of a `"\n"`
        // cstring depends on X86_64_RELOC_SIGNED resolving exactly onto that
        // byte; an off-by-one into the following NUL produces the Phase 13
        // x86 Mach-O symptom `…source\0` instead of `…source\n` while
        // aarch64 (ADRP/ADD) stays green. A stack temporary needs no reloc.
        const char line_feed = '\n';
        write_all(&line_feed, 1);
    }
    return 0;
}

__attribute__((noreturn)) void adesh_native_abort_str(const char *message) {
    adesh_native_print_cstr(message, 1);
#if defined(__i386__) && defined(__linux__)
    i686_exit(101);
#else
    _exit(101);
#endif
}
#endif

static uint64_t print_unsigned_decimal(uint64_t value, int negative, int64_t newline) {
    char buffer[22];
    size_t cursor = sizeof(buffer);
    buffer[--cursor] = '\0';
    do {
        buffer[--cursor] = (char)('0' + (value % 10));
        value /= 10;
    } while (value != 0);
    if (negative) {
        buffer[--cursor] = '-';
    }
    return adesh_native_print_cstr(buffer + cursor, newline);
}

uint64_t aot_print_cstr(const char *text, int64_t newline) {
    return adesh_native_print_cstr(text, newline);
}

uint64_t aot_print_i64(int64_t value, int64_t newline) {
    const int negative = value < 0;
    const uint64_t magnitude = negative
        ? (uint64_t)(-(value + 1)) + 1
        : (uint64_t)value;
    return print_unsigned_decimal(magnitude, negative, newline);
}

uint64_t aot_print_u64(uint64_t value, int64_t newline) {
    return print_unsigned_decimal(value, 0, newline);
}

uint64_t aot_print_i32_narrow(int32_t value, int32_t newline) {
    return aot_print_i64((int64_t)value, (int64_t)newline);
}

uint64_t aot_print_bool(int64_t value, int64_t newline) {
    return adesh_native_print_cstr(value != 0 ? "true" : "false", newline);
}

#define ADESH_BIG_LIMBS 40

typedef struct {
    uint32_t used;
    uint32_t limb[ADESH_BIG_LIMBS];
} adesh_big;

static void big_from_u64(adesh_big *a, uint32_t lo, uint32_t hi) {
    a->used = 0;
    if (lo != 0 || hi != 0) {
        a->limb[a->used++] = lo;
        if (hi != 0) {
            a->limb[a->used++] = hi;
        }
    }
}

static void big_assign(adesh_big *dst, const adesh_big *src) {
    dst->used = src->used;
    for (uint32_t i = 0; i < src->used; ++i) {
        dst->limb[i] = src->limb[i];
    }
}

static void big_mul_small(adesh_big *a, uint32_t factor) {
    uint32_t carry = 0;
    for (uint32_t i = 0; i < a->used; ++i) {
        const uint64_t product = (uint64_t)a->limb[i] * factor + carry;
        a->limb[i] = (uint32_t)product;
        carry = (uint32_t)(product >> 32);
    }
    if (carry != 0 && a->used < ADESH_BIG_LIMBS) {
        a->limb[a->used++] = carry;
    }
}

static void big_shl(adesh_big *a, uint32_t bits) {
    if (a->used == 0) {
        return;
    }
    const uint32_t limbs = bits / 32;
    const uint32_t shift = bits % 32;
    if (limbs != 0) {
        for (uint32_t i = a->used; i > 0; --i) {
            if (i - 1 + limbs < ADESH_BIG_LIMBS) {
                a->limb[i - 1 + limbs] = a->limb[i - 1];
            }
        }
        for (uint32_t i = 0; i < limbs && i < ADESH_BIG_LIMBS; ++i) {
            a->limb[i] = 0;
        }
        a->used += limbs;
        if (a->used > ADESH_BIG_LIMBS) {
            a->used = ADESH_BIG_LIMBS;
        }
    }
    if (shift != 0) {
        uint32_t carry = 0;
        for (uint32_t i = limbs; i < a->used; ++i) {
            const uint32_t next = a->limb[i] >> (32 - shift);
            a->limb[i] = (a->limb[i] << shift) | carry;
            carry = next;
        }
        if (carry != 0 && a->used < ADESH_BIG_LIMBS) {
            a->limb[a->used++] = carry;
        }
    }
}

static int big_cmp(const adesh_big *a, const adesh_big *b) {
    if (a->used != b->used) {
        return a->used > b->used ? 1 : -1;
    }
    for (uint32_t i = a->used; i > 0; --i) {
        if (a->limb[i - 1] != b->limb[i - 1]) {
            return a->limb[i - 1] > b->limb[i - 1] ? 1 : -1;
        }
    }
    return 0;
}

static void big_add(adesh_big *a, const adesh_big *b) {
    uint32_t carry = 0;
    const uint32_t limit = a->used > b->used ? a->used : b->used;
    for (uint32_t i = 0; i < limit; ++i) {
        const uint64_t sum = (uint64_t)(i < a->used ? a->limb[i] : 0)
            + (i < b->used ? b->limb[i] : 0) + carry;
        a->limb[i] = (uint32_t)sum;
        carry = (uint32_t)(sum >> 32);
    }
    a->used = limit;
    if (carry != 0 && a->used < ADESH_BIG_LIMBS) {
        a->limb[a->used++] = carry;
    }
}

static void big_sub(adesh_big *a, const adesh_big *b) {
    uint32_t borrow = 0;
    for (uint32_t i = 0; i < a->used; ++i) {
        const uint64_t lhs = a->limb[i];
        const uint64_t rhs = (uint64_t)(i < b->used ? b->limb[i] : 0) + borrow;
        if (lhs >= rhs) {
            a->limb[i] = (uint32_t)(lhs - rhs);
            borrow = 0;
        } else {
            a->limb[i] = (uint32_t)((lhs + 0x100000000ull) - rhs);
            borrow = 1;
        }
    }
    while (a->used > 0 && a->limb[a->used - 1] == 0) {
        --a->used;
    }
}

static void big_mul_pow10(adesh_big *a, int32_t exponent) {
    while (exponent >= 9) {
        big_mul_small(a, 1000000000u);
        exponent -= 9;
    }
    uint32_t factor = 1;
    while (exponent-- > 0) {
        factor *= 10;
    }
    if (factor != 1) {
        big_mul_small(a, factor);
    }
}

/* Shortest round-trip digits (Burger-Dybvig free-format). `value` is positive
 * and finite. Returns the digit count; value ~= 0.d1d2... * 10^*exponent10. */
static uint32_t shortest_digits(uint32_t hi_bits, uint32_t lo_bits, char *digits, int32_t *exponent10) {
    const uint32_t biased = (hi_bits >> 20) & 0x7FF;
    const uint32_t frac_hi = hi_bits & 0xFFFFF;
    uint32_t f_lo = lo_bits;
    uint32_t f_hi = frac_hi;
    int32_t e;
    if (biased == 0) {
        e = -1074;
    } else {
        f_hi |= 0x100000;
        e = (int32_t)biased - 1075;
    }
    const int unequal = biased > 1 && frac_hi == 0 && lo_bits == 0;
    const int inclusive = (f_lo & 1) == 0;

    adesh_big r, s, mp, mm, t;
    big_from_u64(&r, f_lo, f_hi);
    if (e >= 0) {
        if (!unequal) {
            big_shl(&r, (uint32_t)e + 1);
            big_from_u64(&s, 2, 0);
            big_from_u64(&mp, 1, 0);
            big_shl(&mp, (uint32_t)e);
        } else {
            big_shl(&r, (uint32_t)e + 2);
            big_from_u64(&s, 4, 0);
            big_from_u64(&mp, 1, 0);
            big_shl(&mp, (uint32_t)e + 1);
        }
        big_from_u64(&mm, 1, 0);
        big_shl(&mm, (uint32_t)e);
    } else {
        if (!unequal) {
            big_shl(&r, 1);
            big_from_u64(&s, 1, 0);
            big_shl(&s, (uint32_t)(-e) + 1);
            big_from_u64(&mp, 1, 0);
        } else {
            big_shl(&r, 2);
            big_from_u64(&s, 1, 0);
            big_shl(&s, (uint32_t)(-e) + 2);
            big_from_u64(&mp, 2, 0);
        }
        big_from_u64(&mm, 1, 0);
    }

    uint32_t mantissa_bits = 0;
    {
        uint32_t top = f_hi != 0 ? f_hi : f_lo;
        while (top != 0) {
            ++mantissa_bits;
            top >>= 1;
        }
        if (f_hi != 0) {
            mantissa_bits += 32;
        }
    }
    int32_t k = (((int32_t)mantissa_bits + e - 1) * 1233) >> 12;
    if (k >= 0) {
        big_mul_pow10(&s, k);
    } else {
        big_mul_pow10(&r, -k);
        big_mul_pow10(&mp, -k);
        big_mul_pow10(&mm, -k);
    }
    for (;;) {
        big_assign(&t, &r);
        big_add(&t, &mp);
        if (big_cmp(&t, &s) >= (inclusive ? 0 : 1)) {
            big_mul_small(&s, 10);
            ++k;
            continue;
        }
        big_mul_small(&t, 10);
        if (big_cmp(&t, &s) < 0) {
            big_mul_small(&r, 10);
            big_mul_small(&mp, 10);
            big_mul_small(&mm, 10);
            --k;
            continue;
        }
        break;
    }

    uint32_t count = 0;
    for (;;) {
        big_mul_small(&r, 10);
        big_mul_small(&mp, 10);
        big_mul_small(&mm, 10);
        uint32_t digit = 0;
        while (big_cmp(&r, &s) >= 0) {
            big_sub(&r, &s);
            ++digit;
        }
        const int low = inclusive ? big_cmp(&r, &mm) <= 0 : big_cmp(&r, &mm) < 0;
        big_assign(&t, &r);
        big_add(&t, &mp);
        const int high = inclusive ? big_cmp(&t, &s) >= 0 : big_cmp(&t, &s) > 0;
        if (!low && !high) {
            digits[count++] = (char)('0' + digit);
            continue;
        }
        if (high && low) {
            big_assign(&t, &r);
            big_shl(&t, 1);
            const int order = big_cmp(&t, &s);
            if (order > 0 || (order == 0 && (digit & 1) != 0)) {
                ++digit;
            }
        } else if (high) {
            ++digit;
        }
        digits[count++] = (char)('0' + digit);
        break;
    }
    *exponent10 = k;
    return count;
}

uint64_t aot_print_f64_narrow(double value, int32_t newline) {
    union {
        double number;
        uint32_t word[2];
    } bits;
    bits.number = value;
    const uint32_t lo = bits.word[0];
    const uint32_t hi = bits.word[1];
    const int negative = (hi >> 31) != 0;
    const uint32_t biased = (hi >> 20) & 0x7FF;
    const uint32_t frac_hi = hi & 0xFFFFF;

    char text[400];
    uint32_t length = 0;
    if (biased == 0x7FF) {
        if (frac_hi != 0 || lo != 0) {
            text[length++] = 'N';
            text[length++] = 'a';
            text[length++] = 'N';
        } else {
            if (negative) {
                text[length++] = '-';
            }
            text[length++] = 'i';
            text[length++] = 'n';
            text[length++] = 'f';
        }
    } else {
        if (negative) {
            text[length++] = '-';
        }
        if (biased == 0 && frac_hi == 0 && lo == 0) {
            text[length++] = '0';
        } else {
            char digits[40];
            int32_t exponent10;
            const uint32_t count = shortest_digits(hi & 0x7FFFFFFF, lo, digits, &exponent10);
            if (exponent10 <= 0) {
                text[length++] = '0';
                text[length++] = '.';
                for (int32_t i = exponent10; i < 0; ++i) {
                    text[length++] = '0';
                }
                for (uint32_t i = 0; i < count; ++i) {
                    text[length++] = digits[i];
                }
            } else if ((uint32_t)exponent10 >= count) {
                for (uint32_t i = 0; i < count; ++i) {
                    text[length++] = digits[i];
                }
                for (uint32_t i = count; i < (uint32_t)exponent10; ++i) {
                    text[length++] = '0';
                }
            } else {
                for (uint32_t i = 0; i < (uint32_t)exponent10; ++i) {
                    text[length++] = digits[i];
                }
                text[length++] = '.';
                for (uint32_t i = (uint32_t)exponent10; i < count; ++i) {
                    text[length++] = digits[i];
                }
            }
        }
    }
    text[length] = '\0';
    return adesh_native_print_cstr(text, (int64_t)newline);
}

uint64_t aot_print_space(void) {
    return adesh_native_print_cstr(" ", 0);
}

uint64_t aot_print_newline(void) {
    // NULL body + newline flag: no cstring reloc; the LF is the stack
    // temporary inside adesh_native_print_cstr.
    return adesh_native_print_cstr(NULL, 1);
}

#if defined(_WIN32)
__declspec(noreturn) void adesh_native_panic_at(const char *message, const char *file, uint32_t line) {
    adesh_native_print_cstr("[Adesh Panic] ", 0);
    if (file != NULL && file[0] != '\0') {
        adesh_native_print_cstr(file, 0);
        adesh_native_print_cstr(":", 0);
        aot_print_u64((uint64_t)line, 0);
        adesh_native_print_cstr(" - ", 0);
    }
    adesh_native_print_cstr(message, 1);
    ExitProcess(101);
}

__declspec(noreturn) void aot_abort_str(const char *message) {
    adesh_native_abort_str(message);
}

__declspec(noreturn) void aot_panic_at(const char *message, const char *file, uint32_t line) {
    adesh_native_panic_at(message, file, line);
}
#else
__attribute__((noreturn)) void adesh_native_panic_at(const char *message, const char *file, uint32_t line) {
    adesh_native_print_cstr("[Adesh Panic] ", 0);
    if (file != NULL && file[0] != '\0') {
        adesh_native_print_cstr(file, 0);
        adesh_native_print_cstr(":", 0);
        aot_print_u64((uint64_t)line, 0);
        adesh_native_print_cstr(" - ", 0);
    }
    adesh_native_print_cstr(message, 1);
#if defined(__i386__) && defined(__linux__)
    i686_exit(101);
#else
    _exit(101);
#endif
}

__attribute__((noreturn)) void aot_abort_str(const char *message) {
    adesh_native_abort_str(message);
}

__attribute__((noreturn)) void aot_panic_at(const char *message, const char *file, uint32_t line) {
    adesh_native_panic_at(message, file, line);
}
#endif
