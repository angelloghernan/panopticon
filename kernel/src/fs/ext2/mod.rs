use super::super::klib;
use core::mem;
use core::mem::MaybeUninit;
use klib::ahci::ahcistate::AHCIState;
use klib::ahci::ahcistate::Command;
use klib::ahci::ahcistate::IOError;
use mem::size_of;
use spin::RwLock;

const SUPERBLOCK_MAGIC: u16 = 0xEF53;
const ROOT_INO: u32 = 2;

pub struct Ext2Fs {
    superblock: Superblock,
}

impl Ext2Fs {
    pub fn new(hdd_lock: &RwLock<&mut AHCIState>) -> Result<Self, IOError> {
        let superblock = Superblock::new(hdd_lock)?;
        Ok(Self { superblock })
    }

    fn read_inode(
        &self,
        hdd_lock: &RwLock<&mut AHCIState>,
        inode_number: u32,
    ) -> Result<INode, IOError> {
        let inode_size = self.superblock.inode_size;
        let block_size = (1024 << self.superblock.log_block_size) as u32;
        let inodes_per_group = self.superblock.inodes_per_group;
        let inode_table_block = self.superblock.first_data_block + 2; // FIXME: assuming inode table starts right after block group descriptor

        let inode_index = inode_number - 1;
        let group = inode_index / inodes_per_group;
        let index = inode_index % inodes_per_group;

        let inode_offset = (inode_table_block as u64 * block_size as u64)
            + (group as u64 * inodes_per_group as u64 * inode_size as u64)
            + (index as u64 * inode_size as u64);

        let mut inode: MaybeUninit<INode> = MaybeUninit::uninit();

        AHCIState::read_or_write(
            hdd_lock,
            Command::Read,
            inode.as_bytes_mut(),
            inode_offset as usize,
        )?;

        unsafe { Ok(inode.assume_init()) }
    }
}

#[repr(C)]
#[derive(Debug)]
pub struct Superblock {
    pub inodes_count: u32,
    pub blocks_count: u32,
    pub r_blocks_count: u32,
    pub free_blocks_count: u32,
    pub free_inodes_count: u32,
    pub first_data_block: u32,
    pub log_block_size: u32,
    pub log_frag_size: i32,
    pub blocks_per_group: u32,
    pub frags_per_group: u32,
    pub inodes_per_group: u32,
    pub last_mount_time: u32,
    pub last_written_time: u32,
    pub mnt_count: u16,
    pub max_mnt_count: i16,
    pub magic: u16,
    pub state: FsState,
    pub errors: ErrorHandling,
    pub minor_rev_level: u16,
    pub lastcheck: u32,
    pub checkinterval: u32,
    pub creator_os: u32,
    pub rev_level: u32,
    pub def_resuid: u16,
    pub def_resgid: u16,
    // EXT2_DYNAMIC_REV specific (unused)
    pub first_ino: u32,
    pub inode_size: u16,
    pub block_group_nr: u16,
    pub feature_compat: u32,
    pub feature_incompat: u32,
    pub feature_ro_compat: u32,
    pub uuid: [u8; 16],
    pub volume_name: [u8; 16],
    pub last_mounted: [u8; 64],
    pub algorithm_usage_bitmap: u32,
    // Performance hints (unused)
    pub prealloc_blocks: u8,
    pub prealloc_dir_blocks: u8,
    pub reserved_gdt_blocks: u16,
    // Journaling support (unused)
    pub journal_uuid: [u8; 16],
    pub journal_inum: u32,
    pub journal_device: u32,
    pub last_orphan: u32,
    // Unused bytes
    pub reserved: [u8; 788],
}

const _: () = assert!(
    size_of::<Superblock>() == 1024,
    "Ext2 Superblock must be exactly 1024 bytes"
);

#[repr(C)]
struct INode {
    mode: u16,
    uid: u16,
    size: u32,
    atime: u32,
    ctime: u32,
    mtime: u32,
    dtime: u32,
    gid: u16,
    links_count: u16,
    blocks: u32,
    flags: u32,
    osd1: u32,
    block: [u32; 15],
    generation: u32,
    file_acl: u32,
    dir_acl: u32,
    faddr: u32,
    osd2: [u8; 12],
}

#[repr(u16)]
#[derive(Debug)]
pub enum FsState {
    Clean = 1,
    HasErrors = 2,
}

#[repr(u16)]
#[derive(Debug)]
pub enum ErrorHandling {
    Ignore = 1,
    RemountReadOnly = 2,
    KernelPanic = 3,
}

impl Superblock {
    /// Try to read the superblock into memory.
    /// Returns an error if this disk does not have the EXT2 magic, or if there is an error reading
    /// the disk.
    pub fn new(drive_lock: &RwLock<&mut AHCIState>) -> Result<Self, IOError> {
        let mut uninit_self: MaybeUninit<Self> = MaybeUninit::uninit();

        AHCIState::read_or_write(drive_lock, Command::Read, uninit_self.as_bytes_mut(), 1024)?;

        unsafe {
            let has_sig = uninit_self.assume_init_ref().has_signature();
            match has_sig {
                true => Ok(uninit_self.assume_init()),
                false => Err(IOError::BadData),
            }
        }
    }

    pub fn has_signature(&self) -> bool {
        self.magic == SUPERBLOCK_MAGIC
    }
}
