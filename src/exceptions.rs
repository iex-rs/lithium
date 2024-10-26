use super::{
    backend::{ActiveBackend, Backend},
    heterogeneous_stack::unbounded::Stack,
};

type Header = <ActiveBackend as Backend>::ExceptionHeader;

/// An exception object, to be used by the backend.
#[repr(C)] // header must be the first field
pub struct Exception<E> {
    header: Header,
    cause: Unaligned<E>,
}

#[repr(packed)]
struct Unaligned<T>(T);

impl<E> Exception<E> {
    /// Create a new exception to be thrown.
    fn new(cause: E) -> Self {
        Self {
            header: ActiveBackend::new_header(),
            cause: Unaligned(cause),
        }
    }

    /// Get the cause of the exception.
    ///
    /// # Safety
    ///
    /// This function returns a bitwise copy of the cause. This means that it can only be called
    /// once on each exception.
    pub unsafe fn cause(&self) -> E {
        unsafe { (&raw const self.cause.0).read_unaligned() }
    }
}

thread_local! {
    /// Thread-local exception stack.
    static STACK: Stack<Header> = const { Stack::new() };
}

fn get_alloc_size<E>() -> usize {
    const {
        assert!(
            align_of::<Exception<E>>() == align_of::<Header>(),
            "Exception<E> has unexpected alignment",
        );
    }
    // This is a multiple of align_of::<Exception<E>>(), which we've just checked to be equal to the
    // alignment used for the stack.
    size_of::<Exception<E>>()
}

/// Push an exception onto the thread-local exception stack.
pub fn push<E>(cause: E) -> *mut Exception<E> {
    STACK.with(|stack| {
        let ex: *mut Exception<E> = stack.push(get_alloc_size::<E>()).cast();
        // SAFETY:
        // - The stack allocator guarantees the pointer is dereferenceable and unique.
        // - The stack is configured to align like Header, which get_alloc_size verifies to be the
        //   alignment of Exception<E>.
        unsafe {
            ex.write(Exception::new(cause));
        }
        ex
    })
}

/// Remove an exception from the thread-local exception stack.
///
/// # Safety
///
/// The caller must ensure `ex` corresponds to the exception at the top of the stack, as returned by
/// [`push`] or [`replace_last`] with the same exception type. In addition, the exception must not]
/// be accessed after `pop`.
pub unsafe fn pop<E>(ex: *mut Exception<E>) {
    STACK.with(|stack| {
        // SAFETY: We require `ex` to be correctly obtained and unused after `pop`.
        unsafe {
            stack.pop(ex.cast(), get_alloc_size::<E>());
        }
    });
}

/// Replace the exception on the top of the thread-local exception stack.
///
/// # Safety
///
/// The caller must ensure `ex` corresponds to the exception at the top of the stack, as returned by
/// [`push`] or [`replace_last`] with the same exception type. In addition, the old exception must
/// not be accessed after `replace_last`.
pub unsafe fn replace_last<E, F>(ex: *mut Exception<E>, cause: F) -> *mut Exception<F> {
    STACK.with(|stack| {
        let ex: *mut Exception<F> =
            // SAFETY: We require `ex` to be correctly obtained and unused after `replace_last`.
            unsafe { stack.replace_last(ex.cast(), get_alloc_size::<E>(), get_alloc_size::<F>()) }
                .cast();
        // SAFETY: `replace_last` returns unique aligned storage, good for Exception<F> as per the
        // return value of `get_alloc_size`.
        unsafe {
            ex.write(Exception::new(cause));
        }
        ex
    })
}
