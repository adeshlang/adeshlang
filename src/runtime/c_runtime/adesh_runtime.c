// AdeshLang Runtime Support Library for AOT compiled binaries
// This provides implementations for built-in functions that are called by AOT-compiled code

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
#include <limits.h>

#define ADESH_SKIP_DUPLICATE_RUNTIME_SYMBOLS 1

// Safe JSON string escaping helper: computes escaped byte length (excluding quotes)
static size_t json_escaped_len(const char* s) {
    if (!s) return 0;
    size_t len = 0;
    for (const unsigned char* p = (const unsigned char*)s; *p; p++) {
        switch (*p) {
            case '"':
            case '\\':
            case '\b':
            case '\f':
            case '\n':
            case '\r':
            case '\t':
                len += 2;
                break;
            default:
                len += (*p < 0x20) ? 6 : 1;
                break;
        }
    }
    return len;
}

// Safe JSON string escaping helper: writes escaped bytes (excluding quotes)
static char* json_write_escaped(char* dst, const char* s) {
    if (!s) return dst;
    static const char HEX[] = "0123456789abcdef";
    for (const unsigned char* p = (const unsigned char*)s; *p; p++) {
        switch (*p) {
            case '"':  *dst++ = '\\'; *dst++ = '"'; break;
            case '\\': *dst++ = '\\'; *dst++ = '\\'; break;
            case '\b': *dst++ = '\\'; *dst++ = 'b'; break;
            case '\f': *dst++ = '\\'; *dst++ = 'f'; break;
            case '\n': *dst++ = '\\'; *dst++ = 'n'; break;
            case '\r': *dst++ = '\\'; *dst++ = 'r'; break;
            case '\t': *dst++ = '\\'; *dst++ = 't'; break;
            default:
                if (*p < 0x20) {
                    *dst++ = '\\';
                    *dst++ = 'u';
                    *dst++ = '0';
                    *dst++ = '0';
                    *dst++ = HEX[(*p >> 4) & 0xF];
                    *dst++ = HEX[*p & 0xF];
                } else {
                    *dst++ = (char)*p;
                }
                break;
        }
    }
    return dst;
}

static size_t json_escaped_len_n(const char* s, size_t n) {
    if (!s) return 0;
    size_t len = 0;
    for (size_t i = 0; i < n; i++) {
        unsigned char c = (unsigned char)s[i];
        switch (c) {
            case '"': case '\\': case '\b': case '\f': case '\n': case '\r': case '\t':
                len += 2;
                break;
            default:
                len += (c < 0x20) ? 6 : 1;
                break;
        }
    }
    return len;
}

static char* json_write_escaped_n(char* dst, const char* s, size_t n) {
    if (!s) return dst;
    static const char HEX[] = "0123456789abcdef";
    for (size_t i = 0; i < n; i++) {
        unsigned char c = (unsigned char)s[i];
        switch (c) {
            case '"':  *dst++ = '\\'; *dst++ = '"'; break;
            case '\\': *dst++ = '\\'; *dst++ = '\\'; break;
            case '\b': *dst++ = '\\'; *dst++ = 'b'; break;
            case '\f': *dst++ = '\\'; *dst++ = 'f'; break;
            case '\n': *dst++ = '\\'; *dst++ = 'n'; break;
            case '\r': *dst++ = '\\'; *dst++ = 'r'; break;
            case '\t': *dst++ = '\\'; *dst++ = 't'; break;
            default:
                if (c < 0x20) {
                    *dst++ = '\\';
                    *dst++ = 'u';
                    *dst++ = '0';
                    *dst++ = '0';
                    *dst++ = HEX[(c >> 4) & 0xF];
                    *dst++ = HEX[c & 0xF];
                } else {
                    *dst++ = (char)c;
                }
                break;
        }
    }
    return dst;
}

// Checked size_t addition helper (returns 1 on success, 0 on overflow)
static int checked_add_size(size_t a, size_t b, size_t* out) {
    if (a > SIZE_MAX - b) return 0;
    *out = a + b;
    return 1;
}

// Environment variable storage for runtime loading
#define MAX_ENV_VARS 256
typedef struct {
    char* key;
    char* value;
} EnvVar;

static EnvVar runtime_env[MAX_ENV_VARS];
static int runtime_env_count = 0;

// Get environment variable (checks runtime_env first, then OS environment)
const char* adesh_get_env(const char* key) {
    if (!key) return NULL;
    
    // First check runtime-loaded environment
    for (int i = 0; i < runtime_env_count; i++) {
        if (runtime_env[i].key && strcmp(runtime_env[i].key, key) == 0) {
            return runtime_env[i].value;
        }
    }
    
    // Fall back to OS environment
    return getenv(key);
}

// Parse a single line from .env file
static int parse_env_line(const char* line, char** key_out, char** value_out) {
    if (!line || line[0] == '#' || line[0] == '\0' || line[0] == '\n') {
        return 0; // Skip comments and empty lines
    }
    
    // Skip "export " prefix if present
    const char* p = line;
    if (strncmp(p, "export ", 7) == 0) {
        p += 7;
    }
    
    // Find the '=' sign
    const char* eq = strchr(p, '=');
    if (!eq) return 0;
    
    // Extract key
    size_t key_len = (size_t)(eq - p);
    while (key_len > 0 && (p[key_len-1] == ' ' || p[key_len-1] == '\t')) key_len--;
    if (key_len == 0) return 0;
    
    char* key = (char*)malloc(key_len + 1);
    if (!key) return 0;
    memcpy(key, p, key_len);
    key[key_len] = '\0';
    
    // Extract value
    const char* val_start = eq + 1;
    while (*val_start == ' ' || *val_start == '\t') val_start++;
    
    const char* val_end = val_start + strlen(val_start);
    while (val_end > val_start && (val_end[-1] == '\n' || val_end[-1] == '\r' || val_end[-1] == ' ')) val_end--;
    
    size_t val_len = (size_t)(val_end - val_start);
    
    // Handle quoted strings
    if (val_len >= 2 && ((val_start[0] == '"' && val_end[-1] == '"') || 
                         (val_start[0] == '\'' && val_end[-1] == '\''))) {
        val_start++;
        val_len -= 2;
    }
    
    char* value = (char*)malloc(val_len + 1);
    if (!value) {
        free(key);
        return 0;
    }
    memcpy(value, val_start, val_len);
    value[val_len] = '\0';
    
    *key_out = key;
    *value_out = value;
    return 1;
}

// Load .env file and return as opaque object (just returns the path for now)
// In a full implementation, this would return a hash map structure
const char* adesh_load_env_file(const char* path) {
    if (!path) return NULL;
    
    // For now, just return the path as the "object"
    // The actual values will be loaded on demand
    size_t len = strlen(path);
    char* path_copy = (char*)malloc(len + 1);
    if (!path_copy) return NULL;
    memcpy(path_copy, path, len + 1);
    return path_copy;
}

// Get a specific key from a .env file
const char* adesh_get_env_file(const char* key, const char* path) {
    if (!key || !path) return NULL;
    
    FILE* f = fopen(path, "r");
    if (!f) return NULL;
    
    char line[1024];
    const char* result = NULL;
    
    while (fgets(line, sizeof(line), f)) {
        char* k = NULL;
        char* v = NULL;
        if (parse_env_line(line, &k, &v)) {
            if (strcmp(k, key) == 0) {
                result = v;
                free(k);
                break;
            }
            free(k);
            free(v);
        }
    }
    
    fclose(f);
    return result;
}

// Load environment variables from a file into the runtime environment
// This allows subsequent adesh_get_env() calls to return these values
int adesh_env_runtime_load(const char* path_or_obj) {
    if (!path_or_obj) return 0;
    
    // Treat the "object" as a file path
    const char* path = path_or_obj;
    
    FILE* f = fopen(path, "r");
    if (!f) return 0;
    
    char line[1024];
    int loaded_count = 0;
    
    while (fgets(line, sizeof(line), f) && runtime_env_count < MAX_ENV_VARS) {
        char* key = NULL;
        char* value = NULL;
        if (parse_env_line(line, &key, &value)) {
            // Check if key already exists in runtime_env
            int found = -1;
            for (int i = 0; i < runtime_env_count; i++) {
                if (runtime_env[i].key && strcmp(runtime_env[i].key, key) == 0) {
                    found = i;
                    break;
                }
            }
            
            if (found >= 0) {
                // Update existing entry
                free(runtime_env[found].value);
                runtime_env[found].value = value;
                free(key);
            } else {
                // Add new entry
                runtime_env[runtime_env_count].key = key;
                runtime_env[runtime_env_count].value = value;
                runtime_env_count++;
            }
            loaded_count++;
        }
    }
    
    fclose(f);
    return loaded_count;
}

// Get property from an envFromFile object
// The "object" is actually the file path, so we read it on demand
const char* adesh_get_property(const char* obj_path, const char* property) {
    if (!obj_path || !property) return NULL;
    // Just delegate to adesh_get_env_file
    return adesh_get_env_file(property, obj_path);
}

// Global storage for command-line arguments
static int g_argc = 0;
static char** g_argv = NULL;

// Initialize arguments (called from generated main function)
#ifndef ADESH_SKIP_DUPLICATE_RUNTIME_SYMBOLS
void adesh_init_args(int argc, char** argv) {
    g_argc = argc;
    g_argv = argv;
}
#endif

// Get a slice of arguments starting from index (returns JSON array string)
const char* adesh_args_slice(int start) {
    if (!g_argv || start < 0 || start >= g_argc) {
        return "[]";
    }
    
    // Calculate exact required buffer size with overflow checks and JSON escaping
    size_t total_len = 2; // []
    for (int i = start; i < g_argc; i++) {
        size_t esc = json_escaped_len(g_argv[i]);
        if (!checked_add_size(total_len, esc, &total_len) ||
            !checked_add_size(total_len, 3, &total_len)) { // "arg",
            return "[]";
        }
    }
    if (!checked_add_size(total_len, 1, &total_len)) return "[]";
    
    char* result = (char*)malloc(total_len);
    if (!result) return "[]";
    char* p = result;
    *p++ = '[';
    
    for (int i = start; i < g_argc; i++) {
        if (i > start) *p++ = ',';
        *p++ = '"';
        p = json_write_escaped(p, g_argv[i]);
        *p++ = '"';
    }
    
    *p++ = ']';
    *p = '\0';
    
    return result;
}

// Join arguments with separator
const char* adesh_args_join(const char* separator) {
    if (!g_argv || g_argc <= 0) {
        return "";
    }
    
    if (!separator) separator = " ";
    size_t sep_len = strlen(separator);
    
    // Calculate total length with overflow checks
    size_t total_len = 0;
    for (int i = 0; i < g_argc; i++) {
        if (!checked_add_size(total_len, strlen(g_argv[i]), &total_len)) return "";
        if (i < g_argc - 1) {
            if (!checked_add_size(total_len, sep_len, &total_len)) return "";
        }
    }
    if (!checked_add_size(total_len, 1, &total_len)) return "";
    
    char* result = (char*)malloc(total_len);
    if (!result) return "";
    char* p = result;
    
    for (int i = 0; i < g_argc; i++) {
        const char* arg = g_argv[i];
        while (*arg) {
            *p++ = *arg++;
        }
        if (i < g_argc - 1) {
            const char* sep = separator;
            while (*sep) {
                *p++ = *sep++;
            }
        }
    }
    
    *p = '\0';
    return result;
}

// Get specific argument by index
const char* adesh_get_arg(int index) {
    if (!g_argv || index < 0 || index >= g_argc) {
        return "";
    }
    return g_argv[index];
}

// Get executable name (argv[0])
const char* adesh_exec_name() {
    if (!g_argv || g_argc <= 0) {
        return "";
    }
    return g_argv[0];
}

// Get argv as JSON array string
const char* adesh_get_argv() {
    if (!g_argv || g_argc <= 0) {
        return "[]";
    }
    
    // Calculate exact required buffer size with overflow checks and JSON escaping
    size_t total_len = 2; // []
    for (int i = 0; i < g_argc; i++) {
        size_t esc = json_escaped_len(g_argv[i]);
        if (!checked_add_size(total_len, esc, &total_len) ||
            !checked_add_size(total_len, 3, &total_len)) { // "arg",
            return "[]";
        }
    }
    if (!checked_add_size(total_len, 1, &total_len)) return "[]";
    
    char* result = (char*)malloc(total_len);
    if (!result) return "[]";
    char* p = result;
    *p++ = '[';
    
    for (int i = 0; i < g_argc; i++) {
        if (i > 0) *p++ = ',';
        *p++ = '"';
        p = json_write_escaped(p, g_argv[i]);
        *p++ = '"';
    }
    
    *p++ = ']';
    *p = '\0';
    
    return result;
}

// Concatenate two strings
const char* adesh_string_concat(const char* a, const char* b) {
    if (!a && !b) return "";
    if (!a) return b;
    if (!b) return a;
    
    size_t len_a = strlen(a);
    size_t len_b = strlen(b);
    size_t total = 0;
    if (!checked_add_size(len_a, len_b, &total) || !checked_add_size(total, 1, &total)) {
        return "";
    }
    char* result = (char*)malloc(total);
    if (!result) return "";
    memcpy(result, a, len_a);
    memcpy(result + len_a, b, len_b);
    result[len_a + len_b] = '\0';
    return result;
}

#ifndef ADESH_SKIP_DUPLICATE_RUNTIME_SYMBOLS
// Convert integer to string
const char* adesh_int_to_string(long long val) {
    char* result = (char*)malloc(32);
    snprintf(result, 32, "%lld", val);
    return result;
}

// Convert double to string
const char* adesh_double_to_string(double val) {
    char* result = (char*)malloc(32);
    snprintf(result, 32, "%g", val);
    return result;
}

// Convert array buffer to string representation: [e0, e1, ...]
// ptr: base address of array buffer (metadata then elements)
// len: number of elements
// elem_size: size in bytes of each element (1,2,4,8,16)
// is_float: 1 if elements are floating point (f32/f64), 0 otherwise
const char* adesh_array_to_string(const void* ptr, long long len, long long elem_size, unsigned char is_float) {
    if (!ptr || len <= 0) {
        return "[]";
    }

    const unsigned char* base = (const unsigned char*)ptr;
    // Infer metadata size to locate element region; matches AOT backend logic
    long long metadata_size = (elem_size <= 4) ? 16 : 24;
    const unsigned char* elems = base + metadata_size;

    // Rough buffer estimate: up to 32 chars per element + brackets and commas
    long long est = 2 + (len > 0 ? (len - 1) * 2 : 0) + len * 32;
    char* out = (char*)malloc((size_t)est);
    char* p = out;

    *p++ = '[';
    for (long long i = 0; i < len; i++) {
        if (i > 0) { *p++ = ','; *p++ = ' '; }

        const unsigned char* elem = elems + i * elem_size;
        if (is_float) {
            if (elem_size == 4) {
                float v;
                memcpy(&v, elem, sizeof(float));
                // print with minimal trailing zeros
                p += snprintf(p, 32, "%g", (double)v);
            } else {
                double v;
                memcpy(&v, elem, sizeof(double));
                p += snprintf(p, 32, "%g", v);
            }
        } else {
            switch (elem_size) {
                case 1: {
                    // Treat as signed for display consistency
                    signed char v;
                    memcpy(&v, elem, 1);
                    p += snprintf(p, 32, "%hhd", v);
                    break;
                }
                case 2: {
                    short v;
                    memcpy(&v, elem, 2);
                    p += snprintf(p, 32, "%hd", v);
                    break;
                }
                case 4: {
                    int v;
                    memcpy(&v, elem, 4);
                    p += snprintf(p, 32, "%d", v);
                    break;
                }
                case 8: {
                    long long v;
                    memcpy(&v, elem, 8);
                    p += snprintf(p, 32, "%lld", v);
                    break;
                }
                case 16: {
                    // Minimal support for 128-bit integers: print lower 64 bits
                    long long lo;
                    memcpy(&lo, elem, 8);
                    p += snprintf(p, 32, "%lld", lo);
                    break;
                }
                default: {
                    // Unknown size: print as pointer-sized integer
                    long long v = 0;
                    memcpy(&v, elem, (elem_size < 8) ? elem_size : 8);
                    p += snprintf(p, 32, "%lld", v);
                    break;
                }
            }
        }
    }

    *p++ = ']';
    *p = '\0';
    return out;
}
#endif

// Parse command-line arguments into structured format
// Returns JSON string with flags and positionals
const char* adesh_parse_args() {
    if (!g_argv || g_argc <= 1) {
        return "{\"flags\":{},\"positionals\":[]}";
    }
    
    // Exact upper-bound size calculation with overflow checks and JSON escaping
    size_t buffer_size = 64; // Base JSON wrapper {"flags":{},"positionals":[]}\0
    for (int i = 1; i < g_argc; i++) {
        const char* arg = g_argv[i];
        if (!arg) continue;
        if (arg[0] == '-') {
            if (arg[1] == '-') {
                const char* eq = strchr(arg + 2, '=');
                if (eq) {
                    size_t key_esc = json_escaped_len_n(arg + 2, (size_t)(eq - (arg + 2)));
                    size_t val_esc = json_escaped_len(eq + 1);
                    if (!checked_add_size(buffer_size, key_esc, &buffer_size) ||
                        !checked_add_size(buffer_size, val_esc, &buffer_size) ||
                        !checked_add_size(buffer_size, 8, &buffer_size)) {
                        return "{\"flags\":{},\"positionals\":[]}";
                    }
                } else {
                    size_t key_esc = json_escaped_len(arg + 2);
                    if (!checked_add_size(buffer_size, key_esc, &buffer_size) ||
                        !checked_add_size(buffer_size, 10, &buffer_size)) {
                        return "{\"flags\":{},\"positionals\":[]}";
                    }
                }
            } else {
                // Short flags -abc: each char emits up to ,"\\u00xx":true (15 bytes)
                for (size_t j = 1; arg[j] != '\0'; j++) {
                    if (!checked_add_size(buffer_size, 16, &buffer_size)) {
                        return "{\"flags\":{},\"positionals\":[]}";
                    }
                }
            }
        } else {
            size_t pos_esc = json_escaped_len(arg);
            if (!checked_add_size(buffer_size, pos_esc, &buffer_size) ||
                !checked_add_size(buffer_size, 4, &buffer_size)) {
                return "{\"flags\":{},\"positionals\":[]}";
            }
        }
    }
    
    char* result = (char*)malloc(buffer_size);
    if (!result) return "{\"flags\":{},\"positionals\":[]}";
    char* p = result;
    
    // Start JSON object
    strcpy(p, "{\"flags\":{");
    p += strlen(p);
    
    int flag_count = 0;
    
    // Parse flags (arguments starting with -)
    for (int i = 1; i < g_argc; i++) {
        const char* arg = g_argv[i];
        if (!arg) continue;
        if (arg[0] == '-') {
            if (arg[1] == '-') {
                // Long flag --key=value or --key
                const char* eq = strchr(arg + 2, '=');
                if (eq) {
                    size_t key_len = (size_t)(eq - (arg + 2));
                    if (flag_count > 0) *p++ = ',';
                    *p++ = '"';
                    p = json_write_escaped_n(p, arg + 2, key_len);
                    strcpy(p, "\":\"");
                    p += 3;
                    p = json_write_escaped(p, eq + 1);
                    *p++ = '"';
                    flag_count++;
                } else {
                    if (flag_count > 0) *p++ = ',';
                    *p++ = '"';
                    p = json_write_escaped(p, arg + 2);
                    strcpy(p, "\":true");
                    p += 6;
                    flag_count++;
                }
            } else {
                // Short flags -abc or -v
                for (size_t j = 1; arg[j] != '\0'; j++) {
                    if (flag_count > 0) *p++ = ',';
                    *p++ = '"';
                    p = json_write_escaped_n(p, &arg[j], 1);
                    strcpy(p, "\":true");
                    p += 6;
                    flag_count++;
                }
            }
        }
    }
    
    strcpy(p, "},\"positionals\":[");
    p += strlen(p);
    
    // Parse positionals (non-flag arguments)
    int pos_count = 0;
    for (int i = 1; i < g_argc; i++) {
        const char* arg = g_argv[i];
        if (!arg) continue;
        if (arg[0] != '-') {
            if (pos_count > 0) *p++ = ',';
            *p++ = '"';
            p = json_write_escaped(p, arg);
            *p++ = '"';
            pos_count++;
        }
    }
    
    strcpy(p, "]}");
    
    return result;
}

// Get argument value by key (for --key=value or --key style arguments)
const char* adesh_arg_get(const char* key) {
    if (!g_argv || !key) return NULL;
    
    // Look for --key=value
    char search_key[256];
    snprintf(search_key, sizeof(search_key), "--%s=", key);
    int search_len = strlen(search_key);
    
    for (int i = 1; i < g_argc; i++) {
        if (strncmp(g_argv[i], search_key, search_len) == 0) {
            return g_argv[i] + search_len;
        }
    }
    
    // Look for --key followed by value
    snprintf(search_key, sizeof(search_key), "--%s", key);
    for (int i = 1; i < g_argc - 1; i++) {
        if (strcmp(g_argv[i], search_key) == 0) {
            // Next argument is the value if it doesn't start with -
            if (g_argv[i + 1][0] != '-') {
                return g_argv[i + 1];
            }
        }
    }
    
    return NULL;
}

// Check if a flag is present
int adesh_arg_has(const char* flag) {
    if (!g_argv || !flag) return 0;
    
    // Check for --flag
    char long_flag[256];
    snprintf(long_flag, sizeof(long_flag), "--%s", flag);
    for (int i = 1; i < g_argc; i++) {
        if (strcmp(g_argv[i], long_flag) == 0) {
            return 1;
        }
        // Check for --flag=value
        int len = strlen(long_flag);
        if (strncmp(g_argv[i], long_flag, len) == 0 && g_argv[i][len] == '=') {
            return 1;
        }
    }
    
    // Check for -f (single character)
    if (strlen(flag) == 1) {
        for (int i = 1; i < g_argc; i++) {
            const char* arg = g_argv[i];
            if (arg[0] == '-' && arg[1] != '-') {
                // Short flag group -abc
                for (int j = 1; arg[j] != '\0'; j++) {
                    if (arg[j] == flag[0]) {
                        return 1;
                    }
                }
            }
        }
    }
    
    return 0;
}

// Find index of a value in arguments
int adesh_args_index_of(const char* value) {
    if (!g_argv || !value) return -1;
    
    for (int i = 0; i < g_argc; i++) {
        if (strcmp(g_argv[i], value) == 0) {
            return i;
        }
    }
    
    return -1;
}

#ifndef ADESH_SKIP_DUPLICATE_RUNTIME_SYMBOLS
// String concatenation helper for AOT
// Takes two string pointers (i64 as pointers) and returns a new concatenated string
// This is called by str_concat builtin in AOT compilation
void* adesh_str_concat(const char* left, const char* right) {
    if (!left) left = "";
    if (!right) right = "";
    
    size_t left_len = strlen(left);
    size_t right_len = strlen(right);
    size_t total_len = left_len + right_len + 1;
    
    char* result = (char*)malloc(total_len);
    if (!result) {
        fprintf(stderr, "Error: malloc failed in adesh_str_concat\n");
        exit(1);
    }
    
    strcpy(result, left);
    strcat(result, right);
    
    return (void*)result;
}
#endif

// Convert numeric/untyped value to decimal string representation without
// dereferencing untrusted integers as pointers.
void* adesh_value_to_string_c_old(void* value) {
    char* buffer = (char*)malloc(64);
    if (!buffer) {
        fprintf(stderr, "Error: malloc failed in adesh_value_to_string\n");
        return NULL;
    }
    intptr_t int_val = (intptr_t)value;
    snprintf(buffer, 64, "%lld", (long long)int_val);
    return buffer;
}

#define ADESH_MAX_RANGE_ELEMENTS ((uint64_t)(16 * 1024 * 1024))

// Create a range array: adesh_rt_range(start, end, inclusive) -> array pointer
// Returns a pointer to an array with metadata: [length_i64][capacity_i64][elements...]
// inclusive: 0 for exclusive (start..end), 1 for inclusive (start..=end)
void* adesh_rt_range(int64_t start, int64_t end, int64_t inclusive) {
    // Compute element count in unsigned arithmetic to prevent signed overflow UB
    // (e.g. start = INT64_MIN, end = INT64_MAX).
    uint64_t ucount = 0;
    if (end >= start) {
        uint64_t diff = (uint64_t)end - (uint64_t)start;
        if (inclusive) {
            if (diff == UINT64_MAX) return NULL;
            ucount = diff + 1;
        } else {
            ucount = diff;
        }
    }
    
    if (ucount > ADESH_MAX_RANGE_ELEMENTS || ucount > ((uint64_t)SIZE_MAX - 16) / 8) {
        fprintf(stderr, "Error: range size exceeds safe limit in adesh_rt_range\n");
        return NULL;
    }
    
    size_t total_size = 16 + (size_t)ucount * 8;
    char* array = (char*)malloc(total_size);
    if (!array) {
        fprintf(stderr, "Error: malloc failed in adesh_rt_range\n");
        return NULL;
    }
    
    int64_t count = (int64_t)ucount;
    *(int64_t*)array = count;
    *(int64_t*)(array + 8) = count;
    
    // Fill elements using unsigned addition to avoid signed overflow on last iteration
    uint64_t current = (uint64_t)start;
    for (uint64_t i = 0; i < ucount; i++) {
        *(int64_t*)(array + 16 + (size_t)i * 8) = (int64_t)current;
        current += 1;
    }
    
    return array;
}

// Allocate a typed array: adesh_rt_alloc_typed(count, elem_size) -> array pointer
// Returns a pointer to an array with metadata: [length_i64][capacity_i64][elements...]
void* adesh_rt_alloc_typed(int64_t count, int64_t elem_size) {
    if (count < 0 || elem_size <= 0) return NULL;
    
    // Typed load/store operate on 8-byte slots; ensure at least 8 bytes per slot
    // so smaller elem_size values never cause out-of-bounds reads/writes.
    uint64_t effective_elem = (elem_size < 8) ? 8 : (uint64_t)elem_size;
    uint64_t ucount = (uint64_t)count;
    if (ucount > ((uint64_t)SIZE_MAX - 16) / effective_elem) {
        fprintf(stderr, "Error: allocation overflow in adesh_rt_alloc_typed\n");
        return NULL;
    }
    
    size_t total_size = 16 + (size_t)(ucount * effective_elem);
    char* array = (char*)malloc(total_size);
    if (!array) {
        fprintf(stderr, "Error: malloc failed in adesh_rt_alloc_typed\n");
        return NULL;
    }
    
    memset(array, 0, total_size);
    *(int64_t*)array = count;
    *(int64_t*)(array + 8) = count;
    return array;
}

// Helper for bounds-checked indexing with negative index support (no signed overflow)
static int adesh_resolve_index(int64_t length, int64_t index, size_t* out_idx) {
    if (length <= 0) return 0;
    if (index < 0) {
        if (index < -length) return 0;
        index = length + index;
    }
    if (index < 0 || index >= length) return 0;
    *out_idx = (size_t)index;
    return 1;
}

// Get element from array: adesh_rt_get_index(array_ptr, index) -> element value
int64_t adesh_rt_get_index(void* array_ptr, int64_t index) {
    if (!array_ptr) return 0;
    
    char* array = (char*)array_ptr;
    int64_t length = *(int64_t*)array;
    size_t idx = 0;
    if (!adesh_resolve_index(length, index, &idx)) {
        return 0;
    }
    
    return *(int64_t*)(array + 16 + idx * 8);
}

// Typed load: adesh_rt_load_typed(ptr, index) -> element value (for pointer-based array indexing)
int64_t adesh_rt_load_typed(void* array_ptr, int64_t index) {
    if (!array_ptr) return 0;
    
    char* array = (char*)array_ptr;
    int64_t length = *(int64_t*)array;
    size_t idx = 0;
    if (!adesh_resolve_index(length, index, &idx)) {
        return 0;
    }
    
    return *(int64_t*)(array + 16 + idx * 8);
}

// Typed store: adesh_rt_store_typed(ptr, index, value) -> 1 on success, 0 on failure/bounds error
int64_t adesh_rt_store_typed(void* array_ptr, int64_t index, int64_t value) {
    if (!array_ptr) return 0;
    
    char* array = (char*)array_ptr;
    int64_t length = *(int64_t*)array;
    size_t idx = 0;
    if (!adesh_resolve_index(length, index, &idx)) {
        return 0;
    }
    
    *(int64_t*)(array + 16 + idx * 8) = value;
    return 1;
}
