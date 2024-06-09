use core::mem::size_of;
use core::slice::from_raw_parts;

pub fn as_u8_slice<T: Sized>(obj: &T) -> &[u8] {
    unsafe { from_raw_parts((obj as *const T) as *const u8, size_of::<T>()) }
}

#[inline]
pub fn kernel_to_physical_address(addr: u64) -> u64 {
    addr
}

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
    pub fn write(&mut self, data: &T) {
        let ptr = &mut self.data as *mut T;
        unsafe { core::ptr::write_volatile(ptr, *data) }
    }
}
