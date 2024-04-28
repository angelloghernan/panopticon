pub mod ahcistate;
// Made with help from Chickadee OS source (https://github.com/CS161/chickadee/)
// And of course, my own source code from this class (although I am obliged to make that code private)

#[repr(u8)]
enum FISType {
    RegHostToDevice = 0x27,
    RegDeviceToHost = 0x34,
    DmaActivate = 0x39, // activate DMA transfer
    DmaSetup = 0x41,    // bidirectional
    Data = 0x46,        // bidirectional
    Bist = 0x58,        // Built In Self Test
    PioSetup = 0x5F,    // setting up PIO (slower than DMA)
    SetDeviceBits = 0xA1,
}

#[repr(C)]
struct PortRegisters {
    cmdlist_addr: u64,          // PxCLB -- Port x Command List Base Address
    rfis_base_addr: u64, // PxRFIS -- Port x RFIS Base Address -- the base address of rfis_state
    interrupt_status: u32, // PxIS
    interrupt_enable: u32, // PxIE
    command_and_status: u32, // PxCMD -- Port x Command and Status
    reserved2: u32,      // 0x2C - 0x2F are reserved
    tfd: u32,            // PxTFD -- Port x Task File Data
    sig: u32,            // PxSIG -- Port x Signature
    sstatus: u32,        // PxSSTS -- Port x SATA Status, 0 = no device detected
    scontrol: u32,       // PxSCTL -- Port x SATA Control
    serror: u32,         // PxSERR -- Port x SATA Error
    ncq_active: u32,     // PxSACT -- Port x SATA Active
    command_mask: u32,   // PxCI -- Port x Command Issue
    sata_notification: u32, // PxSNTF -- Port x SATA Notification
    fis_switch_control: u32, // PxFBS -- Port x FIS-based switching control
    device_sleep: u32,   // PxDEVSLP -- Port x Device Sleep
    vendor_specific: [u32; 14], // PxVS -- Port x Vendor Specific (ignore)
}

struct Registers {
    capabilities: u32,       // CAP: HBA capabilities [R]
    global_hba_control: u32, // GHC: global HBA control [R/W]
    interrupt_status: u32,   // IS: interrupt status
    port_mask: u32,          // PI: addressable ports
    ahci_version: u32,       // VS: AHCI version
    ccc_control: u32,        // CCC_CTL: Command Completion Coalescing Control
    ccc_port_mask: u32,      // CCC_PORTS
    em_loc: u32,             // EM_LOC: Enclosure Management Location
    em_control: u32,         // EM_CTL: Enclosure Management Control
    cap2: u32,               // CAP2: HBA Capabilities extended
    bohc: u32,               // BOHC: BIOS/OS Handoff Control and Status
    reserved: [u32; 53],     // Vendor specific registers
    port_regs: [PortRegisters; 32],
}

#[repr(u32)]
enum PortCommandMasks {
    InterfaceMask = 0xF0000000,
    InterfaceActive = 0x10000000,
    InterfaceIdle = 0x0,
    CommandRunning = 0x8000,
    RFISRunning = 0x4000,
    RFISEnable = 0x10,
    RFISClear = 0x8,
    PowerUp = 0x6,
    Start = 0x1,
}

#[repr(u32)]
enum RStatusMasks {
    Busy = 0x80,
    DataReq = 0x8,
    Error = 0x1,
}

#[repr(u32)]
enum InterruptMasks {
    DeviceToHost = 0x1,
    NCQComplete = 0x8,
    ErrorMask = 0x7D800010,
    FatalErrorMask = 0x78000000, // HBFS|HBDS|IFS|TFES
}

#[repr(u32)]
enum GHCMasks {
    InterruptEnable = 0x2,
    AHCIEnable = 0x80000000,
}

// DMA structures for device comm.
// The disk drive uses these to communicate with the OS.

// PRD -- this is distinct from the ATA PRD/PRDT
struct PRD {
    address: u64,
    reserved: u32,
    data_byte_count: u32, // Bit 31: Interrupt on completion flag
                          // The byte count is the number of bytes in the buffer - 1
                          // Technically, the bits [30:22] are reserved, but we do not expect this to ever matter
}

#[repr(align(128))]
struct CommandTable {
    cfis: [u32; 16], // Command definitions
    acmd: [u32; 4],
    reserved: [u32; 12],
    prdt: [PRD; 16],
}

struct CommandHeader {
    flags: u16,
    num_buffers: u16,
    buffer_byte_pos: u32,
    command_table_address: u32,
    reserved_64: u32, // IMPORTANT: When porting, change above u32 to u64
    reserved: [u64; 2],
}

#[repr(align(256))]
struct RFISState {
    rfis: [u32; 64],
}

#[repr(align(1024))]
struct DMAState {
    ch: [CommandHeader; 32],
    rfis: RFISState, // This is volatile.
    ct: [CommandTable; 32],
}

const CFIS_COMMAND: u32 = 0x8027;

#[repr(u32)]
enum CHFlag {
    Clear = 0x400,
    Write = 0x40,
}
