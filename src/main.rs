#![no_std]
#![no_main]
#![feature(const_mut_refs)]
#![feature(abi_x86_interrupt)]

mod klib;
use klib::idt;
use lazy_static::lazy_static;

lazy_static! {
    static ref IDT: idt::DescriptorTable = {
        let mut idt: idt::DescriptorTable = Default::default();
        idt.breakpoint.set_handler_fn(breakpoint_handler);
        idt.user_interrupts[0].set_handler_fn(keyboard_handler);
        idt
    };
}

#[no_mangle]
pub extern "C" fn _start() -> ! {
    IDT.load();
    klib::x86_64::int3();
    println!("Hello, World!");
    loop {}
}

use core::panic::PanicInfo;

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    println!("{}", info);
    loop {}
}

extern "x86-interrupt" fn keyboard_handler(stack_frame: idt::StackFrame) {

}

extern "x86-interrupt" fn breakpoint_handler(stack_frame: idt::StackFrame) {
    println!("Breakpoint: {:#?}", stack_frame);
}
