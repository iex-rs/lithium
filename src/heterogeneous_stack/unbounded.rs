use super::{align::get_rounded_size, array::Stack as BoundedStack, heap::Heap};

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

    /// Allocate enough space for an instance of `T`, returning a pointer to the new instance.
    ///
    /// `T` must be no more aligned than `AlignAs`. This is checked statically.
    ///
    /// The pointer is guaranteed to be aligned and valid, but will point at an uninitialized value.
    ///
    /// # Panics
    ///
    /// This function panics if allocating the object fails.
    pub fn push<T>(&self) -> *mut T {
        let n = get_rounded_size::<AlignAs, T>();
        let ptr = self
            .bounded_stack
            .try_push(n)
            .unwrap_or_else(|| self.heap.alloc(n));
        // - The pointer is aligned for `T` thanks to `get_rounded_size`.
        // - The pointer is valid for writes and doesn't alias by guarantees of `try_push`/`alloc`.
        ptr.cast()
    }

    /// Remove an element from the top of the stack.
    ///
    /// The element is not dropped, so it may be uninitialized. If the value needs to be dropped, do
    /// that manually.
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
    pub unsafe fn pop<T>(&self, ptr: *mut T) {
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
    pub unsafe fn replace_last<T, U>(&self, ptr: *mut T) -> *mut U {
        let old_n = get_rounded_size::<AlignAs, T>();
        let new_n = get_rounded_size::<AlignAs, U>();
        if old_n == new_n {
            // Can reuse the allocation
            return ptr.cast();
        }
        let was_on_stack = self.bounded_stack.contains_allocated(ptr.cast(), old_n);
        // SAFETY: Valid by transitive requirements.
        unsafe {
            self.pop(ptr);
        }
        if was_on_stack && new_n < old_n {
            let ptr = self.bounded_stack.try_push(new_n);
            // SAFETY: If the previous allocation was on the stack and the new allocation is
            // smaller, it must necessarily succeed.
            let ptr = unsafe { ptr.unwrap_unchecked() };
            return ptr.cast();
        }
        if !was_on_stack && new_n > old_n {
            // If the previous allocation was on the heap and the new allocation is bigger, it won't
            // fit on stack either.
            return self.heap.alloc(new_n).cast();
        }
        self.push::<U>()
    }

    /// Get a pointer to the top of the stack.
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
    /// - The element is recoverable.
    ///
    /// If the caller dereferences the pointer, it must separately ensure that no references alias.
    pub unsafe fn recover_last_mut<T>(&self) -> *mut T {
        let n = get_rounded_size::<AlignAs, T>();
        // SAFETY: As the element is recoverable, it must have been allocated on the stack. Thus
        // there are at least `n` bytes allocated.
        let ptr = unsafe { self.bounded_stack.last_mut(n) };
        ptr.cast()
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
