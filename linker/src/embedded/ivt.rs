//! Interrupt Vector Table (IVT) layouts for ARM Cortex-M and RISC-V microcontrollers.

/// ARM Cortex-M Vector Table Layout (Cortex-M0, M3, M4, M7, M33).
#[derive(Debug, Clone)]
pub struct ArmCortexVectorTable {
    pub initial_sp: u32,
    pub reset_handler: u32,
    pub nmi_handler: u32,
    pub hard_fault_handler: u32,
    pub mem_manage_handler: u32,
    pub bus_fault_handler: u32,
    pub usage_fault_handler: u32,
    pub svcall_handler: u32,
    pub debug_mon_handler: u32,
    pub pendsv_handler: u32,
    pub systick_handler: u32,
    pub external_irqs: Vec<u32>,
}

impl ArmCortexVectorTable {
    pub fn new(initial_sp: u32, reset_handler: u32) -> Self {
        // Thumb mode bit: handler address LSB must be 1 for ARM Thumb
        let thumb_reset = reset_handler | 1;
        Self {
            initial_sp,
            reset_handler: thumb_reset,
            nmi_handler: thumb_reset,
            hard_fault_handler: thumb_reset,
            mem_manage_handler: thumb_reset,
            bus_fault_handler: thumb_reset,
            usage_fault_handler: thumb_reset,
            svcall_handler: thumb_reset,
            debug_mon_handler: thumb_reset,
            pendsv_handler: thumb_reset,
            systick_handler: thumb_reset,
            external_irqs: Vec::new(),
        }
    }

    /// Encode vector table to binary slice.
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(64 + self.external_irqs.len() * 4);
        buf.extend_from_slice(&self.initial_sp.to_le_bytes());
        buf.extend_from_slice(&self.reset_handler.to_le_bytes());
        buf.extend_from_slice(&self.nmi_handler.to_le_bytes());
        buf.extend_from_slice(&self.hard_fault_handler.to_le_bytes());
        buf.extend_from_slice(&self.mem_manage_handler.to_le_bytes());
        buf.extend_from_slice(&self.bus_fault_handler.to_le_bytes());
        buf.extend_from_slice(&self.usage_fault_handler.to_le_bytes());
        buf.extend_from_slice(&[0; 16]); // Reserved 4 words (vectors 7-10)
        buf.extend_from_slice(&self.svcall_handler.to_le_bytes());
        buf.extend_from_slice(&self.debug_mon_handler.to_le_bytes());
        buf.extend_from_slice(&[0; 4]); // Reserved vector 13
        buf.extend_from_slice(&self.pendsv_handler.to_le_bytes());
        buf.extend_from_slice(&self.systick_handler.to_le_bytes());

        for irq in &self.external_irqs {
            buf.extend_from_slice(&(irq | 1).to_le_bytes());
        }
        buf
    }
}

/// RISC-V Trap Vector Table Layout (Direct and Vectored mode).
#[derive(Debug, Clone)]
pub struct RiscvTrapVectorTable {
    pub base_trap_handler: u32,
    pub vectored: bool,
    pub irq_handlers: Vec<u32>,
}

impl RiscvTrapVectorTable {
    pub fn direct(trap_handler: u32) -> Self {
        Self {
            base_trap_handler: trap_handler,
            vectored: false,
            irq_handlers: Vec::new(),
        }
    }

    pub fn vectored(base_handler: u32, irq_handlers: Vec<u32>) -> Self {
        Self {
            base_trap_handler: base_handler,
            vectored: true,
            irq_handlers,
        }
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        // RISC-V 32-bit `j <target>` or `jal x0, <target>` jump table
        if !self.vectored {
            buf.extend_from_slice(&self.base_trap_handler.to_le_bytes());
        } else {
            buf.extend_from_slice(&self.base_trap_handler.to_le_bytes());
            for irq in &self.irq_handlers {
                buf.extend_from_slice(&irq.to_le_bytes());
            }
        }
        buf
    }
}
