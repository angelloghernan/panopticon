use core::mem::MaybeUninit;

/// A circular buffer allocated on the stack (or data section).
/// Cannot grow in size and only has as much memory as given by N.
pub struct CircularBuffer<const N: usize, T> {
    size: u32,
    start: u32,
    items: [MaybeUninit<T>; N],
}

impl<const N: usize, T> CircularBuffer<N, T> {
    #[must_use]
    pub fn new() -> Self {
        Self {
            size: 0,
            start: 0,
            items: MaybeUninit::uninit_array(),
        }
    }

    fn front_maybe_uninit_mut(&mut self) -> &mut MaybeUninit<T> {
        &mut self.items[self.start as usize]
    }

    fn front_maybe_uninit(&mut self) -> &MaybeUninit<T> {
        &self.items[self.start as usize]
    }

    fn back_maybe_uninit_mut(&mut self) -> &mut MaybeUninit<T> {
        &mut self.items[Self::mod_add(self.start, self.size - 1) as usize]
    }

    fn back_maybe_uninit(&mut self) -> &MaybeUninit<T> {
        &self.items[Self::mod_add(self.start, self.size - 1) as usize]
    }

    fn inc_start(&mut self) {
        self.start = (self.start + 1) % (N as u32);
    }

    fn dec_size(&mut self) {
        self.size -= 1
    }

    /// Return whether the circular buffer is empty.
    pub fn empty(&self) -> bool {
        self.size == 0
    }

    /// Push back a new item to the buffer. Or, if the buffer is full, replace the first element in
    /// the buffer with the item that has been pushed back.
    pub fn push_back(&mut self, item: T) {
        if N == 0 {
            return;
        }

        if self.size as usize >= N {
            // We can assume this is safe, since by this point we must have written to the front
            unsafe { core::ptr::drop_in_place(self.front_maybe_uninit_mut().as_mut_ptr()) };
            self.front_maybe_uninit_mut().write(item);
            self.inc_start()
        } else {
            self.back_maybe_uninit_mut().write(item);
            self.inc_start()
        }
    }

    /// Pop back the last element. If there is no element, return None, else Some(T)
    pub fn pop_back(&mut self) -> Option<T> {
        if self.size == 0 {
            None
        } else {
            let item = unsafe { self.back_maybe_uninit().assume_init_read() };

            self.dec_size();

            Some(item)
        }
    }

    /// Pop back without checking if there is an element at the location.
    pub unsafe fn pop_back_unchecked(&mut self) -> T {
        let item = self.back_maybe_uninit();
        unsafe { item.assume_init_read() }
    }

    #[inline]
    fn mod_add(x: u32, y: u32) -> u32 {
        (x + y) % (N as u32)
    }
}
