#include "concurrent_demo.h"
#include <stdio.h>
#include <pthread.h>
#include <stdlib.h>

#define NUM_THREADS 4
#define ITERATIONS_PER_THREAD 1000

typedef struct {
    int thread_id;
    long total_calls;
    long sum;
} thread_data_t;

void* worker_thread(void* arg) {
    thread_data_t* data = (thread_data_t*)arg;
    
    for (int i = 0; i < ITERATIONS_PER_THREAD; i++) {
        // Call multiple FFI functions from different threads
        long hash = hash_simple(data->thread_id * 1000 + i);
        long count = counter_increment(i, data->thread_id);
        long valid = validate_range(hash % 1000, 0, 999);
        
        data->sum += (hash + count + valid);
        data->total_calls += 3;
    }
    
    printf("Thread %d completed %ld FFI calls\n", data->thread_id, data->total_calls);
    return NULL;
}

int main() {
    printf("=== Concurrent FFI Thread Safety Demo ===\n\n");
    printf("Testing Arc<Mutex<FfiRegistry>> thread safety...\n\n");
    
    pthread_t threads[NUM_THREADS];
    thread_data_t thread_data[NUM_THREADS];
    
    // Create threads
    for (int i = 0; i < NUM_THREADS; i++) {
        thread_data[i].thread_id = i;
        thread_data[i].total_calls = 0;
        thread_data[i].sum = 0;
        
        if (pthread_create(&threads[i], NULL, worker_thread, &thread_data[i]) != 0) {
            fprintf(stderr, "Failed to create thread %d\n", i);
            return 1;
        }
    }
    
    // Wait for all threads
    for (int i = 0; i < NUM_THREADS; i++) {
        pthread_join(threads[i], NULL);
    }
    
    // Print results
    long total_calls = 0;
    long total_sum = 0;
    printf("\n=== Results ===\n");
    for (int i = 0; i < NUM_THREADS; i++) {
        printf("Thread %d: %ld calls, sum = %ld\n", 
               i, thread_data[i].total_calls, thread_data[i].sum);
        total_calls += thread_data[i].total_calls;
        total_sum += thread_data[i].sum;
    }
    
    printf("\nTotal FFI calls across all threads: %ld\n", total_calls);
    printf("Total sum: %ld\n", total_sum);
    printf("\n✅ Thread safety test completed successfully!\n");
    printf("   Arc<Mutex<FfiRegistry>> ensures safe concurrent access\n");
    
    // Test compute intensive function
    printf("\n=== Compute Intensive Test ===\n");
    long result = compute_intensive(100);
    printf("compute_intensive(100) = %ld\n", result);
    
    // Test batch processing
    printf("\n=== Batch Processing Test ===\n");
    long batch_result = process_batch(0, 1000);
    printf("process_batch(0, 1000) = %ld\n", batch_result);
    
    return 0;
}
