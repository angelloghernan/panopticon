use noto_sans_mono_bitmap::{
    get_raster, get_raster_width, FontWeight, RasterHeight, RasterizedChar,
};
use bootloader_api::info::{FrameBuffer, FrameBufferInfo, PixelFormat};
use core::fmt::Write;
use spin::once::Once;
use core::fmt;

static mut FRAMEBUFFER: Once<FrameBufferWriter> = spin::Once::new();

const LINE_SPACING: usize = 2;

const LETTER_SPACING: usize = 0;

const BORDER_PADDING: usize = 1;

const CHAR_RASTER_HEIGHT: RasterHeight = RasterHeight::Size20;

const CHAR_RASTER_WIDTH: usize = get_raster_width(FontWeight::Regular, CHAR_RASTER_HEIGHT);

const FONT_WEIGHT: FontWeight = FontWeight::Regular;

pub const BACKSPACE: char = 0x08 as char;

fn get_rasterized_char(ch: char) -> RasterizedChar {
    get_raster(ch, FONT_WEIGHT, CHAR_RASTER_HEIGHT).unwrap()
}

pub struct FrameBufferWriter {
    framebuffer: &'static mut [u8],
    info: FrameBufferInfo,
    x: usize,
    y: usize,
}

/// Initialize the framebuffer.
/// SAFETY: This function should only be called once, in one thread. ALSO: This should be
/// called immediately after booting.
pub unsafe fn init_framebuffer(framebuffer: &'static mut FrameBuffer) {
    let info = framebuffer.info();
    unsafe { FRAMEBUFFER.call_once(|| FrameBufferWriter::new(framebuffer.buffer_mut(), info) ) };
}

impl FrameBufferWriter {
    fn new(framebuffer: &'static mut [u8], info: FrameBufferInfo) -> Self {
        let mut writer = Self {
            framebuffer,
            info,
            x: BORDER_PADDING,
            y: BORDER_PADDING,
        };

        writer.clear();
        writer
    }

    pub fn clear(&mut self) {
        self.x = BORDER_PADDING;
        self.y = BORDER_PADDING;
        self.framebuffer.fill(0);
    }

    fn newline(&mut self) {
        self.y += CHAR_RASTER_HEIGHT.val() + LINE_SPACING;
        self.x = BORDER_PADDING;
    }

    fn backspace(&mut self) {
        if self.x <= BORDER_PADDING {
            if self.y > BORDER_PADDING {
                self.y -= CHAR_RASTER_HEIGHT.val() + LINE_SPACING;
                self.x = self.width() - BORDER_PADDING - CHAR_RASTER_WIDTH;
            }
        } else {
            self.x -= CHAR_RASTER_WIDTH + LETTER_SPACING;
        }

        for (y, row) in get_rasterized_char(' ').raster().iter().enumerate() {
            for (x, byte) in row.iter().enumerate() {
                self.write_pixel(self.x + x, self.y + y, *byte);
            }
        }
    }

    pub fn width(&self) -> usize {
        self.info.width
    }

    pub fn height(&self) -> usize {
        self.info.height
    }

    fn write_char(&mut self, ch: char) {
        match ch {
            '\n' => self.newline(),
            BACKSPACE => self.backspace(),
            ch => {
                let new_x = self.x + CHAR_RASTER_WIDTH;
                if new_x >= self.width() {
                    self.newline();
                }

                let new_y = self.y + CHAR_RASTER_HEIGHT.val() + BORDER_PADDING;

                if new_y >= self.height() {
                    self.clear();
                }

                self.write_rendered_char(get_rasterized_char(ch));
            }
        }
    }

    fn write_rendered_char(&mut self, rendered_char: RasterizedChar) {
        for (y, row) in rendered_char.raster().iter().enumerate() {
            for (x, byte) in row.iter().enumerate() {
                self.write_pixel(self.x + x, self.y + y, *byte);
            }
        }

        self.x += rendered_char.width() + LETTER_SPACING;
    }

    fn write_pixel(&mut self, x: usize, y: usize, intensity: u8) {
        let pixel_offset = y * self.info.stride + x;
        let color = match self.info.pixel_format {
            PixelFormat::Bgr => [intensity, intensity, intensity, 0],
            PixelFormat::U8 => [if intensity > 200 { 0xFF } else { 0x0 }, 0, 0, 0],
            _ => [intensity, intensity, intensity, 0],
        };

        let bytes_per_pixel = self.info.bytes_per_pixel;
        let byte_offset = bytes_per_pixel * pixel_offset;
        self.framebuffer[byte_offset..(byte_offset + bytes_per_pixel)]
            .copy_from_slice(&color[..bytes_per_pixel]);

        let _ = unsafe { core::ptr::read_volatile(&self.framebuffer[byte_offset]) };
    }

}

unsafe impl Send for FrameBufferWriter {}
unsafe impl Sync for FrameBufferWriter {}

impl fmt::Write for FrameBufferWriter {
    fn write_str(&mut self, string: &str) -> fmt::Result {
        for ch in string.chars() {
            self.write_char(ch)
        }

        Ok(())
    }
}

#[doc(hidden)]
pub fn _print(args: fmt::Arguments) {
    unsafe { FRAMEBUFFER.get_mut_unchecked().write_fmt(args).unwrap(); };
}

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => ($crate::klib::graphics::framebuffer::_print(format_args!($($arg)*)));
}

#[macro_export]
macro_rules! println {
    () => ($crate::print!("\n"));
    ($($arg:tt)*) => ($crate::print!("{}\n", format_args!($($arg)*)));
}
