// AdeshLang Runtime Support Library for AOT compiled binaries
// This provides implementations for built-in functions that are called by AOT-compiled code

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>

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
    int key_len = eq - p;
    while (key_len > 0 && (p[key_len-1] == ' ' || p[key_len-1] == '\t')) key_len--;
    if (key_len == 0) return 0;
    
    char* key = (char*)malloc(key_len + 1);
    strncpy(key, p, key_len);
    key[key_len] = '\0';
    
    // Extract value
    const char* val_start = eq + 1;
    while (*val_start == ' ' || *val_start == '\t') val_start++;
    
    const char* val_end = val_start + strlen(val_start);
    while (val_end > val_start && (val_end[-1] == '\n' || val_end[-1] == '\r' || val_end[-1] == ' ')) val_end--;
    
    int val_len = val_end - val_start;
    
    // Handle quoted strings
    if (val_len >= 2 && ((val_start[0] == '"' && val_end[-1] == '"') || 
                         (val_start[0] == '\'' && val_end[-1] == '\''))) {
        val_start++;
        val_len -= 2;
    }
    
    char* value = (char*)malloc(val_len + 1);
    strncpy(value, val_start, val_len);
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
    char* path_copy = (char*)malloc(strlen(path) + 1);
    strcpy(path_copy, path);
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
void adesh_init_args(int argc, char** argv) {
    g_argc = argc;
    g_argv = argv;
}

// Get a slice of arguments starting from index (returns JSON array string)
const char* adesh_args_slice(int start) {
    if (!g_argv || start < 0 || start >= g_argc) {
        return "[]";
    }
    
    // Calculate required buffer size
    int total_len = 2; // []
    for (int i = start; i < g_argc; i++) {
        total_len += strlen(g_argv[i]) + 4; // "arg",
    }
    
    char* result = (char*)malloc(total_len + 1);
    char* p = result;
    *p++ = '[';
    
    for (int i = start; i < g_argc; i++) {
        if (i > start) *p++ = ',';
        *p++ = '"';
        const char* arg = g_argv[i];
        while (*arg) {
            *p++ = *arg++;
        }
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
    int sep_len = strlen(separator);
    
    // Calculate total length
    int total_len = 0;
    for (int i = 0; i < g_argc; i++) {
        total_len += strlen(g_argv[i]);
        if (i < g_argc - 1) total_len += sep_len;
    }
    
    char* result = (char*)malloc(total_len + 1);
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
    
    // Calculate required buffer size
    int total_len = 2; // []
    for (int i = 0; i < g_argc; i++) {
        total_len += strlen(g_argv[i]) + 4; // "arg",
    }
    
    char* result = (char*)malloc(total_len + 1);
    char* p = result;
    *p++ = '[';
    
    for (int i = 0; i < g_argc; i++) {
        if (i > 0) *p++ = ',';
        *p++ = '"';
        const char* arg = g_argv[i];
        while (*arg) {
            *p++ = *arg++;
        }
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
    
    int len_a = strlen(a);
    int len_b = strlen(b);
    char* result = (char*)malloc(len_a + len_b + 1);
    strcpy(result, a);
    strcat(result, b);
    return result;
}

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

// Parse command-line arguments into structured format
// Returns JSON string with flags and positionals
const char* adesh_parse_args() {
    if (!g_argv || g_argc <= 1) {
        return "{\"flags\":{},\"positionals\":[]}";
    }
    
    // First pass: count to allocate buffer
    int buffer_size = 100; // Start with base size
    for (int i = 1; i < g_argc; i++) {
        buffer_size += strlen(g_argv[i]) + 20; // Extra space for JSON formatting
    }
    
    char* result = (char*)malloc(buffer_size);
    char* p = result;
    
    // Start JSON object
    strcpy(p, "{\"flags\":{");
    p += strlen(p);
    
    int flag_count = 0;
    
    // Parse flags (arguments starting with -)
    for (int i = 1; i < g_argc; i++) {
        const char* arg = g_argv[i];
        if (arg[0] == '-') {
            if (arg[1] == '-') {
                // Long flag --key=value or --key
                const char* eq = strchr(arg + 2, '=');
                if (eq) {
                    // --key=value
                    int key_len = eq - (arg + 2);
                    if (flag_count > 0) *p++ = ',';
                    *p++ = '"';
                    strncpy(p, arg + 2, key_len);
                    p += key_len;
                    strcpy(p, "\":\"");
                    p += 3;
                    const char* val = eq + 1;
                    while (*val) *p++ = *val++;
                    *p++ = '"';
                    flag_count++;
                } else {
                    // --key (boolean flag)
                    if (flag_count > 0) *p++ = ',';
                    *p++ = '"';
                    const char* key = arg + 2;
                    while (*key) *p++ = *key++;
                    strcpy(p, "\":true");
                    p += 6;
                    flag_count++;
                }
            } else {
                // Short flags -abc or -v
                for (int j = 1; arg[j] != '\0'; j++) {
                    if (flag_count > 0) *p++ = ',';
                    *p++ = '"';
                    *p++ = arg[j];
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
        if (arg[0] != '-') {
            if (pos_count > 0) *p++ = ',';
            *p++ = '"';
            const char* a = arg;
            while (*a) *p++ = *a++;
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

// Convert any value to string representation
// This is called by str_concat and other string operations in AOT
// The value is passed as an i64 that could be:
// - A pointer to a string (just return it)
// - An integer (convert to decimal)
// - A double (convert to decimal with precision)
// For simplicity, we treat it as a pointer first, and if it looks invalid, treat as int
void* adesh_value_to_string_c_old(void* value) {
    // For now, we'll implement a simple version that handles common cases
    // A more complete version would need type information
    
    // Allocate buffer for conversion
    char* buffer = (char*)malloc(256);
    if (!buffer) {
        fprintf(stderr, "Error: malloc failed in adesh_value_to_string\n");
        exit(1);
    }
    
    // Try to detect if it's a valid string pointer or a numeric value
    // This is a heuristic: check if pointer looks reasonable
    void* ptr = value;
    if (ptr && (uintptr_t)ptr > 1000) {
        // Looks like a valid pointer, try to use as string
        const char* str = (const char*)ptr;
        // Check if it looks like a valid C string (NULL-terminated, printable)
        if (strlen(str) < 200) {
            strcpy(buffer, str);
            return buffer;
        }
    }
    
    // Otherwise, treat as integer and convert
    long long int_val = (long long)value;
    snprintf(buffer, 256, "%lld", int_val);
    
    return buffer;
}

// Create a range array: adesh_rt_range(start, end, inclusive) -> array pointer
// Returns a pointer to an array with metadata: [length_i64][capacity_i64][elements...]
// inclusive: 0 for exclusive (start..end), 1 for inclusive (start..=end)
void* adesh_rt_range(int64_t start, int64_t end, int64_t inclusive) {
    // For now, step is always 1 for forward ranges
    int64_t step = 1;
    
    // Calculate number of elements
    int64_t count = 0;
    if (inclusive) {
        // Inclusive range: start..=end includes both endpoints
        count = (end >= start) ? (end - start + 1) : 0;
    } else {
        // Exclusive range: start..end excludes end
        count = (end >= start) ? (end - start) : 0;
    }
    
    if (count < 0) count = 0;
    
    // Allocate array: 8 bytes length + 8 bytes capacity + (8 bytes per element)
    int64_t total_size = 16 + (count * 8);
    char* array = (char*)malloc(total_size);
    if (!array) {
        fprintf(stderr, "Error: malloc failed in adesh_rt_range\n");
        return NULL;
    }
    
    // Store length and capacity
    *(int64_t*)array = count;
    *(int64_t*)(array + 8) = count;
    
    // Fill elements
    int64_t current = start;
    for (int64_t i = 0; i < count; i++) {
        *(int64_t*)(array + 16 + i * 8) = current;
        current += step;
    }
    
    return array;
}

// Allocate a typed array: adesh_rt_alloc_typed(count, elem_size) -> array pointer
// Returns a pointer to an array with metadata: [length_i64][capacity_i64][elements...]
void* adesh_rt_alloc_typed(int64_t count, int64_t elem_size) {
    if (count < 0 || elem_size <= 0) return NULL;
    
    int64_t total_size = 16 + (count * elem_size);
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

// Get element from array: adesh_rt_get_index(array_ptr, index) -> element value
int64_t adesh_rt_get_index(void* array_ptr, int64_t index) {
    if (!array_ptr) return 0;
    
    char* array = (char*)array_ptr;
    int64_t length = *(int64_t*)array;
    
    // Support negative indexing
    if (index < 0) {
        index = length + index;
    }
    
    // Bounds check
    if (index < 0 || index >= length) {
        return 0;
    }
    
    // Get element (assuming 8-byte integers)
    int64_t result = *(int64_t*)(array + 16 + index * 8);
    return result;
}

// Typed load: adesh_rt_load_typed(ptr, index) -> element value (for pointer-based array indexing)
int64_t adesh_rt_load_typed(void* array_ptr, int64_t index) {
    if (!array_ptr) return 0;
    
    char* array = (char*)array_ptr;
    int64_t length = *(int64_t*)array;
    
    // Support negative indexing
    if (index < 0) {
        index = length + index;
    }
    
    // Bounds check
    if (index < 0 || index >= length) {
        return 0;
    }
    
    // Get element at offset 16 (metadata) + index * 8 (8-byte elements)
    int64_t result = *(int64_t*)(array + 16 + index * 8);
    return result;
}

// Typed store: adesh_rt_store_typed(ptr, index, value) -> 1 on success, 0 on failure/bounds error
int64_t adesh_rt_store_typed(void* array_ptr, int64_t index, int64_t value) {
    if (!array_ptr) return 0;
    
    char* array = (char*)array_ptr;
    int64_t length = *(int64_t*)array;
    
    // Support negative indexing
    if (index < 0) {
        index = length + index;
    }
    
    // Bounds check
    if (index < 0 || index >= length) {
        return 0;
    }
    
    *(int64_t*)(array + 16 + index * 8) = value;
    return 1;
}
