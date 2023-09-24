use lazy_static::lazy_static;
use crate::klib::x86_64;
use spin::Mutex;

const BASE_COMMAND_PORT: u16 = 0x20;
const BASE_DATA_PORT: u16 = 0x21;

const HIGHER_COMMAND_PORT: u16 = 0xA0;
const HIGHER_DATA_PORT: u16 = 0xA1;

const ICW1_INIT: u8 = 0x10;
const ICW1_ICW4: u8 = 0x1;

const ICW4_8086: u8 = 0x1;

const END_OF_INTERRUPT: u8 = 0x20;

const PIC_IRQ_OFFSET: u8 = 0x20;

#[repr(u8)]
pub enum Irq {
    Timer = 0x0,
    Keyboard = 0x1,
}

lazy_static! {
    pub static ref PIC: Mutex<PicPair> = { 
        Mutex::new(PicPair::new(PIC_IRQ_OFFSET, PIC_IRQ_OFFSET + 8))
    };
}

#[derive(Debug)]
pub struct PicPair {
    pub base_pic: Pic,
    pub higher_pic: Pic,
}

#[derive(Debug)]
pub struct Pic {
    /// The interrupt vector offset this PIC handles
    offset: u8,
}

impl PicPair {
    /// Creates and initializes both PIC interfaces.
    pub fn new(offset1: u8, offset2: u8) -> Self {
        Self {
            base_pic: Pic { offset: offset1 },
            higher_pic: Pic { offset: offset2 },
        }
    }

    /// ## Safety
    /// The function should only be called on one PicPair object, ever. Calling more than once will result in undefined
    /// behavior. In addition, both offsets should have a distance of 8 from each other.
    pub unsafe fn initialize(&mut self) {
        let mask1 = x86_64::port_read_u8(BASE_DATA_PORT);
        let mask2 = x86_64::port_read_u8(HIGHER_DATA_PORT);

        x86_64::port_write_u8(BASE_COMMAND_PORT, ICW1_INIT | ICW1_ICW4);
        x86_64::io_wait();
        x86_64::port_write_u8(HIGHER_COMMAND_PORT, ICW1_INIT | ICW1_ICW4);
        x86_64::io_wait();

        x86_64::port_write_u8(BASE_DATA_PORT, self.base_pic.offset);
        x86_64::io_wait();
        x86_64::port_write_u8(HIGHER_DATA_PORT, self.higher_pic.offset);
        x86_64::io_wait();

        // Set up chaining on these PICs
        x86_64::port_write_u8(BASE_DATA_PORT, 0x04);
        x86_64::io_wait();
        x86_64::port_write_u8(HIGHER_DATA_PORT, 0x02);
        x86_64::io_wait();

        x86_64::port_write_u8(BASE_DATA_PORT, ICW4_8086);
        x86_64::io_wait();
        x86_64::port_write_u8(HIGHER_DATA_PORT, ICW4_8086);
        x86_64::io_wait();

        self.write_interrupt_masks(mask1, mask2);
    }

    #[inline]
    pub unsafe fn write_interrupt_masks(&mut self, mask1: u8, mask2: u8) {
        x86_64::port_write_u8(BASE_DATA_PORT, mask1);
        x86_64::port_write_u8(HIGHER_DATA_PORT, mask2);
    }

    #[inline]
    pub unsafe fn disable(&mut self) {
        self.write_interrupt_masks(0xFF, 0xFF)
    }

    #[inline]
    pub unsafe fn enable_all(&mut self) {
        self.write_interrupt_masks(0x00, 0x00)
    }

    pub unsafe fn end_of_interrupt(&mut self, irq_offset: u8) {
        if self.higher_pic.handles_interrupt(irq_offset) {
            x86_64::port_write_u8(HIGHER_COMMAND_PORT, END_OF_INTERRUPT);
        }

        x86_64::port_write_u8(BASE_COMMAND_PORT, END_OF_INTERRUPT)
    }
}

impl Pic {
    #[inline]
    pub fn handles_interrupt(&self, irq_offset: u8) -> bool {
        irq_offset <= self.offset && self.offset < irq_offset + 8
    }
}
