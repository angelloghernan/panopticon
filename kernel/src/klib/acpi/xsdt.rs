use super::super::util::as_u8_slice;
use super::fadt::Fadt;
use super::SdtHeader;
use core::slice::from_raw_parts;
use core::mem::size_of;
use core::ptr;

#[repr(C)]
pub struct Xsdt {
    pub header: SdtHeader,
}

impl Xsdt {
    fn get_header_ptrs_start(&self) -> *const u64 {
        let u8_ptr = unsafe { ((self as *const Xsdt) as *const u8).add(size_of::<SdtHeader>()) };

        u8_ptr as *const u64
    }

    fn find_fadt(&self) -> Option<Fadt> {
        let mut ptr = self.get_header_ptrs_start();
        
        for i in 0..self.header.length {
            let table_ptr = unsafe { ptr::read_unaligned(ptr) };

            ptr = unsafe { ptr.add(1) };


        }

        None
    }
}
