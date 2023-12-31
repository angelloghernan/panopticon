use core::fmt;
use spin::Mutex;
use crate::klib::x86_64;
use volatile::Volatile;
use lazy_static::lazy_static;

lazy_static! {
    pub static ref CONSOLE_WRITER: Mutex<ConsoleWriter> = Mutex::new(ConsoleWriter::new());
}

const CONSOLE_ADDRESS: usize = 0xb8000;
const BUFFER_WIDTH: usize = 80;
const BUFFER_HEIGHT: usize = 25;

const SET_REGISTER: u16 = 0x3D4;
const CURSOR_CONTROL: u16 = 0x3D5;
const CURSOR_START: u8 = 0x0A;
const CURSOR_END: u8 = 0xB;
const CURSOR_LOCATION_HIGH: u8 = 0xE;
const CURSOR_LOCATION_LOW: u8 = 0xF;

pub const BACKSPACE: u8 = 0x08;


pub struct ConsoleWriter {
    row: u16,
    col: u16,
    color_code: ColorCode,
    buffer: &'static mut Buffer,
}

impl ConsoleWriter {
    pub fn new() -> Self {
        unsafe {
            x86_64::port_write_u8(SET_REGISTER, CURSOR_START);
            x86_64::port_write_u8(CURSOR_CONTROL, 0x20);
            x86_64::port_write_u8(SET_REGISTER, CURSOR_START);
            // Upper two bits are reserved
            let mut existing = x86_64::port_read_u8(CURSOR_CONTROL) & 0xC;
            // Enable cursor (bit 5 set to 0) and set start position to 0
            x86_64::port_write_u8(CURSOR_CONTROL, existing);

            x86_64::port_write_u8(SET_REGISTER, CURSOR_END);
            // Upper three bits are reserved for cursor end
            existing = x86_64::port_read_u8(CURSOR_CONTROL) & 0xE;
            // Set end position to 15 (take up entire block)
            x86_64::port_write_u8(CURSOR_CONTROL, existing);
        }

        let buffer = unsafe { &mut *(CONSOLE_ADDRESS as *mut Buffer) };

        let empty_space = Character {
            ascii_char: b' ',
            color_code: ColorCode::new(Color::White, Color::Black),
        };

        for row in 0..BUFFER_HEIGHT {
            for col in 0..BUFFER_WIDTH {
                buffer.chars[row][col].write(empty_space);
            }
        }

        
        Self {
            row: 0,
            col: 0,
            color_code: ColorCode::new(Color::White, Color::Black),
            buffer: unsafe { &mut *(CONSOLE_ADDRESS as *mut Buffer) },
        }
    }

    fn move_cursor(&mut self) {
        let pos = (self.row as usize) * BUFFER_WIDTH + (self.col as usize);
        let pos_lo = (pos & 0xFF) as u8;
        let pos_hi = (pos >> 8) as u8;

        unsafe {
            x86_64::port_write_u8(SET_REGISTER, CURSOR_LOCATION_LOW);
            x86_64::port_write_u8(CURSOR_CONTROL, pos_lo);
            x86_64::port_write_u8(SET_REGISTER, CURSOR_LOCATION_HIGH);
            x86_64::port_write_u8(CURSOR_CONTROL, pos_hi);
        }
    }

    pub fn write_ascii_char(&mut self, byte: u8) {
        match byte {
            b'\n' => self.new_line(),
            BACKSPACE => self.backspace(),
            byte => {
                if self.col as usize >= BUFFER_WIDTH {
                    self.new_line();
                }

                let (row, col) = (self.row as usize, self.col as usize);

                self.buffer.chars[row][col].write(Character {
                    ascii_char: byte,
                    color_code: self.color_code,
                });

                self.col += 1
            }
        }
    }

    pub fn write_ascii_string(&mut self, string: &str) {
        for byte in string.bytes() {
            self.write_ascii_char(byte)
        }

        self.move_cursor()
    }

    pub fn new_line(&mut self) {
        self.col = 0;
        self.row += 1;
        if self.row as usize >= BUFFER_HEIGHT {
            self.row -= 1;
            self.scroll()
        }
    }

    pub fn backspace(&mut self) {
        if self.col == 0 {
            if self.row != 0 {
                self.row -= (self.row != 0) as u16;
                self.col = (BUFFER_WIDTH - 1) as u16;
            }
        } else {
            self.col -= 1
        }
        
        let (row, col) = (self.row as usize, self.col as usize);

        self.buffer.chars[row][col].write(Character {
            ascii_char: b' ',
            color_code: self.color_code,
        })
    }

    pub fn scroll(&mut self) {
        for i in 0..(BUFFER_HEIGHT - 1) {
            for j in 0..BUFFER_WIDTH {
                self.buffer.chars[i][j].write(
                    self.buffer.chars[i + 1][j].read()
                )
            }
        }

        let blank = Character {
            ascii_char: b' ',
            color_code: self.color_code,
        };

        for i in 0..BUFFER_WIDTH {
            unsafe {
                self.buffer
                    .chars
                    .last_mut()
                    .unwrap_unchecked()
                    .get_unchecked_mut(i)
                    .write(blank)
            }
        }
    }

    pub fn change_color(&mut self, foreground: Color, background: Color) {
        self.color_code = ColorCode::new(foreground, background) 
    }
}

impl fmt::Write for ConsoleWriter {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.write_ascii_string(s);
        Ok(())
    }
}

#[repr(transparent)]
struct Buffer {
    chars: [[Volatile<Character>; BUFFER_WIDTH]; BUFFER_HEIGHT],
}


#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
struct Character {
    ascii_char: u8,
    color_code: ColorCode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
struct ColorCode(u8);

impl ColorCode {
    fn new(foreground: Color, background: Color) -> ColorCode {
        let code: u8 = ((background as u8) << 4) | foreground as u8;
        Self(code)
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Color {
    Black = 0,
    Blue = 1,
    Green = 2,
    Cyan = 3,
    Red = 4,
    Magenta = 5,
    Brown = 6,
    LightGray = 7,
    DarkGray = 8,
    LightBlue = 9,
    LightGreen = 10,
    LightCyan = 11,
    LightRed = 12,
    Pink = 13,
    Yellow = 14,
    White = 15,
}

macro_rules! print {
    ($($arg:tt)*) => ($crate::klib::vga_console::_print(format_args!($($arg)*)));
}


macro_rules! println {
    () => ($crate::print!("\n"));
    ($($arg:tt)*) => ($crate::print!("{}\n", format_args!($($arg)*)));
}

#[doc(hidden)]
pub fn _print(args: fmt::Arguments) {
    use core::fmt::Write;
    x86_64::without_interrupts(|| CONSOLE_WRITER.lock().write_fmt(args).unwrap())
}
