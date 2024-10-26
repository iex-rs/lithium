use super::array::Stack as BoundedStack;
use core::alloc::Layout;
use core::mem::MaybeUninit;

/// A thread-unsafe heterogeneous stack, using statically allocated space when possible.
// Safety invariants:
// - ZSTs are always allocated on the bounded stack.
pub struct Stack<AlignAs> {
    bounded_stack: BoundedStack<AlignAs, 4096>,
}

/// Whether the top element can be accessed without storing a reference.
pub struct Recoverability(pub bool);

impl<AlignAs> Stack<AlignAs> {
    /// Create an empty stack.
    pub const fn new() -> Self {
        Self {
            bounded_stack: BoundedStack::new(),
        }
    }

    /// Allocate enough space for an instance of `T`.
    ///
    /// This function returns a reference to the new instance, as well as a flag indicating whether
    /// the reference can be recovered without storing the reference (see
    /// [`Stack::recover_last_mut`]).
    ///
    /// `T` must be no more aligned than `AlignAs`. This is checked statically.
    ///
    /// The new instance might be uninitialized and needs to be initialized before use.
    ///
    /// # Panics
    ///
    /// This function panics if allocating the object fails.
    pub fn push<T>(&self) -> (&mut MaybeUninit<T>, Recoverability) {
        match self.bounded_stack.try_push::<T>() {
            Some(alloc) => (alloc, Recoverability(true)),
            None => (
                Box::leak(Box::new(MaybeUninit::uninit())),
                Recoverability(false),
            ),
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
    /// # Safety
    ///
    /// The same considerations apply as to [`Stack::pop`]. The caller must ensure that:
    /// - The passed pointer was obtained from `push` (or `replace_last`) to this instance of
    ///   [`Stack`].
    /// - The passed pointer corresponds to the top element of the stack (i.e. has a matching type,
    ///   address, and provenance).
    /// - The element is not accessed after the call to `replace_last`.
    pub unsafe fn replace_last<T, U>(
        &self,
        alloc: *mut MaybeUninit<T>,
    ) -> (&mut MaybeUninit<U>, Recoverability) {
        if self.bounded_stack.contains_allocated::<T>(alloc.cast()) {
            unsafe {
                self.bounded_stack.pop_unchecked::<T>();
            }
            if size_of::<U>() <= size_of::<T>() {
                // Necessarily fits in local data
                return (
                    unsafe { self.bounded_stack.try_push().unwrap_unchecked() },
                    Recoverability(true),
                );
            }
        } else {
            // Box<T>'s are compatible as long as Ts have identical layouts. Which is a good thing,
            // because that's a lot easier to check than type equality.
            if Layout::new::<T>() == Layout::new::<U>() {
                return (
                    unsafe { &mut *alloc.cast::<MaybeUninit<U>>() },
                    Recoverability(false),
                );
            }
            unsafe {
                let _ = Box::from_raw(alloc);
            }
            // Can't fit in local data
            if size_of::<T>() >= size_of::<U>() {
                return (
                    Box::leak(Box::new(MaybeUninit::uninit())),
                    Recoverability(false),
                );
            }
        }
        self.push::<U>()
    }

    /// Get a mutable reference to the top of the stack.
    ///
    /// This is only possible if the element is recoverable.
    ///
    /// `T` must be no more aligned than `AlignAs`. This is checked statically.
    ///
    /// # Safety
    ///
    /// The caller must ensure that:
    /// - The stack is non-empty.
    /// - The top element has type `T` (but not necessarily initialized).
    /// - No references to the top element exist.
    /// - When the element was pushed, `push` returned `Recoverability(true)`.
    #[expect(clippy::mut_from_ref)]
    pub unsafe fn recover_last_mut<T>(&self) -> &mut MaybeUninit<T> {
        // SAFETY: If `push` returned `Recoverability(true)`, the value must have been allocated on
        // the stack, this the call to `last_mut` is valid given that other requirements hold (which
        // they do, as we just forward them).
        unsafe { self.bounded_stack.last_mut() }
    }
}
