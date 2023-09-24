use core::arch::asm;
use core::fmt;

#[inline]
pub fn hlt() {
    unsafe {
        asm!("hlt", options(nomem, nostack, preserves_flags))
    }
}

/// Read the "cs" register.
#[inline]
pub fn read_cs() -> u16 {
    let cs: u16;
    unsafe {
        asm!("mov {0:x}, cs", out(reg) cs, options(nomem, nostack, preserves_flags));
    }
    cs
}

/// Load idt located at the specified descriptor table pointer.
#[inline]
pub unsafe fn lidt(idt: &DescriptorTablePointer) {
    unsafe {
        asm!("lidt [{}]", in(reg) idt, options(readonly, nostack, preserves_flags))
    }
}

#[inline]
pub fn enable_interrupts() {
    unsafe {
        asm!("sti", options(nostack, nomem))
    }
}

#[inline]
pub fn disable_interrupts() {
    unsafe {
        asm!("cli", options(nostack, nomem))
    }
}

#[inline]
pub fn read_rflags() -> u64 {
    let r: u64;

    unsafe {
        asm!("pushfq; pop {}", out(reg) r, options(nomem, preserves_flags));
    }

    r
}

#[inline]
pub fn interrupts_enabled() -> bool {
    let rflags = read_rflags();

    rflags & (1 << 9) > 0
}

#[inline]
pub fn without_interrupts<F, R>(f: F) -> R 
where
    F: FnOnce() -> R
{

    let enabled = interrupts_enabled();

    if enabled {
        disable_interrupts();
    }

    let result = f();

    if enabled {
        enable_interrupts();
    }

    result
}

#[inline]
pub unsafe fn port_read_u8(port: u16) -> u8 {
    let ret: u8;
    unsafe {
        asm!("in al, dx", out("al") ret, in("dx") port, options(nomem, nostack, preserves_flags))
    }
    ret
}

#[inline]
pub unsafe fn port_read_u16(port: u16) -> u16 {
    let ret: u16;
    unsafe {
        asm!("in ax, dx", out("ax") ret, in("dx") port, options(nomem, nostack, preserves_flags))
    }
    ret
}

#[inline]
pub unsafe fn port_read_u32(port: u16) -> u32 {
    let ret: u32;
    unsafe {
        asm!("in eax, dx", out("eax") ret, in("dx") port, options(nomem, nostack, preserves_flags))
    }
    ret
}

#[inline]
pub unsafe fn port_write_u8(port: u16, data: u8) {
    asm!("out dx, al", in("dx") port, in("al") data, options(nomem, nostack, preserves_flags))
}

#[inline]
pub unsafe fn port_write_u16(port: u16, data: u16) {
    asm!("out dx, al", in("dx") port, in("ax") data, options(nomem, nostack, preserves_flags))
}

#[inline]
pub unsafe fn port_write_u32(port: u16, data: u32) {
    asm!("out dx, al", in("dx") port, in("eax") data, options(nomem, nostack, preserves_flags))
}

#[inline]
pub unsafe fn io_wait() {
    port_write_u8(0x80, 0x00);
}


#[inline]
pub fn int3() {
    unsafe {
        asm!("int3", options(nostack, nomem))
    }
}

#[repr(C, packed(2))]
pub struct DescriptorTablePointer {
    pub limit: u16,
    pub base: CanonicalAddress,
}

#[repr(transparent)]
#[derive(Clone, Copy)]
pub struct CanonicalAddress(u64);

impl CanonicalAddress {
    /// ## Safety
    /// This must be a valid canonical address (i.e., an address with its 47th bit sign-extended up
    /// to 64 bits, i.e. in the ranges 0x0000_0000_0000_0000..=0x0000_7FFF_FFFF_FFFF OR 0xFFFF_F000_0000_0000..)
    #[inline]
    pub unsafe fn new_unsafe(addr: u64) -> Self {
        Self(addr)
    }

    /// ## Panics
    /// This function will panic if passed an invalid canonical address (see new_unsafe for
    /// details).
    pub fn new(addr: u64) -> Self {
        let mask = addr & 0xFFFF_0000_0000_0000;

        if (mask != 0xFFFF_0000_0000_0000 && mask != 0x0) ||
           (mask > 0 && addr & 0x8000_0000_0000 == 0) ||
           (addr & 0x8000_0000_0000 != 0) {
            panic!("Invalid address for canonical address");
        }

        Self(addr)
    }
}

impl fmt::Debug for CanonicalAddress {   
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("CanonicalAddress")
         .field(&format_args!("{:#x}", self.0))
         .finish()
    }
}
