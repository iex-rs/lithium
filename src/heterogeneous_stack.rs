use core::cell::{Cell, UnsafeCell};
use core::mem::{size_of, MaybeUninit};

/// A thread-unsafe heterogeneous arrayvec-like stack.
// Safety invariants:
// - len is a factor of `align_of::<AlignAs>()`
// - `len <= CAPACITY`
// - There are no references to `data[len..]`; `data[..len]` may be arbitrarily referenced
// - All elements are located in order at offsets `[0; len)`, with sizes rounded up to a factor of
//   `align_of::<AlignAs>()`
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

    /// Returns the size of `T`, rounded up to a facctor of `align_of::<AlignAs>()`.
    ///
    /// Also ensures `T` is no more aligned than `AlignAs`.
    fn get_aligned_size<T>() -> usize {
        const {
            assert!(align_of::<T>() <= align_of::<AlignAs>());
        }
        size_of::<T>().next_multiple_of(align_of::<AlignAs>())
    }

    /// Allocate enough space for an instance of `T`, returning a reference to the new instance.
    ///
    /// `T` must be no more aligned than `AlignAs`. This is checked statically.
    ///
    /// The new instance might be uninitialized and needs to be initialized before use.
    ///
    /// Returns `None` if there isn't enough space.
    pub fn try_push<T>(&self) -> Option<&mut MaybeUninit<T>> {
        let size = Self::get_aligned_size::<T>();

        // SAFETY: len <= CAPACITY is an invariant
        let capacity_left = unsafe { CAPACITY.unchecked_sub(self.len.get()) };
        if size > capacity_left {
            return None;
        }

        // SAFETY: len is in-bounds for data by the invariant
        let ptr = unsafe { self.data.get().byte_add(self.len.get()) };
        let ptr: *const UnsafeCell<MaybeUninit<T>> = ptr.cast();

        let ptr: *mut MaybeUninit<T> = UnsafeCell::raw_get(ptr);

        // SAFETY:
        // - ptr is aligned because both data and len are aligned to align_of::<AlignAs>(), and T is
        //   no more aligned than AlignAs
        // - ptr is non-null
        // - ptr is dereferenceable because its provenance is inferred from data
        // - MaybeUninit<T> is always valid
        // - By the invariant, there are no references to data[len..]
        let ptr: &mut MaybeUninit<T> = unsafe { &mut *ptr };

        // SAFETY: size <= capacity - len implies len + size <= capacity < usize::MAX
        self.len.set(unsafe { self.len.get().unchecked_add(size) });

        // Type invariants:
        // - len' is a factor of align_of::<AlignAs>(), as size is a factor of alignment
        // - len' <= CAPACITY still holds
        // - No references to data[len'..] have been created
        // - The new element is located immediately at len with no empty space, len' is minimal
        Some(ptr)
    }

    /// Remove an element from the top of the stack.
    ///
    /// `T` must be no more aligned than `AlignAs`. This is checked statically.
    ///
    /// # Safety
    ///
    /// The caller must ensure that:
    /// - The stack is non-empty.
    /// - The top element has type `T` (but not necessarily initialized).
    /// - All references to the top element, both immutable or mutable, must have been dropped by
    ///   the moment `pop_unchecked` is called.
    pub unsafe fn pop_unchecked<T>(&self) {
        let size = Self::get_aligned_size::<T>();

        // SAFETY: len >= size because the top element is an instance of T
        self.len.set(unsafe { self.len.get().unchecked_sub(size) });

        // Type invariants:
        // - len' is a factor of align_of::<AlignAs>(), as size is a factor of alignment
        // - len' <= len <= CAPACITY holds
        // - References to data[len'..len] have been dropped due to the safety requirement
        // - The new element is located immediately at len with no empty space, len' is minimal
    }

    /// Get a mutable reference to the top of the stack.
    ///
    /// `T` must be no more aligned than `AlignAs`. This is checked statically.
    ///
    /// # Safety
    ///
    /// The caller must ensure that:
    /// - The stack is non-empty.
    /// - The top element has type `T` (but not necessarily initialized).
    /// - No references to the top element exist.
    #[expect(clippy::mut_from_ref)]
    pub unsafe fn last_mut<T>(&self) -> &mut MaybeUninit<T> {
        let size = Self::get_aligned_size::<T>();

        // SAFETY: len >= size because the top element is an instance of T
        let offset = unsafe { self.len.get().unchecked_sub(size) };

        // SAFETY: offset is in-bounds because offset <= len <= CAPACITY
        let ptr = unsafe { self.data.get().byte_add(offset) };
        let ptr: *const UnsafeCell<MaybeUninit<T>> = ptr.cast();

        let ptr: *mut MaybeUninit<T> = UnsafeCell::raw_get(ptr);

        // SAFETY:
        // - ptr is aligned because both data and offset are aligned to align_of::<AlignAs>(), and T
        //   is no more aligned than AlignAs
        // - ptr is non-null
        // - ptr is dereferenceable because its provenance is inferred from data
        // - MaybeUninit<T> is always valid
        // - No references to the value exist by the requirement
        unsafe { &mut *ptr }
    }

    /// Check whether a reference points to within the stack.
    ///
    /// **For-zero-sized types, this always returns `true`, ignoring the reference.**
    ///
    /// Otherwise, if the reference was originally produced by [`Stack::try_push`] or
    /// [`Stack::last_mut`], this returns `true`. And if the reference was originally produced by
    /// another allocation mechanism that cannot point at objects within `Stack`, this returns
    /// `false`.
    ///
    /// If `ptr` was allocated with a type other than `T`, the return value is unspecified.
    pub fn contains_allocated<T>(&self, ptr: &T) -> bool {
        // For non-ZSTs, stack-allocated pointers addresses are within [data; data + CAPACITY), and
        // this region cannot intersect with non-ZSTs allocated by other methods.
        size_of::<T>() == 0
            || std::ptr::from_ref(ptr)
                .addr()
                .wrapping_sub(self.data.get().addr())
                < CAPACITY
    }
}
