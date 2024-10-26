use super::array::Stack as BoundedStack;
use core::alloc::Layout;
use core::mem::MaybeUninit;

const CAPACITY: usize = 4096;

/// A thread-unsafe heterogeneous stack, using statically allocated space when possible.
// Safety invariants:
// - ZSTs are always allocated on the bounded stack.
pub struct Stack<AlignAs> {
    bounded_stack: BoundedStack<AlignAs, CAPACITY>,
}

impl<AlignAs> Stack<AlignAs> {
    /// Create an empty stack.
    pub const fn new() -> Self {
        Self {
            bounded_stack: BoundedStack::new(),
        }
    }

    /// Allocate enough space for an instance of `T`, returning a reference to the new instance.
    ///
    /// `T` must be no more aligned than `AlignAs`. This is checked statically.
    ///
    /// The new instance might be uninitialized and needs to be initialized before use.
    ///
    /// # Panics
    ///
    /// This function panics if allocating the object fails.
    pub fn push<T>(&self) -> &mut MaybeUninit<T> {
        match self.bounded_stack.try_push::<T>() {
            Some(alloc) => alloc,
            None => Box::leak(Box::new(MaybeUninit::uninit())),
        }
    }

    /// Remove an element from the top of the stack.
    ///
    /// The element is not dropped and may be uninitialized. Use [`MaybeUninit::assume_init_drop`]
    /// to drop the value explicitly.
    ///
    /// `T` must be no more aligned than `AlignAs`. This is checked statically.
    ///
    /// # Safety
    ///
    /// The caller must ensure that:
    /// - The passed pointer was obtained from `push` to this instance of [`Stack`].
    /// - The passed pointer corresponds to the top element of the stack (i.e. has a matching type,
    ///   address, and provenance).
    /// - The element is not accessed after the call to `pop`.
    pub unsafe fn pop<T>(&self, alloc: *mut MaybeUninit<T>) {
        if self.bounded_stack.contains_allocated(alloc) {
            // SAFETY:
            // - `contains_allocated` returned `true`, so either the element is allocated on the
            //   stack or it's a ZST. ZST allocation always succeeds, so this must be on the stack.
            //   By the safety requirements, it's the top element of the stack and has type `T`.
            // - The element is not accessed after the call.
            unsafe {
                self.bounded_stack.pop_unchecked::<T>();
            }
        } else {
            // SAFETY: `contains_allocated` returned `false`, so the allocation is not on the stack.
            // By the requirements, the pointer was produced by `push`, so the allocation has to be
            // on the heap with `Box`.
            unsafe {
                let _ = Box::from_raw(alloc);
            }
        }
    }

    /// Modify the last element, possibly changing its type.
    ///
    /// This is a more efficient version of
    ///
    /// ```no_compile
    /// stack.pop(alloc);
    /// stack.push()
    /// ```
    ///
    /// Both `T` and `U` must be no more aligned than `AlignAs`. This is checked statically.
    ///
    /// # Panics
    ///
    /// This function panics if allocating the object fails.
    ///
    /// # Safety
    ///
    /// The same considerations apply as to [`Stack::pop`]. The caller must ensure that:
    /// - The passed pointer was obtained from `push` (or `replace_last`) to this instance of
    ///   [`Stack`].
    /// - The passed pointer corresponds to the top element of the stack (i.e. has a matching type,
    ///   address, and provenance).
    /// - The element is not accessed after the call to `replace_last`.
    pub unsafe fn replace_last<T, U>(&self, alloc: *mut MaybeUninit<T>) -> &mut MaybeUninit<U> {
        let old_size = BoundedStack::<AlignAs, CAPACITY>::get_aligned_size::<T>();
        let new_size = BoundedStack::<AlignAs, CAPACITY>::get_aligned_size::<U>();
        if self.bounded_stack.contains_allocated::<T>(alloc.cast()) {
            unsafe {
                self.bounded_stack.pop_unchecked::<T>();
            }
            if new_size <= old_size {
                // Necessarily fits in local data
                unsafe { self.bounded_stack.try_push::<U>().unwrap_unchecked() };
                return unsafe { &mut *alloc.cast::<MaybeUninit<U>>() };
            }
        } else {
            // Box<T>'s are compatible as long as Ts have identical layouts. Which is a good thing,
            // because that's a lot easier to check than type equality.
            if Layout::new::<T>() == Layout::new::<U>() {
                return unsafe { &mut *alloc.cast::<MaybeUninit<U>>() };
            }
            unsafe {
                let _ = Box::from_raw(alloc);
            }
            // Can't fit in local data
            if new_size > old_size {
                return Box::leak(Box::new(MaybeUninit::uninit()));
            }
        }
        self.push::<U>()
    }

    /// Get a mutable reference to the top of the stack.
    ///
    /// This is only possible if the element is recoverable, that is, if [`Stack::is_recoverable`]
    /// has returned true for the element.
    ///
    /// `T` must be no more aligned than `AlignAs`. This is checked statically.
    ///
    /// # Safety
    ///
    /// The caller must ensure that:
    /// - The stack is non-empty.
    /// - The top element has type `T` (but not necessarily initialized).
    /// - No references to the top element exist.
    /// - The element is recoverable.
    #[expect(clippy::mut_from_ref)]
    pub unsafe fn recover_last_mut<T>(&self) -> &mut MaybeUninit<T> {
        // SAFETY: as the element is recoverable, it must have been allocated on the stack, thus the
        // call to `last_mut` is valid given that other requirements hold (which hey do, as we just
        // forward them).
        unsafe { self.bounded_stack.last_mut() }
    }

    /// Check whether an element reference can be recovered.
    ///
    /// If this function returns `true` for an element, [`Stack::recover_last_mut`] can be used to
    /// obtain a reference to this element when it's at the top.
    ///
    /// If `ptr` wasn't produced by `push`, `replace_last`, or `recover_last_mut`, the return value
    /// is unspecified.
    pub fn is_recoverable<T>(&self, ptr: *const T) -> bool {
        self.bounded_stack.contains_allocated(ptr)
    }
}
