#![no_std]
#![no_main]
#![feature(const_mut_refs)]
#![feature(abi_x86_interrupt)]

mod klib;
use klib::idt;
use klib::pic;
use pic::PIC;
use idt::StackFrame;
use lazy_static::lazy_static;
use klib::x86_64;

lazy_static! {
    static ref IDT: idt::DescriptorTable = {
        let mut idt: idt::DescriptorTable = Default::default();
        idt.breakpoint.set_handler_fn(breakpoint_handler);
        idt.double_fault.set_handler_fn(double_fault_handler);
        idt.user_interrupts[0].set_handler_fn(timer_handler);
        idt
    };
}

#[no_mangle]
pub extern "C" fn _start() -> ! {
    init();
    println!("Hello, World!");
    loop {}
}

fn init() {
    IDT.load();
    unsafe { 
        let mut pic_guard = PIC.lock();
        pic_guard.initialize();
        pic_guard.enable_all();
    };
    x86_64::enable_interrupts();
}

use core::panic::PanicInfo;

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    println!("{}", info);
    loop {}
}

extern "x86-interrupt" fn timer_handler(_stack_frame: StackFrame) {
    println!("timer");
    unsafe { PIC.lock().end_of_interrupt(0x20) }
}

extern "x86-interrupt" fn double_fault_handler(stack_frame: StackFrame, error_code: u64) -> ! {
    println!("Double Fault: {:#?}\n{}", stack_frame, error_code);
    loop {}
}

extern "x86-interrupt" fn breakpoint_handler(stack_frame: StackFrame) {
    println!("Breakpoint: {:#?}", stack_frame);
}
