use crate::println;
use crate::KERNEL_PAGETABLE;
use core::mem::size_of;
use core::ops::BitAnd;
use core::ops::DerefMut;
use core::slice::from_raw_parts;
use x86_64::structures::paging::Mapper;
use x86_64::structures::paging::Page;
use x86_64::structures::paging::Size4KiB;
use x86_64::VirtAddr;

pub fn as_u8_slice<T: Sized>(obj: &T) -> &[u8] {
    unsafe { from_raw_parts((obj as *const T) as *const u8, size_of::<T>()) }
}

#[inline]
pub fn kernel_to_physical_address(addr: u64) -> u64 {
    let offset = addr & 0xFFF;
    // println!("offset: {offset}");
    let pt_lock = KERNEL_PAGETABLE.get().unwrap().read();
    (*pt_lock)
        .translate_page(Page::<Size4KiB>::containing_address(VirtAddr::new(addr)))
        .unwrap()
        .start_address()
        .as_u64()
        + offset
}

#[inline]
pub fn physical_to_kernel_address(addr: u64) -> u64 {
    addr
}

#[repr(transparent)]
#[derive(Clone, Copy)]
pub struct Volatile<T: Clone + Copy> {
    data: T,
}

impl<T: Clone + Copy> Volatile<T> {
    #[inline]
    pub fn new(data: T) -> Self {
        Self { data }
    }

    #[inline]
    pub fn read(&self) -> T {
        let ptr = &self.data as *const T;
        unsafe { core::ptr::read_volatile(ptr) }
    }

    #[inline]
    pub fn write(&mut self, data: T) {
        let ptr = &mut self.data as *mut T;
        unsafe { core::ptr::write_volatile(ptr, data) }
    }
}
