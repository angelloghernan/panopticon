use super::PAGESIZE;
use crate::println;
use bitfield::bitfield;
use spin::Mutex;

/// PAN_SLEB -- Simple List of Extended Blocks
/// Suggestions open for what the 'E' should actually stand for if anyone actually reads this.
/// I'm just aping Linux naming conventions lol
///
/// Design -- Since allocations happen on a page-level basis, we begin the start of a "collection"
/// using a page of SLEB metadata. The metadata defines the sizes and types of allocations for the
/// next megabyte(?) of allocations.
///
/// An alignment of 16 is assumed for all of these allocations. Alignments larger than 16 should not be
/// served by SLEB, although we don't expect that to basically ever happen.

#[repr(transparent)]
#[derive(Clone, Copy)]
struct BucketIndex {
    index: u32,
}

const BUCKET_INDEX_NONE: u32 = u32::MAX - 1;

static TINY_BUCKETS: Mutex<[BucketIndex; 1]> = Mutex::new([BucketIndex::new(BUCKET_INDEX_NONE); 1]);

static MEDIUM_BUCKETS: Mutex<[BucketIndex; 5]> =
    Mutex::new([BucketIndex::new(BUCKET_INDEX_NONE); 5]);

static NONE_BUCKET: Mutex<BucketIndex> = Mutex::new(BucketIndex::new(BUCKET_INDEX_NONE));

#[repr(C)]
#[derive(Clone)]
struct SlebMetadata {
    pub bits: SlebBits,
    pub extra_bits: u64, // Extra bits used for medium-sized blocks, as a bitmap
}

impl SlebMetadata {
    fn get_type(&self) -> MetadataType {
        use MetadataType::*;
        match self.bits.sleb_type() {
            0 => Empty,
            1 => Tiny32,
            2 => Medium64,
            3 => Medium128,
            4 => Medium256,
            5 => Medium512,
            6 => Medium1024,
            _ => Medium2048,
        }
    }
}

const ENTRIES_PER_PAGE: usize = (PAGESIZE as usize) / core::mem::size_of::<SlebMetadata>() - 1;
const ONE_MIB: usize = 1048576;

#[repr(C)]
pub struct SlebMetadataPage {
    _padding: [u8; 16], // reserved for future use
    metadata: [SlebMetadata; ENTRIES_PER_PAGE],
}

impl SlebMetadataPage {
    /// Initialize a page for SLEB metadata
    /// ### Safety
    /// The pointer must be valid and point to a contiguous region of memory at least 1 MiB in
    /// size. The pointer must remain valid for the entire lifetime of this page.
    pub unsafe fn init(page: *mut u8) -> *mut Self {
        page.write_bytes(0, PAGESIZE as usize);
        page as *mut Self
    }

    pub fn within_bounds(&self, ptr: *const u8) -> bool {
        let self_ptr = self as *const SlebMetadataPage as *const u8;
        let self_addr = self_ptr as usize;
        let ptr_addr = ptr as usize;

        self_addr < ptr_addr && ptr_addr < self_addr + ONE_MIB
    }

    fn index_to_ptr(&self, index: u32) -> *const u8 {
        let base = self as *const SlebMetadataPage as u64;
        (base + PAGESIZE * (index + 1) as u64) as *const u8
    }

    fn index_to_mut_ptr(&mut self, index: u32) -> *mut u8 {
        let base = self as *mut SlebMetadataPage as u64;
        (base + PAGESIZE * (index + 1) as u64) as *mut u8
    }

    // Find and return an empty page, making its type equal to the type passed in.
    fn find_empty_page(&mut self, md_type: MetadataType) -> Option<u32> {
        for (i, md) in self.metadata.iter_mut().enumerate() {
            if let MetadataType::Empty = md.get_type() {
                md.bits.set_type(md_type as u8);
                return Some(i as u32);
            }
        }
        None
    }

    fn alloc_tiny(&mut self) -> *mut u8 {
        let mut buckets = TINY_BUCKETS.lock();
        while let Some(i) = buckets[0].get() {
            let metadata = self.metadata[i as usize].clone();
            let page = self.index_to_mut_ptr(i) as *mut TinyMetadataPage;
            let slot = unsafe { TinyMetadataPage::find_free_slot(page) };
            if !slot.is_null() {
                return slot;
            } else {
                let next = metadata.bits.next_index();
                buckets[0] = BucketIndex::new(next);
            }
        }

        match self.find_empty_page(MetadataType::Tiny32) {
            None => 0u64 as *mut u8,
            Some(i) => {
                let page = self.index_to_mut_ptr(i) as *mut TinyMetadataPage;
                unsafe {
                    // Reset bitfield
                    for bits in (*page).bitfield.iter_mut() {
                        *bits = 0;
                    }
                    TinyMetadataPage::find_free_slot(page)
                }
            }
        }
    }

    fn alloc_medium(&mut self, index: usize) -> *mut u8 {
        let mut buckets = MEDIUM_BUCKETS.lock();
        while let Some(i) = buckets[index].get() {
            let slot = self.metadata[i as usize].extra_bits.trailing_ones();
            if slot < 64 {
                self.metadata[i as usize].extra_bits |= 1u64 << slot;
                let page = self.index_to_mut_ptr(i) as *mut MetadataPage;
                // explanation: 1 << (index + 6) = 2^(index + 6). we add 6 since the smallest
                // medium block size is 64
                return unsafe { MetadataPage::take_slot(page, slot, 1 << (index + 6)) };
            } else {
                let next = self.metadata[i as usize].bits.next_index();
                buckets[index] = BucketIndex::new(next);
            }
        }

        // Extra bits (the bitmap) has the first block taken, along with any blocks past the end of
        // the page being "used".
        let (md_type, extra_bits) = match index {
            0 => (MetadataType::Medium64, 0x0000_0000_0000_0001u64),
            1 => (MetadataType::Medium128, 0xFFFF_FFFF_0000_0001u64),
            2 => (MetadataType::Medium256, 0xFFFF_FFFF_FFFF_0001u64),
            3 => (MetadataType::Medium512, 0xFFFF_FFFF_FFFF_FF01u64),
            4 => (MetadataType::Medium1024, 0xFFFF_FFFF_FFFF_FFF1u64),
            5 => (MetadataType::Medium2048, 0xFFFF_FFFF_FFFF_FFFDu64),
            _ => unreachable!(),
        };

        match self.find_empty_page(md_type) {
            None => 0u64 as *mut u8,
            Some(i) => {
                self.metadata[i as usize].extra_bits = extra_bits;
                let page = self.index_to_mut_ptr(i) as *mut MetadataPage;
                unsafe { MetadataPage::take_slot(page, 0, 1 << (index + 6)) }
            }
        }
    }

    pub fn alloc(&mut self, size: usize) -> *mut u8 {
        match size {
            0..=32 => self.alloc_tiny(),
            33..=64 => self.alloc_medium(0),
            65..=128 => self.alloc_medium(1),
            129..=256 => self.alloc_medium(2),
            257..=512 => self.alloc_medium(3),
            513..=1024 => self.alloc_medium(4),
            1025..=2048 => self.alloc_medium(5),
            _ => panic!("Bad allocation size"),
        }
    }

    unsafe fn free_tiny(&mut self, md_index: u32, ptr: *mut u8) {
        println!("Free tiny with {md_index} and {:#x}", ptr as usize);
        let ptr_addr = ptr as usize;
        let ptr_offset = ptr_addr % (PAGESIZE as usize);
        let page = self.index_to_mut_ptr(md_index) as *mut TinyMetadataPage;
        let slot = ptr_offset / 32;

        debug_assert!((*page).bitfield[slot / 64] & (1u64 << (slot % 64)) != 0);

        let was_full = (*page).bitfield.iter().all(|&x| x == u64::MAX);

        (*page).bitfield[slot / 64] &= !(1u64 << (slot % 64));
        if was_full {
            // This bitfield has some space left. Make the bucket point to it in the linked list.
            // TODO: Make this the tail instead. Since it only has one free slot it should come
            // *last* in the priority. Buckets should hold heads and tails.
            let mut bucket = TINY_BUCKETS.lock();
            match bucket[0].get() {
                None => bucket[0].set(md_index),
                Some(i) => {
                    self.metadata[i as usize].bits.set_prev(md_index);
                    bucket[0].set(md_index);
                    self.metadata[md_index as usize].bits.set_next(i);
                }
            }
        }
    }

    unsafe fn free_medium(&mut self, md_index: u32, bucket_index: u32, ptr: *mut u8) {
        let ptr_addr = ptr as usize;
        let ptr_offset = ptr_addr % (PAGESIZE as usize);
        let slot = ptr_offset / 32;

        debug_assert!(self.metadata[md_index as usize].extra_bits & 1u64 << slot != 0);

        let was_full = self.metadata[md_index as usize].extra_bits == u64::MAX;

        self.metadata[md_index as usize].extra_bits &= !(1u64 << slot);

        if was_full {
            // This bitfield has some space left. Make the bucket point to it in the linked list.
            // TODO: Make this the tail instead. Since it only has one free slot it should come
            // *last* in the priority. Buckets should hold heads and tails.
            let mut bucket = MEDIUM_BUCKETS.lock();
            match bucket[bucket_index as usize].get() {
                None => bucket[bucket_index as usize].set(md_index),
                Some(i) => {
                    self.metadata[i as usize].bits.set_prev(md_index);
                    bucket[bucket_index as usize].set(md_index);
                    self.metadata[md_index as usize].bits.set_next(i);
                }
            }
        }
    }

    pub unsafe fn free(&mut self, ptr: *mut u8) {
        println!("Free called");
        let ptr_addr = ptr as usize;
        let self_ptr = self as *mut SlebMetadataPage as *mut u8 as usize;
        let distance = (ptr_addr - self_ptr) as usize;
        let index = distance / 4096 - 1;
        match self.metadata[index].get_type() {
            MetadataType::Tiny32 => self.free_tiny(index as u32, ptr),
            MetadataType::Medium64 => self.free_medium(index as u32, 0, ptr),
            MetadataType::Medium128 => self.free_medium(index as u32, 1, ptr),
            MetadataType::Medium256 => self.free_medium(index as u32, 2, ptr),
            MetadataType::Medium512 => self.free_medium(index as u32, 3, ptr),
            MetadataType::Medium1024 => self.free_medium(index as u32, 4, ptr),
            MetadataType::Medium2048 => self.free_medium(index as u32, 5, ptr),
            _ => panic!("Passed an \"empty\" page"),
        }
    }
}

const SMALL_32_BITFIELD_SIZE_64: usize = (4096 / 32) / 64;
const SMALL_32_PAGES_PER_PAGE: usize = (4096 / 32) - 1;

struct MemEntry<const S: usize> {
    data: [u8; S],
}

#[repr(C)]
struct TinyMetadataPage {
    bitfield: [u64; SMALL_32_BITFIELD_SIZE_64],
    entries: [MemEntry<32>; SMALL_32_PAGES_PER_PAGE],
    _padding: [u8; 16], // Use for something else?
}

#[repr(C)]
struct MetadataPage {
    entry_data: [u8; 4096],
}

impl TinyMetadataPage {
    /// Find a free slot, if possible. If no free slots, return nullptr.
    /// ### Safety:
    /// The pointer to "page" must be valid. (Live, pointing to actual TinyMetadataPage object)
    pub unsafe fn find_free_slot(page: *mut TinyMetadataPage) -> *mut u8 {
        for (i, bits) in (0..).zip((*page).bitfield) {
            if bits != u64::MAX {
                let trailing = bits.trailing_ones();
                let slot = i * 64 + trailing;
                (*page).bitfield[i as usize] |= 1u64 << trailing;
                return &mut (*page).entries[slot as usize] as *mut MemEntry<32> as *mut u8;
            }
        }
        0u64 as *mut u8
    }
}

impl MetadataPage {
    /// Take a free slot, if possible.
    /// ### Safety:
    /// The pointer to "page" must be valid. (Live, pointing to actual MetadataPage object)
    /// Additionally, the slot must be free, and the slot should be marked used after using this
    /// function. Check the metadata bitfield.
    pub unsafe fn take_slot(page: *mut MetadataPage, slot: u32, size: usize) -> *mut u8 {
        let index = (slot as usize) * size;
        &mut (*page).entry_data[index] as *mut u8
    }
}

#[repr(u8)]
#[derive(Clone, Copy)]
enum MetadataType {
    Empty = 0,
    Tiny32 = 1,
    Medium64 = 2,
    Medium128 = 3,
    Medium512 = 4,
    Medium256 = 5,
    Medium1024 = 6,
    Medium2048 = 7,
}

bitfield! {
    #[derive(Clone, Copy)]
    struct SlebBits(u64);
    impl Debug;
    u8;
    pub sleb_type, set_type: 2, 0;
    u32;
    pub next_index, set_next: 32, 3; // Notice that, since each page is 2^12 bytes, this addresses
                                     // 2^(30 + 12) = 2^42 = 4398 gb of memory
    pub prev_index, set_prev: 62, 33;
}

impl BucketIndex {
    const fn new(index: u32) -> Self {
        Self { index }
    }

    fn set(&mut self, index: u32) {
        if index == u32::MAX - 1 {
            panic!("Invalid range");
        } else {
            self.index = index;
        }
    }

    fn get(&self) -> Option<u32> {
        if self.index == u32::MAX - 1 {
            None
        } else {
            Some(self.index)
        }
    }

    unsafe fn new_unchecked(index: u32) -> Self {
        Self { index }
    }

    unsafe fn set_unchecked(&mut self, index: u32) {
        self.index = index;
    }
}
