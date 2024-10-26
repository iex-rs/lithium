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
            return Some(std::ptr::dangling_mut::<AlignAs>().cast());
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

    /// Remove bytes from the top of the stack.
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

    /// Get a mutable pointer to the top `n` bytes of the stack.
    ///
    /// The return value points at the *first* of the `n` bytes.
    ///
    /// # Safety
    ///
    /// The caller must ensure that the stack has at least `n` bytes allocated.
    ///
    /// Dereferencing the resulting pointer requires that the caller ensures it doesn't alias.
    pub unsafe fn last_mut(&self, n: usize) -> *mut u8 {
        if n == 0 {
            return std::ptr::dangling_mut();
        }

        // SAFETY: len >= n by the safety requirement
        let offset = unsafe { self.len.get().unchecked_sub(n) };

        // SAFETY: offset is in-bounds because offset <= len <= CAPACITY
        let ptr = unsafe { self.data.get().byte_add(offset) };
        ptr.cast()
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
