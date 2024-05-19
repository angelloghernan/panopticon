pub mod ide_controller;
pub mod pcistate;
use bitfield::bitfield;

const CONFIG_ADDRESS: u32 = 0xCF8;
const CONFIG_DATA: u32 = 0xCFC;
const NO_VENDOR: u16 = 0xFFFF;
const NO_DEVICE: u16 = 0xFFFF;

#[repr(u8)]
#[derive(Clone, Copy)]
pub enum Register {
    VendorId = 0x0,
    DeviceId = 0x2,
    Command = 0x4,
    Status = 0x6,
    RevisionId = 0x8,
    ProgIF = 0x9,
    Subclass = 0xA,
    ClassCode = 0xB,
    CacheLineSize = 0xC,
    LatencyTimer = 0xD,
    HeaderType = 0xE,
    BIST = 0xF,
    GDBaseAddress0 = 0x10,
    GDBaseAddress1 = 0x14,
    GDBaseAddress2 = 0x18,
    GDBaseAddress3 = 0x1C,
    GDBaseAddress4 = 0x20,
    GDBaseAddress5 = 0x24,
    CardBusCISPointer = 0x28,
    SubsystemVendorID = 0x2C,
    SubsystemID = 0x2E,
    ExpansionROMBaseAddr = 0x30,
    CapabilitiesPointer = 0x34,
    InterruptLine = 0x3C,
    InterruptPIN = 0x3D,
    MinGrant = 0x3E,
    MaxLatency = 0x3F,
}

#[repr(u8)]
enum HeaderType {
    GeneralDevice = 0x0,
    PciToPci = 0x1,
    PciToCardBus = 0x2,
    MultiFunction,
    Unknown,
}

bitfield! {
    #[derive(Clone, Copy)]
    pub struct CommandRegister(u16);
    bool;
    pub io_space, set_io_space: 0;
    pub memory_space, set_memory_space: 1;
    pub bus_master, set_bus_master: 2;
    pub special_cycles, _: 3;
    pub memory_write_invalidate_enable, _: 4;
    pub vga_palette_snoop, _: 5;
    pub parity_error_response, set_parity_error_response: 6;
    // bit 7 is reserved
    pub serr_no_enable, set_serr_no: 8;
    pub fast_back_to_back_enable, _: 9;
    pub interrupt_disable, set_interrupt_disable: 10;
}

bitfield! {
    #[derive(Clone, Copy)]
    pub struct StatusRegister(u16);
    // bits 0-2 are reserved
    bool;
    pub interrupt_status, _: 3;
    pub capabilities_list, _: 4;
    pub mhz_66_capable, _: 5;
    // bit 6 is reserved
    pub fast_back_to_back_capable, _: 7;
    pub master_data_parity_error, _: 8;
    u8;
    pub devsel_timing, _: 10, 9;
    bool;
    pub signaled_target_abort, _: 11;
    pub received_target_abort, _: 12;
    pub received_master_abort, _: 13;
    pub signaled_system_error, _: 14;
    pub detected_parity_error, _: 15;
}
