use core::marker::PhantomData;
use core::fmt;
use volatile::Volatile;

pub struct DescriptorTable {
    pub division_error: Entry<Handler>,
    pub debug: Entry<Handler>,
    pub non_maskable: Entry<Handler>,
    pub breakpoint: Entry<Handler>,
    pub overflow: Entry<Handler>,
    pub bound_range_exceeded: Entry<Handler>,
    pub invalid_opcode: Entry<Handler>,
    pub device_not_available: Entry<Handler>,
    pub double_fault: Entry<ErrorCodeHandler>,
    _coprocessor_segment_overrun: Entry<Handler>, // left unused on purpose; for legacy systems
    pub invalid_tss: Entry<ErrorCodeHandler>,
    pub segment_not_present: Entry<ErrorCodeHandler>,
    pub stack_segment_fault: Entry<ErrorCodeHandler>,
    pub general_protection_fault: Entry<ErrorCodeHandler>,
    pub page_fault: Entry<PageFaultHandler>,
    _reserved: Entry<Handler>,
    pub x87_floating_point_exception: Entry<Handler>,
    pub alignment_check: Entry<ErrorCodeHandler>,
    pub machine_check: Entry<Handler>,
    pub simd_floating_point_exception: Entry<Handler>,
    pub virtualization_exception: Entry<Handler>,
    pub control_protection_exception: Entry<ErrorCodeHandler>,
    _reserved2: Entry<Handler>,
    pub hypervisor_injection_exception: Entry<Handler>,
    pub vmm_communication_exception: Entry<ErrorCodeHandler>,
    pub security_exception: Entry<ErrorCodeHandler>,
    _reserved3: Entry<Handler>,
    interrupts: [Entry<Handler>; 0xFF - 0x1F],
}

#[repr(C)]
#[derive(Clone)]
pub struct Entry<F> {
    pointer_low: u16,
    gdt_selector: u16,
    options: EntryOptions,
    pointer_middle: u16,
    pointer_high: u32,
    zero: u32,
    _phantom: PhantomData<F>,
}

impl<F> Entry<F> {
    fn empty() -> Self {
        Self {
            pointer_low: 0,
            gdt_selector: 0,
            options: EntryOptions::minimal(),
            pointer_middle: 0,
            pointer_high: 0,
            zero: 0,
            _phantom: PhantomData
        }
    }
}

impl<F> fmt::Debug for Entry<F> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Entry")
         .field("pointer_low", &self.pointer_low)
         .field("gdt_selector", &self.gdt_selector)
         .field("options", &self.options)
         .field("pointer_middle", &self.pointer_middle)
         .field("pointer_high", &self.pointer_high)
         .field("zero", &self.zero)
         .finish()
    }
}

impl<T> PartialEq for Entry<T> {
    fn eq(&self, other: &Self) -> bool {
        self.pointer_low == other.pointer_low           &&
            self.gdt_selector == other.gdt_selector     &&
            self.options == other.options               &&
            self.pointer_middle == other.pointer_middle &&
            self.pointer_high == other.pointer_high
    }
}

#[repr(u8)]
pub enum PrivilegeLevel {
    Kernel = 0u8,
    User = 3u8, // We are not using 1-2; these are probably going to be deprecated
}

#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EntryOptions(u16);

impl EntryOptions {
    #[inline]
    pub fn minimal() -> Self {
        Self(0b1110_0000_0000)
    }

    #[inline]
    pub fn set_present(&mut self, present: bool) -> &mut Self {
        self.0 &= ((!1) | (present as u16)) << 15;
        self
    }

    #[inline]
    pub fn disable_interrupts_when_invoked(&mut self, present: bool) -> &mut Self {
        // Switch between an interrupt gate and a trap gate
        self.0 &= ((!1) | (present as u16)) << 8;
        self
    }

    #[inline]
    pub fn set_privilege_level(&mut self, privilege_level: PrivilegeLevel) -> &mut Self {
        self.0 &= ((!0b11) | (privilege_level as u16)) << 13;
        self
    }

    /// ## Safety
    /// This function must be called with a value in the range [0, 6]. In addition, the
    /// caller must ensure that the passed stack index value is valid and not used by other
    /// interrupts.
    #[inline]
    pub unsafe fn set_stack_index(&mut self, index: u16) -> &mut Self {
        self.0 &= (!0b111) | (index + 1);
        self
    }
}


#[repr(C)]
pub struct StackFrame {
    info: StackFrameInfo,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct StackFrameInfo {
    pub rip: u64,
    pub cs: u64,
    pub rflags: u64,
    pub rsp: u64,
    pub ss: u64,
}

#[repr(transparent)]
pub struct PageFaultErrorCode(u64);

impl PageFaultErrorCode {
    #[inline]
    fn present(&self) -> bool {
        self.0 & 1 != 0
    }

    #[inline]
    fn write(&self) -> bool {
        self.0 & (1 << 1) != 0
    }

    #[inline]
    fn user(&self) -> bool {
        self.0 & (1 << 2) != 0
    }

    #[inline]
    fn reserved(&self) -> bool {
        self.0 & (1 << 3) != 0
    }

    #[inline]
    fn instruction_fetch(&self) -> bool {
        self.0 & (1 << 4) != 0
    }
}

pub type Handler = extern "x86-interrupt" fn(StackFrame);
pub type ErrorCodeHandler = extern "x86-interrupt" fn(StackFrame, error_code: u64);
pub type PageFaultHandler = extern "x86-interrupt" fn(StackFrame, error_code: PageFaultErrorCode);

pub type HandlerNoReturn = extern "x86-interrupt" fn(StackFrame) -> !;
pub type ErrorCodeHandlerNoReturn = extern "x86-interrupt" fn(StackFrame, error_code: u64) -> !;
