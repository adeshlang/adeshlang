//! Phase 9 Compiler Query Architecture.
//!
//! Provides a query-based compiler middle-end:
//! - Deterministic query memoization and caching
//! - Automatic dependency tracking between query nodes
//! - Cache invalidation and dirty marking
//! - Comprehensive query statistics (hits, misses, evictions)

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

/// Identifier for a query target module or function.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct QueryKey {
    pub kind: QueryKind,
    pub target: String,
}

/// Types of queries supported by the compiler query engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum QueryKind {
    Parse,
    Resolve,
    TypeCheck,
    LowerHir,
    LowerMir,
    Optimize,
    GenerateMachineIr,
    EmitObject,
}

/// Generic query output value wrapper.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueryResult {
    pub key: QueryKey,
    pub payload: String,
    pub fingerprint: u64,
    pub dependencies: Vec<QueryKey>,
}

/// Statistics for the query engine execution.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct QueryStats {
    pub total_queries: usize,
    pub cache_hits: usize,
    pub cache_misses: usize,
    pub invalidations: usize,
}

/// Query Engine for compiler memoization.
pub struct QueryEngine {
    cache: Mutex<HashMap<QueryKey, QueryResult>>,
    dep_graph: Mutex<HashMap<QueryKey, HashSet<QueryKey>>>, // key -> dependents
    stats: Mutex<QueryStats>,
}

impl QueryEngine {
    pub fn new() -> Self {
        Self {
            cache: Mutex::new(HashMap::new()),
            dep_graph: Mutex::new(HashMap::new()),
            stats: Mutex::new(QueryStats::default()),
        }
    }

    /// Execute a query with memoization. If cached and valid, returns cached result.
    /// Otherwise executes `computer`, caches result, records dependencies, and returns it.
    pub fn query<F>(&self, key: QueryKey, computer: F) -> QueryResult
    where
        F: FnOnce(&QueryKey) -> QueryResult,
    {
        {
            let mut stats = self.stats.lock().unwrap();
            stats.total_queries += 1;
        }

        {
            let cache = self.cache.lock().unwrap();
            if let Some(res) = cache.get(&key) {
                let mut stats = self.stats.lock().unwrap();
                stats.cache_hits += 1;
                return res.clone();
            }
        }

        // Cache miss
        {
            let mut stats = self.stats.lock().unwrap();
            stats.cache_misses += 1;
        }

        let result = computer(&key);

        // Record dependencies in dep_graph
        {
            let mut dep_graph = self.dep_graph.lock().unwrap();
            for dep in &result.dependencies {
                dep_graph
                    .entry(dep.clone())
                    .or_default()
                    .insert(key.clone());
            }
        }

        // Store in cache
        {
            let mut cache = self.cache.lock().unwrap();
            cache.insert(key, result.clone());
        }

        result
    }

    /// Invalidate a key and all dependent queries recursively.
    pub fn invalidate(&self, key: &QueryKey) -> usize {
        let mut invalidated = 0;
        let mut to_invalidate = vec![key.clone()];
        let mut visited = HashSet::new();

        while let Some(current) = to_invalidate.pop() {
            if visited.contains(&current) {
                continue;
            }
            visited.insert(current.clone());

            // Remove from cache
            {
                let mut cache = self.cache.lock().unwrap();
                if cache.remove(&current).is_some() {
                    invalidated += 1;
                }
            }

            // Find dependents
            {
                let dep_graph = self.dep_graph.lock().unwrap();
                if let Some(dependents) = dep_graph.get(&current) {
                    for dep in dependents {
                        to_invalidate.push(dep.clone());
                    }
                }
            }
        }

        let mut stats = self.stats.lock().unwrap();
        stats.invalidations += invalidated;
        invalidated
    }

    /// Retrieve current statistics.
    pub fn stats(&self) -> QueryStats {
        self.stats.lock().unwrap().clone()
    }

    /// Clear all cached query results.
    pub fn clear(&self) {
        self.cache.lock().unwrap().clear();
        self.dep_graph.lock().unwrap().clear();
    }
}
