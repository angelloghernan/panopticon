mod sleb;

// use crate::println;
use core::alloc::GlobalAlloc;
use core::alloc::Layout;
use core::cmp::max;
use lazy_static::lazy_static;
use spin::Mutex;
use x86_64::{
    structures::paging::{
        mapper::MapToError, FrameAllocator, Mapper, Page, PageTableFlags, Size4KiB,
    },
    VirtAddr,
};

pub const HEAP_START: u64 = 0x_4444_4444_0000u64;
const KB: u64 = 1024;
pub const HEAP_SIZE: u64 = 4096 * KB;
pub const HEAP_END: u64 = HEAP_START + HEAP_SIZE;

pub const PAGESIZE: u64 = 4096;
const BLOCK_SIZE: u64 = PAGESIZE;

const START_ORDER: u16 = PAGESIZE.ilog2() as u16;

const NUM_ORDERS: u16 = HEAP_SIZE.ilog2() as u16 - START_ORDER;
const NUM_BLOCKS: u64 = HEAP_SIZE / 4096;

const NO_BLOCK: u16 = 0xFFFF;

#[global_allocator]
static ALLOCATOR: Locked<BuddyAllocator> = Locked::new(BuddyAllocator::new());

lazy_static! {
    static ref SLEB_ALLOCATOR: Locked<&'static mut sleb::SlebMetadataPage> = {
        unsafe {
            let ptr = ALLOCATOR.alloc(Layout::from_size_align_unchecked(1048576, 1));
            let sleb = sleb::SlebMetadataPage::init(ptr);

            Locked::new(&mut *(sleb))
        }
    };
}

pub struct Locked<A> {
    inner: spin::Mutex<A>,
}

impl<A> Locked<A> {
    pub const fn new(inner: A) -> Self {
        Locked {
            inner: spin::Mutex::new(inner),
        }
    }

    pub fn lock(&self) -> spin::MutexGuard<A> {
        self.inner.lock()
    }
}

#[derive(Clone, Copy)]
struct Block {
    previous: u16,
    next: u16,
    order: u16,
    free: bool,
}

impl Block {
    fn split(order: u16, index: u16) -> (u16, u16) {
        let new_order = order - 1;

        let left_block = index;
        let right_block = left_block + (1 << new_order);

        (left_block, right_block)
    }

    fn index_to_ptr(index: u16) -> *mut u8 {
        let index_64 = index as u64;
        return (HEAP_START + index_64 * BLOCK_SIZE) as *mut u8;
    }

    fn get_buddy_index(order: u16, address: u64) -> Option<u16> {
        let shifted_address = address - HEAP_START;
        let is_lower = shifted_address % (1 << (order + 1 + START_ORDER)) == 0;
        let buddy_address = if is_lower {
            shifted_address.checked_add(1 << (order + START_ORDER))
        } else {
            shifted_address.checked_sub(1 << (order + START_ORDER))
        };

        match buddy_address {
            None => None,
            Some(address) => {
                if address + HEAP_START < HEAP_END {
                    Some((address / PAGESIZE) as u16)
                } else {
                    None
                }
            }
        }
    }
}

struct BuddyAllocator {
    blocks: [Block; NUM_BLOCKS as usize],
    heads: [u16; NUM_ORDERS as usize],
}

impl BuddyAllocator {
    const fn new() -> Self {
        let block = Block {
            previous: NO_BLOCK,
            next: NO_BLOCK,
            order: 1,
            free: false,
        };

        let mut blocks = [block; NUM_BLOCKS as usize];
        blocks[0].free = true;
        blocks[0].order = NUM_ORDERS - 1;

        let mut heads = [NO_BLOCK; NUM_ORDERS as usize];
        heads[(NUM_ORDERS - 1) as usize] = 0;

        Self {
            blocks: [block; NUM_BLOCKS as usize],
            heads,
        }
    }

    fn get_block_index(block_ptr: *mut u8) -> usize {
        let block_addr = block_ptr as u64;
        let block_offset = block_addr - HEAP_START;
        (block_offset / PAGESIZE) as usize
    }

    fn pop_head(&mut self, order: u16) -> Option<u16> {
        let head_index = self.heads[order as usize];

        if head_index == NO_BLOCK {
            return None;
        }

        self.blocks[head_index as usize].free = false;

        let head = self.blocks[head_index as usize];
        self.heads[head_index as usize] = head.next;

        Some(head_index)
    }

    fn push_block(&mut self, order: u16, block_index: u16) {
        debug_assert!(!self.blocks[block_index as usize].free);
        let head_index = self.heads[order as usize];

        if head_index != NO_BLOCK {
            self.blocks[head_index as usize].previous = block_index;
        }

        self.heads[order as usize] = block_index;

        self.blocks[block_index as usize].next = head_index;
        self.blocks[block_index as usize].order = order;
        self.blocks[block_index as usize].free = true;
    }

    fn remove_block(&mut self, order: u16, block_index: u16) {
        debug_assert!(self.blocks[block_index as usize].free);
        self.blocks[block_index as usize].free = false;
        let prev_index = self.blocks[block_index as usize].previous;
        let next_index = self.blocks[block_index as usize].next;

        if prev_index != NO_BLOCK {
            self.blocks[block_index as usize].previous = NO_BLOCK;
            self.blocks[prev_index as usize].next = next_index;
        } else if self.heads[order as usize] == block_index {
            self.heads[order as usize] = next_index;
        }

        if next_index != NO_BLOCK {
            self.blocks[block_index as usize].next = NO_BLOCK;
            self.blocks[next_index as usize].previous = prev_index;
        }
    }
}

fn round_up_pow2(mut num: u64) -> u64 {
    num -= 1;
    num |= num >> 1;
    num |= num >> 2;
    num |= num >> 4;
    num |= num >> 8;
    num |= num >> 16;
    num |= num >> 32;
    num + 1
}

unsafe impl GlobalAlloc for Locked<BuddyAllocator> {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let size = max(layout.size(), layout.align());
        if size <= 2048 {
            let mut sleb_alloc = SLEB_ALLOCATOR.lock();
            let ptr = sleb_alloc.alloc(size);
            return ptr;
        }

        let alloc_size = if layout.size() > layout.align() {
            max(PAGESIZE, round_up_pow2(layout.size() as u64)).checked_ilog2()
        } else {
            max(PAGESIZE, round_up_pow2(layout.align() as u64)).checked_ilog2()
        };

        // Safety: The checked_ilog2 call cannot fail since GlobalAlloc
        // must never be called with layout size == 0
        let alloc_size = unsafe { alloc_size.unwrap_unchecked() };

        let order_start = (alloc_size - PAGESIZE.ilog2()) as u16;

        let mut allocator = self.lock();

        if let Some(index) = allocator.pop_head(order_start) {
            return Block::index_to_ptr(index);
        }

        for order in (order_start + 1)..(NUM_ORDERS as u16) {
            if let Some(index) = allocator.pop_head(order) {
                for down_order in ((order_start + 1)..=order).rev() {
                    let (_, right_block) = Block::split(down_order, index);
                    allocator.push_block(down_order - 1, right_block);
                }

                allocator.blocks[index as usize].order = order_start;

                return Block::index_to_ptr(index);
            }
        }

        0u64 as *mut u8
    }

    unsafe fn dealloc(&self, ptr: *mut u8, _layout: Layout) {
        {
            let mut sleb = SLEB_ALLOCATOR.lock();
            if sleb.within_bounds(ptr) {
                return sleb.free(ptr);
            }
        };

        let block_index = BuddyAllocator::get_block_index(ptr);
        let block_addr = ptr as u64;
        let mut allocator = self.lock();
        let mut order = allocator.blocks[block_index].order;

        while let Some(buddy_index) = Block::get_buddy_index(order, block_addr) {
            if !allocator.blocks[buddy_index as usize].free {
                break;
            }
            allocator.remove_block(order, buddy_index);
            order += 1;
        }

        allocator.push_block(order, block_index as u16);
    }
}

pub fn init_heap(
    mapper: &mut impl Mapper<Size4KiB>,
    frame_allocator: &mut impl FrameAllocator<Size4KiB>,
) -> Result<(), MapToError<Size4KiB>> {
    let page_range = {
        let heap_start = VirtAddr::new(HEAP_START as u64);
        let heap_end = heap_start + HEAP_SIZE - 1u64;
        let heap_start_page = Page::containing_address(heap_start);
        let heap_end_page = Page::containing_address(heap_end);
        Page::range_inclusive(heap_start_page, heap_end_page)
    };

    for page in page_range {
        let frame = frame_allocator
            .allocate_frame()
            .ok_or(MapToError::FrameAllocationFailed)?;
        let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;
        unsafe { mapper.map_to(page, frame, flags, frame_allocator)?.flush() };
    }

    Ok(())
}
