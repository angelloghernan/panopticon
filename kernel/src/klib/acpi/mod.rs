pub mod rsdp;
pub mod xsdt;
pub mod fadt;
use core::slice::from_raw_parts;

#[repr(C)]
pub struct SdtHeader {
    pub signature: [u8; 4],
    pub length: u32,
    pub revision: u8,
    pub checksum: u8,
    pub oemid: [u8; 6],
    pub oem_table_id: [u8; 8],
    pub oem_revision: u32,
    pub creator_id: u32,
    pub creator_revision: u32,
}

impl SdtHeader {
    fn validate_checksum(&self) -> bool {
        let checksum = self.as_u8_slice().iter().fold(0u8, |acc, &x| acc.wrapping_add(x));
        
        checksum.wrapping_sub(self.checksum) == 0u8
    }
    
    fn as_u8_slice(&self) -> &[u8] {
        unsafe {
            from_raw_parts(
                (self as *const Self) as *const u8,
                self.length as usize,
            )
        }
    }
}
