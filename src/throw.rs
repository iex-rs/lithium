use super::{exceptions::UnwindException, stack_allocator};

extern "C-unwind" {
    fn _Unwind_RaiseException(ex: *mut UnwindException) -> !;
}

/// Throw an exception.
///
/// If uncaught, exceptions eventually terminate the process or the thread.
///
/// # Safety
///
/// See the safety section of [this module](super).
///
/// # Example
///
/// ```should_panic
/// use lithium::*;
///
/// unsafe {
///     throw::<&'static str>("Oops!");
/// }
/// ```
#[inline]
pub unsafe fn throw<E>(cause: E) -> ! {
    let ex = stack_allocator::push(cause);
    unsafe { _Unwind_RaiseException(ex.cast()) };
}
