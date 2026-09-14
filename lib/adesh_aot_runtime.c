/*
 * AdeshLang AOT Runtime - Memory Tracking
 * 
 * Provides RAII-based memory tracking for AOT-compiled AdeshLang code.
 * These functions are called from Cranelift-generated native code to
 * track allocations and provide debug-mode safety checks.
 *
 * Compile with:
 *   gcc -c -O2 adesh_aot_runtime.c -o adesh_aot_runtime.o
 *   ar rcs libadesh_aot_runtime.a adesh_aot_runtime.o
 *
 * Link with AOT-compiled code:
 *   gcc main.o -L. -ladesh_aot_runtime -o main
 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>

/* Allocation metadata structure (must match Rust layout) */
typedef struct {
    uint64_t id;
    uint64_t size;
    uint64_t elem_size;
    uint64_t scope_id;
    const char* source_file;
    uint32_t source_line;
    uint32_t source_column;
    uint8_t is_freed;
} RuntimeAllocationMetadata;

/* Global allocation tracking (only enabled in debug mode) */
#ifdef ADESH_DEBUG_MEMORY

#define MAX_ALLOCATIONS 10000

typedef struct {
    void* ptr;
    RuntimeAllocationMetadata metadata;
} AllocationRecord;

static AllocationRecord g_allocations[MAX_ALLOCATIONS];
static int g_allocation_count = 0;

/* Find allocation record by pointer */
static AllocationRecord* find_allocation(void* ptr) {
    for (int i = 0; i < g_allocation_count; i++) {
        if (g_allocations[i].ptr == ptr) {
            return &g_allocations[i];
        }
    }
    return NULL;
}

/* Register new allocation */
static void register_allocation(void* ptr, RuntimeAllocationMetadata* metadata) {
    if (g_allocation_count >= MAX_ALLOCATIONS) {
        fprintf(stderr, "ERROR: Too many allocations (max %d)\n", MAX_ALLOCATIONS);
        abort();
    }
    
    g_allocations[g_allocation_count].ptr = ptr;
    g_allocations[g_allocation_count].metadata = *metadata;
    g_allocations[g_allocation_count].metadata.is_freed = 0;
    g_allocation_count++;
}

/* Unregister allocation */
static void unregister_allocation(void* ptr) {
    for (int i = 0; i < g_allocation_count; i++) {
        if (g_allocations[i].ptr == ptr) {
            /* Shift remaining allocations */
            for (int j = i; j < g_allocation_count - 1; j++) {
                g_allocations[j] = g_allocations[j + 1];
            }
            g_allocation_count--;
            return;
        }
    }
}

#endif /* ADESH_DEBUG_MEMORY */

/*
 * Allocate tracked memory
 * Returns pointer to allocated memory
 */
void* adesh_rt_alloc_tracked(uint64_t size, RuntimeAllocationMetadata* metadata) {
    void* ptr = malloc(size);
    
    if (ptr == NULL) {
        fprintf(stderr, "ERROR: malloc(%llu) failed at %s:%u:%u\n",
                (unsigned long long)size,
                metadata->source_file,
                metadata->source_line,
                metadata->source_column);
        abort();
    }
    
#ifdef ADESH_DEBUG_MEMORY
    register_allocation(ptr, metadata);
    
    if (getenv("ADESH_TRACE_ALLOC")) {
        printf("ALLOC[%llu]: %p size=%llu at %s:%u:%u\n",
               (unsigned long long)metadata->id,
               ptr,
               (unsigned long long)size,
               metadata->source_file,
               metadata->source_line,
               metadata->source_column);
    }
#endif
    
    return ptr;
}

/*
 * Free tracked memory
 * Checks for double-free and use-after-free in debug mode
 */
void adesh_rt_free_tracked(void* ptr, RuntimeAllocationMetadata* metadata) {
    if (ptr == NULL) {
        return; /* Freeing NULL is safe */
    }
    
#ifdef ADESH_DEBUG_MEMORY
    AllocationRecord* record = find_allocation(ptr);
    
    if (record == NULL) {
        fprintf(stderr, "ERROR: Invalid free at %s:%u:%u\n",
                metadata->source_file,
                metadata->source_line,
                metadata->source_column);
        fprintf(stderr, "       Pointer %p was not allocated by adesh_rt_alloc_tracked\n", ptr);
        abort();
    }
    
    if (record->metadata.is_freed) {
        fprintf(stderr, "ERROR: Double free at %s:%u:%u\n",
                metadata->source_file,
                metadata->source_line,
                metadata->source_column);
        fprintf(stderr, "       Allocation[%llu] was already freed\n",
                (unsigned long long)record->metadata.id);
        abort();
    }
    
    if (getenv("ADESH_TRACE_ALLOC")) {
        printf("FREE[%llu]: %p at %s:%u:%u\n",
               (unsigned long long)record->metadata.id,
               ptr,
               metadata->source_file,
               metadata->source_line,
               metadata->source_column);
    }
    
    record->metadata.is_freed = 1;
    unregister_allocation(ptr);
#endif
    
    free(ptr);
}

/*
 * Cleanup all allocations in a scope (RAII)
 * Called automatically at scope exit
 */
void adesh_rt_scope_exit(uint64_t scope_id) {
#ifdef ADESH_DEBUG_MEMORY
    if (getenv("ADESH_TRACE_ALLOC")) {
        printf("SCOPE_EXIT[%llu]\n", (unsigned long long)scope_id);
    }
    
    /* In debug mode, check for leaks in this scope */
    int leaks = 0;
    for (int i = 0; i < g_allocation_count; i++) {
        if (g_allocations[i].metadata.scope_id == scope_id && 
            !g_allocations[i].metadata.is_freed) {
            if (leaks == 0) {
                fprintf(stderr, "WARNING: Memory leaks in scope %llu:\n",
                        (unsigned long long)scope_id);
            }
            fprintf(stderr, "  Allocation[%llu]: %llu bytes at %s:%u:%u\n",
                    (unsigned long long)g_allocations[i].metadata.id,
                    (unsigned long long)g_allocations[i].metadata.size,
                    g_allocations[i].metadata.source_file,
                    g_allocations[i].metadata.source_line,
                    g_allocations[i].metadata.source_column);
            leaks++;
        }
    }
#endif
}

/*
 * Validate pointer before use (debug mode)
 * Returns 1 if valid, 0 if invalid
 */
int adesh_rt_validate_ptr(void* ptr, RuntimeAllocationMetadata* metadata) {
#ifdef ADESH_DEBUG_MEMORY
    if (ptr == NULL) {
        fprintf(stderr, "ERROR: NULL pointer dereference at %s:%u:%u\n",
                metadata->source_file,
                metadata->source_line,
                metadata->source_column);
        return 0;
    }
    
    AllocationRecord* record = find_allocation(ptr);
    
    if (record == NULL) {
        fprintf(stderr, "ERROR: Invalid pointer dereference at %s:%u:%u\n",
                metadata->source_file,
                metadata->source_line,
                metadata->source_column);
        fprintf(stderr, "       Pointer %p was not allocated\n", ptr);
        return 0;
    }
    
    if (record->metadata.is_freed) {
        fprintf(stderr, "ERROR: Use-after-free at %s:%u:%u\n",
                metadata->source_file,
                metadata->source_line,
                metadata->source_column);
        fprintf(stderr, "       Allocation[%llu] was already freed\n",
                (unsigned long long)record->metadata.id);
        return 0;
    }
    
    return 1;
#else
    /* In release mode, assume all pointers are valid (compile-time checked) */
    return 1;
#endif
}

/*
 * Print memory usage statistics
 * Useful for debugging and profiling
 */
void adesh_rt_print_memory_stats(void) {
#ifdef ADESH_DEBUG_MEMORY
    printf("\n=== AdeshLang Memory Statistics ===\n");
    printf("Active allocations: %d\n", g_allocation_count);
    
    uint64_t total_bytes = 0;
    for (int i = 0; i < g_allocation_count; i++) {
        total_bytes += g_allocations[i].metadata.size;
    }
    
    printf("Total memory: %llu bytes\n", (unsigned long long)total_bytes);
    
    if (g_allocation_count > 0) {
        printf("\nActive allocations:\n");
        for (int i = 0; i < g_allocation_count; i++) {
            printf("  [%llu] %p: %llu bytes at %s:%u:%u\n",
                   (unsigned long long)g_allocations[i].metadata.id,
                   g_allocations[i].ptr,
                   (unsigned long long)g_allocations[i].metadata.size,
                   g_allocations[i].metadata.source_file,
                   g_allocations[i].metadata.source_line,
                   g_allocations[i].metadata.source_column);
        }
    }
    printf("===================================\n\n");
#else
    printf("Memory tracking disabled (compile with -DADESH_DEBUG_MEMORY)\n");
#endif
}
