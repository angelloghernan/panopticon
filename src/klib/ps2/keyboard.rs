use spin::Mutex;
use lazy_static::lazy_static;
use crate::klib::containers::circular_buffer;
use circular_buffer::CircularBuffer;
use crate::klib::ps2::controller::Ps2Controller;

lazy_static! {
    pub static ref KEYBOARD: Mutex<Keyboard> = Mutex::new(Keyboard::new());
}

pub struct Keyboard {
    key_buffer: CircularBuffer<512, Key>,
    cmd_buffer: CircularBuffer<256, Command>,
    controller: Ps2Controller,
}


impl Keyboard {
    pub fn new() -> Self {
        Self {
            key_buffer: CircularBuffer::new(),
            cmd_buffer: CircularBuffer::new(),
            controller: Ps2Controller {},
        }
    }

    pub fn enable(&mut self) {
        self.controller.enable_first()
    }

    pub fn enqueue_command(&mut self, command: Command) -> Result<(), ()> {
        if self.cmd_buffer.empty() {
            self.send_command(command)
        } else {
            self.cmd_buffer.push_back(command);
            Ok(())
        }
    }

    pub fn send_next_command(&mut self) -> Result<(), ()> {
        match self.cmd_buffer.pop_back() {
            Some(command) => self.send_command(command),
            None => Ok(())
        }
    }

    fn send_command(&mut self, command: Command) -> Result<(), ()> {
        use Command::*;

        self.controller.nonblocking_write(command.into())?;
        match command {
            SetLEDs(state) => self.controller.nonblocking_write(state.0)?,
            GetSetScanCodeSet(set) => self.controller.nonblocking_write(set as u8)?,
            _ => {},
        }

        Ok(())
    }

    pub fn read_byte(&mut self) -> Result<u8, ()> {
        self.controller.nonblocking_read()
    }
}

#[repr(u8)]
#[derive(Copy, Clone)]
pub enum Command {
    SetLEDs(LEDState)              = 0xED,
    Echo                           = 0xEE,
    GetSetScanCodeSet(ScanCodeSet) = 0xF0,
    IdentifyKeyboard               = 0xF2,
    Enable                         = 0xF4,
    Disable                        = 0xF5,
    SetDefault                     = 0xF6,
    ResendLastByte                 = 0xFE,
    ResetAndSelfTest               = 0xFF,
}

impl From<Command> for u8 {
    /// Safe since we specified its layout to have a u8 layout
    /// This specific code is adapted from the Rust reference.
    fn from(command: Command) -> Self {
        unsafe { *(&command as *const Command as *const u8) }
    }
}


#[repr(transparent)]
#[derive(Copy, Clone)]
pub struct LEDState(u8);

impl LEDState {
    #[inline]
    pub fn new() -> Self {
        Self(0)
    }
    
    #[inline]
    pub fn enable_scroll_lock(&mut self) -> &mut Self {
        self.0 |= 0b1;
        self
    }

    #[inline]
    pub fn enable_num_lock(&mut self) -> &mut Self {
        self.0 |= 0b10;
        self
    }

    #[inline]
    pub fn enable_caps_lock(&mut self) -> &mut Self {
        self.0 |= 0b100;
        self
    }
}

#[repr(u8)]
#[derive(Copy, Clone)]
pub enum ScanCodeSet {
    GetSet = 0,
    One = 1,
    Two = 2,
    Three = 3,
}

pub enum Key {
   Upper(KeyCode),
   Lower(KeyCode),
   Special(KeyCode),
}


#[repr(u8)]
pub enum KeyCode {
    A = 0,
}
