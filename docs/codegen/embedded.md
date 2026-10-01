# Embedded & Bare-Metal Backend Specification

## 1. Overview

The Embedded backend targets microcontroller and bare-metal systems (e.g. ARM Cortex-M0/M3/M4F/M7F, RISC-V RV32IMAC, Xtensa, AVR).

---

## 2. Key Characteristics

* **No OS Dependency**: Freestanding execution mode without requiring standard libc or operating system services.
* **Explicit Memory Mapping**: ADOB records define physical hardware regions (`.flash`, `.sram`, `.mmio`) with custom base addresses, sizes, and memory protection attributes (`R`, `RW`, `RX`, `RWX`).
* **Vector Table & Reset Vector**: Direct mapping of interrupt vectors and startup entry points into `.flash` sections.
