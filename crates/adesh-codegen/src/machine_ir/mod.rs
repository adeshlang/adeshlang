/// Register class distinguishing GPR (Integer/Pointer) and Float (SSE/XMM) registers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum RegisterClass {
    Gpr,
    Float,
}

/// Register identifier for virtual registers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct VirtualRegister(pub u32);

/// Register identifier for target physical registers.
/// In x86-64:
/// 0..15: GPR registers (RAX, RCX, RDX, RBX, RSP, RBP, RSI, RDI, R8..R15)
/// 16..31: XMM registers (XMM0..XMM15)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PhysicalRegister(pub u8);

impl PhysicalRegister {
    pub const fn gpr(id: u8) -> Self {
        Self(id)
    }

    pub const fn xmm(id: u8) -> Self {
        Self(16 + id)
    }

    pub fn is_gpr(&self) -> bool {
        self.0 < 16
    }

    pub fn is_xmm(&self) -> bool {
        self.0 >= 16 && self.0 < 32
    }

    pub fn gpr_index(&self) -> u8 {
        self.0
    }

    pub fn xmm_index(&self) -> u8 {
        if self.0 >= 16 { self.0 - 16 } else { self.0 }
    }

    pub fn class(&self) -> RegisterClass {
        if self.is_xmm() {
            RegisterClass::Float
        } else {
            RegisterClass::Gpr
        }
    }
}

/// Generic machine register, either virtual (before register allocation) or physical.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum MachineRegister {
    Virtual(VirtualRegister),
    Physical(PhysicalRegister),
}

impl MachineRegister {
    pub fn is_physical(&self) -> bool {
        matches!(self, MachineRegister::Physical(_))
    }

    pub fn physical(&self) -> Option<PhysicalRegister> {
        match self {
            MachineRegister::Physical(p) => Some(*p),
            _ => None,
        }
    }
}

/// Condition codes for comparisons and conditional branching.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ConditionCode {
    Equal,
    NotEqual,
    LessThan,
    LessOrEqual,
    GreaterThan,
    GreaterOrEqual,
    Below,
    BelowOrEqual,
    Above,
    AboveOrEqual,
    Zero,
    NotZero,
    Parity,
    NotParity,
}

/// Operands for Machine Instructions.
#[derive(Debug, Clone, PartialEq)]
pub enum MachineOperand {
    Register(MachineRegister),
    Immediate(i64),
    FloatImmediate(f64),
    StackSlot(i32),
    Memory {
        base: MachineRegister,
        offset: i32,
        index: Option<(MachineRegister, u8)>, // (index_reg, scale)
    },
    Label(String),
    Symbol(String),
}

impl MachineOperand {
    pub fn reg(id: u32) -> Self {
        MachineOperand::Register(MachineRegister::Virtual(VirtualRegister(id)))
    }

    pub fn phys(id: u8) -> Self {
        MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(id)))
    }

    pub fn imm(val: i64) -> Self {
        MachineOperand::Immediate(val)
    }

    pub fn stack(slot: i32) -> Self {
        MachineOperand::StackSlot(slot)
    }

    pub fn registers(&self) -> Vec<MachineRegister> {
        let mut regs = Vec::new();
        match self {
            MachineOperand::Register(r) => regs.push(*r),
            MachineOperand::Memory { base, index, .. } => {
                regs.push(*base);
                if let Some((idx_reg, _)) = index {
                    regs.push(*idx_reg);
                }
            }
            _ => {}
        }
        regs
    }

    pub fn register_def(&self) -> Option<MachineRegister> {
        match self {
            MachineOperand::Register(r) => Some(*r),
            _ => None,
        }
    }
}

/// An abstract location for parallel move resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum MoveLocation {
    /// Physical register on the target architecture.
    PhysicalRegister(PhysicalRegister),
    /// Virtual register before register allocation.
    VirtualRegister(VirtualRegister),
    /// Frame or stack slot with an explicit base register and displacement.
    StackSlot { base: PhysicalRegister, offset: i32 },
    /// Memory location with base register, offset, and optional scaled index.
    Memory {
        base: MachineRegister,
        offset: i32,
        index: Option<(MachineRegister, u8)>,
    },
    /// 64-bit integer immediate constant.
    Immediate(i64),
    /// 64-bit floating-point constant (bits preserved as u64/i64).
    FloatImmediate(u64),
}

impl MoveLocation {
    #[inline]
    pub fn phys(id: u8) -> Self {
        MoveLocation::PhysicalRegister(PhysicalRegister(id))
    }

    #[inline]
    pub fn virt(id: u32) -> Self {
        MoveLocation::VirtualRegister(VirtualRegister(id))
    }

    #[inline]
    pub fn stack(base: PhysicalRegister, offset: i32) -> Self {
        MoveLocation::StackSlot { base, offset }
    }

    #[inline]
    pub fn rbp_slot(offset: i32) -> Self {
        MoveLocation::StackSlot {
            base: PhysicalRegister(5),
            offset,
        }
    }

    #[inline]
    pub fn rsp_slot(offset: i32) -> Self {
        MoveLocation::StackSlot {
            base: PhysicalRegister(4),
            offset,
        }
    }

    #[inline]
    pub fn imm(val: i64) -> Self {
        MoveLocation::Immediate(val)
    }

    #[inline]
    pub fn is_register(&self) -> bool {
        matches!(
            self,
            MoveLocation::PhysicalRegister(_) | MoveLocation::VirtualRegister(_)
        )
    }

    #[inline]
    pub fn is_memory_or_stack(&self) -> bool {
        matches!(
            self,
            MoveLocation::StackSlot { .. } | MoveLocation::Memory { .. }
        )
    }

    #[inline]
    pub fn is_immediate(&self) -> bool {
        matches!(
            self,
            MoveLocation::Immediate(_) | MoveLocation::FloatImmediate(_)
        )
    }

    #[inline]
    pub fn physical_reg(&self) -> Option<PhysicalRegister> {
        match self {
            MoveLocation::PhysicalRegister(p) => Some(*p),
            _ => None,
        }
    }

    pub fn to_operand(&self) -> MachineOperand {
        match self {
            MoveLocation::PhysicalRegister(p) => {
                MachineOperand::Register(MachineRegister::Physical(*p))
            }
            MoveLocation::VirtualRegister(v) => {
                MachineOperand::Register(MachineRegister::Virtual(*v))
            }
            MoveLocation::StackSlot { base, offset } => MachineOperand::Memory {
                base: MachineRegister::Physical(*base),
                offset: *offset,
                index: None,
            },
            MoveLocation::Memory {
                base,
                offset,
                index,
            } => MachineOperand::Memory {
                base: *base,
                offset: *offset,
                index: *index,
            },
            MoveLocation::Immediate(v) => MachineOperand::Immediate(*v),
            MoveLocation::FloatImmediate(bits) => {
                MachineOperand::FloatImmediate(f64::from_bits(*bits))
            }
        }
    }

    pub fn from_operand(
        op: &MachineOperand,
        default_stack_base: Option<PhysicalRegister>,
    ) -> Option<Self> {
        match op {
            MachineOperand::Register(MachineRegister::Physical(p)) => {
                Some(MoveLocation::PhysicalRegister(*p))
            }
            MachineOperand::Register(MachineRegister::Virtual(v)) => {
                Some(MoveLocation::VirtualRegister(*v))
            }
            MachineOperand::StackSlot(slot) => {
                let base = default_stack_base.unwrap_or(PhysicalRegister(5));
                Some(MoveLocation::StackSlot {
                    base,
                    offset: *slot,
                })
            }
            MachineOperand::Memory {
                base,
                offset,
                index,
            } => match base {
                MachineRegister::Physical(p) if index.is_none() => Some(MoveLocation::StackSlot {
                    base: *p,
                    offset: *offset,
                }),
                _ => Some(MoveLocation::Memory {
                    base: *base,
                    offset: *offset,
                    index: *index,
                }),
            },
            MachineOperand::Immediate(v) => Some(MoveLocation::Immediate(*v)),
            MachineOperand::FloatImmediate(f) => Some(MoveLocation::FloatImmediate(f.to_bits())),
            _ => None,
        }
    }
}

impl std::fmt::Display for MoveLocation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MoveLocation::PhysicalRegister(p) => write!(f, "phys_r{}", p.0),
            MoveLocation::VirtualRegister(v) => write!(f, "v{}", v.0),
            MoveLocation::StackSlot { base, offset } => {
                if *offset >= 0 {
                    write!(f, "[phys_r{}+{}]", base.0, offset)
                } else {
                    write!(f, "[phys_r{}{}]", base.0, offset)
                }
            }
            MoveLocation::Memory {
                base,
                offset,
                index,
            } => {
                let base_str = match base {
                    MachineRegister::Physical(p) => format!("phys_r{}", p.0),
                    MachineRegister::Virtual(v) => format!("v{}", v.0),
                };
                if let Some((idx_reg, scale)) = index {
                    let idx_str = match idx_reg {
                        MachineRegister::Physical(p) => format!("phys_r{}", p.0),
                        MachineRegister::Virtual(v) => format!("v{}", v.0),
                    };
                    write!(f, "[{}+{}+{}*{}]", base_str, offset, idx_str, scale)
                } else if *offset >= 0 {
                    write!(f, "[{}+{}]", base_str, offset)
                } else {
                    write!(f, "[{}{}]", base_str, offset)
                }
            }
            MoveLocation::Immediate(v) => write!(f, "{}", v),
            MoveLocation::FloatImmediate(bits) => write!(f, "0x{:016x}", bits),
        }
    }
}

/// A single move operation representing `dst <- src` of `size` bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MoveOperation {
    pub dst: MoveLocation,
    pub src: MoveLocation,
    pub size: u8,
}

impl MoveOperation {
    pub fn new(dst: MoveLocation, src: MoveLocation, size: u8) -> Self {
        Self { dst, src, size }
    }

    pub fn new_qword(dst: MoveLocation, src: MoveLocation) -> Self {
        Self { dst, src, size: 8 }
    }
}

impl std::fmt::Display for MoveOperation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} <- {} ({}b)", self.dst, self.src, self.size)
    }
}

/// Generic target-neutral machine instruction.
#[derive(Debug, Clone, PartialEq)]
pub enum MachineInstruction {
    Nop,
    ParallelMove {
        moves: Vec<MoveOperation>,
    },
    Move {
        dst: MachineOperand,
        src: MachineOperand,
    },
    Load {
        dst: MachineOperand,
        src: MachineOperand,
        size: u8,
    },
    Store {
        dst: MachineOperand,
        src: MachineOperand,
        size: u8,
    },
    Add {
        dst: MachineOperand,
        src: MachineOperand,
    },
    Sub {
        dst: MachineOperand,
        src: MachineOperand,
    },
    Mul {
        dst: MachineOperand,
        src: MachineOperand,
    },
    Div {
        dst: MachineOperand,
        src: MachineOperand,
    },
    Mod {
        dst: MachineOperand,
        src: MachineOperand,
    },
    Neg {
        dst: MachineOperand,
    },
    Not {
        dst: MachineOperand,
    },
    And {
        dst: MachineOperand,
        src: MachineOperand,
    },
    Or {
        dst: MachineOperand,
        src: MachineOperand,
    },
    Xor {
        dst: MachineOperand,
        src: MachineOperand,
    },
    Shl {
        dst: MachineOperand,
        src: MachineOperand,
    },
    Shr {
        dst: MachineOperand,
        src: MachineOperand,
    },
    Sar {
        dst: MachineOperand,
        src: MachineOperand,
    },
    Compare {
        lhs: MachineOperand,
        rhs: MachineOperand,
    },
    Test {
        lhs: MachineOperand,
        rhs: MachineOperand,
    },
    SetCc {
        dst: MachineOperand,
        cc: ConditionCode,
    },
    Branch {
        target: String,
    },
    BranchCc {
        cc: ConditionCode,
        target: String,
    },
    Call {
        target: MachineOperand,
        num_args: usize,
    },
    Return,
    Push {
        src: MachineOperand,
    },
    Pop {
        dst: MachineOperand,
    },
    Vector {
        op: String,
        dst: MachineOperand,
        src: MachineOperand,
    },
    Atomic {
        op: String,
        dst: MachineOperand,
        src: MachineOperand,
    },
    Barrier,
    Custom {
        name: String,
        operands: Vec<MachineOperand>,
    },
    // Floating-Point (SSE / SSE2) instructions
    FAdd {
        dst: MachineOperand,
        src: MachineOperand,
        size: u8,
    },
    FSub {
        dst: MachineOperand,
        src: MachineOperand,
        size: u8,
    },
    FMul {
        dst: MachineOperand,
        src: MachineOperand,
        size: u8,
    },
    FDiv {
        dst: MachineOperand,
        src: MachineOperand,
        size: u8,
    },
    FNeg {
        dst: MachineOperand,
        size: u8,
    },
    FCmp {
        lhs: MachineOperand,
        rhs: MachineOperand,
        size: u8,
    },
    FCvtIntToFloat {
        dst: MachineOperand,
        src: MachineOperand,
        is_f64: bool,
        is_signed: bool,
    },
    FCvtFloatToInt {
        dst: MachineOperand,
        src: MachineOperand,
        is_f64: bool,
        is_signed: bool,
    },
    FCvtFloatToFloat {
        dst: MachineOperand,
        src: MachineOperand,
        to_f64: bool,
    },
}

impl MachineInstruction {
    /// Collect registers defined (written) by this instruction, including implicit registers and call return values.
    pub fn defs(&self) -> Vec<MachineRegister> {
        let mut defs = Vec::new();
        match self {
            MachineInstruction::Move { dst, .. }
            | MachineInstruction::Load { dst, .. }
            | MachineInstruction::Add { dst, .. }
            | MachineInstruction::Sub { dst, .. }
            | MachineInstruction::Mul { dst, .. }
            | MachineInstruction::Neg { dst }
            | MachineInstruction::Not { dst }
            | MachineInstruction::And { dst, .. }
            | MachineInstruction::Or { dst, .. }
            | MachineInstruction::Xor { dst, .. }
            | MachineInstruction::Shl { dst, .. }
            | MachineInstruction::Shr { dst, .. }
            | MachineInstruction::Sar { dst, .. }
            | MachineInstruction::SetCc { dst, .. }
            | MachineInstruction::Pop { dst }
            | MachineInstruction::FAdd { dst, .. }
            | MachineInstruction::FSub { dst, .. }
            | MachineInstruction::FMul { dst, .. }
            | MachineInstruction::FDiv { dst, .. }
            | MachineInstruction::FNeg { dst, .. }
            | MachineInstruction::FCvtIntToFloat { dst, .. }
            | MachineInstruction::FCvtFloatToInt { dst, .. }
            | MachineInstruction::FCvtFloatToFloat { dst, .. } => {
                if let Some(r) = dst.register_def() {
                    defs.push(r);
                }
            }
            MachineInstruction::Div { dst, src: _ } | MachineInstruction::Mod { dst, src: _ } => {
                if let Some(r) = dst.register_def() {
                    defs.push(r);
                }
                defs.push(MachineRegister::Physical(PhysicalRegister::gpr(0)));
                defs.push(MachineRegister::Physical(PhysicalRegister::gpr(2)));
            }
            MachineInstruction::Call { .. } => {
                defs.push(MachineRegister::Physical(PhysicalRegister::gpr(0)));
                defs.push(MachineRegister::Physical(PhysicalRegister::xmm(0)));
            }
            MachineInstruction::ParallelMove { moves } => {
                for m in moves {
                    if let MoveLocation::PhysicalRegister(p) = m.dst {
                        defs.push(MachineRegister::Physical(p));
                    } else if let MoveLocation::VirtualRegister(v) = m.dst {
                        defs.push(MachineRegister::Virtual(v));
                    }
                }
            }
            MachineInstruction::Custom { operands, .. } => {
                if let Some(dst) = operands.first() {
                    if let Some(r) = dst.register_def() {
                        defs.push(r);
                    }
                }
            }
            _ => {}
        }
        defs.dedup();
        defs
    }

    /// Collect registers used (read) by this instruction, including memory base/index, binary source operands, and explicit call arguments.
    pub fn uses(&self) -> Vec<MachineRegister> {
        let mut uses = Vec::new();
        let mut add_op = |op: &MachineOperand| {
            uses.extend(op.registers());
        };

        match self {
            MachineInstruction::Move { src, dst }
            | MachineInstruction::Load { src, dst, .. }
            | MachineInstruction::FCvtIntToFloat { src, dst, .. }
            | MachineInstruction::FCvtFloatToInt { src, dst, .. }
            | MachineInstruction::FCvtFloatToFloat { src, dst, .. } => {
                add_op(src);
                if matches!(dst, MachineOperand::Memory { .. }) {
                    add_op(dst);
                }
            }
            MachineInstruction::Store { dst, src, .. } => {
                add_op(dst);
                add_op(src);
            }
            MachineInstruction::Add { dst, src }
            | MachineInstruction::Sub { dst, src }
            | MachineInstruction::Mul { dst, src }
            | MachineInstruction::Div { dst, src }
            | MachineInstruction::Mod { dst, src }
            | MachineInstruction::And { dst, src }
            | MachineInstruction::Or { dst, src }
            | MachineInstruction::Xor { dst, src }
            | MachineInstruction::Shl { dst, src }
            | MachineInstruction::Shr { dst, src }
            | MachineInstruction::Sar { dst, src }
            | MachineInstruction::FAdd { dst, src, .. }
            | MachineInstruction::FSub { dst, src, .. }
            | MachineInstruction::FMul { dst, src, .. }
            | MachineInstruction::FDiv { dst, src, .. } => {
                add_op(dst);
                add_op(src);
            }
            MachineInstruction::Compare { lhs, rhs }
            | MachineInstruction::Test { lhs, rhs }
            | MachineInstruction::FCmp { lhs, rhs, .. } => {
                add_op(lhs);
                add_op(rhs);
            }
            MachineInstruction::Neg { dst }
            | MachineInstruction::Not { dst }
            | MachineInstruction::FNeg { dst, .. }
            | MachineInstruction::Push { src: dst } => {
                add_op(dst);
            }
            MachineInstruction::Call { target, .. } => {
                add_op(target);
            }
            MachineInstruction::ParallelMove { moves } => {
                for m in moves {
                    if let MoveLocation::PhysicalRegister(p) = m.src {
                        uses.push(MachineRegister::Physical(p));
                    } else if let MoveLocation::VirtualRegister(v) = m.src {
                        uses.push(MachineRegister::Virtual(v));
                    } else if let MoveLocation::Memory { base, index, .. } = &m.src {
                        uses.push(*base);
                        if let Some((idx_reg, _)) = index {
                            uses.push(*idx_reg);
                        }
                    }
                }
            }
            MachineInstruction::Custom { operands, .. } => {
                for op in operands {
                    add_op(op);
                }
            }
            _ => {}
        }
        uses.dedup();
        uses
    }
}

/// Basic block containing machine instructions.
#[derive(Debug, Clone, PartialEq)]
pub struct MachineBlock {
    pub id: u32,
    pub label: String,
    pub instructions: Vec<MachineInstruction>,
    pub predecessors: Vec<u32>,
    pub successors: Vec<u32>,
}

impl MachineBlock {
    pub fn new(id: u32, label: impl Into<String>) -> Self {
        Self {
            id,
            label: label.into(),
            instructions: Vec::new(),
            predecessors: Vec::new(),
            successors: Vec::new(),
        }
    }

    pub fn push(&mut self, inst: MachineInstruction) {
        self.instructions.push(inst);
    }
}

/// Machine Function representation.
#[derive(Debug, Clone, PartialEq)]
pub struct MachineFunction {
    pub name: String,
    pub blocks: Vec<MachineBlock>,
    pub stack_size: u64,
    pub is_exported: bool,
    pub vreg_count: u32,
    pub vreg_classes: std::collections::HashMap<VirtualRegister, RegisterClass>,
}

impl MachineFunction {
    pub fn new(name: impl Into<String>) -> Self {
        let mut func = Self {
            name: name.into(),
            blocks: Vec::new(),
            stack_size: 0,
            is_exported: false,
            vreg_count: 0,
            vreg_classes: std::collections::HashMap::new(),
        };
        func.blocks.push(MachineBlock::new(0, "entry"));
        func
    }

    pub fn alloc_vreg(&mut self) -> VirtualRegister {
        self.alloc_vreg_with_class(RegisterClass::Gpr)
    }

    pub fn alloc_fp_vreg(&mut self) -> VirtualRegister {
        self.alloc_vreg_with_class(RegisterClass::Float)
    }

    pub fn alloc_vreg_with_class(&mut self, class: RegisterClass) -> VirtualRegister {
        let r = VirtualRegister(self.vreg_count);
        self.vreg_count += 1;
        self.vreg_classes.insert(r, class);
        r
    }

    pub fn vreg_class(&self, vreg: VirtualRegister) -> RegisterClass {
        self.vreg_classes
            .get(&vreg)
            .copied()
            .unwrap_or(RegisterClass::Gpr)
    }

    pub fn create_block(&mut self, label: impl Into<String>) -> u32 {
        let id = self.blocks.len() as u32;
        self.blocks.push(MachineBlock::new(id, label));
        id
    }

    /// Build and update control flow graph edges (`predecessors` and `successors`) for all basic blocks.
    pub fn rebuild_cfg(&mut self) {
        let label_to_id: std::collections::HashMap<String, u32> = self
            .blocks
            .iter()
            .map(|b| (b.label.clone(), b.id))
            .collect();

        for block in &mut self.blocks {
            block.predecessors.clear();
            block.successors.clear();
        }

        let num_blocks = self.blocks.len();
        let mut edges: Vec<(u32, u32)> = Vec::new();

        for (i, block) in self.blocks.iter().enumerate() {
            let src_id = block.id;
            let mut has_unconditional_jump = false;
            let mut terminates = false;

            for inst in &block.instructions {
                match inst {
                    MachineInstruction::Branch { target } => {
                        if let Some(&tgt_id) = label_to_id.get(target) {
                            edges.push((src_id, tgt_id));
                        }
                        has_unconditional_jump = true;
                    }
                    MachineInstruction::BranchCc { target, .. } => {
                        if let Some(&tgt_id) = label_to_id.get(target) {
                            edges.push((src_id, tgt_id));
                        }
                    }
                    MachineInstruction::Return => {
                        terminates = true;
                    }
                    _ => {}
                }
            }

            if !has_unconditional_jump && !terminates && i + 1 < num_blocks {
                let fallthrough_id = self.blocks[i + 1].id;
                edges.push((src_id, fallthrough_id));
            }
        }

        for (src, dst) in edges {
            if let Some(src_block) = self.blocks.iter_mut().find(|b| b.id == src) {
                if !src_block.successors.contains(&dst) {
                    src_block.successors.push(dst);
                }
            }
            if let Some(dst_block) = self.blocks.iter_mut().find(|b| b.id == dst) {
                if !dst_block.predecessors.contains(&src) {
                    dst_block.predecessors.push(src);
                }
            }
        }
    }

    pub fn entry_block_mut(&mut self) -> &mut MachineBlock {
        &mut self.blocks[0]
    }
}

/// Complete lowered native module.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct NativeModule {
    pub name: String,
    pub functions: Vec<MachineFunction>,
    pub data_sections: Vec<(String, Vec<u8>)>,
    pub string_pool: Vec<String>,
    pub imports: Vec<String>,
    pub exports: Vec<String>,
}

impl NativeModule {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            functions: Vec::new(),
            data_sections: Vec::new(),
            string_pool: Vec::new(),
            imports: Vec::new(),
            exports: Vec::new(),
        }
    }

    pub fn add_function(&mut self, func: MachineFunction) {
        self.functions.push(func);
    }

    pub fn add_string(&mut self, s: impl Into<String>) -> usize {
        let str_val = s.into();
        if let Some(idx) = self.string_pool.iter().position(|x| x == &str_val) {
            idx
        } else {
            let idx = self.string_pool.len();
            self.string_pool.push(str_val);
            idx
        }
    }
}
