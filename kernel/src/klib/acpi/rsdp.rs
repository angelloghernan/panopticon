use super::super::util::as_u8_slice;

#[repr(C, packed)]
pub struct Rsdp {
    pub signature: [u8; 8],
    pub checksum: u8,
    pub oemid: [u8; 6],
    pub revision: u8,
    pub rsdt_address: u32,    // Deprecated past version 1
    pub length: u32,
    pub xsdt_address: u64,
    pub extended_checksum: u8,
    _reserved: [u8; 3],
}

pub enum SdtAddr {
    Rsdt(u64),
    Xsdt(u64),
}

impl Rsdp {
    /// "Get" the rsdp structure, given the rsdp address.
    pub unsafe fn get(rsdp_addr: usize) -> &'static Self {
        unsafe { &*(rsdp_addr as *const Self) }
    }

    pub fn validate_signature(&self) -> bool {
        self.signature == [b'R', b'S', b'D', b' ', b'P', b'T', b'R', b' ']
    }
    
    /// Return the Sdt address if this RSDP is valid.
    pub fn get_sdt_addr(&self) -> Option<SdtAddr> {
        if !self.validate_checksum() {
            None
        } else {
            match self.revision {
                0 => Some(SdtAddr::Rsdt(self.rsdt_address as u64)),
                _ => Some(SdtAddr::Xsdt(self.xsdt_address)),
            }
        }
    }

    pub fn validate_checksum(&self) -> bool {
        let (rev_1_bytes, rev_2_bytes) = as_u8_slice(self) 
                                         .split_at(core::mem::offset_of!(Rsdp, length));

        let checksum1 = rev_1_bytes
                        .iter()
                        .fold(0, |acc: u8, &x| acc.wrapping_add(x));

        if checksum1.wrapping_sub(self.checksum) != self.checksum {
            return false;
        }

        if self.revision == 0 {
            return true;
        }

        let checksum2 = rev_2_bytes
                        .iter()
                        .fold(0, |acc: u8, &x| acc.wrapping_add(x));

        checksum2.wrapping_sub(self.extended_checksum) == self.extended_checksum
    }
}
