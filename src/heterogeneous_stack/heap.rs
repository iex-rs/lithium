use super::align::assert_aligned;
use alloc::alloc;
use core::alloc::Layout;
use core::marker::PhantomData;

/// A heap-backed allocator.
///
/// This allocator can allocate values with sizes that are multiples of `align_of::<AlignAs>()`,
/// guaranteeing alignment to `align_of::<AlignAs>()`.
pub struct Heap<AlignAs>(PhantomData<AlignAs>);

impl<AlignAs> Heap<AlignAs> {
    /// Create an allocator.
    pub const fn new() -> Self {
        Self(PhantomData)
    }

    /// Allocate `n` bytes.
    ///
    /// The returned pointer is guaranteed to be aligned to `align_of::<AlignAs>()` and valid for
    /// reads/writes for `n` bytes. It is also guaranteed to be unique.
    ///
    /// # Panics
    ///
    /// Panics if `n` is not a multiple of `align_of::<AlignAs>()` or `n` is 0, or if out of memory.
    #[expect(clippy::unused_self)]
    pub fn alloc(&self, n: usize) -> *mut u8 {
        assert_aligned::<AlignAs>(n);
        assert_ne!(n, 0, "Allocating 0 bytes is invalid");
        assert!(
            n.next_multiple_of(align_of::<AlignAs>()) <= isize::MAX as usize,
            "Size overflow",
        );
        // SAFETY:
        // - `align` is a power of two, as `align_of` returns a power of two
        // - We've checked that `n <= isize::MAX` after rounding up
        let layout = unsafe { Layout::from_size_align_unchecked(n, align_of::<AlignAs>()) };
        // SAFETY: n != 0 has been checked
        unsafe { alloc::alloc(layout) }
    }

    /// Deallocate `n` bytes.
    ///
    /// # Safety
    ///
    /// The caller must ensure that the pointer was produced by a call to [`Heap::alloc`] with the
    /// same value of `n`. In addition, references to the deallocated memory must not be used after
    /// `dealloc` is called.
    #[expect(clippy::unused_self)]
    pub unsafe fn dealloc(&self, ptr: *mut u8, n: usize) {
        // SAFETY: alloc would fail if the preconditions for this weren't established
        let layout = unsafe { Layout::from_size_align_unchecked(n, align_of::<AlignAs>()) };
        // SAFETY:
        // - ptr was allocated with the same allocator
        // - alloc would fail if n == 0, so we know n != 0 holds here
        unsafe { alloc::dealloc(ptr, layout) }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    #[should_panic]
    fn alloc_zero() {
        Heap::<u8>::new().alloc(0);
    }

    #[test]
    #[should_panic]
    fn alloc_unaligned() {
        Heap::<u16>::new().alloc(3);
    }

    #[test]
    fn overaligned() {
        #[repr(align(256))]
        struct Overaligned;
        let heap = Heap::<Overaligned>::new();
        let ptr = heap.alloc(256);
        assert_eq!(ptr.addr() % 256, 0);
        unsafe {
            heap.dealloc(ptr, 256);
        }
    }

    #[test]
    fn unique() {
        let heap = Heap::<u8>::new();
        let ptr1 = unsafe { &mut *heap.alloc(1) };
        let ptr2 = unsafe { &mut *heap.alloc(1) };
        *ptr1 = 1;
        *ptr2 = 2;
        assert_eq!(*ptr1, 1);
        assert_eq!(*ptr2, 2);
        unsafe {
            heap.dealloc(ptr1, 1);
        }
        unsafe {
            heap.dealloc(ptr2, 1);
        }
    }
}
