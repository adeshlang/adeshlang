//! Machine Intermediate Representation (Machine IR) - Target-Neutral Low-Level IR.

/// Register identifier for virtual registers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct VirtualRegister(pub u32);

/// Register identifier for target physical registers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PhysicalRegister(pub u8);

/// Generic machine register, either virtual (before register allocation) or physical.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
}

/// Generic target-neutral machine instruction.
#[derive(Debug, Clone, PartialEq)]
pub enum MachineInstruction {
    Nop,
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
}

impl MachineFunction {
    pub fn new(name: impl Into<String>) -> Self {
        let mut func = Self {
            name: name.into(),
            blocks: Vec::new(),
            stack_size: 0,
            is_exported: false,
            vreg_count: 0,
        };
        func.blocks.push(MachineBlock::new(0, "entry"));
        func
    }

    pub fn alloc_vreg(&mut self) -> VirtualRegister {
        let r = VirtualRegister(self.vreg_count);
        self.vreg_count += 1;
        r
    }

    pub fn create_block(&mut self, label: impl Into<String>) -> u32 {
        let id = self.blocks.len() as u32;
        self.blocks.push(MachineBlock::new(id, label));
        id
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
