use super::{backend, stack_allocator};

/// Throw an exception.
///
/// If uncaught, exceptions eventually terminate the process or the thread.
///
/// # Safety
///
/// See the safety section of [this module](super) for information on matching types.
///
/// In addition, the caller must ensure that the exception cannot be caught by the system runtime.
/// This includes [`std::panic::catch_unwind`] and [`std::thread::spawn`].
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
    let is_recoverable = stack_allocator::is_recoverable(ex);
    unsafe {
        backend::throw(is_recoverable, ex);
    }
}
