#![no_std]
#![no_main]
#![feature(const_mut_refs)]
#![feature(abi_x86_interrupt)]
#![feature(maybe_uninit_uninit_array)]
#![feature(generic_const_exprs)]
#![feature(dropck_eyepatch)]
#![feature(never_type)]
#![feature(offset_of)]

mod allocator;
mod klib;
mod memory;
use crate::klib::ahci::ahcistate::AHCIState;
use crate::klib::ahci::ahcistate::SATA_DISK0;
use crate::klib::once_lock::OnceLock;
use crate::klib::pci::ide_controller::Command::ReadFPDMAQueued;
use crate::klib::pci::ide_controller::IDEController;
use crate::klib::ps2;
use crate::memory::init_page_table;
use crate::memory::BootInfoFrameAllocator;
use bootloader_api::config::{BootloaderConfig, Mapping};
use bootloader_api::{entry_point, BootInfo};
use idt::StackFrame;
use klib::acpi::rsdp::Rsdp;
use klib::graphics::framebuffer;
use klib::idt;
use klib::pic;
use klib::pic::Irq;
use lazy_static::lazy_static;
use pic::PIC;
use ps2::keyboard::KeyCode;
use ps2::keyboard::SpecialKey;
use ps2::keyboard::KEYBOARD;
use spin::RwLock;
use x86_64::instructions::interrupts;
use x86_64::structures::paging::FrameAllocator;
use x86_64::structures::paging::Mapper;
use x86_64::structures::paging::OffsetPageTable;
use x86_64::structures::paging::Page;
use x86_64::structures::paging::PageTableFlags;
use x86_64::structures::paging::Size4KiB;
use x86_64::VirtAddr;

extern crate alloc;
use core::sync::atomic::AtomicU64;

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

static TIMER: AtomicU64 = AtomicU64::new(0);

static KERNEL_PAGETABLE: OnceLock<RwLock<OffsetPageTable<'static>>> = OnceLock::new();

fn kernel_main(boot_info: &'static mut BootInfo) -> ! {
    init(boot_info);

    loop {
        interrupts::without_interrupts(|| {
            if let Some(key) = KEYBOARD.lock().pop_key() {
                print_key(key)
            }
        });
        x86_64::instructions::hlt();
    }
}

pub static BOOTLOADER_CONFIG: BootloaderConfig = {
    let mut config = BootloaderConfig::new_default();
    config.mappings.physical_memory = Some(Mapping::Dynamic);
    config
};

entry_point!(kernel_main, config = &BOOTLOADER_CONFIG);

fn print_key(key: KeyCode) {
    use KeyCode::*;
    static mut LSHIFT_PRESSED: bool = false;
    static mut RSHIFT_PRESSED: bool = false;
    static mut CAPS_PRESSED: bool = false;

    match key {
        AsciiDown(key) => {
            let shift_pressed = unsafe { LSHIFT_PRESSED || RSHIFT_PRESSED || CAPS_PRESSED };
            let ch = if shift_pressed {
                key.get_shifted()
            } else {
                key.get()
            };

            print!("{}", ch as char);
        }

        SpecialDown(SpecialKey::Enter) => print!("\n"),
        SpecialDown(SpecialKey::Backspace) => print!("\x08"),
        SpecialDown(SpecialKey::LeftShift) => unsafe { LSHIFT_PRESSED = true },
        SpecialDown(SpecialKey::RightShift) => unsafe { RSHIFT_PRESSED = true },
        SpecialDown(SpecialKey::CapsLock) => unsafe { CAPS_PRESSED = !CAPS_PRESSED },
        SpecialUp(SpecialKey::LeftShift) => unsafe { LSHIFT_PRESSED = false },
        SpecialUp(SpecialKey::RightShift) => unsafe { RSHIFT_PRESSED = false },
        _ => {}
    }
}

fn init(boot_info: &'static mut BootInfo) {
    unsafe { framebuffer::init_framebuffer(boot_info.framebuffer.as_mut().unwrap()) };

    // let rsdp = unsafe { Rsdp::get(rsdp_addr as usize) };

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

    let mut frame_allocator = unsafe { BootInfoFrameAllocator::init(&boot_info.memory_regions) };

    let phys_mem_offset = VirtAddr::new(boot_info.physical_memory_offset.into_option().unwrap());
    let mut mapper = unsafe { init_page_table(phys_mem_offset) };

    allocator::init_heap(&mut mapper, &mut frame_allocator).expect("Failed to initialize heap");

    interrupts::enable();

    let rsdp_addr = boot_info.rsdp_addr.into_option().unwrap();
    let rsdp_page: Page<Size4KiB> = Page::containing_address(VirtAddr::new(rsdp_addr));
    let frame = frame_allocator
        .allocate_frame()
        .expect("Failed to map rsdp");
    let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;
    unsafe {
        mapper
            .map_to(rsdp_page, frame, flags, &mut frame_allocator)
            .expect("Failed to map rsdp")
            .flush()
    };

    println!("Rsdp addr is {:x}", rsdp_addr);

    // let ide_controller = IDEController::new();

    KERNEL_PAGETABLE.set(RwLock::new(mapper));

    let rsdp = unsafe { Rsdp::get(rsdp_addr as usize) };
    println!("Rsdp validation returns {}", rsdp.validate_checksum());
    println!("Attempting to get ahci state");
    let _ = unsafe { AHCIState::new(&mut frame_allocator, 0, 0, 0) };

    match SATA_DISK0.get() {
        Some(disk_lock) => {
            let mut disk = disk_lock.write();
            unsafe { disk.enable_interrupts() };
            println!(
                "Initialized AHCI disk, interrupts enabled: {}",
                interrupts::are_enabled()
            );
            let mut buf = [0u8; 1024];
            let res = disk.read_or_write(ReadFPDMAQueued, &mut buf, 0);

            match res {
                Ok(_) => println!("Read {} bytes from disk", buf.len()),
                Err(_) => println!("Failed to read bytes from disk"),
            }
        }
        None => panic!("Failed to initialize AHCI disk"),
    };
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
            Ok(byte) => {
                let _ = keyboard.push_key(byte);
            }
            Err(_) => println!("Couldn't get key"),
        }

        let _ = keyboard.send_next_command();
    }

    unsafe { PIC.lock().end_of_interrupt(Irq::Keyboard as u8) }
}

extern "x86-interrupt" fn timer_handler(_stack_frame: StackFrame) {
    use core::sync::atomic::Ordering::*;
    let time = TIMER.load(SeqCst);
    let _ = TIMER.compare_exchange_weak(time, time + 1, SeqCst, SeqCst);
    unsafe { PIC.lock().end_of_interrupt(Irq::Timer as u8) }
}

fn sleep(milliseconds: u64) {
    use core::sync::atomic::Ordering::*;
    let time = TIMER.load(SeqCst);
    while TIMER.load(SeqCst) < time + milliseconds {
        // TODO: Do something else instead of just waiting; do other scheduled tasks.
        unsafe { klib::x86_64::io_wait() };
    }
}

extern "x86-interrupt" fn ahci_handler(_stack_frame: StackFrame) {
    match SATA_DISK0.get() {
        Some(disk_lock) => {
            (*disk_lock.write()).handle_interrupt();
        }
        None => {
            panic!("Unexpected call to AHCI handler");
        }
    }
}

extern "x86-interrupt" fn double_fault_handler(stack_frame: StackFrame, error_code: u64) -> ! {
    println!("Double Fault: {:#?}\n{}", stack_frame, error_code);
    loop {}
}

extern "x86-interrupt" fn breakpoint_handler(stack_frame: StackFrame) {
    println!("Breakpoint: {:#?}", stack_frame);
}
