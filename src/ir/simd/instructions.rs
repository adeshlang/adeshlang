//! SIMD IR instructions — architecture-neutral vector operations

use super::types::SimdType;

/// Unique identifier for SIMD SSA values
pub type SimdValueId = u32;

/// Architecture-neutral SIMD instruction set
#[derive(Debug, Clone)]
pub enum SimdInst {
    // --- Construction ---
    /// Broadcast scalar to all lanes: splat
    Splat(SimdValueId, SimdType, SimdValueId),
    /// Load aligned vector from memory
    Load(SimdValueId, SimdType, SimdValueId /* ptr */),
    /// Load unaligned vector from memory
    LoadUnaligned(SimdValueId, SimdType, SimdValueId),
    /// Store aligned vector to memory
    Store(SimdValueId /* ptr */, SimdValueId),
    /// Store unaligned vector to memory
    StoreUnaligned(SimdValueId /* ptr */, SimdValueId),

    // --- Arithmetic ---
    Add(SimdValueId, SimdValueId, SimdValueId),
    Sub(SimdValueId, SimdValueId, SimdValueId),
    Mul(SimdValueId, SimdValueId, SimdValueId),
    Div(SimdValueId, SimdValueId, SimdValueId),
    Neg(SimdValueId, SimdValueId),
    Abs(SimdValueId, SimdValueId),
    Sqrt(SimdValueId, SimdValueId),
    Fma(SimdValueId, SimdValueId, SimdValueId, SimdValueId), // a * b + c
    Min(SimdValueId, SimdValueId, SimdValueId),
    Max(SimdValueId, SimdValueId, SimdValueId),
    Floor(SimdValueId, SimdValueId),
    Ceil(SimdValueId, SimdValueId),
    Round(SimdValueId, SimdValueId),

    // --- Bitwise ---
    BitAnd(SimdValueId, SimdValueId, SimdValueId),
    BitOr(SimdValueId, SimdValueId, SimdValueId),
    BitXor(SimdValueId, SimdValueId, SimdValueId),
    BitNot(SimdValueId, SimdValueId),
    Shl(SimdValueId, SimdValueId, SimdValueId),
    Shr(SimdValueId, SimdValueId, SimdValueId),

    // --- Comparison & selection ---
    CmpEq(SimdValueId, SimdValueId, SimdValueId),
    CmpNe(SimdValueId, SimdValueId, SimdValueId),
    CmpLt(SimdValueId, SimdValueId, SimdValueId),
    CmpLe(SimdValueId, SimdValueId, SimdValueId),
    CmpGt(SimdValueId, SimdValueId, SimdValueId),
    CmpGe(SimdValueId, SimdValueId, SimdValueId),
    Select(SimdValueId, SimdValueId /* mask */, SimdValueId, SimdValueId),
    Blend(SimdValueId, SimdValueId, SimdValueId, SimdValueId /* mask */),

    // --- Shuffle / permute ---
    Shuffle(SimdValueId, SimdValueId, SimdValueId, Vec<u32>),
    Permute(SimdValueId, SimdValueId, Vec<u32>),

    // --- Horizontal reductions ---
    ReduceAdd(SimdValueId, SimdValueId),
    ReduceMul(SimdValueId, SimdValueId),
    ReduceMin(SimdValueId, SimdValueId),
    ReduceMax(SimdValueId, SimdValueId),

    // --- Mask operations ---
    MaskLoad(SimdValueId, SimdType, SimdValueId /* ptr */, SimdValueId /* mask */),
    MaskStore(SimdValueId /* ptr */, SimdValueId, SimdValueId /* mask */),

    // --- Extract / insert lane ---
    ExtractLane(SimdValueId, SimdValueId, u32),
    InsertLane(SimdValueId, SimdValueId, u32, SimdValueId /* scalar */),
}

/// A block of SIMD instructions within a function
#[derive(Debug, Clone, Default)]
pub struct SimdBlock {
    pub instructions: Vec<SimdInst>,
}

/// SIMD function body (attached to LIR/VIR functions)
#[derive(Debug, Clone, Default)]
pub struct SimdFunction {
    pub simd_type_table: Vec<SimdType>,
    pub blocks: Vec<SimdBlock>,
}
