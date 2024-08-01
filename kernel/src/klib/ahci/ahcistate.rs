use super::super::pci;
use super::super::util;
use super::{DMAState, PortCommandMasks, PortRegisters, Registers};
use crate::klib::ahci::GHCMasks;
use crate::klib::once_lock::OnceLock;
use crate::klib::pci::pcistate::PCI_STATE;
use crate::klib::x86_64::pause;
use crate::println;
use crate::BootInfoFrameAllocator;
use alloc::boxed::Box;
use core::cell::OnceCell;
use core::marker::PhantomData;
use core::ptr::addr_of;
use pci::pcistate::PCIState;
use pci::Register;
use spin::RwLock;
use util::Volatile;
use x86_64::structures::paging::Mapper;
use x86_64::structures::paging::OffsetPageTable;
use x86_64::structures::paging::Page;
use x86_64::structures::paging::PageTableFlags;
use x86_64::structures::paging::PhysFrame;
use x86_64::structures::paging::Size4KiB;
use x86_64::PhysAddr;
use x86_64::VirtAddr;

// TODO: change to dynamic var in ahci state, this is not true for all drives
const SECTOR_SIZE: u32 = 512;

const CFIS_COMMAND: u32 = 0x8027;

static DRIVE_REGISTER: OnceLock<RwLock<&'static mut Registers>> = OnceLock::new();

pub static SATA_DISK0: OnceLock<RwLock<&'static mut AHCIState>> = OnceLock::new();

// NCQ slot statuses; i.e., showing which commands have finished.
// I would love to lower this into AHCIState safely, but rn my brain is cooked and I can't really
// think of a nice way to do it. this is the quick and dirty way. I don't anticipate any major
// safety problems from this anyway *at the moment*; eventually this will have to change.
static mut SLOT_STATUS: [*mut u32; 32] = [core::ptr::null_mut(); 32];

#[repr(C)]
pub struct AHCIState {
    dma: Box<DMAState>,
    bus: u32,
    slot: u32,
    func: u32,
    sata_port: u32,
    // Drive register fields are shared by all devices, so it needs a lock.
    drive_registers: &'static RwLock<&'static mut Registers>,

    // Port registers are per-drive; other devices should not access.
    port_registers: &'static mut PortRegisters,

    // These should remain constant after loading
    irq: u32,
    num_sectors: usize,
    num_ncq_slots: u32,
    slots_full_mask: u32,

    // These are modifiable
    num_slots_available: u16,
    slots_outstanding_mask: u16,
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
    /// Also, the `regs` argument should ultimately point to valid register memory.
    unsafe fn init(
        bus: u32,
        slot: u32,
        func_number: u32,
        sata_port: u32,
        regs: &'static RwLock<&'static mut Registers>,
    ) -> Box<Self> {
        use PortCommandMasks::*;

        let port_reg_ptr = {
            let regs_ptr = *regs.read() as *const Registers as *const u8;
            // Index into the array of port registers, located past the drive registers.
            // This doesn't overlap with the drive registers, so this is OK to do,
            // *assuming we have not called with the same sata port before*, as specified
            // in the invariants above.
            let port_reg_ptr = regs_ptr
                .add(core::mem::size_of::<Registers>())
                .add(core::mem::size_of::<PortRegisters>() * sata_port as usize)
                as *mut PortRegisters;
            &mut (*port_reg_ptr)
        };

        let dma: Box<DMAState> = Box::new(unsafe { core::mem::zeroed() });

        let mut ahci = Box::new(AHCIState {
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
        });

        let mut pci = PCIState::new();
        unsafe { pci.config_write(bus, slot, func_number, Register::Command, 0x7u16) }; // Enable I/O
        let mask = !((RFISEnable as u32) | (Start as u32));

        {
            let running_mask = (CommandRunning as u32) | (RFISRunning as u32);

            ahci.port_registers.command_and_status &= mask;
            while ahci.port_registers.command_and_status & running_mask != 0 {
                // TODO: Maybe change this to use a wait queue?
                pause();
            }

            for ch in ahci.dma.ch.iter_mut() {
                ch.command_table_address =
                    util::kernel_to_physical_address(ch.command_table_address);
            }

            // Pretty much everything here is unsafe. Look at those pointer derefs!
            // It's ok though, it's only dereferencing the port registers and the drive registers.
            unsafe {
                use super::InterruptMasks::*;
                use super::PortCommandMasks::*;
                use super::RStatusMasks::*;

                ahci.port_registers.cmdlist_addr =
                    util::kernel_to_physical_address(addr_of!(ahci.dma.ch[0]) as u64);
                ahci.port_registers.rfis_base_addr =
                    util::kernel_to_physical_address(addr_of!(ahci.dma.rfis) as u64);

                ahci.port_registers.serror = !0;
                ahci.port_registers.command_mask =
                    ahci.port_registers.command_mask | (PowerUp as u32);

                ahci.port_registers.interrupt_status = !0;

                {
                    let mut regs_ptr = ahci.drive_registers.write();
                    (*regs_ptr).interrupt_status = !0; // TODO change this? want to change only
                                                       // this port, i think, if that's how it
                                                       // works
                }

                ahci.port_registers.interrupt_enable =
                    DeviceToHost as u32 | NCQComplete as u32 | ErrorMask as u32;

                let busy = Busy as u32 | DataReq as u32;

                while ahci.port_registers.tfd & busy != 0
                    || !sstatus_active(ahci.port_registers.sstatus)
                {
                    pause();
                }

                ahci.port_registers.command_and_status = (ahci.port_registers.command_and_status
                    & !(InterfaceMask as u32))
                    | InterfaceActive as u32;

                while ahci.port_registers.command_and_status & InterfaceMask as u32
                    != InterfaceIdle as u32
                {
                    pause();
                }

                ahci.port_registers.command_and_status =
                    ahci.port_registers.command_and_status | Start as u32;

                let mut id_buf: [Volatile<u16>; 256] = core::mem::zeroed();

                ahci.dma.ch[slot as usize].num_buffers = 0;
                ahci.dma.ch[slot as usize].buffer_byte_pos = 0;

                let handle = ahci.push_buffer(0, &mut id_buf);
                ahci.issue_meta(0, pci::ide_controller::Command::Identify, 0, u32::MAX);
                ahci.await_basic(0);
                ahci.clear_slot(handle);

                ahci.num_sectors = id_buf[100].read() as usize
                    | ((id_buf[101].read() as usize) << 16)
                    | ((id_buf[102].read() as usize) << 32)
                    | ((id_buf[102].read() as usize) << 48);
                {
                    let drive_regs = ahci.drive_registers.read();
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
                ahci.port_registers.interrupt_status = !0;
                (*(*ahci.drive_registers.write())).interrupt_status = !0;
            }
        }

        ahci
    }

    pub unsafe fn new(
        mapper: &mut OffsetPageTable<'_>,
        frame_allocator: &mut BootInfoFrameAllocator,
        bus: u32,
        slot: u32,
        func: u32,
    ) -> Result<(), ()> {
        let mut pci = PCIState::new();
        let mut addr_opt = Some((bus, slot, func));

        while let Some((bus, slot, func)) = addr_opt {
            // println!("Looping: {bus}, {slot}, {func}");
            let subclass = pci.config_read_16(bus, slot, func, pci::Register::Subclass);
            if subclass != 0x0106 {
                addr_opt = pci.next_addr(bus, slot, func);
                continue;
            }

            let phys_addr =
                pci.config_read_32(bus, slot, func, pci::Register::GDBaseAddress5) as u64;

            if phys_addr == 0 {
                addr_opt = pci.next_addr(bus, slot, func);
                continue;
            }

            println!("going through: {bus}, {slot}, {func}");

            let regs_page: Page<Size4KiB> = Page::containing_address(VirtAddr::new(phys_addr));
            // FIXME XXX: Need to make sure we don't overlap with allocated frames in the BootInfoFrameAllocator.
            let frame = PhysFrame::containing_address(PhysAddr::new(phys_addr));

            let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;
            unsafe {
                mapper
                    .map_to(regs_page, frame, flags, frame_allocator)
                    .expect("Failed to map AHCI address in pagetable!")
                    .flush()
            };

            // FIXME: This isn't quite ready for multi-drive support. Needs to *ensure* that the
            // same slot is not used twice.
            let drive_regs_ptr = util::physical_to_kernel_address(phys_addr) as *mut Registers;
            if (drive_regs_ptr.read_volatile()).global_hba_control & GHCMasks::AHCIEnable as u32
                == 0
            {
                (*drive_regs_ptr).global_hba_control = GHCMasks::AHCIEnable as u32;
            }

            println!("Drive regs ptr: {:#x}", drive_regs_ptr as u64);
            println!("Capabilites: {:#x}", (*drive_regs_ptr).capabilities);
            println!(
                "global_hba_control: {}",
                (*drive_regs_ptr).global_hba_control
            );

            println!("Port mask: {}", (drive_regs_ptr.read_volatile()).port_mask);

            for fslot in 0..32 {
                let port_reg_ptr = (drive_regs_ptr as *mut u8)
                    .add(core::mem::size_of::<Registers>())
                    .add(core::mem::size_of::<PortRegisters>() * fslot as usize)
                    as *mut PortRegisters;
                if ((drive_regs_ptr.read_volatile()).port_mask & (1u32 << slot)) != 0
                    && (port_reg_ptr.read_volatile()).sstatus != 0
                {
                    match DRIVE_REGISTER.set(RwLock::new(&mut *drive_regs_ptr)) {
                        Err(_) => {
                            // TODO: This slot has been claimed. Assume *for now* this is the same
                            // drive.
                        }
                        Ok(()) => {
                            println!("Found one: {fslot}");
                            let lock_ref = DRIVE_REGISTER.get().unwrap();
                            let ahci_state =
                                unsafe { AHCIState::init(bus, fslot, func, fslot, lock_ref) };
                            let _ = SATA_DISK0.set(RwLock::new(Box::<AHCIState>::leak(ahci_state)));
                            return Ok(());
                        }
                    }
                }
            }
            println!("Next one");
            addr_opt = pci.next_addr(bus, slot, func);
        }

        Err(())
    }

    // Issue an NCQ (Native Command Queueing) command to the disk
    // Must preceed call with clear_slot(slot) and push_buffer(slot).
    // `fua`: If true, then don't acknowledge the write until data has been durably
    // written to disk. `priority`: 0 is normal priority, 2 is high priority
    fn issue_ncq(
        &mut self,
        slot: u32,
        command: pci::ide_controller::Command,
        sector: usize,
        fua: bool,
        priority: u32,
    ) {
        use pci::ide_controller::Command::*;

        let nsectors = self.dma.ch[slot as usize].buffer_byte_pos / SECTOR_SIZE;
        self.dma.ct[slot as usize].cfis[0] =
            CFIS_COMMAND | ((command as u32) << 16) | ((nsectors & 0xFF) << 24);
        self.dma.ct[slot as usize].cfis[1] =
            (sector as u32 & 0xFFFFFF) | (u32::from(fua) << 31) | 0x40000000;
        self.dma.ct[slot as usize].cfis[2] = ((sector >> 24) as u32) | ((nsectors & 0xFF00) << 16);
        self.dma.ct[slot as usize].cfis[3] = (slot << 3) | (priority << 14);

        self.dma.ch[slot as usize].flags = 4 /* # words in `cfis` */
            | (CHFlag::Clear as u16)
            | (if let WriteFPDMAQueued = command { CHFlag::Write as u16 } else { 0 });
        self.dma.ch[slot as usize].buffer_byte_pos = 0;

        // ensure all previous writes have made it out to memory
        // IMPORTANT: Add this back in when we have multicore and have implemented
        // atomic std::atomic_thread_fence(std::sync::atomic::Ordering::Release);

        (*self.port_registers).ncq_active = 1 << slot; // tell interface NCQ slot used
        (*self.port_registers).command_mask = 1 << slot; // tell interface command available
                                                         // The write to `command_mask` wakes up the device.

        self.slots_outstanding_mask |= 1 << slot; // remember slot
        self.num_slots_available -= 1;
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

    fn handle_interrupt(&mut self) {
        unsafe {
            // use super::RStatusMasks;
            // let is_error = ((*self.port_registers).interrupt_status
            //     & InterruptMasks::FatalErrorMask as u32
            //     != 0)
            //     || ((*self.port_registers).tfd & RStatusMasks::Error as u32 != 0);
            let mut drive_registers = self.drive_registers.write();
            (*self.port_registers).interrupt_status = !0;
            (*drive_registers).interrupt_status = !0;
            let mut acks =
                self.slots_outstanding_mask & !((*self.port_registers).ncq_active as u16);
            let mut slot = 0;
            while acks != 0 {
                if acks & 1 != 0 {
                    self.acknowledge(slot, 0);
                }
                acks >>= 1;
                slot += 1;
            }
        }
    }

    fn issue_meta(
        &mut self,
        slot: u32,
        command: pci::ide_controller::Command,
        features: u32,
        count: u32,
    ) {
        use pci::ide_controller::Command::*;

        let mut num_sectors = self.dma.ch[slot as usize].buffer_byte_pos / SECTOR_SIZE;

        if let SetFeatures = command {
            if count != u32::MAX {
                num_sectors = count;
            }
        }

        self.dma.ct[slot as usize].cfis[0] =
            CFIS_COMMAND | ((command as u32) << 16) | (features << 24);
        self.dma.ct[slot as usize].cfis[1] = 0;
        self.dma.ct[slot as usize].cfis[2] = ((features as u32) & 0xFF00) << 16;
        self.dma.ct[slot as usize].cfis[3] = num_sectors;

        self.dma.ch[slot as usize].flags = 4 | (CHFlag::Clear as u16);
        self.dma.ch[slot as usize].buffer_byte_pos = 0;

        // IMPORTANT: Uncomment once multicore and atomic are done
        // std::atomic_thread_fence(std::memory_order_release);

        // tell interface command is available
        (*self.port_registers).command_mask = 1 << slot;

        self.slots_outstanding_mask |= 1 << slot;
        self.num_slots_available -= 1;
    }

    fn clear_slot<'b, T>(&mut self, handle: BufferHandle<'b, T>) {
        self.dma.ch[handle.slot as usize].num_buffers = 0;
        self.dma.ch[handle.slot as usize].buffer_byte_pos = 0;
    }

    unsafe fn await_basic(&mut self, slot: u32) {
        while (*self.port_registers).command_mask & (1u32 << slot) != 0 {
            pause();
        }

        unsafe { self.acknowledge(slot, 0) };
    }

    unsafe fn acknowledge(&mut self, slot: u32, result: u32) {
        self.slots_outstanding_mask ^= 1u16 << slot;
        self.num_slots_available += 1;

        // technically this is safe because this is only called when locked, but this is basically
        // like juggling knives.. fixme?
        if SLOT_STATUS[slot as usize].is_null() {
            unsafe { SLOT_STATUS[slot as usize].write_volatile(result) };
            SLOT_STATUS[slot as usize] = core::ptr::null_mut();
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

#[repr(u16)]
enum CHFlag {
    Clear = 0x400,
    Write = 0x40,
}

#[repr(u32)]
enum InterruptMasks {
    DeviceToHost = 0x1,
    NCQComplete = 0x8,
    ErrorMask = 0x7D800010,
    FatalErrorMask = 0x78000000, // HBFS|HBDS|IFS|TFES
}
