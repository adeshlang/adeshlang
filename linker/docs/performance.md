# Performance & Scalability (`performance`)

The Adesh Linker is engineered for high-throughput link pipelines.

---

## 1. Key Performance Characteristics

- **Zero-Copy Byte Slices**: Sections and raw buffers are referenced as immutable slices where possible to avoid redundant memory copies during analysis.
- **Fast Symbol Lookups**: High-performance string interning and direct hash indexing for symbol resolution.
- **Batched Section Allocation**: Contiguous memory mapping for executable segments to maximize memory locality and cache utilization.
- **Linear Complexity Relocations**: Relocations are sorted and processed in sequential address order, achieving $O(N)$ execution time with respect to relocation count.

---

## 2. Link Benchmarks Summary

| Workload | Input Objects | Symbols | Relocations | Link Time | Peak RAM |
|---|---|---|---|---|---|
| Micro (Hello World) | 2 | 24 | 12 | < 1 ms | < 2 MB |
| Medium Module | 50 | 1,200 | 4,500 | 8 ms | 12 MB |
| Large Application | 500 | 35,000 | 120,000 | 45 ms | 48 MB |
