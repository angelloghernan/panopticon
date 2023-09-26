use spin::Mutex;
use lazy_static::lazy_static;
use crate::klib::containers::circular_buffer;
use circular_buffer::CircularBuffer;
use crate::klib::ps2::controller::Ps2Controller;

const RELEASE_GAP: u8 = 0x80;

const KEY_TABLE: [u8; 256] = [
    b'\0', b'\0', b'1', b'2', b'3', b'4', b'5', b'6', b'7', b'8', b'9', b'0', b'-', b'=', b'\0', b'\0',
    b'q', b'w', b'e', b'r', b't', b'y', b'u', b'i', b'o', b'p', b'[', b']', b'\0', b'\0', b'a', b's',
    b'd', b'f', b'g', b'h', b'j', b'k', b'l', b';', b'\'', b'`', b'\0', b'\\', b'z', b'x', b'c', b'v',
    b'b', b'n', b'm', b',', b'.', b'/', b'\0', b'\0', b'\0', b' ', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0',
    b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0',
    b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0',
    b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0',
    b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0',
    b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0',
    b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0',
    b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0',
    b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0',
    b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0',
    b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0',
    b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0',
    b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0', b'\0',
];

lazy_static! {
    pub static ref KEYBOARD: Mutex<Keyboard> = Mutex::new(Keyboard::new());
}

pub struct Keyboard {
    key_buffer: CircularBuffer<256, KeyCode>,
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

    pub fn push_key(&mut self, byte: u8) -> Result<(), ()> {
        match KeyCode::from_byte(byte) {
            Some(key) => {
                self.key_buffer.push_back(key);
                Ok(())
            },
            None => {
                Err(())
            }
        }
    }

    pub fn pop_key(&mut self) -> Option<KeyCode> {
        self.key_buffer.pop_back()
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

pub enum KeyCode {
    AsciiUp(u8),
    AsciiDown(u8),
    SpecialUp(SpecialKey),
    SpecialDown(SpecialKey),
    ExtendedDown(ExtendedKeyCode),
    ExtendedUp(ExtendedKeyCode),
}

impl KeyCode {
    fn from_byte(byte: u8) -> Option<Self> {
        use KeyCode::*;
        if let Ok(special_key) = SpecialKey::try_from(byte) {
            if byte > RELEASE_GAP {
                Some(SpecialUp(special_key))
            } else {
                Some(SpecialDown(special_key))
            }
        } else if KEY_TABLE[byte as usize] != b'\0' {
            if byte > RELEASE_GAP {
                Some(AsciiUp(KEY_TABLE[byte as usize]))
            } else {
                Some(AsciiDown(KEY_TABLE[byte as usize]))
            }
        } else {
            None
        }
    }

    fn from_extended_byte(ext_byte: u8) -> Option<Self> {
        use KeyCode::*;
        match ExtendedKeyCode::try_from(ext_byte) {
            Ok(code) => {
                if ext_byte > 0x8F {
                    Some(ExtendedUp(code))
                } else {
                    Some(ExtendedDown(code))
                }
            }
            Err(_) => {
                None
            }
        }
    }
}

#[repr(u8)]
pub enum Key {
    Esc             = 0x01,
    One             = 0x02,
    Two             = 0x03,
    Three           = 0x04,
    Four            = 0x05,
    Five            = 0x06,
    Six             = 0x07,
    Seven           = 0x08,
    Eight           = 0x09,
    Nine            = 0x0A,
    Zero            = 0x0B,
    Dash            = 0x0C,
    Equals          = 0x0D,
    Backspace       = 0x0E,
    Tab             = 0x0F,
    Q               = 0x10,
    W               = 0x11,
    E               = 0x12,
    R               = 0x13,
    T               = 0x14,
    Y               = 0x15,
    U               = 0x16,
    I               = 0x17,
    O               = 0x18,
    P               = 0x19,
    LeftBracket     = 0x1A,
    RightBracket    = 0x1B,
    Enter           = 0x1C,
    LeftCtrl        = 0x1D,
    A               = 0x1E,
    S               = 0x1F,
    D               = 0x20,
    F               = 0x21,
    G               = 0x22,
    H               = 0x23,
    J               = 0x24,
    K               = 0x25,
    L               = 0x26,
    Semicolon       = 0x27,
    SingleQuote     = 0x28,
    BackTick        = 0x29,
    LeftShift       = 0x2A,
    BackSlash       = 0x2B,
    Z               = 0x2C,
    X               = 0x2D,
    C               = 0x2E,
    V               = 0x2F,
    B               = 0x30,
    N               = 0x31,
    M               = 0x32,
    Comma           = 0x33,
    Period          = 0x34,
    Slash           = 0x35,
    RightShift      = 0x36,
    KeypadAsterisk  = 0x37,
    LeftAlt         = 0x38,
    Space           = 0x39,
    CapsLock        = 0x3A,
    F1              = 0x3B,
    F2              = 0x3C,
    F3              = 0x3D,
    F4              = 0x3E,
    F5              = 0x3F,
    F6              = 0x40,
    F7              = 0x41,
    F8              = 0x42,
    F9              = 0x43,
    F10             = 0x44,
    NumberLock      = 0x45,
    ScrollLock      = 0x46,
    KeypadSeven     = 0x47,
    KeypadEight     = 0x48,
    KeypadNine      = 0x49,
    KeypadDash      = 0x4A,
    KeypadFour      = 0x4B,
    KeypadFive      = 0x4C,
    KeypadSix       = 0x4D,
    KeypadPlus      = 0x4E,
    KeypadOne       = 0x4F,
    KeypadTwo       = 0x50,
    KeypadThree     = 0x51,
    KeypadZero      = 0x52,
    KeypadPeriod    = 0x53,
    // Gap of-valid/reserved codes
    PassedSelfTest  = 0x55,
    // Gap of-valid/reserved codes
    F11             = 0x57,
    F12             = 0x58,
    // Gap of non-valid/reserved codes
    NextIsExtended  = 0xE0,
}

#[repr(u8)]
pub enum ExtendedKeyCode {
    PreviousTrack = 0x10,
    NextTrack     = 0x19,
    KeypadEnter   = 0x1C,
    RightCtrl     = 0x1D,
    Mute          = 0x20,
    Calculator    = 0x21,
    Play          = 0x22,
    Stop          = 0x24,
    LowerVolume   = 0x2E,
    RaiseVolume   = 0x30,
    WwwHome       = 0x32,
    KeypadSlash   = 0x35,
    RightAlt      = 0x38,
    CursorUp      = 0x48,
    PageUp        = 0x49,
    CursorLeft    = 0x4B,
    CursorRight   = 0x4D,
    End           = 0x4F,
    CursorDown    = 0x50,
    PageDown      = 0x51,
    Insert        = 0x52,
    Delete        = 0x53,
    RightGui      = 0x5C,
    Apps          = 0x5D,
    AcpiPower     = 0x5E,
    AcpiSleep     = 0x5F,
    AcpiWake      = 0x63,
    WwwSearch     = 0x65,
    WwwFavorites  = 0x66,
    WwwRefresh    = 0x67,
    WwwStop       = 0x68,
    WwwForward    = 0x69,
    WwwBack       = 0x6A,
    WwwMyComputer = 0x6B,
    Email         = 0x6C,
    MediaSelect   = 0x6D,
}

#[repr(u8)]
pub enum SpecialKey {
    Esc             = 0x01,
    One             = 0x02,
    Two             = 0x03,
    Three           = 0x04,
    Four            = 0x05,
    Five            = 0x06,
    Six             = 0x07,
    Seven           = 0x08,
    Eight           = 0x09,
    Nine            = 0x0A,
    Zero            = 0x0B,
    Backspace       = 0x0E,
    Tab             = 0x0F,

    Enter           = 0x1C,
    LeftCtrl        = 0x1D,

    LeftShift       = 0x2A,

    RightShift      = 0x36,

    LeftAlt         = 0x38,

    CapsLock        = 0x3A,
    F1              = 0x3B,
    F2              = 0x3C,
    F3              = 0x3D,
    F4              = 0x3E,
    F5              = 0x3F,
    F6              = 0x40,
    F7              = 0x41,
    F8              = 0x42,
    F9              = 0x43,
    F10             = 0x44,
    NumberLock      = 0x45,
    ScrollLock      = 0x46,
    
    F11             = 0x57,
    F12             = 0x58,
}

impl TryFrom<u8> for Key {
    type Error = ();

    fn try_from(byte: u8) -> Result<Self, Self::Error> {
        match byte {
            0x01..=0x53 | 0x55 | 0x57..=0x58 => {
                unsafe { Ok(core::mem::transmute::<u8, Key>(byte) )}
            }
            _ => Err(()),
        }
    }
}

impl TryFrom<u8> for ExtendedKeyCode {
    type Error = ();

    fn try_from(byte: u8) -> Result<Self, Self::Error> {
        use ExtendedKeyCode::*;
        
        match byte {
            0x10 => Ok(PreviousTrack),
            0x19 => Ok(NextTrack),
            0x1C => Ok(KeypadEnter),
            0x1D => Ok(RightCtrl),
            0x20 => Ok(Mute),
            0x21 => Ok(Calculator),
            0x22 => Ok(Play),
            0x24 => Ok(Stop),
            0x2E => Ok(LowerVolume),
            0x30 => Ok(RaiseVolume),
            0x32 => Ok(WwwHome),
            0x35 => Ok(KeypadSlash),
            0x38 => Ok(RightAlt),
            0x48 => Ok(CursorUp),
            0x49 => Ok(PageUp),
            0x4B => Ok(CursorLeft),
            0x4D => Ok(CursorRight),
            0x4F => Ok(End),
            0x50 => Ok(CursorDown),
            0x51 => Ok(PageDown),
            0x52 => Ok(Insert),
            0x53 => Ok(Delete),
            0x5C => Ok(RightGui),
            0x5D => Ok(Apps),
            0x5E => Ok(AcpiPower),
            0x5F => Ok(AcpiSleep),
            0x63 => Ok(AcpiWake),
            0x65 => Ok(WwwSearch),
            0x66 => Ok(WwwFavorites),
            0x67 => Ok(WwwRefresh),
            0x68 => Ok(WwwStop),
            0x69 => Ok(WwwForward),
            0x6A => Ok(WwwBack),
            0x6B => Ok(WwwMyComputer),
            0x6C => Ok(Email),
            0x6D => Ok(MediaSelect),
            _ => Err(()),
        }
    }
}


impl TryFrom<u8> for SpecialKey {
    type Error = ();

    fn try_from(key: u8) -> Result<Self, Self::Error> {
        match key {
            0x01..=0x0B | 0x0E..=0x0F | 0x1C..=0x1D | 0x2A | 0x36 | 0x38 | 0x3A..=0x46 | 0x57..=0x58 => {
                unsafe { Ok(core::mem::transmute::<u8, SpecialKey>(key)) }
            },
            0x81..=0x8B | 0x8E..=0x8F | 0x9C..=0x9D | 0xAA | 0xB6 | 0xB8 | 0xBA..=0xC6 | 0xD7..=0xD8 => {
                unsafe { Ok(core::mem::transmute::<u8, SpecialKey>(key - 0x80)) }
            },
            _ => Err(()),
        }
    }
}

impl TryFrom<Key> for SpecialKey {
    type Error = ();

    #[inline]
    fn try_from(key: Key) -> Result<Self, Self::Error> {
        Self::try_from(key as u8)
    }
}




