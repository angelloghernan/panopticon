use super::super::pci;
use super::super::util;
use super::super::x86_64;
use super::{DMAState, PortCommandMasks, PortRegisters, Registers};
use crate::klib::pci::pcistate::PCI_STATE;
use core::marker::PhantomData;
use core::ptr::{addr_of, addr_of_mut};
use pci::pcistate::PCIState;
use pci::Register;
use spin::RwLock;
use util::Volatile;

#[repr(C)]
pub struct AHCIState {
    dma: DMAState,
    bus: u32,
    slot: u32,
    func: u32,
    sata_port: u32,
    // These are pointers because the port registers technically alias the drive registers.
    // Drive registers are shared by all devices, so it needs a lock.
    drive_registers: &'static RwLock<*mut Registers>,

    // Port registers are per-drive; other devices should not access.
    port_registers: *mut PortRegisters,

    // These should remain constant after loading
    irq: u32,
    num_sectors: usize,
    num_ncq_slots: u32,
    slots_full_mask: u32,

    // These are modifiable
    num_slots_available: u16,
    slots_outstanding_mask: u16,

    // This is modified by the hardware itself and our code. Shared across threads.
    slot_status: [*mut u32; 32],
    // TODO: Add buffer cache
}

impl AHCIState {
    /// Create a new object to keep track of AHCI-relevant state
    /// `bus` / `slot` / `func_number`: the relevant PCI bus/slot/function for the AHCI controller
    /// `sata_port`: the port for this device on the AHCI controller
    /// `regs`: the drive registers, as pointed to by BAR 5 of the AHCI controller
    ///
    /// ### Safety
    /// This should be called only ONCE per drive. Each drive on the AHCI controller has a unique sata port number.
    /// Also, the `regs` argument should ultimately point to a valid register memory.
    unsafe fn init(
        bus: u32,
        slot: u32,
        func_number: u32,
        sata_port: u32,
        regs: &'static RwLock<*mut Registers>,
    ) -> Option<Self> {
        use PortCommandMasks::*;

        let dma: DMAState = unsafe { core::mem::zeroed() };
        let port_reg_ptr = {
            let regs_ptr = *regs.write();
            addr_of_mut!((*regs_ptr).port_regs[sata_port as usize])
        };

        let mut ahci = AHCIState {
            dma,
            bus,
            slot,
            func: func_number,
            sata_port,
            drive_registers: regs,
            port_registers: port_reg_ptr,
            irq: 0,
            num_sectors: 0,
            slots_full_mask: 0,
            slots_outstanding_mask: 0,
            num_slots_available: 1,
            num_ncq_slots: 1,
            slot_status: unsafe { core::mem::zeroed() },
        };

        let mut pci = PCIState::new();
        unsafe { pci.config_write(bus, slot, func_number, Register::Command, 0x7u16) }; // Enable I/O
        let mask = !((RFISEnable as u32) | (Start as u32));

        {
            let running_mask = (CommandRunning as u32) | (RFISRunning as u32);

            unsafe {
                (*port_reg_ptr).command_and_status &= mask;
                while (*port_reg_ptr).command_and_status & running_mask != 0 {
                    // TODO: Maybe change this to use a wait queue?
                    x86_64::pause();
                }
            };

            for ch in ahci.dma.ch.iter_mut() {
                ch.command_table_address =
                    util::kernel_to_physical_address(ch.command_table_address);
            }

            unsafe {
                use super::InterruptMasks::*;
                use super::PortCommandMasks::*;
                use super::RStatusMasks::*;

                (*port_reg_ptr).cmdlist_addr =
                    util::kernel_to_physical_address(addr_of!(ahci.dma.ch[0]) as u64);
                (*port_reg_ptr).rfis_base_addr =
                    util::kernel_to_physical_address(addr_of!(ahci.dma.rfis) as u64);

                (*port_reg_ptr).serror = !0;
                (*port_reg_ptr).command_mask = (*port_reg_ptr).command_mask | (PowerUp as u32);

                (*port_reg_ptr).interrupt_status = !0;

                {
                    let regs_ptr = *ahci.drive_registers.write();
                    (*regs_ptr).interrupt_status = !0; // TODO change this? want to change only
                                                       // this port, i think, if that's how it
                                                       // works
                }

                (*port_reg_ptr).interrupt_enable =
                    DeviceToHost as u32 | NCQComplete as u32 | ErrorMask as u32;

                let busy = Busy as u32 | DataReq as u32;

                while (*port_reg_ptr).tfd & busy != 0 || !sstatus_active((*port_reg_ptr).sstatus) {
                    x86_64::pause();
                }

                (*port_reg_ptr).command_and_status = ((*port_reg_ptr).command_and_status
                    & !(InterfaceMask as u32))
                    | InterfaceActive as u32;

                while (*port_reg_ptr).command_and_status & InterfaceMask as u32
                    != InterfaceIdle as u32
                {
                    x86_64::pause();
                }

                (*port_reg_ptr).command_and_status =
                    (*port_reg_ptr).command_and_status | Start as u32;

                let mut id_buf: [Volatile<u16>; 256] = core::mem::zeroed();

                ahci.dma.ch[slot as usize].num_buffers = 0;
                ahci.dma.ch[slot as usize].buffer_byte_pos = 0;

                let handle = ahci.push_buffer(0, &mut id_buf);
                ahci.issue_meta(0, pci::ide_controller::Command::Identify, 0, u32::MAX);
                unsafe { ahci.await_basic(0) };
                ahci.clear_slot(handle);

                ahci.num_sectors = id_buf[100].read() as usize
                    | ((id_buf[101].read() as usize) << 16)
                    | ((id_buf[102].read() as usize) << 32)
                    | ((id_buf[102].read() as usize) << 48);
                {
                    let drive_regs = *ahci.drive_registers.read();
                    // slots per controller
                    ahci.num_ncq_slots = ((*drive_regs).capabilities & 0x1F) + 1;
                }

                if (((id_buf[75].read() & 0x1F) + 1) as u32) < ahci.num_ncq_slots {
                    // slots per disk
                    ahci.num_ncq_slots = ((id_buf[75].read() & 0x1F) + 1) as u32;
                }

                ahci.slots_full_mask = if ahci.num_ncq_slots == 32 {
                    u32::MAX
                } else {
                    (1u32 << ahci.num_ncq_slots) - 1
                };

                ahci.num_slots_available = ahci.num_ncq_slots as u16;

                // set features
                ahci.dma.ch[slot as usize].num_buffers = 0;
                ahci.dma.ch[slot as usize].buffer_byte_pos = 0;
                ahci.issue_meta(0, pci::ide_controller::Command::SetFeatures, 0x02, u32::MAX); // write cache enable
                ahci.await_basic(0);

                ahci.dma.ch[slot as usize].num_buffers = 0;
                ahci.dma.ch[slot as usize].buffer_byte_pos = 0;
                ahci.issue_meta(0, pci::ide_controller::Command::SetFeatures, 0xAA, u32::MAX); // read lookahead enable
                ahci.await_basic(0);

                // determine IRQ
                let intr_line = PCI_STATE.lock().config_read_8(
                    bus,
                    slot,
                    func_number,
                    pci::Register::InterruptLine,
                );

                ahci.irq = intr_line as u32;

                // finally, clear pending interrupts again
                (*ahci.port_registers).interrupt_status = !0;
                // _drive_registers.interrupt_status = ~0U;
            }
        }

        Some(ahci)
    }

    fn push_buffer<'b, T: Sized>(&mut self, slot: u32, buf: &'b mut [T]) -> BufferHandle<'b, T> {
        let phys_addr = util::kernel_to_physical_address(buf.as_mut_ptr() as u64);

        let num_buffers = self.dma.ch[slot as usize].num_buffers;
        let size = (buf.len() * core::mem::size_of::<T>() - 1) as u32;

        self.dma.ct[slot as usize].prdt[num_buffers as usize].address = phys_addr;
        self.dma.ct[slot as usize].prdt[num_buffers as usize].data_byte_count = size - 1;

        self.dma.ch[slot as usize].num_buffers = num_buffers + 1;
        self.dma.ch[slot as usize].buffer_byte_pos += size;

        BufferHandle {
            slot,
            _phantom: PhantomData,
        }
    }

    fn issue_meta(
        &mut self,
        slot: u32,
        command: pci::ide_controller::Command,
        features: u32,
        count: u32,
    ) {
    }

    fn clear_slot<'b, T>(&mut self, handle: BufferHandle<'b, T>) {
        self.dma.ch[handle.slot as usize].num_buffers = 0;
        self.dma.ch[handle.slot as usize].buffer_byte_pos = 0;
    }

    unsafe fn await_basic(&mut self, slot: u32) {
        unsafe {
            while (*self.port_registers).command_mask & (1u32 << slot) != 0 {
                x86_64::pause();
            }
        }

        unsafe { self.acknowledge(slot, 0) };
    }

    unsafe fn acknowledge(&mut self, slot: u32, result: u32) {
        self.slots_outstanding_mask ^= 1u16 << slot;
        self.num_slots_available += 1;

        if !self.slot_status[slot as usize].is_null() {
            unsafe { self.slot_status[slot as usize].write_volatile(result) };
            self.slot_status[slot as usize] = core::ptr::null();
        }
    }
}

#[repr(transparent)]
#[must_use]
struct BufferHandle<'a, T> {
    pub slot: u32,
    _phantom: PhantomData<&'a mut T>,
}

fn sstatus_active(sstatus: u32) -> bool {
    return (sstatus & 0x03) == 3 || ((1u32 << ((sstatus & 0xF00) >> 8)) & 0x144) != 0;
}
