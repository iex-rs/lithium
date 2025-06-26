use super::{
    backend::{ActiveBackend, RethrowHandle, ThrowByPointer, ThrowByValue},
    heterogeneous_stack::unbounded::Stack,
};
use core::mem::{ManuallyDrop, offset_of};

// Module invariant: thrown exceptions of type `E` are passed to the pointer-throwing backend as
// instances of `Exception<E>` with the cause filled, which is immediately read out upon catch.

// SAFETY:
// - The main details are forwarded to the `ThrowByPointer` impl.
// - We ask the impl not to modify the object, just the header, so the object stays untouched.
unsafe impl ThrowByValue for ActiveBackend {
    type RethrowHandle<E> = PointerRethrowHandle<E>;

    #[inline]
    unsafe fn throw<E>(cause: E) -> ! {
        let ex = push(cause);
        // SAFETY: Just allocated.
        let ex = unsafe { Exception::header(ex) };
        // SAFETY:
        // - The exception is a unique pointer to an exception object, as allocated by `push`.
        // - "Don't mess with exceptions" is required transitively.
        unsafe {
            <Self as ThrowByPointer>::throw(ex);
        }
    }

    #[inline]
    fn intercept<Func: FnOnce() -> R, R, E>(func: Func) -> Result<R, (E, Self::RethrowHandle<E>)> {
        <Self as ThrowByPointer>::intercept(func).map_err(|ex| {
            // SAFETY: By the safety requirements of `throw`, unwinding could only happen from
            // `throw` with type `E`. Backend guarantees the pointer is passed as-is, and `throw`
            // only throws unique pointers to valid instances of `Exception<E>` via the backend.
            let ex = unsafe { Exception::<E>::from_header(ex) };
            let cause = {
                // SAFETY: Same as above.
                let ex_ref = unsafe { &mut *ex };
                // SAFETY: We only read the cause here once.
                unsafe { ex_ref.cause() }
            };
            (cause, PointerRethrowHandle { ex })
        })
    }
}

// Type invariant: `ex` is a unique pointer to the exception object on the exception stack.
#[derive(Debug)]
pub(crate) struct PointerRethrowHandle<E> {
    ex: *mut Exception<E>,
}

impl<E> Drop for PointerRethrowHandle<E> {
    #[inline]
    fn drop(&mut self) {
        // SAFETY:
        // - `ex` is a unique pointer to the exception object by the type invariant.
        // - The safety requirements on throwing functions guarantee that all exceptions that are
        //   thrown between `intercept` and `drop` are balanced. This exception was at the top of
        //   the stack when `intercept` returned, so it must still be at the top when `drop` is
        //   invoked.
        unsafe { pop(self.ex) }
    }
}

impl<E> RethrowHandle for PointerRethrowHandle<E> {
    #[inline]
    unsafe fn rethrow<F>(self, new_cause: F) -> ! {
        let ex = core::mem::ManuallyDrop::new(self);
        // SAFETY: The same logic that proves `pop` in `drop` is valid applies here. We're not
        // *really* dropping `self`, but the user code does not know that.
        let ex = unsafe { replace_last(ex.ex, new_cause) };
        // SAFETY: Just allocated.
        let ex = unsafe { Exception::header(ex) };
        // SAFETY:
        // - `ex` is a unique pointer to the exception object because it was just produced by
        //   `replace_last`.
        // - "Don't mess with exceptions" is required transitively.
        unsafe {
            <ActiveBackend as ThrowByPointer>::throw(ex);
        }
    }
}

type Header = <ActiveBackend as ThrowByPointer>::ExceptionHeader;

/// An exception object, to be used by the backend.
pub struct Exception<E> {
    header: Header,
    cause: ManuallyDrop<Unaligned<E>>,
}

#[repr(C, packed)]
struct Unaligned<T>(T);

impl<E> Exception<E> {
    /// Create a new exception to be thrown.
    fn new(cause: E) -> Self {
        Self {
            header: ActiveBackend::new_header(),
            cause: ManuallyDrop::new(Unaligned(cause)),
        }
    }

    /// Get pointer to header.
    ///
    /// # Safety
    ///
    /// `ex` must be a unique pointer at an exception object.
    pub const unsafe fn header(ex: *mut Self) -> *mut Header {
        // SAFETY: Required transitively.
        unsafe { ex.byte_add(offset_of!(Self, header)) }.cast()
    }

    /// Restore pointer from pointer to header.
    ///
    /// # Safety
    ///
    /// `header` must have been produced by [`Exception::header`], and the corresponding object must
    /// be alive.
    pub const unsafe fn from_header(header: *mut Header) -> *mut Self {
        // SAFETY: Required transitively.
        unsafe { header.byte_sub(offset_of!(Self, header)) }.cast()
    }

    /// Get the cause of the exception.
    ///
    /// # Safety
    ///
    /// This function returns a bitwise copy of the cause. This means that it can only be called
    /// once on each exception.
    pub unsafe fn cause(&mut self) -> E {
        // SAFETY: We transitively require that the cause is not read twice.
        unsafe { ManuallyDrop::take(&mut self.cause).0 }
    }
}

#[cfg(thread_local = "std")]
std::thread_local! {
    /// Thread-local exception stack.
    static STACK: Stack<Header> = const { Stack::new() };
}

#[cfg(thread_local = "attribute")]
#[thread_local]
static STACK: Stack<Header> = const { Stack::new() };

/// Get a reference to the thread-local exception stack.
///
/// # Safety
///
/// The reference is lifetime-extended to `'static` and is only valid for access until the end of
/// the thread. This includes at least the call frame of the immediate caller.
// Unfortunately, replacing this unsafe API with a safe `with_stack` doesn't work, as `with` fails
// to inline.
#[inline]
unsafe fn get_stack() -> &'static Stack<Header> {
    #[cfg(thread_local = "std")]
    // SAFETY: We require the caller to not use the reference anywhere near the end of the thread,
    // so as long as `with` succeeds, there is no problem.
    return STACK.with(|r| unsafe { core::mem::transmute(r) });

    #[cfg(thread_local = "attribute")]
    // SAFETY: We require the caller to not use the reference anywhere near the end of the thread,
    // so if `&STACK` is sound in the first place, there is no problem.
    return unsafe { core::mem::transmute::<&Stack<Header>, &'static Stack<Header>>(&STACK) };

    #[cfg(thread_local = "unimplemented")]
    compile_error!("Unable to compile Lithium on a platform does not support thread locals")
}

const fn get_alloc_size<E>() -> usize {
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
#[inline(always)]
pub fn push<E>(cause: E) -> *mut Exception<E> {
    // SAFETY: We don't let the stack leak past the call frame.
    let stack = unsafe { get_stack() };
    let ex: *mut Exception<E> = stack.push(get_alloc_size::<E>()).cast();
    // SAFETY:
    // - The stack allocator guarantees the pointer is dereferenceable and unique.
    // - The stack is configured to align like Header, which get_alloc_size verifies to be the
    //   alignment of Exception<E>.
    unsafe {
        ex.write(Exception::new(cause));
    }
    ex
}

/// Remove an exception from the thread-local exception stack.
///
/// # Safety
///
/// The caller must ensure `ex` corresponds to the exception at the top of the stack, as returned by
/// [`push`] or [`replace_last`] with the same exception type. In addition, the exception must not
/// be accessed after `pop`.
pub unsafe fn pop<E>(ex: *mut Exception<E>) {
    // SAFETY: We don't let the stack leak past the call frame.
    let stack = unsafe { get_stack() };
    // SAFETY: We require `ex` to be correctly obtained and unused after `pop`.
    unsafe {
        stack.pop(ex.cast(), get_alloc_size::<E>());
    }
}

/// Replace the exception on the top of the thread-local exception stack.
///
/// # Safety
///
/// The caller must ensure `ex` corresponds to the exception at the top of the stack, as returned by
/// [`push`] or [`replace_last`] with the same exception type. In addition, the old exception must
/// not be accessed after `replace_last`.
pub unsafe fn replace_last<E, F>(ex: *mut Exception<E>, cause: F) -> *mut Exception<F> {
    // SAFETY: We don't let the stack leak past the call frame.
    let stack = unsafe { get_stack() };
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
}

#[cfg(test)]
mod test {
    use super::*;
    use alloc::string::String;

    #[test]
    fn exception_cause() {
        let mut ex = Exception::new(String::from("Hello, world!"));
        assert_eq!(unsafe { ex.cause() }, "Hello, world!");
    }

    #[test]
    fn stack() {
        let ex1 = push(String::from("Hello, world!"));
        let ex2 = push(123i32);
        assert_eq!(unsafe { (*ex2).cause() }, 123);
        let ex3 = unsafe { replace_last(ex2, "Third time's a charm") };
        assert_eq!(unsafe { (*ex3).cause() }, "Third time's a charm");
        unsafe {
            pop(ex3);
        }
        assert_eq!(unsafe { (*ex1).cause() }, "Hello, world!");
        unsafe {
            pop(ex1);
        }
    }
}
