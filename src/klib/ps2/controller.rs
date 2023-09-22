use lazy_static::lazy_static;
use crate::klib::x86_64;
use spin::Mutex;

const DATA_PORT: u16            = 0x60;
const CMD_STATUS_REGISTER: u16  = 0x64;

const ENABLE_FIRST_PORT: u8     = 0xAE;
const DISABLE_FIRST_PORT: u8    = 0xAD;

const SELF_CHECK_SUCCESS: u8    = 0x55;

const PERFORM_SELF_CHECK: u8    = 0xAA;

/// TODO: We don't really follow through on all steps for initialization, but eventually we should
/// for completeness. In most cases the PS/2 controller should be initialized correctly.
pub struct Ps2Controller {}

impl Ps2Controller {
    /// Enable the first PS2 port. This is the only port that can be reliably enabled.
    pub fn enable_first(&mut self) {
        unsafe { 
            x86_64::port_write_u8(CMD_STATUS_REGISTER, ENABLE_FIRST_PORT);
            x86_64::port_write_u8(CMD_STATUS_REGISTER, 0x60);
            while x86_64::port_read_u8(CMD_STATUS_REGISTER) & 0b10 != 0 {
                x86_64::io_wait();
            }
            x86_64::port_write_u8(CMD_STATUS_REGISTER, 0b101)
        }
    }

    /// Start a non-blocking read of the port. This will attempt to read a byte from the first
    /// device port, returning None if a byte is not received within three attempts.
    pub fn nonblocking_read(&mut self) -> Result<u8, ()> {
        let mut count = 0;
        unsafe {
            while (x86_64::port_read_u8(CMD_STATUS_REGISTER) & 0b1) != 1 && count < 3 {
                x86_64::io_wait();
                count += 1
            }
        }

        if count == 3 {
            Err(())
        } else {
            unsafe { Ok(x86_64::port_read_u8(DATA_PORT)) }
        }
    }


    pub fn nonblocking_write(&mut self, val: u8) -> Result<(), ()> {
        let mut count = 0;
        unsafe {
            while (x86_64::port_read_u8(CMD_STATUS_REGISTER) & 0b10) != 1 && count < 3 {
                x86_64::io_wait();
                count += 1
            }
        }


        unsafe { x86_64::port_write_u8(DATA_PORT, val) };

        if count == 3 {
            Err(())
        } else {
            Ok(())
        }
    }

    /// Read a byte from this PS/2 controller. Does not check if a byte is ready or not, so this is
    /// an unsafe operation (can end up giving junk data)
    #[inline]
    pub unsafe fn read_raw(&mut self) -> u8 {
        unsafe { x86_64::port_read_u8(DATA_PORT) }
    }

    /// Write a byte to this PS/2 controller. Does not check when a byte is ready to write or not, so this is
    /// an unsafe operation
    #[inline]
    pub unsafe fn write_raw(&mut self, byte: u8) {
        unsafe { x86_64::port_write_u8(DATA_PORT, byte) }
    }
}
