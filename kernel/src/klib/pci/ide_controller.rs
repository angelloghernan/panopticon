use super::super::super::sleep;
use super::super::util::Volatile;
use super::super::x86_64::{port_read_u8, port_write_u8};
use crate::print;
use crate::println;

#[repr(C)]
pub struct IDEController {
    channel_registers: [ChannelRegister; 2],
    devices: [Device; 4],
    prdts: [PRDT; 4],
    buffer: [u8; 2048],
    atapi_packet: [u8; 12],
    irq_invoked: Volatile<bool>,
    bus: u8,
    slot: u8,
    mode: Mode,
}

impl IDEController {
    pub fn read(&mut self, channel_type: ChannelType, reg: Register) -> u8 {
        let u8_channel = channel_type as u8;

        let channel = self.channel_registers[u8_channel as usize];

        let reg_type = reg.to_register_type();

        if let RegisterType::HighLevel = reg_type {
            self.enable_hob(channel_type);
        }

        let u16_reg = reg as u16;

        let result = unsafe {
            match reg_type {
                RegisterType::LowLevel => port_read_u8(channel.io_base + u16_reg),
                RegisterType::HighLevel => port_read_u8(channel.io_base + u16_reg - 0x06),
                RegisterType::DeviceControlOrStatus => {
                    port_read_u8(channel.control + u16_reg - 0x0A)
                }
                RegisterType::BusMasterIDE => port_read_u8(channel.bus_master_ide + u16_reg - 0x0E),
            }
        };

        if let RegisterType::HighLevel = reg_type {
            self.disable_hob(channel_type);
        }

        result
    }

    pub fn write(&mut self, channel_type: ChannelType, reg: Register, data: u8) {
        let usize_channel = channel_type as usize;

        let channel = self.channel_registers[usize_channel];

        let reg_type = reg.to_register_type();

        if let RegisterType::HighLevel = reg_type {
            self.enable_hob(channel_type);
        }

        let u16_reg = reg as u16;

        unsafe {
            match reg_type {
                RegisterType::LowLevel => port_write_u8(channel.io_base + u16_reg, data),
                RegisterType::HighLevel => port_write_u8(channel.io_base + u16_reg - 0x06, data),
                RegisterType::DeviceControlOrStatus => {
                    port_write_u8(channel.control + u16_reg - 0x0A, data)
                }
                RegisterType::BusMasterIDE => {
                    port_write_u8(channel.bus_master_ide + u16_reg - 0x0E, data)
                }
            }
        };

        if let RegisterType::HighLevel = reg_type {
            self.disable_hob(channel_type);
        }
    }

    pub fn read_buffer(&mut self, channel: ChannelType, reg: Register, count: u32) {}

    pub fn read_drive_dma(channel: ChannelType) {}

    pub fn enable_hob(&mut self, channel_type: ChannelType) {
        let u8_channel = channel_type as u8;
        let channel = self.channel_registers[u8_channel as usize];
        let en_hob = ControlBits::HighOrderByte as u8 | channel.no_interrupts as u8;

        self.write(channel_type, Register::Control, en_hob);
    }

    pub fn disable_hob(&mut self, channel_type: ChannelType) {}

    pub fn new(bar_4: u16) -> Self {
        // safety: it doesn't even matter what values the IDE controller struct starts out with. i'm just
        // doing this for convenience since MaybeUninit carries with it a lot more constraints
        let mut controller: IDEController = unsafe { core::mem::zeroed() };
        controller.channel_registers[0].io_base = 0x1F0;
        controller.channel_registers[0].control = 0x3F6;
        controller.channel_registers[0].bus_master_ide = bar_4;

        controller.channel_registers[1].io_base = 0x170;
        controller.channel_registers[1].control = 0x376;
        controller.channel_registers[1].bus_master_ide = bar_4 + 8;

        let disable_interrupt = ControlBits::InterruptDisable as u8;

        controller.write(ChannelType::Primary, Register::Control, disable_interrupt);
        controller.write(ChannelType::Secondary, Register::Control, disable_interrupt);
        controller.detect_drives();
        controller
    }

    pub fn detect_drives(&mut self) {
        let mut count = 0;
        for i in 0..2 {
            for j in 0..2 {
                self.devices[count].reserved = false;

                let select_master = 0xA0 | (j << 4);

                let channel = if i == 0 {
                    ChannelType::Primary
                } else {
                    ChannelType::Secondary
                };
                let control_type = if j == 0 {
                    ControlType::Master
                } else {
                    ControlType::Slave
                };

                self.write(channel, Register::HDDevSel, select_master);

                sleep(1);

                self.write(channel, Register::CommandOrStatus, Command::Identify as u8);

                sleep(1);
                let drive = if i == 0 {
                    "Master drive"
                } else {
                    "Slave drive"
                };
                let channel_type = if j == 0 { "primary" } else { "secondary" };

                if self.read(channel, Register::CommandOrStatus) == 0 {
                    println!("{} {} channel is inactive", drive, channel_type);
                    continue;
                }

                let mut had_error = false;

                loop {
                    let status = self.read(channel, Register::CommandOrStatus);
                    if status & Status::Error as u8 != 0 {
                        println!("Drive {} {} had an error", i, j);
                        had_error = true;
                        break;
                    }

                    let busy = status & Status::Busy as u8;
                    let ready = status & Status::DataRequestReady as u8;

                    if busy == 0 && ready != 0 {
                        break;
                    }
                }

                let mut if_type = InterfaceType::ATA;

                println!("DRIVE REPORT: {} {}", drive, channel_type);

                if !had_error {
                    let c_lower = self.read(channel, Register::LBA1);
                    let c_higher = self.read(channel, Register::LBA2);
                    print!("Interface type: ");

                    if (c_lower == 0x69 && c_higher == 0x96) || c_lower == 0x14 {
                        if_type = InterfaceType::ATAPI;
                        println!("ATAPI");
                    } else {
                        println!(
                            "Unknown. LBA1/2 is {:?} and {:?}. Assuming ATA.",
                            c_lower as *const (), c_higher as *const ()
                        );
                    }

                    self.write(
                        channel,
                        Register::CommandOrStatus,
                        Command::IdentifyPacket as u8,
                    );
                    sleep(1);
                }

                self.read_buffer(channel, Register::Data, 256);

                let buf_ptr = self.buffer.as_ptr();

                self.devices[count].reserved = true;
                self.devices[count].interface_type = if_type;
                self.devices[count].channel_type = channel;
                self.devices[count].control_type = control_type;
                self.devices[count].drive_signature =
                    unsafe { *(buf_ptr.add(IdentityField::DeviceType as usize) as *const u16) };
                self.devices[count].capabilities =
                    unsafe { *(buf_ptr.add(IdentityField::Capabilities as usize) as *const u16) };
                self.devices[count].command_sets =
                    unsafe { *(buf_ptr.add(IdentityField::CommandSets as usize) as *const u32) };

                print!("Addressing scheme: ");

                if self.devices[count].command_sets & (1 << 26) != 0 {
                    println!("48-bit");
                    self.devices[count].size =
                        unsafe { *(buf_ptr.add(IdentityField::MaxLBAExt as usize) as *const u32) };
                } else {
                    println!("32-bit");
                    self.devices[count].size =
                        unsafe { *(buf_ptr.add(IdentityField::MaxLBA as usize) as *const u32) };
                }

                print!("Name: ");

                for k in (0..40).step_by(2) {
                    self.devices[count].model[k] =
                        self.buffer[IdentityField::Model as usize + k + 1];
                    self.devices[count].model[k + 1] =
                        self.buffer[IdentityField::Model as usize + k];

                    if self.devices[count].model[k] == 0 {
                        break;
                    }
                    print!("{}", self.devices[count].model[k] as char);

                    if self.devices[count].model[k + 1] == 0 {
                        break;
                    }
                    print!("{}", self.devices[count].model[k + 1] as char);
                }

                println!();
                self.devices[count].model[40] = 0;
                println!(
                    "Size: {} total sectors, or {} MB",
                    self.devices[count].size,
                    self.devices[count].size * 512 / (1024 * 1024)
                );

                println!();

                count += 1;
            }
        }
    }
}

pub enum RegisterType {
    HighLevel,
    LowLevel,
    DeviceControlOrStatus,
    BusMasterIDE,
}

#[repr(C)]
struct Device {
    size: u32,         // size in sectors
    command_sets: u32, // supported command sets
    drive_signature: u16,
    capabilities: u16,
    model: [u8; 41], // drive model name (string)
    channel_type: ChannelType,
    control_type: ControlType,
    interface_type: InterfaceType,
    reserved: bool,
}

enum Status {
    Busy = 0x80,
    ReadyDrive = 0x40,
    DriveWriteFault = 0x20,
    DriveSeekComplete = 0x10,
    DataRequestReady = 0x08,
    CorrectedData = 0x04,
    Index = 0x02,
    Error = 0x01,
}

enum Error {
    BadBlock = 0x80,
    Uncorrectable = 0x40,
    MediaChanged = 0x20,
    IDMarkNotFound = 0x10,
    MediaChangeRequest = 0x08,
    CommandAborted = 0x04,
    Track0NotFound = 0x02,
    NoAddressMark = 0x01,
}

#[derive(Clone, Copy)]
pub enum Command {
    ReadPIO = 0x20,
    ReadPIOExt = 0x24,
    ReadDMA = 0xC8,
    ReadDMAExt = 0x25,
    WritePIO = 0x30,
    WritePIOExt = 0x34,
    WriteDMA = 0xCA,
    WriteDMAExt = 0x35,
    CacheFlush = 0xE7,
    CacheFlushExt = 0xEA,
    Packet = 0xA0,
    IdentifyPacket = 0xA1,
    Identify = 0xEC,
    ReadFPDMAQueued = 0x60,
    WriteFPDMAQueued = 0x61,
    SetFeatures = 0xEF,
}

#[derive(Clone, Copy)]
pub enum Register {
    Data = 0x00,
    ErrorOrFeatures = 0x01,
    SecCount0 = 0x02,
    LBA0 = 0x03,
    LBA1 = 0x04,
    LBA2 = 0x05,
    HDDevSel = 0x06,
    CommandOrStatus = 0x07,
    SecCount1 = 0x08,
    LBA3 = 0x09,
    LBA4 = 0x0A,
    LBA5 = 0x0B,
    Control = 0x0C,
    DevAddress = 0x0D,
}

impl Register {
    pub fn to_register_type(&self) -> RegisterType {
        let u8_reg = *self as u8;

        if u8_reg < 0x08 {
            RegisterType::LowLevel
        } else if u8_reg >= 0x08 && u8_reg <= 0x0B {
            RegisterType::HighLevel
        } else if u8_reg < 0x0E {
            RegisterType::DeviceControlOrStatus
        } else {
            RegisterType::BusMasterIDE
        }
    }
}

enum BusMasterRegister {
    Command = 0x0,
    Status = 0x2,
    PRDTAddress = 0x4,
}

enum ATAPICommand {
    Read = 0xA8,
    Eject = 0x1B,
}

enum InterfaceType {
    ATA = 0x0,
    ATAPI = 0x1,
}

enum ControlType {
    Master = 0x0,
    Slave = 0x1,
}

#[derive(Clone, Copy)]
pub enum ChannelType {
    Primary = 0x0,
    Secondary = 0x1,
}

enum IdentityField {
    DeviceType = 0,
    Cylinders = 2,
    Heads = 6,
    Sectors = 12,
    Serial = 20,
    Model = 54,
    Capabilities = 98,
    FieldValid = 106,
    MaxLBA = 120,
    CommandSets = 164,
    MaxLBAExt = 200,
}

enum ControlBits {
    HighOrderByte = 0b10000000,    // 1 = Use 48-LBA addressing
    SoftwareReset = 0b00000100,    // 1 = Reset the device
    InterruptDisable = 0b00000010, // 1 = Disable interrupts
}

#[repr(u8)]
enum Mode {
    Native,
    Compatibility,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct ChannelRegister {
    io_base: u16,
    control: u16,
    bus_master_ide: u16,
    no_interrupts: bool,
}

#[repr(C)]
struct PRDEntry {
    pub buffer_address: u32, // address of the buffer
    pub buffer_size: u16,    // size of the buffer in *bytes*
    last_entry: u16, // reserved, except for MSB indicating this is the last entry, should be 0
}

impl PRDEntry {
    fn set_last_entry_flag(&mut self) {
        self.last_entry |= 0b1 << 15;
    }

    fn clear_last_entry_flag(&mut self) {
        self.last_entry &= !(0b1 << 15);
    }
}

#[repr(u8)]
enum DMAOpMask {
    Read = 0b11111111,
    Write = 0b11110111,
}

#[repr(u8)]
pub enum PRDChannelType {
    Primary = 0x0,
    Secondary = 0x8,
}

#[repr(u8)]
enum StatusBits {
    DriveGeneratedIRQ = 0b100,
    DMAFailed = 0b010,
    InDMAMode = 0b001,
}

// Set up DMA transfer by loading data into the PRDT, setting the bus master register's
// "start" bit, and setting a R/W operation.
// This will stop and restart any DMA operations running on this drive channel,
// so be careful to make sure a DMA is not in progress.
//
// This will not fully start DMA. The IDE controller must be issued a Read/Write DMA
// command.
// pub fn set_up_dma(entries: &[usize, usize], channel: ChannelType, operation: DMAOpMask) ->
// Result<(), ()> {}
// ChannelType channel,
// DMAOpMask operation) -> Result<Null, u16>;
//
// fn status();
//
// pub fn command_start(StartMask bit, ChannelType channel);
// pub fn set_read_write(DMAOpMask operation);

/// Class representing the PRDT (Physical Region Descriptor Table), for use with ATA
//
// This allows us to use DMA to read from disk asynchronously, as opposed to PIO
/// which blocks the CPU.
//
/// The PRDT must be 32-bit (4-byte) aligned and cannot cross a 64k-boundary.
#[repr(C, align(8))]
pub struct PRDT {
    prdt_location: *mut PRDEntry,
    entry_count: u16,
    bus_master_register: u16,
}

impl PRDT {
    pub fn init(
        entry_count: u16,
        bus_master_register: u16,
        channel: PRDChannelType,
    ) -> Result<Self, ()> {
        Err(())
    }
}

#[repr(u8)]
enum BMROffset {
    Command = 0x0,
    Status = 0x2,
    PRDTAddress = 0x4,
}

#[repr(u8)]
enum StartMask {
    Stop = 0b11111110,
    Start = 0b11111111,
}
