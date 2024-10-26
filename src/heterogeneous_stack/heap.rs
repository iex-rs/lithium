use core::alloc::Layout;
use core::marker::PhantomData;
use std::alloc;

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
    /// Panics if `n` is not a multiple of `align_of::<AlignAs>()` or `n` is 0.
    pub fn alloc(&self, n: usize) -> *mut u8 {
        assert!(n % align_of::<AlignAs>() == 0);
        assert!(n != 0);
        let layout = Layout::from_size_align(n, align_of::<AlignAs>()).unwrap();
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
    pub unsafe fn dealloc(&self, ptr: *mut u8, n: usize) {
        let layout = Layout::from_size_align(n, align_of::<AlignAs>()).unwrap();
        // SAFETY:
        // - ptr was allocated with the same allocator
        // - alloc would fail if n == 0, so we know n != 0 holds here
        unsafe { alloc::dealloc(ptr, layout) }
    }
}
