use core::mem::size_of;
use core::slice::from_raw_parts;

pub fn as_u8_slice<T: Sized>(obj: &T) -> &[u8] {
    unsafe {
        from_raw_parts(
            (obj as *const T) as *const u8,
            size_of::<T>(),
        )
    }
}
