use crate::x86_64::PhysicalAddress;

#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct PageTableEntry {
    raw: u64,
}

const PTE_COUNT : usize = 4096usize / core::mem::size_of::<PageTableEntry>();

#[derive(Clone)]
#[repr(align(4096))]
#[repr(C)]
pub struct PageTable {
    entries: [PageTableEntry; PTE_COUNT],
}

#[repr(u64)]
pub enum Flag {
    Present = 0b1,
    Writable = 0b1 << 1,
    UserAccessible = 0b1 << 2,
    WriteThrough = 0b1 << 3,
    NoCache = 0b1 << 4,
    Accessed = 0b1 << 5,
    Dirty = 0b1 << 6,
    HugePage = 0b1 << 7,
    Global = 0b1 << 8,
}

impl PageTableEntry {
    #[inline]
    pub fn new() -> Self {
        PageTableEntry { raw: 0u64 }
    }
    
    #[inline]
    pub fn zero_out(&mut self) {
        self.raw = 0;
    }
    
    #[inline]
    pub fn get_flag(&self, flag: Flag) -> bool {
        self.raw & (flag as u64) != 0
    }

    #[inline]
    pub fn clear_flag(&mut self, flag: Flag) {
        self.raw &= !(flag as u64);
    }

    #[inline]
    pub fn set_flag(&mut self, flag: Flag) {
        self.raw |= flag as u64;
    }

    #[inline]
    pub fn addr(&self) -> PhysicalAddress {
        unsafe { PhysicalAddress::new_unsafe(self.raw & 0x000f_ffff_ffff_f000) }
    }
}

impl PageTable {
    pub fn new() -> Self {
        Self {
            entries: [PageTableEntry::new(); PTE_COUNT],
        }
    }

    pub fn zero_out(&mut self) {
        for entry in self.entries.iter_mut() {
            entry.zero_out()
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = &PageTableEntry> {
        self.entries.iter()
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut PageTableEntry> {
        self.entries.iter_mut()
    }
}
