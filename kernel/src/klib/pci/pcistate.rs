use super::Register;
use crate::klib::pci::{CommandRegister, CONFIG_ADDRESS, CONFIG_DATA};
use x86_64::instructions::port::Port;
use x86_64::structures::port::PortWrite;

const MAX_BUSES: u32 = 0x8;
const MAX_SLOTS: u32 = 32;
const MAX_FUNCS: u32 = 8;

pub struct PCIState {}

impl PCIState {
    pub fn new() -> Self {
        Self {}
    }

    pub unsafe fn config_read_32(
        &self,
        bus: u32,
        slot: u32,
        func_number: u32,
        offset: Register,
    ) -> u32 {
        let address = pci_address(bus, slot, func_number, offset);

        let mut address_port = Port::new(CONFIG_ADDRESS as u16);
        address_port.write(address);

        let mut data_port: Port<u32> = Port::new(CONFIG_DATA as u16);

        // Magic: read the first word (16 bits) of the data register
        return data_port.read();
    }

    pub unsafe fn config_read_16(
        &self,
        bus: u32,
        slot: u32,
        func_number: u32,
        offset: Register,
    ) -> u16 {
        let address = pci_address(bus, slot, func_number, offset);

        let mut address_port = Port::new(CONFIG_ADDRESS as u16);
        address_port.write(address);

        let mut data_port: Port<u32> = Port::new(CONFIG_DATA as u16);

        // Magic: read the first word (16 bits) of the data register
        return ((data_port.read() >> ((offset as u8 & 2) * 8)) & 0xFFFF) as u16;
    }

    pub unsafe fn config_read_8(
        &self,
        bus: u32,
        slot: u32,
        func_number: u32,
        offset: Register,
    ) -> u8 {
        let word = self.config_read_16(bus, slot, func_number, offset);
        let offset_u8 = offset as u8;

        if offset_u8 & 0b1 > 0 {
            return (word >> 8) as u8;
        } else {
            return (word & 0xFF) as u8;
        }
    }

    pub unsafe fn config_write<T>(
        &mut self,
        bus: u32,
        slot: u32,
        func_number: u32,
        offset: Register,
        data: T,
    ) where
        T: PortWrite,
    {
        let address = pci_address(bus as u32, slot as u32, func_number as u32, offset);

        let mut address_port = Port::new(CONFIG_ADDRESS as u16);
        address_port.write(address);
        let mut data_port: Port<T> = Port::new(CONFIG_DATA as u16);

        unsafe { data_port.write(data) }
    }

    pub unsafe fn enable_interrupts(&mut self, bus: u32, slot: u32, func_number: u32) {
        let bytes = self.config_read_16(bus, slot, func_number, Register::Command);

        let mut command = CommandRegister(bytes);

        command.set_interrupt_disable(false);

        self.config_write(bus, slot, func_number, Register::Command, command.0);
    }

    /// Find and return the next valid PCI device address in a (bus, slot, func) tuple.
    pub unsafe fn next_addr(
        &mut self,
        mut bus: u32,
        mut slot: u32,
        mut func: u32,
    ) -> (u32, u32, u32) {
        let lthbc = self.config_read_32(bus, slot, func, Register::CacheLineSize);

        loop {
            if func == 0 && (lthbc == u32::MAX || (lthbc & 0x800000 == 0)) {
                slot += 1;
                if slot >= MAX_SLOTS {
                    slot = 0;
                    bus += 1;
                }
            } else {
                func += 1;
                if func >= MAX_FUNCS {
                    func = 0;
                    slot += 1;
                }
            }

            if bus >= MAX_BUSES {
                return (bus, slot, func);
            }

            let lthbc = self.config_read_32(bus, slot, func, Register::CacheLineSize);

            if lthbc != 0xFF {
                return (bus, slot, func);
            }
        }
    }
}

fn pci_address(bus: u32, slot: u32, func_number: u32, offset: Register) -> u32 {
    let offset_u32 = offset as u8 as u32;

    // Layout:
    // Bit 31: Enable bit
    // Bits 30-24: Reserved
    // Bits 23-16: Bus number
    // Bits 15-11: Slot/device number
    // Bits 10-8: Function number
    // Bits 7-0: Register offset
    (0x80000000u32) | (bus << 16) | (slot << 11) | (func_number << 8) | (offset_u32 & 0xFC)
}
