#![no_std]
#![no_main]
#![feature(const_mut_refs)]
#![feature(abi_x86_interrupt)]

mod klib;

#[no_mangle]
pub extern "C" fn _start() -> ! {
    for i in 0..100 {
        println!("Hello, world! {}", i);
    }
    loop {}
}

use core::panic::PanicInfo;

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    println!("{}", info);
    loop {}
}

