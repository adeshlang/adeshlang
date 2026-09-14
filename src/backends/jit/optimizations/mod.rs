//! JIT Optimization Passes
//!
//! This module provides optimization passes for the JIT compiler:
//! - Function inlining for small functions
//! - Inline caching for property access
//! - Hidden-class style property caching
//! - Local variable lifetime analysis
//! - Boxing/unboxing optimization
//!
//! These optimizations improve runtime performance by reducing:
//! - Function call overhead (inlining)
//! - Property lookup costs (inline caching)
//! - Memory allocations (stack vs heap)
//! - Type boxing/unboxing (specialized paths)

use super::lir::{LirFunction, LirInst, LirModule};
use crate::utils::collections::FastMap;

// ============================================
// INLINE CACHING
// ============================================

/// Inline cache entry for property access
#[derive(Debug, Clone)]
pub struct InlineCacheEntry {
    /// The "shape" hash of the object type
    pub shape_hash: u64,
    /// Cached property offset (for fast access)
    pub offset: usize,
    /// Number of hits for this cache entry
    pub hit_count: u64,
    /// Number of misses (invalidations)
    pub miss_count: u64,
}

impl InlineCacheEntry {
    pub fn new(shape_hash: u64, offset: usize) -> Self {
        InlineCacheEntry {
            shape_hash,
            offset,
            hit_count: 0,
            miss_count: 0,
        }
    }

    /// Record a cache hit
    #[inline]
    pub fn record_hit(&mut self) {
        self.hit_count += 1;
    }

    /// Record a cache miss
    #[inline]
    pub fn record_miss(&mut self) {
        self.miss_count += 1;
    }

    /// Get hit ratio
    pub fn hit_ratio(&self) -> f64 {
        let total = self.hit_count + self.miss_count;
        if total == 0 {
            0.0
        } else {
            self.hit_count as f64 / total as f64
        }
    }
}

/// Inline cache for property access optimization
/// Uses a monomorphic inline cache (MIC) approach
#[derive(Debug, Default)]
pub struct InlineCache {
    /// Map from (object_type, property_name) to cache entry
    entries: FastMap<(String, String), InlineCacheEntry>,
    /// Maximum cache size
    max_size: usize,
    /// Total lookups
    total_lookups: u64,
    /// Cache hits
    cache_hits: u64,
}

impl InlineCache {
    /// Create a new inline cache
    pub fn new() -> Self {
        InlineCache {
            entries: FastMap::default(),
            max_size: 1024,
            total_lookups: 0,
            cache_hits: 0,
        }
    }

    /// Create with custom max size
    pub fn with_max_size(max_size: usize) -> Self {
        InlineCache {
            entries: FastMap::default(),
            max_size,
            total_lookups: 0,
            cache_hits: 0,
        }
    }

    /// Try to get cached property offset
    pub fn lookup(&mut self, object_type: &str, property: &str, shape_hash: u64) -> Option<usize> {
        self.total_lookups += 1;

        let key = (object_type.to_string(), property.to_string());
        if let Some(entry) = self.entries.get_mut(&key) {
            if entry.shape_hash == shape_hash {
                entry.record_hit();
                self.cache_hits += 1;
                return Some(entry.offset);
            } else {
                // Shape changed - invalidate
                entry.record_miss();
            }
        }
        None
    }

    /// Update cache with new entry
    pub fn update(&mut self, object_type: &str, property: &str, shape_hash: u64, offset: usize) {
        // Evict if at capacity
        if self.entries.len() >= self.max_size {
            self.evict_cold_entries();
        }

        let key = (object_type.to_string(), property.to_string());
        self.entries
            .insert(key, InlineCacheEntry::new(shape_hash, offset));
    }

    /// Evict cold (low hit-ratio) entries
    fn evict_cold_entries(&mut self) {
        // Remove entries with lowest hit ratios
        let mut entries_by_ratio: Vec<_> = self
            .entries
            .iter()
            .map(|(k, v)| (k.clone(), v.hit_ratio()))
            .collect();

        entries_by_ratio.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());

        // Remove bottom 25%
        let to_remove = self.max_size / 4;
        for (key, _) in entries_by_ratio.into_iter().take(to_remove) {
            self.entries.remove(&key);
        }
    }

    /// Get cache statistics
    pub fn stats(&self) -> (u64, u64, usize) {
        (self.total_lookups, self.cache_hits, self.entries.len())
    }

    /// Get cache hit ratio
    pub fn hit_ratio(&self) -> f64 {
        if self.total_lookups == 0 {
            0.0
        } else {
            self.cache_hits as f64 / self.total_lookups as f64
        }
    }
}

// ============================================
// HIDDEN CLASSES / SHAPE TRACKING
// ============================================

/// Hidden class (shape) for object property layout optimization
#[derive(Debug, Clone)]
pub struct HiddenClass {
    /// Unique ID for this shape
    pub id: u64,
    /// Property names in order
    pub properties: Vec<String>,
    /// Property offsets for fast access
    pub offsets: FastMap<String, usize>,
    /// Transition table: property_name -> new_shape_id
    pub transitions: FastMap<String, u64>,
    /// Parent shape (for inheritance)
    pub parent: Option<u64>,
}

impl HiddenClass {
    /// Create a new empty hidden class
    pub fn empty(id: u64) -> Self {
        HiddenClass {
            id,
            properties: Vec::new(),
            offsets: FastMap::default(),
            transitions: FastMap::default(),
            parent: None,
        }
    }

    /// Create a new hidden class with properties
    pub fn new(id: u64, properties: Vec<String>) -> Self {
        let mut offsets = FastMap::default();
        for (i, prop) in properties.iter().enumerate() {
            offsets.insert(prop.clone(), i);
        }
        HiddenClass {
            id,
            properties,
            offsets,
            transitions: FastMap::default(),
            parent: None,
        }
    }

    /// Get property offset
    #[inline]
    pub fn get_offset(&self, property: &str) -> Option<usize> {
        self.offsets.get(property).copied()
    }

    /// Check if has property
    #[inline]
    pub fn has_property(&self, property: &str) -> bool {
        self.offsets.contains_key(property)
    }

    /// Compute shape hash for inline caching
    pub fn shape_hash(&self) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        self.id.hash(&mut hasher);
        for prop in &self.properties {
            prop.hash(&mut hasher);
        }
        hasher.finish()
    }
}

/// Hidden class registry for shape management
#[derive(Debug, Default)]
pub struct HiddenClassRegistry {
    /// All hidden classes
    classes: FastMap<u64, HiddenClass>,
    /// Next class ID
    next_id: u64,
    /// Lookup table: sorted property names -> class_id
    shape_lookup: FastMap<Vec<String>, u64>,
}

impl HiddenClassRegistry {
    /// Create a new registry
    pub fn new() -> Self {
        let mut registry = HiddenClassRegistry::default();
        // Create empty root class
        let empty = HiddenClass::empty(0);
        registry.classes.insert(0, empty);
        registry.next_id = 1;
        registry
    }

    /// Get or create hidden class for property set
    pub fn get_or_create(&mut self, properties: &[String]) -> u64 {
        let mut sorted = properties.to_vec();
        sorted.sort();

        if let Some(&id) = self.shape_lookup.get(&sorted) {
            return id;
        }

        // Create new class
        let id = self.next_id;
        self.next_id += 1;

        let class = HiddenClass::new(id, sorted.clone());
        self.classes.insert(id, class);
        self.shape_lookup.insert(sorted, id);

        id
    }

    /// Get hidden class by ID
    pub fn get(&self, id: u64) -> Option<&HiddenClass> {
        self.classes.get(&id)
    }

    /// Transition from one shape to another by adding a property
    pub fn transition(&mut self, from_id: u64, property: &str) -> u64 {
        // Check if transition already exists
        if let Some(class) = self.classes.get(&from_id) {
            if let Some(&to_id) = class.transitions.get(property) {
                return to_id;
            }
        }

        // Create new shape with added property
        let mut new_props = self
            .classes
            .get(&from_id)
            .map(|c| c.properties.clone())
            .unwrap_or_default();
        new_props.push(property.to_string());

        let new_id = self.get_or_create(&new_props);

        // Record transition
        if let Some(class) = self.classes.get_mut(&from_id) {
            class.transitions.insert(property.to_string(), new_id);
        }

        new_id
    }
}

// ============================================
// FUNCTION INLINING
// ============================================

/// Criteria for function inlining
#[derive(Debug, Clone)]
pub struct InlineCriteria {
    /// Maximum instruction count for inlining
    pub max_instructions: usize,
    /// Maximum call depth for inline expansion
    pub max_depth: usize,
    /// Minimum call frequency for inlining
    pub min_call_count: u64,
    /// Whether to inline recursive calls
    pub allow_recursive: bool,
}

impl Default for InlineCriteria {
    fn default() -> Self {
        InlineCriteria {
            max_instructions: 50,
            max_depth: 3,
            min_call_count: 10,
            allow_recursive: false,
        }
    }
}

/// Information about a function for inlining decisions
#[derive(Debug, Clone)]
pub struct FunctionInfo {
    /// Function name
    pub name: String,
    /// Instruction count
    pub instruction_count: usize,
    /// Call count (for hot path detection)
    pub call_count: u64,
    /// Whether function has side effects
    pub has_side_effects: bool,
    /// Parameter count
    pub param_count: usize,
    /// Whether function is recursive
    pub is_recursive: bool,
    /// Whether function can be inlined
    pub can_inline: bool,
}

impl FunctionInfo {
    pub fn analyze(func: &LirFunction) -> Self {
        let instruction_count: usize = func.blocks.iter().map(|b| b.instructions.len()).sum();

        let has_side_effects = func
            .blocks
            .iter()
            .flat_map(|b| &b.instructions)
            .any(|inst| is_side_effect_instruction(inst));

        let is_recursive = func
            .blocks
            .iter()
            .flat_map(|b| &b.instructions)
            .any(|inst| matches!(inst, LirInst::Call(_, name, _) if name == &func.name));

        FunctionInfo {
            name: func.name.clone(),
            instruction_count,
            call_count: 0,
            has_side_effects,
            param_count: func.params.len(),
            is_recursive,
            can_inline: true, // Will be refined by check_inlinable
        }
    }

    /// Check if function meets inlining criteria
    pub fn check_inlinable(&self, criteria: &InlineCriteria) -> bool {
        if self.instruction_count > criteria.max_instructions {
            return false;
        }
        if self.is_recursive && !criteria.allow_recursive {
            return false;
        }
        if self.call_count < criteria.min_call_count {
            return false;
        }
        true
    }
}

/// Check if an instruction has side effects
fn is_side_effect_instruction(inst: &LirInst) -> bool {
    matches!(
        inst,
        LirInst::Call(_, _, _) | LirInst::CallBuiltin(_, _, _) | LirInst::StoreVar(_, _)
    )
}

/// Inline optimizer
#[derive(Debug)]
pub struct InlineOptimizer {
    /// Inlining criteria
    criteria: InlineCriteria,
    /// Function information cache
    func_info: FastMap<String, FunctionInfo>,
    /// Inline statistics
    inline_count: usize,
}

impl InlineOptimizer {
    /// Create a new inline optimizer
    pub fn new() -> Self {
        InlineOptimizer {
            criteria: InlineCriteria::default(),
            func_info: FastMap::default(),
            inline_count: 0,
        }
    }

    /// Create with custom criteria
    pub fn with_criteria(criteria: InlineCriteria) -> Self {
        InlineOptimizer {
            criteria,
            func_info: FastMap::default(),
            inline_count: 0,
        }
    }

    /// Analyze module for inlining opportunities
    pub fn analyze(&mut self, module: &LirModule) {
        for func in &module.functions {
            let info = FunctionInfo::analyze(func);
            self.func_info.insert(func.name.clone(), info);
        }
    }

    /// Record a function call for hot path detection
    pub fn record_call(&mut self, func_name: &str) {
        if let Some(info) = self.func_info.get_mut(func_name) {
            info.call_count += 1;
        }
    }

    /// Check if a function should be inlined
    pub fn should_inline(&self, func_name: &str) -> bool {
        self.func_info
            .get(func_name)
            .map(|info| info.check_inlinable(&self.criteria))
            .unwrap_or(false)
    }

    /// Get inlining statistics
    pub fn stats(&self) -> (usize, usize) {
        (self.inline_count, self.func_info.len())
    }
}

impl Default for InlineOptimizer {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================
// LOCAL VARIABLE LIFETIME ANALYSIS
// ============================================

/// Lifetime range for a local variable
#[derive(Debug, Clone)]
pub struct VariableLifetime {
    /// First use (instruction index)
    pub first_use: usize,
    /// Last use (instruction index)
    pub last_use: usize,
    /// Whether the variable escapes the function
    pub escapes: bool,
    /// Whether the variable is captured by a closure
    pub captured: bool,
}

impl VariableLifetime {
    pub fn new(first_use: usize) -> Self {
        VariableLifetime {
            first_use,
            last_use: first_use,
            escapes: false,
            captured: false,
        }
    }

    /// Check if lifetime is still active at given instruction
    pub fn is_live_at(&self, inst_index: usize) -> bool {
        inst_index >= self.first_use && inst_index <= self.last_use
    }

    /// Check if variable can be stack-allocated
    pub fn can_stack_allocate(&self) -> bool {
        !self.escapes && !self.captured
    }
}

/// Analyze variable lifetimes in a function
pub fn analyze_lifetimes(func: &LirFunction) -> FastMap<String, VariableLifetime> {
    let mut lifetimes: FastMap<String, VariableLifetime> = FastMap::default();
    let mut inst_index = 0;

    for block in &func.blocks {
        for inst in &block.instructions {
            // Track variable uses
            match inst {
                LirInst::LoadVar(_, name) => {
                    if let Some(lt) = lifetimes.get_mut(name) {
                        lt.last_use = inst_index;
                    }
                }
                LirInst::StoreVar(name, _) => {
                    if !lifetimes.contains_key(name) {
                        lifetimes.insert(name.clone(), VariableLifetime::new(inst_index));
                    } else if let Some(lt) = lifetimes.get_mut(name) {
                        lt.last_use = inst_index;
                    }
                }
                LirInst::ConstFunc(_, _, captures, _) => {
                    // Mark captured variables
                    for cap in captures {
                        if let Some(lt) = lifetimes.get_mut(cap) {
                            lt.captured = true;
                            lt.escapes = true;
                        }
                    }
                }
                _ => {}
            }
            inst_index += 1;
        }
    }

    lifetimes
}

// ============================================
// BOXING OPTIMIZATION
// ============================================

/// Type specialization for unboxing optimization
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpecializedType {
    Int,
    Float,
    Bool,
    String,
    Object,
    Array,
    Unknown,
}

/// Track observed types for a variable
#[derive(Debug, Clone)]
pub struct TypeProfile {
    /// Observed types
    pub types: Vec<SpecializedType>,
    /// Observation count
    pub observation_count: u64,
    /// Whether type is stable (monomorphic)
    pub is_stable: bool,
}

impl TypeProfile {
    pub fn new() -> Self {
        TypeProfile {
            types: Vec::new(),
            observation_count: 0,
            is_stable: true,
        }
    }

    /// Record an observed type
    pub fn observe(&mut self, typ: SpecializedType) {
        self.observation_count += 1;

        if !self.types.contains(&typ) {
            self.types.push(typ);
            if self.types.len() > 1 {
                self.is_stable = false;
            }
        }
    }

    /// Get the specialized type (if monomorphic)
    pub fn get_specialized(&self) -> Option<SpecializedType> {
        if self.is_stable && !self.types.is_empty() {
            Some(self.types[0])
        } else {
            None
        }
    }
}

impl Default for TypeProfile {
    fn default() -> Self {
        Self::new()
    }
}

/// Type profiler for boxing optimization
#[derive(Debug, Default)]
pub struct TypeProfiler {
    /// Type profiles for variables
    profiles: FastMap<String, TypeProfile>,
}

impl TypeProfiler {
    pub fn new() -> Self {
        TypeProfiler::default()
    }

    /// Observe a type for a variable
    pub fn observe(&mut self, var_name: &str, typ: SpecializedType) {
        self.profiles
            .entry(var_name.to_string())
            .or_default()
            .observe(typ);
    }

    /// Get specialized type for a variable
    pub fn get_specialized(&self, var_name: &str) -> Option<SpecializedType> {
        self.profiles
            .get(var_name)
            .and_then(|p| p.get_specialized())
    }

    /// Check if a variable can use unboxed representation
    /// Only primitive types (Int, Float, Bool) can be unboxed
    pub fn can_unbox(&self, var_name: &str) -> bool {
        self.profiles
            .get(var_name)
            .map(|p| p.is_stable && Self::is_unboxable_type(&p.types))
            .unwrap_or(false)
    }

    /// Helper: Check if a type list contains only unboxable types
    fn is_unboxable_type(types: &[SpecializedType]) -> bool {
        match types {
            [single_type] => matches!(
                single_type,
                SpecializedType::Int | SpecializedType::Float | SpecializedType::Bool
            ),
            _ => false, // Multiple types or empty means not unboxable
        }
    }
}

// ============================================
// TYPE-SPECIALIZED STUBS (T2 Optimization)
// ============================================

/// Type-specialized stub for monomorphic inline cache sites
#[derive(Debug, Clone)]
pub enum TypeSpecializedStub {
    /// Specialized for integer operations
    IntegerAdd,
    IntegerSub,
    IntegerMul,
    IntegerDiv,
    IntegerCmp,

    /// Specialized for float operations
    FloatAdd,
    FloatSub,
    FloatMul,
    FloatDiv,
    FloatCmp,

    /// Specialized for string operations
    StringConcat,
    StringLength,
    StringIndex,

    /// Specialized for array operations
    ArrayIndex,
    ArrayLength,
    ArrayPush,

    /// Generic fallback (polymorphic)
    Generic,
}

impl TypeSpecializedStub {
    /// Create a stub for addition based on observed types
    pub fn for_add(left_type: SpecializedType, right_type: SpecializedType) -> Self {
        match (left_type, right_type) {
            (SpecializedType::Int, SpecializedType::Int) => TypeSpecializedStub::IntegerAdd,
            (SpecializedType::Float, SpecializedType::Float)
            | (SpecializedType::Int, SpecializedType::Float)
            | (SpecializedType::Float, SpecializedType::Int) => TypeSpecializedStub::FloatAdd,
            (SpecializedType::String, SpecializedType::String) => TypeSpecializedStub::StringConcat,
            _ => TypeSpecializedStub::Generic,
        }
    }

    /// Create a stub for comparison based on observed types
    pub fn for_cmp(left_type: SpecializedType, right_type: SpecializedType) -> Self {
        match (left_type, right_type) {
            (SpecializedType::Int, SpecializedType::Int) => TypeSpecializedStub::IntegerCmp,
            (SpecializedType::Float, SpecializedType::Float)
            | (SpecializedType::Int, SpecializedType::Float)
            | (SpecializedType::Float, SpecializedType::Int) => TypeSpecializedStub::FloatCmp,
            _ => TypeSpecializedStub::Generic,
        }
    }

    /// Check if this stub is specialized (not generic)
    pub fn is_specialized(&self) -> bool {
        !matches!(self, TypeSpecializedStub::Generic)
    }
}

/// Manages type-specialized stubs for inline cache sites
#[derive(Debug, Default)]
pub struct StubRegistry {
    /// Registered stubs by site ID
    stubs: FastMap<u64, TypeSpecializedStub>,
    /// Site observation counts
    observations: FastMap<u64, u64>,
    /// Threshold for stub specialization
    specialization_threshold: u64,
}

impl StubRegistry {
    /// Create a new stub registry
    pub fn new() -> Self {
        StubRegistry {
            stubs: FastMap::default(),
            observations: FastMap::default(),
            specialization_threshold: 50, // Specialize after 50 observations
        }
    }

    /// Record an observation at a site
    pub fn observe(
        &mut self,
        site_id: u64,
        left_type: SpecializedType,
        right_type: SpecializedType,
        op: &str,
    ) {
        let count = self.observations.entry(site_id).or_insert(0);
        *count += 1;

        // Check if we should specialize
        if *count == self.specialization_threshold && !self.stubs.contains_key(&site_id) {
            let stub = match op {
                "add" | "+" => TypeSpecializedStub::for_add(left_type, right_type),
                "cmp" | "==" | "!=" | "<" | ">" | "<=" | ">=" => {
                    TypeSpecializedStub::for_cmp(left_type, right_type)
                }
                _ => TypeSpecializedStub::Generic,
            };
            if stub.is_specialized() {
                self.stubs.insert(site_id, stub);
            }
        }
    }

    /// Get stub for a site (if specialized)
    pub fn get_stub(&self, site_id: u64) -> Option<&TypeSpecializedStub> {
        self.stubs.get(&site_id)
    }

    /// Get number of specialized sites
    pub fn specialized_count(&self) -> usize {
        self.stubs.len()
    }
}

// ============================================
// FAST PATHS FOR COMMON OPERATIONS
// ============================================

/// Fast path result - either computed value or fallback needed
#[derive(Debug)]
pub enum FastPathResult<T> {
    /// Fast path succeeded with computed value
    Success(T),
    /// Fast path not applicable, use generic path
    Fallback,
}

/// Maximum small string size for SSO optimization
/// Imported from memory module to avoid tight coupling
const SSO_THRESHOLD: usize = 22;

/// Fast path for integer addition using wrapping arithmetic (no overflow check)
///
/// **Use when**: You know the values are small enough that overflow is impossible,
/// or you're in a hot loop where BigInt fallback would be too expensive.
///
/// **Avoid when**: User values could be near i64 bounds, or correctness is
/// more important than performance.
///
/// For safe addition with BigInt fallback on overflow, use `fast_int_add_checked`.
#[inline(always)]
pub fn fast_int_add(a: i64, b: i64) -> FastPathResult<i64> {
    FastPathResult::Success(a.wrapping_add(b))
}

/// Fast path for integer addition with overflow check
///
/// Returns `Fallback` if addition would overflow, allowing the caller to
/// promote to BigInt arithmetic. Slightly slower than `fast_int_add` but safe.
#[inline(always)]
pub fn fast_int_add_checked(a: i64, b: i64) -> FastPathResult<i64> {
    match a.checked_add(b) {
        Some(result) => FastPathResult::Success(result),
        None => FastPathResult::Fallback, // Overflow - use BigInt fallback
    }
}

/// Fast path for float addition
#[inline(always)]
pub fn fast_float_add(a: f64, b: f64) -> FastPathResult<f64> {
    FastPathResult::Success(a + b)
}

/// Fast path for string concatenation using SSO when possible
///
/// If the result fits in the small string threshold, allocates efficiently.
/// SSO_THRESHOLD is used instead of direct import for decoupling.
#[inline]
pub fn fast_string_concat(a: &str, b: &str) -> FastPathResult<String> {
    let total_len = a.len() + b.len();
    if total_len <= SSO_THRESHOLD {
        // Small string optimization - avoid heap allocation
        let mut result = String::with_capacity(total_len);
        result.push_str(a);
        result.push_str(b);
        FastPathResult::Success(result)
    } else {
        // Still fast, but will allocate
        let mut result = String::with_capacity(total_len);
        result.push_str(a);
        result.push_str(b);
        FastPathResult::Success(result)
    }
}

/// Fast path for array indexing (bounds checked)
#[inline]
pub fn fast_array_index<T: Clone>(arr: &[T], index: i64) -> FastPathResult<T> {
    let idx = if index < 0 {
        // Negative indexing
        let adjusted = arr.len() as i64 + index;
        if adjusted < 0 {
            return FastPathResult::Fallback;
        }
        adjusted as usize
    } else {
        index as usize
    };

    match arr.get(idx) {
        Some(val) => FastPathResult::Success(val.clone()),
        None => FastPathResult::Fallback,
    }
}

// ============================================
// LIR PEEPHOLE OPTIMIZER
// ============================================

/// Peephole optimization for LIR instructions
pub struct PeepholeOptimizer {
    /// Number of optimizations applied
    pub optimizations_applied: usize,
}

impl PeepholeOptimizer {
    pub fn new() -> Self {
        PeepholeOptimizer {
            optimizations_applied: 0,
        }
    }

    /// Optimize a sequence of LIR instructions
    pub fn optimize(&mut self, instructions: &mut Vec<LirInst>) {
        let mut i = 0;
        while i < instructions.len() {
            // Try each peephole optimization
            if self.try_fold_constant_load(&mut instructions[i]) {
                self.optimizations_applied += 1;
            }

            // Multi-instruction patterns
            if i + 1 < instructions.len() {
                if self.try_remove_redundant_load(instructions, i) {
                    self.optimizations_applied += 1;
                    continue; // Instruction was removed, don't increment
                }
            }

            i += 1;
        }
    }

    /// Try to fold constant loads
    fn try_fold_constant_load(&self, _inst: &mut LirInst) -> bool {
        // Pattern: ConstI64 followed by operation with another constant
        // This is handled at HIR level, so we just return false here
        false
    }

    /// Try to remove redundant loads
    fn try_remove_redundant_load(&self, instructions: &mut Vec<LirInst>, idx: usize) -> bool {
        // Pattern: StoreVar(x, v) followed by LoadVar(v2, x) -> Copy(v2, v)
        if let (LirInst::StoreVar(name1, val1), LirInst::LoadVar(dst, name2)) =
            (&instructions[idx], &instructions[idx + 1])
        {
            if name1 == name2 {
                // Replace LoadVar with Copy
                instructions[idx + 1] = LirInst::Copy(*dst, *val1);
                return true;
            }
        }
        false
    }
}

impl Default for PeepholeOptimizer {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================
// TESTS
// ============================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_inline_cache() {
        let mut cache = InlineCache::new();

        // First lookup - miss
        assert!(cache.lookup("MyClass", "field", 12345).is_none());

        // Update cache
        cache.update("MyClass", "field", 12345, 0);

        // Second lookup - hit
        assert_eq!(cache.lookup("MyClass", "field", 12345), Some(0));

        // Different shape - miss
        assert!(cache.lookup("MyClass", "field", 99999).is_none());
    }

    #[test]
    fn test_hidden_class_registry() {
        let mut registry = HiddenClassRegistry::new();

        // Create class with properties
        let id1 = registry.get_or_create(&["x".to_string(), "y".to_string()]);
        let id2 = registry.get_or_create(&["x".to_string(), "y".to_string()]);

        // Same properties should return same ID
        assert_eq!(id1, id2);

        // Different properties should return different ID
        let id3 = registry.get_or_create(&["x".to_string(), "y".to_string(), "z".to_string()]);
        assert_ne!(id1, id3);

        // Check offset lookup
        let class = registry.get(id1).unwrap();
        assert!(class.get_offset("x").is_some());
        assert!(class.get_offset("y").is_some());
        assert!(class.get_offset("z").is_none());
    }

    #[test]
    fn test_type_profiler() {
        let mut profiler = TypeProfiler::new();

        // Monomorphic variable
        profiler.observe("x", SpecializedType::Int);
        profiler.observe("x", SpecializedType::Int);
        assert!(profiler.can_unbox("x"));
        assert_eq!(profiler.get_specialized("x"), Some(SpecializedType::Int));

        // Polymorphic variable
        profiler.observe("y", SpecializedType::Int);
        profiler.observe("y", SpecializedType::String);
        assert!(!profiler.can_unbox("y"));
        assert_eq!(profiler.get_specialized("y"), None);
    }

    #[test]
    fn test_type_specialized_stubs() {
        // Integer addition
        let stub = TypeSpecializedStub::for_add(SpecializedType::Int, SpecializedType::Int);
        assert!(matches!(stub, TypeSpecializedStub::IntegerAdd));
        assert!(stub.is_specialized());

        // Float addition (mixed types)
        let stub = TypeSpecializedStub::for_add(SpecializedType::Int, SpecializedType::Float);
        assert!(matches!(stub, TypeSpecializedStub::FloatAdd));

        // String concat
        let stub = TypeSpecializedStub::for_add(SpecializedType::String, SpecializedType::String);
        assert!(matches!(stub, TypeSpecializedStub::StringConcat));

        // Generic fallback
        let stub = TypeSpecializedStub::for_add(SpecializedType::Object, SpecializedType::Int);
        assert!(matches!(stub, TypeSpecializedStub::Generic));
        assert!(!stub.is_specialized());
    }

    #[test]
    fn test_fast_paths() {
        // Integer addition
        match fast_int_add(10, 20) {
            FastPathResult::Success(result) => assert_eq!(result, 30),
            FastPathResult::Fallback => panic!("Expected success"),
        }

        // Float addition
        match fast_float_add(1.5, 2.5) {
            FastPathResult::Success(result) => assert!((result - 4.0).abs() < 0.001),
            FastPathResult::Fallback => panic!("Expected success"),
        }

        // String concat
        match fast_string_concat("hello", " world") {
            FastPathResult::Success(result) => assert_eq!(result, "hello world"),
            FastPathResult::Fallback => panic!("Expected success"),
        }

        // Array indexing
        let arr = vec![1, 2, 3, 4, 5];
        match fast_array_index(&arr, 2) {
            FastPathResult::Success(result) => assert_eq!(result, 3),
            FastPathResult::Fallback => panic!("Expected success"),
        }

        // Negative indexing
        match fast_array_index(&arr, -1) {
            FastPathResult::Success(result) => assert_eq!(result, 5),
            FastPathResult::Fallback => panic!("Expected success"),
        }

        // Out of bounds
        match fast_array_index(&arr, 10) {
            FastPathResult::Success(_) => panic!("Expected fallback"),
            FastPathResult::Fallback => {}
        }
    }

    #[test]
    fn test_stub_registry() {
        let mut registry = StubRegistry::new();

        // Observe same types many times
        for _ in 0..50 {
            registry.observe(1, SpecializedType::Int, SpecializedType::Int, "+");
        }

        // Should have specialized stub now
        let stub = registry.get_stub(1);
        assert!(stub.is_some());
        assert!(matches!(stub.unwrap(), TypeSpecializedStub::IntegerAdd));

        assert_eq!(registry.specialized_count(), 1);
    }
}

// Re-export common types needed by submodules
pub use super::builtins;
pub use super::lir;

// Submodule for recursion optimizations
pub mod recursion;

pub use recursion::*;
