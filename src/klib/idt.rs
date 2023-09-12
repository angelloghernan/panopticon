use crate::klib::x86_64;
use x86_64::CanonicalAddress;

use core::marker::PhantomData;
use core::fmt;

#[repr(C)]
#[repr(align(16))]
pub struct DescriptorTable {
    pub division_error: Entry<Handler>,
    pub debug: Entry<Handler>,
    pub non_maskable: Entry<Handler>,
    pub breakpoint: Entry<Handler>,
    pub overflow: Entry<Handler>,
    pub bound_range_exceeded: Entry<Handler>,
    pub invalid_opcode: Entry<Handler>,
    pub device_not_available: Entry<Handler>,
    pub double_fault: Entry<ErrorCodeHandlerNoReturn>,
    _coprocessor_segment_overrun: Entry<Handler>, // left unused on purpose; for legacy systems
    pub invalid_tss: Entry<ErrorCodeHandler>,
    pub segment_not_present: Entry<ErrorCodeHandler>,
    pub stack_segment_fault: Entry<ErrorCodeHandler>,
    pub general_protection_fault: Entry<ErrorCodeHandler>,
    pub page_fault: Entry<PageFaultHandler>,
    _reserved: Entry<Handler>,
    pub x87_floating_point_exception: Entry<Handler>,
    pub alignment_check: Entry<ErrorCodeHandler>,
    pub machine_check: Entry<HandlerNoReturn>,
    pub simd_floating_point_exception: Entry<Handler>,
    pub virtualization_exception: Entry<Handler>,
    pub control_protection_exception: Entry<ErrorCodeHandler>,
    _reserved2: [Entry<Handler>; 8],
    pub hypervisor_injection_exception: Entry<Handler>,
    pub vmm_communication_exception: Entry<ErrorCodeHandler>,
    pub security_exception: Entry<ErrorCodeHandler>,
    _reserved3: Entry<Handler>,
    pub user_interrupts: [Entry<Handler>; 256 - 32],
}

impl DescriptorTable {
    #[inline]
    pub fn load(&'static self) {
        unsafe {
            x86_64::lidt(&self.pointer())
        }
    }

    #[inline]
    pub fn pointer(&self) -> x86_64::DescriptorTablePointer {
        x86_64::DescriptorTablePointer {
            limit: (core::mem::size_of::<Self>() - 1) as u16,
            base: unsafe { CanonicalAddress::new_unsafe(self as *const _ as u64) },
        }
    }
}

impl Default for DescriptorTable {
    #[inline]
    fn default() -> Self {
        Self {
            division_error: Default::default(),
            debug: Default::default(),
            non_maskable: Default::default(),
            breakpoint: Default::default(),
            overflow: Default::default(),
            bound_range_exceeded: Default::default(),
            invalid_opcode: Default::default(),
            device_not_available: Default::default(),
            double_fault: Default::default(),
            _coprocessor_segment_overrun: Default::default(),  
            invalid_tss: Default::default(),
            segment_not_present: Default::default(),
            stack_segment_fault: Default::default(),
            general_protection_fault: Default::default(),
            page_fault: Default::default(),
            _reserved: Default::default(),
            x87_floating_point_exception: Default::default(),
            alignment_check: Default::default(),
            machine_check: Default::default(),
            simd_floating_point_exception: Default::default(),
            virtualization_exception: Default::default(),
            control_protection_exception: Default::default(),
            _reserved2: Default::default(),
            hypervisor_injection_exception: Default::default(),
            vmm_communication_exception: Default::default(),
            security_exception: Default::default(),
            _reserved3: Default::default(),
            user_interrupts: [Default::default(); 256 - 32],
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
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
    fn set_handler_addr(&mut self, handler_addr: u64) {
        self.pointer_low = handler_addr as u16;
        self.pointer_middle = (handler_addr >> 16) as u16;
        self.pointer_high = (handler_addr >> 32) as u32;
        self.gdt_selector = x86_64::read_cs();
        self.options.set_present(true);
    }
}

impl<F> Default for Entry<F> {
    fn default() -> Self {
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
                // as ring 1/2 are not used
}

#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EntryOptions(u16);

impl EntryOptions {
    #[inline]
    /// Set the present bit, as well as the privilege bit to be 3 (or, user mode allowed)
    pub fn minimal() -> Self {
        Self(0b1110_0000_0000)
    }

    #[inline]
    pub fn set_present(&mut self, present: bool) -> &mut Self {
        if present {
            self.0 |= 1 << 15;
        } else {
            self.0 &= 0x7FFF;
        }
        self
    }

    #[inline]
    pub fn disable_interrupts_when_invoked(&mut self, present: bool) -> &mut Self {
        // Switch between an interrupt gate and a trap gate
        // FIXME
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

impl fmt::Debug for StackFrame {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.info.fmt(f)
    }
}

#[repr(C)]
#[derive(Clone, Debug)]
pub struct StackFrameInfo {
    pub rip: CanonicalAddress,
    pub cs: u64,
    pub rflags: u64,
    pub rsp: CanonicalAddress,
    pub ss: u64,
}

#[repr(transparent)]
#[derive(Debug)]
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

macro_rules! impl_set_handler_fn {
    ($h:ty) => {
        impl Entry<$h> {
            /// Set this IDT entry to use the passed handler function.
            /// The IDT entry will also automatically use the current code segment.
            #[inline]
            pub fn set_handler_fn(&mut self, handler: $h) {
                self.set_handler_addr(handler as u64)
            }
        }
    }
}

impl_set_handler_fn!(Handler);
impl_set_handler_fn!(ErrorCodeHandler);
impl_set_handler_fn!(PageFaultHandler);

impl_set_handler_fn!(HandlerNoReturn);
impl_set_handler_fn!(ErrorCodeHandlerNoReturn);
