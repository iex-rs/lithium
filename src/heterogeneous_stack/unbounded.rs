use super::{align::assert_aligned, array::Stack as BoundedStack, heap::Heap};

/// A thread-unsafe heterogeneous stack, using statically allocated space when possible.
///
/// Although the stack doesn't track runtime types, all elements are considered independent. Stack
/// operations must be consistent, i.e. pushing 2 bytes and then popping 1 byte twice is unsound.
// Safety invariants:
// - ZSTs are always allocated on the bounded stack.
pub struct Stack<AlignAs> {
    bounded_stack: BoundedStack<AlignAs, 4096>,
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

    /// Push an `n`-byte object.
    ///
    /// The returned pointer is guaranteed to be aligned to `align_of::<AlignAs>()` and valid for
    /// reads/writes for `n` bytes. It is also guaranteed to be unique.
    ///
    /// # Panics
    ///
    /// Panics if `n` is not a multiple of `align_of::<AlignAs>()` or if allocating the object
    /// fails.
    pub fn push(&self, n: usize) -> *mut u8 {
        self.bounded_stack
            .try_push(n)
            .unwrap_or_else(|| self.heap.alloc(n))
    }

    /// Remove an `n`-byte object from the top of the stack.
    ///
    /// # Safety
    ///
    /// The caller must ensure that:
    /// - The passed pointer was obtained from `push` to this instance of [`Stack`].
    /// - The passed pointer corresponds to the top element of the stack, i.e. it has matching `n`,
    ///   address, and provenance.
    /// - The element is not accessed after the call to `pop`.
    pub unsafe fn pop(&self, ptr: *mut u8, n: usize) {
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

    /// Modify the last element, possibly changing its size.
    ///
    /// This is a more efficient version of
    ///
    /// ```no_compile
    /// stack.pop(ptr, old_n);
    /// stack.push(new_n)
    /// ```
    ///
    /// # Panics
    ///
    /// Panics if `new_n` is not a multiple of `align_of::<AlignAs>()` or if allocating the object
    /// fails.
    ///
    /// # Safety
    ///
    /// The same considerations apply as to [`Stack::pop`]. The caller must ensure that:
    /// - The passed pointer was obtained from `push` (or `replace_last`) to this instance of
    ///   [`Stack`].
    /// - The passed pointer corresponds to the top element of the stack (i.e. has matching `old_n`,
    ///   address, and provenance).
    /// - The element is not accessed after the call to `replace_last`.
    pub unsafe fn replace_last(&self, old_ptr: *mut u8, old_n: usize, new_n: usize) -> *mut u8 {
        assert_aligned::<AlignAs>(new_n);
        if old_n == new_n {
            // Can reuse the allocation
            return old_ptr;
        }
        let was_on_stack = self.bounded_stack.contains_allocated(old_ptr, old_n);
        // SAFETY: Valid by transitive requirements.
        unsafe {
            self.pop(old_ptr, old_n);
        }
        if was_on_stack && new_n < old_n {
            let new_ptr = self.bounded_stack.try_push(new_n);
            // SAFETY: If the previous allocation was on the stack and the new allocation is
            // smaller, it must necessarily succeed.
            return unsafe { new_ptr.unwrap_unchecked() };
        }
        if !was_on_stack && new_n > old_n {
            // If the previous allocation was on the heap and the new allocation is bigger, it won't
            // fit on stack either.
            return self.heap.alloc(new_n);
        }
        self.push(new_n)
    }
}
