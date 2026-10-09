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
#include <unistd.h>

static size_t c_string_length(const char *text) {
    size_t length = 0;
    if (text != NULL) {
        while (text[length] != '\0') {
            ++length;
        }
    }
    return length;
}

static void write_all(const char *bytes, size_t length) {
    while (length > 0) {
        const ssize_t written = write(STDOUT_FILENO, bytes, length);
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
        write_all("\n", 1);
    }
    return 0;
}

__attribute__((noreturn)) void adesh_native_abort_str(const char *message) {
    adesh_native_print_cstr(message, 1);
    _exit(101);
}
#endif
