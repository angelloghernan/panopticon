use super::{DMAState, PortRegisters, Registers};
use core::sync::atomic::AtomicU32;

#[repr(C)]
pub struct AHCIState {
    dma: DMAState,
    bus: u32,
    slot: u32,
    func: u32,
    sata_port: u32,
    drive_registers: Registers,
    port_registers: PortRegisters,

    // These should remain constant after loading
    irq: u32,
    num_sectors: usize,
    num_irq_slots: u32,
    slots_full_mask: u32,

    // These are modifiable at any time, including by hardware
    num_slots_available: u16,
    slots_outstanding_mask: u16,
    slot_status: [AtomicU32; 32],
    // TODO: Add buffer cache
}

impl AHCIState {
    /// Create a new object to keep track of AHCI-relevant state
    /// `bus` / `slot` / `func_number`: the relevant PCI bus/slot/function for the AHCI controller
    /// `sata_port`: the port for this device on the AHCI controller
    /// `regs`: the drive registers, as pointed to by BAR 5 of the AHCI controller
    fn init(bus: u8, slot: u8, func_number: u8, sata_port: u32, regs: Registers) -> Option<Self> {
        None
    }
}
