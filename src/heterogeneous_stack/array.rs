use super::align::assert_aligned;
use core::cell::{Cell, UnsafeCell};
use core::mem::MaybeUninit;

/// A thread-unsafe array-backed stack allocator.
///
/// This allocator can allocate values with sizes that are multiples of `align_of::<AlignAs>()`,
/// guaranteeing alignment to `align_of::<AlignAs>()`.
///
/// The allocated bytes are always consecutive.
// Safety invariants:
// - len is a factor of `align_of::<AlignAs>()`
// - `len <= CAPACITY`
// - References to `data[len..]` are not used; `data[..len]` may be arbitrarily referenced
// - All elements are consecutive, with the last element ending at `len`
// - Element sizes are multiples of `align_of::<AlignAs>()`
// - Allocation necessarily succeeds if there is enough capacity left
#[repr(C)]
pub struct Stack<AlignAs, const CAPACITY: usize> {
    _align: [AlignAs; 0],
    data: UnsafeCell<[MaybeUninit<u8>; CAPACITY]>,
    len: Cell<usize>,
}

impl<AlignAs, const CAPACITY: usize> Stack<AlignAs, CAPACITY> {
    /// Create an empty stack.
    pub const fn new() -> Self {
        Self {
            len: Cell::new(0),
            _align: [],
            data: UnsafeCell::new([MaybeUninit::uninit(); CAPACITY]),
        }
    }

    /// Allocate `n` bytes.
    ///
    /// The returned pointer is guaranteed to be aligned to `align_of::<AlignAs>()` and valid for
    /// reads/writes for `n` bytes. It is also guaranteed to be unique.
    ///
    /// Returns `None` if there isn't enough space. It is guaranteed that allocation always succeeds
    /// if there's at least `n` free capacity. In particular, allocating 0 bytes always succeeds.
    ///
    /// # Panics
    ///
    /// Panics if `n` is not a multiple of `align_of::<AlignAs>()`.
    pub fn try_push(&self, n: usize) -> Option<*mut u8> {
        assert_aligned::<AlignAs>(n);

        if n == 0 {
            // Dangling pointers to ZSTs are always valid and unique. Creating `*mut AlignAs`
            // instead of *mut u8` forces alignment.
            return Some(core::ptr::dangling_mut::<AlignAs>().cast());
        }

        // SAFETY: len <= CAPACITY is an invariant
        let capacity_left = unsafe { CAPACITY.unchecked_sub(self.len.get()) };
        if n > capacity_left {
            // Type invariant: not enough capacity left
            return None;
        }

        // SAFETY: len is in-bounds for data by the invariant
        let ptr = unsafe { self.data.get().byte_add(self.len.get()) };

        // - `ptr` is aligned because both `data` and `len` are aligned
        // - `ptr` is valid for reads/writes for `n` bytes because it's a subset of an allocation
        // - `ptr` is unique by the type invariant
        let ptr: *mut u8 = ptr.cast();

        // SAFETY: n <= capacity - len implies len + n <= capacity < usize::MAX
        self.len.set(unsafe { self.len.get().unchecked_add(n) });

        // Type invariants:
        // - len' is a factor of align_of::<AlignAs>(), as n is a factor of alignment
        // - len' <= CAPACITY still holds
        // - References to data[len'..] are not used by the invariant as len' >= len
        // - The new element is located immediately at len with no empty space, len' is minimal
        Some(ptr)
    }

    /// Remove `n` bytes from the top of the stack.
    ///
    /// # Panics
    ///
    /// Panics if `n` is not a multiple of `align_of::<AlignAs>()`.
    ///
    /// # Safety
    ///
    /// The caller must ensure that:
    /// - The stack has at least `n` bytes allocated.
    /// - References to the top `n` bytes, both immutable or mutable, are not used after
    ///   `pop_unchecked` is called.
    pub unsafe fn pop_unchecked(&self, n: usize) {
        assert_aligned::<AlignAs>(n);

        // For ZSTs, this is a no-op.
        // SAFETY: len >= n by the safety requirement
        self.len.set(unsafe { self.len.get().unchecked_sub(n) });

        // Type invariants:
        // - len' is a factor of align_of::<AlignAs>(), as n is a factor of alignment
        // - len' <= len <= CAPACITY holds
        // - References to data[len'..len] are not used by the safety requirement
        // - The previous allocation ends at len'
    }

    /// Check whether an allocation is within the stack.
    ///
    /// If `ptr` was produced from allocating `n` bytes with this stack and the stack hasn't been
    /// moved since the allocation, this returns `true`.
    ///
    /// If `ptr` was produced by another allocator that couldn't have used the stack space
    /// **and `n > 0`**, this returns `false`.
    ///
    /// If `n = 0`, this always returns `true`, ignoring the pointer.
    ///
    /// In all other cases, the return value is unspecified.
    pub fn contains_allocated(&self, ptr: *const u8, n: usize) -> bool {
        if n == 0 {
            return true;
        }
        // Types larger than CAPACITY can never be successfully allocated
        CAPACITY.checked_sub(n).is_some_and(|limit| {
            // For non-ZSTs, stack-allocated pointers addresses are within
            // [data; data + CAPACITY - n], and this region cannot intersect with non-ZSTs
            // allocated by other methods.
            ptr.addr().wrapping_sub(self.data.get().addr()) <= limit
        })
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    #[should_panic]
    fn unaligned_push() {
        Stack::<u16, 16>::new().try_push(3);
    }

    #[test]
    #[should_panic]
    fn unaligned_pop() {
        unsafe {
            Stack::<u16, 16>::new().pop_unchecked(1);
        }
    }

    #[test]
    fn overaligned() {
        #[repr(align(256))]
        struct Overaligned;
        let stack = Stack::<Overaligned, 256>::new();
        let ptr = stack.try_push(256).expect("failed to allocate");
        assert_eq!(ptr.addr() % 256, 0);
    }

    #[test]
    fn consecutive() {
        let stack = Stack::<u8, 256>::new();
        let ptr1 = stack.try_push(5).expect("failed to allocate");
        let ptr2 = stack.try_push(8).expect("failed to allocate");
        let ptr3 = stack.try_push(1).expect("failed to allocate");
        assert_eq!(ptr2.addr() - ptr1.addr(), 5);
        assert_eq!(ptr3.addr() - ptr2.addr(), 8);
        unsafe { stack.pop_unchecked(1) };
        let ptr4 = stack.try_push(2).expect("failed to allocate");
        assert_eq!(ptr3.addr(), ptr4.addr());
    }

    #[test]
    fn too_large() {
        let stack = Stack::<u8, 16>::new();
        stack.try_push(5);
        assert!(stack.try_push(12).is_none(), "allocation fit");
    }

    #[test]
    fn pop_zero() {
        let stack = Stack::<u8, 16>::new();
        unsafe {
            stack.pop_unchecked(0);
        }
    }

    #[test]
    fn push_zero() {
        let stack = Stack::<u8, 16>::new();
        stack.try_push(16).expect("failed to allocate");
        stack.try_push(0).expect("failed to allocate");
        stack.try_push(0).expect("failed to allocate");
    }

    #[test]
    fn contains_allocated() {
        let stack = Stack::<u8, 16>::new();
        let ptr = stack.try_push(1).expect("failed to allocate");
        assert!(stack.contains_allocated(ptr, 1));
        let ptr = stack.try_push(14).expect("failed to allocate");
        assert!(stack.contains_allocated(ptr, 14));
        let ptr = stack.try_push(1).expect("failed to allocate");
        assert!(stack.contains_allocated(ptr, 1));
        let ptr = stack.try_push(0).expect("failed to allocate");
        assert!(stack.contains_allocated(ptr, 0));
        assert!(stack.contains_allocated(core::ptr::null(), 0));
        assert!(!stack.contains_allocated(core::ptr::null(), 1));
        assert!(!stack.contains_allocated(&*Box::new(1), 1));
    }
}
