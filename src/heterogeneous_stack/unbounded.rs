use super::{align::get_rounded_size, array::Stack as BoundedStack, heap::Heap};
use core::mem::MaybeUninit;

const CAPACITY: usize = 4096;

/// A thread-unsafe heterogeneous stack, using statically allocated space when possible.
// Safety invariants:
// - ZSTs are always allocated on the bounded stack.
pub struct Stack<AlignAs> {
    bounded_stack: BoundedStack<AlignAs, CAPACITY>,
    heap: Heap<AlignAs>,
}

impl<AlignAs> Stack<AlignAs> {
    /// Create an empty stack.
    pub const fn new() -> Self {
        Self {
            bounded_stack: BoundedStack::new(),
            heap: Heap::new(),
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
        let n = get_rounded_size::<AlignAs, T>();
        let ptr = self
            .bounded_stack
            .try_push(n)
            .unwrap_or_else(|| self.heap.alloc(n));
        // SAFETY:
        // - The pointer is aligned for `T` thanks to `get_rounded_size`.
        // - The pointer is valid for writes and doesn't alias by guarantees of `try_push`/`alloc`.
        unsafe { &mut *ptr.cast::<MaybeUninit<T>>() }
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
    pub unsafe fn pop<T>(&self, ptr: *mut MaybeUninit<T>) {
        let n = get_rounded_size::<AlignAs, T>();
        let ptr = ptr.cast();
        if self.bounded_stack.contains_allocated(ptr, n) {
            // SAFETY:
            // - `contains_allocated` returned `true`, so either the element is allocated on the
            //   stack or it's a ZST. ZST allocation always succeeds, so this must be on the stack.
            //   By the safety requirements, it's the top element of the stack, thus there are at
            //   least `n` bytes.
            // - The element is not accessed after the call by a transitive requirement.
            unsafe {
                self.bounded_stack.pop_unchecked(n);
            }
        } else {
            // SAFETY: `contains_allocated` returned `false`, so the allocation is not on the stack.
            // By the requirements, the pointer was produced by `push`, so the allocation has to be
            // on the heap.
            unsafe {
                self.heap.dealloc(ptr, n);
            }
        }
    }

    /// Modify the last element, possibly changing its type.
    ///
    /// This is a more efficient version of
    ///
    /// ```no_compile
    /// stack.pop(ptr);
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
    pub unsafe fn replace_last<T, U>(&self, ptr: *mut MaybeUninit<T>) -> &mut MaybeUninit<U> {
        let old_n = get_rounded_size::<AlignAs, T>();
        let new_n = get_rounded_size::<AlignAs, U>();
        if self.bounded_stack.contains_allocated(ptr.cast(), old_n) {
            unsafe {
                self.bounded_stack.pop_unchecked(old_n);
            }
            if new_n <= old_n {
                // Necessarily fits in local data
                unsafe { self.bounded_stack.try_push(new_n).unwrap_unchecked() };
                return unsafe { &mut *ptr.cast() };
            }
        } else {
            if old_n == new_n {
                // Can reuse the allocation
                return unsafe { &mut *ptr.cast() };
            }
            unsafe {
                self.heap.dealloc(ptr.cast(), old_n);
            }
            // Can't fit in local data
            if new_n > old_n {
                return unsafe { &mut *self.heap.alloc(new_n).cast() };
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
        let n = get_rounded_size::<AlignAs, T>();
        // SAFETY: As the element is recoverable, it must have been allocated on the stack. Thus
        // there are at least `n` bytes allocated.
        let ptr = unsafe { self.bounded_stack.last_mut(n) };
        // SAFETY: `ptr` points at a valid allocation of `T`, unique by the safety requirement.
        unsafe { &mut *ptr.cast() }
    }

    /// Check whether an element reference can be recovered.
    ///
    /// If this function returns `true` for an element, [`Stack::recover_last_mut`] can be used to
    /// obtain a reference to this element when it's at the top.
    ///
    /// If `ptr` wasn't produced by `push`, `replace_last`, or `recover_last_mut`, the return value
    /// is unspecified.
    pub fn is_recoverable<T>(&self, ptr: *const T) -> bool {
        let n = get_rounded_size::<AlignAs, T>();
        self.bounded_stack.contains_allocated(ptr.cast(), n)
    }
}
