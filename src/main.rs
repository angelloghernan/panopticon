#![no_std]
#![no_main]
#![feature(const_mut_refs)]
#![feature(abi_x86_interrupt)]
#![feature(maybe_uninit_uninit_array)]

mod klib;
use klib::idt;
use klib::pic;
use pic::PIC;
use klib::x86_64;
use klib::pic::Irq;
use idt::StackFrame;
use crate::klib::ps2;
use ps2::keyboard::KeyCode;
use ps2::keyboard::KEYBOARD;
use ps2::keyboard::SpecialKey;
use lazy_static::lazy_static;

lazy_static! {
    static ref IDT: idt::DescriptorTable = {
        let mut idt: idt::DescriptorTable = Default::default();
        idt.breakpoint.set_handler_fn(breakpoint_handler);
        idt.double_fault.set_handler_fn(double_fault_handler);
        idt.user_interrupts[Irq::Timer as usize].set_handler_fn(timer_handler);
        idt.user_interrupts[Irq::Keyboard as usize].set_handler_fn(keyboard_handler);
        idt
    };
}

#[no_mangle]
pub extern "C" fn _start() -> ! {
    init();
    loop {
        x86_64::without_interrupts(|| 
            if let Some(key) = KEYBOARD.lock().pop_key() {
                print_key(key)
            });
        x86_64::hlt();
    }
}

fn print_key(key: KeyCode) {
    use KeyCode::*;
    static mut LSHIFT_PRESSED: bool = false;
    static mut RSHIFT_PRESSED: bool = false;
    static mut CAPS_PRESSED: bool = false;

    match key {
        AsciiDown(key) => {
            let shift_pressed = unsafe { LSHIFT_PRESSED || RSHIFT_PRESSED || CAPS_PRESSED };
            let ch = if shift_pressed {
                key.to_ascii_uppercase()
            } else {
                key
            };

            print!("{}", ch as char);
        },
        SpecialDown(SpecialKey::Enter) => print!("\n"),
        SpecialDown(SpecialKey::Backspace) => print!("\x08"),
        SpecialDown(SpecialKey::LeftShift) => unsafe { LSHIFT_PRESSED = true },
        SpecialDown(SpecialKey::RightShift) => unsafe { RSHIFT_PRESSED = true },
        SpecialDown(SpecialKey::CapsLock) => unsafe { CAPS_PRESSED = !CAPS_PRESSED },
        SpecialUp(SpecialKey::LeftShift) => unsafe { LSHIFT_PRESSED = false },
        SpecialUp(SpecialKey::RightShift) => unsafe { RSHIFT_PRESSED = false },
        _ => {},
    }
}

fn init() {
    IDT.load();
    unsafe {
        let mut pic_guard = PIC.lock();
        pic_guard.initialize();
        pic_guard.enable_all();
    };
    {
        let mut keyboard = KEYBOARD.lock();
        keyboard.enable();
    }
    x86_64::enable_interrupts();
}

use core::panic::PanicInfo;

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    println!("{}", info);
    loop {}
}

extern "x86-interrupt" fn keyboard_handler(_stack_frame: StackFrame) {
    {
        let mut keyboard = KEYBOARD.lock();
        let key = { keyboard.read_byte() };



        match key {
            Ok(byte) => { let _ = keyboard.push_key(byte); },
            Err(_) => println!("Couldn't get key"),
        }


        let _ = keyboard.send_next_command();
    }

    unsafe { PIC.lock().end_of_interrupt(Irq::Keyboard as u8) }
}

extern "x86-interrupt" fn timer_handler(_stack_frame: StackFrame) {
    unsafe { PIC.lock().end_of_interrupt(Irq::Timer as u8) }
}

extern "x86-interrupt" fn double_fault_handler(stack_frame: StackFrame, error_code: u64) -> ! {
    println!("Double Fault: {:#?}\n{}", stack_frame, error_code);
    loop {}
}

extern "x86-interrupt" fn breakpoint_handler(stack_frame: StackFrame) {
    println!("Breakpoint: {:#?}", stack_frame);
}
